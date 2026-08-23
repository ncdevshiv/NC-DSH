"""
LanceMem Vector DB - Powered by LanceDB.
High-performance vector database for autonomous agent memory.

Features:
- Sub-millisecond vector search
- Built-in versioning
- Schema evolution (add columns anytime)
- Cloud-native storage (S3, GCS, etc.)
- Automatic index optimization
- ACID transactions
- BGE Reranker support for improved relevance
"""

from __future__ import annotations

import os
import sys
import json
import uuid
import hashlib
import logging
import traceback
import numpy as np
from pathlib import Path
from datetime import datetime
from typing import List, Dict, Any, Optional, Union, TYPE_CHECKING
from dataclasses import dataclass, field

# Module logger
logger = logging.getLogger("openmem.vector_db")

try:
    import lancedb
    from lancedb.embeddings import EmbeddingFunction
    from lancedb.table import Table
    LANCEDB_AVAILABLE = True
except ImportError:
    LANCEDB_AVAILABLE = False

# PyArrow schema definition (separate from LanceDB import)
try:
    import pyarrow as pa
    import pyarrow.compute as pc
    PA_ARROW_AVAILABLE = True
except ImportError:
    PA_ARROW_AVAILABLE = False

try:
    from sentence_transformers import SentenceTransformer
    ST_AVAILABLE = True
except ImportError:
    ST_AVAILABLE = False

# Reranker support
try:
    from sentence_transformers import CrossEncoder
    RERANKER_AVAILABLE = True
except ImportError:
    RERANKER_AVAILABLE = False

# GPU detection
try:
    import torch
    TORCH_AVAILABLE = True
except ImportError:
    TORCH_AVAILABLE = False


def is_gpu_available() -> bool:
    """Detect if GPU is available for computation."""
    if not TORCH_AVAILABLE:
        return False
    try:
        return torch.cuda.is_available()
    except Exception:
        return False


# Reranker model names
RERANKER_CPU = "BAAI/bge-reranker-base"
RERANKER_GPU = "BAAI/bge-reranker-large"

# Default embedding dimension (all-MiniLM-L6-v2). Overridden per-instance
# when the loaded embedder reports its own dimension.
DEFAULT_EMBEDDING_DIM = 384

# Known embedding models -> native output dimension, consulted when no live
# embedder is loaded (offline mode) to size fixed-size vector columns.
EMBEDDING_MODEL_DIMENSIONS = {
    "all-MiniLM-L6-v2": 384,
    "all-MiniLM-L12-v2": 384,
    "paraphrase-MiniLM-L6-v2": 384,
    "bge-small-en-v1.5": 384,
    "all-mpnet-base-v2": 768,
    "bge-base-en-v1.5": 768,
}

# lance refuses to train IVF_PQ below 256 rows ("Not enough rows to train
# PQ. Requires 256 rows") and rejects empty-table index creation outright;
# below this threshold index creation is skipped cleanly and search falls
# back to a brute-force scan.
MIN_ROWS_FOR_VECTOR_INDEX = 256


def _resolve_default_db_path() -> str:
    """
    Resolve the default database path.

    Resolution order:
    1. OPENMEM_DB_PATH environment variable — hard override used for test
       isolation and to point auxiliary processes (e.g. the MCP server) at
       a specific store.
    2. config.json "memory.db_path", read the same way core.llm reads its
       configuration.
    3. <repo>/data/lancedb (centralized data directory).

    Returns:
        Absolute-ish database directory path as a string
    """
    env_path = os.environ.get("OPENMEM_DB_PATH")
    if env_path:
        return os.path.abspath(os.path.expanduser(env_path))

    config_path = Path(__file__).parent.parent / "config.json"
    if config_path.exists():
        try:
            with open(config_path, "r") as f:
                config = json.load(f)
            configured = config.get("memory", {}).get("db_path")
            if configured:
                return configured
        except (json.JSONDecodeError, OSError):
            pass

    base = Path(__file__).parent.parent
    return os.path.join(base, "data", "lancedb")


@dataclass
class Memory:
    """A single memory entry."""
    id: str
    content: str
    session_id: Optional[str]
    timestamp: str
    importance: float
    tags: List[str]
    metadata: Dict[str, Any]
    vector: Optional[List[float]] = None

    def to_dict(self) -> Dict:
        return {
            "id": self.id,
            "content": self.content,
            "session_id": self.session_id,
            "timestamp": self.timestamp,
            "importance": self.importance,
            "tags": self.tags,
            "metadata": self.metadata
        }


class LanceDBVectorStore:
    """
    LanceDB-backed vector store for semantic memory search.
    
    Features:
    - Sub-millisecond vector search
    - Built-in versioning
    - Schema evolution (add columns anytime)
    - Cloud-native storage (S3, GCS, etc.)
    - Automatic index optimization
    - ACID transactions
    - BGE Reranker for improved relevance scoring
    """

    # LanceDB Table Schema (PyArrow)
    # Note: SCHEMA is only defined if pyarrow is available.
    # lancedb 0.37 REQUIRES fixed-size list vector columns for ANN search;
    # variable-size List(Float32) columns raise
    # "Data type is not a vector". Importance/confidence use float64 so
    # scores round-trip exactly (float32 broke 0.9 -> 0.8999999...).
    if PA_ARROW_AVAILABLE:
        SCHEMA = pa.schema([
            pa.field("id", pa.string()),
            pa.field("content", pa.string()),
            pa.field("session_id", pa.string(), nullable=True),
            pa.field("timestamp", pa.string()),
            pa.field("importance", pa.float64()),
            pa.field("tags", pa.list_(pa.string())),
            pa.field("metadata", pa.string()),  # JSON serialized
            pa.field(
                "vector",
                pa.list_(pa.float32(), DEFAULT_EMBEDDING_DIM),
                nullable=True,
            ),
        ])
    else:
        SCHEMA = None

    # Index configuration for optimized search
    INDEX_CONFIG = {
        "num_sub_vectors": 96,  # For HNSW
        "distance_type": "cosine",  # or "l2", "dot"
    }

    def __init__(self, db_path: str = None, table_name: str = "memories",
                 embedding_model: str = None, reranker_model: str = None,
                 force_rerank: bool = True, embedding_dim: Optional[int] = None):
        self.db_path = db_path or self._default_db_path()
        self.table_name = table_name
        # Configurable embedding model (defaults to all-MiniLM-L6-v2)
        self.embedding_model_name = embedding_model or self._load_embedding_model_config()
        # Explicit dimension override (constructor > config.json
        # memory.embedding_dim > loaded embedder > known-model map > default)
        self._dim_override = embedding_dim
        self._db = None
        self._table = None
        self._local_embedder = None
        # Reranker settings
        self._reranker = None
        self._reranker_model_name = None
        self._force_rerank = force_rerank
        # GPU status
        self._has_gpu = is_gpu_available()
        
        # Determine reranker model
        self._init_reranker(reranker_model)

        # Initialize embedder first so the embedding dimension is known
        # before tables are created or migrated.
        self._init_embedder()
        self._embedding_dim = self._detect_embedding_dim()
        self._init_lancedb()

    def _load_embedding_model_config(self) -> str:
        """Load embedding model name from config.json if available."""
        config_path = Path(__file__).parent.parent / "config.json"
        if config_path.exists():
            try:
                with open(config_path, "r") as f:
                    config = json.load(f)
                model = config.get("memory", {}).get("embedding_model")
                if model:
                    return model
            except (json.JSONDecodeError, OSError):
                pass
        return "all-MiniLM-L6-v2"

    def _load_reranker_model_config(self) -> Optional[str]:
        """Load reranker model name from config.json if available."""
        config_path = Path(__file__).parent.parent / "config.json"
        if config_path.exists():
            try:
                with open(config_path, "r") as f:
                    config = json.load(f)
                model = config.get("memory", {}).get("reranker_model")
                if model:
                    return model
            except (json.JSONDecodeError, OSError):
                pass
        return None

    def _init_reranker(self, model_override: str = None):
        """
        Initialize BGE reranker based on GPU availability.
        
        Uses:
        - BAAI/bge-reranker-large when GPU is available (better quality)
        - BAAI/bge-reranker-base when running on CPU (faster)
        """
        if not RERANKER_AVAILABLE:
            logger.warning("[LanceDB] sentence-transformers not available. Reranking disabled.")
            return

        # Check config for explicit model override
        config_model = self._load_reranker_model_config()
        if model_override:
            self._reranker_model_name = model_override
        elif config_model:
            self._reranker_model_name = config_model
        else:
            # Auto-select based on GPU
            self._reranker_model_name = RERANKER_GPU if self._has_gpu else RERANKER_CPU

        try:
            self._reranker = CrossEncoder(self._reranker_model_name)
            gpu_label = "GPU" if self._has_gpu else "CPU"
            logger.info(f"[LanceDB] Reranker loaded: {self._reranker_model_name} ({gpu_label})")
        except Exception as e:
            logger.error(f"[LanceDB] Failed to load reranker '{self._reranker_model_name}': {e}")
            self._reranker = None
            self._reranker_model_name = None

    def _rerank_results(self, query: str, results: List[Dict], top_k: int = None) -> List[Dict]:
        """
        Rerank search results using BGE reranker.
        
        Args:
            query: The original search query
            results: Initial vector search results
            top_k: Number of results to return after reranking
            
        Returns:
            Reranked results with updated scores
        """
        if not self._reranker or not results:
            return results

        try:
            # Prepare query-document pairs for reranker
            doc_pairs = [(query, r.get("content", "")) for r in results]
            
            # Get rerank scores
            scores = self._reranker.predict(doc_pairs)
            
            # Add rerank score to results and sort
            for i, r in enumerate(results):
                r["rerank_score"] = float(scores[i]) if hasattr(scores[i], '__float__') else float(scores[i])
            
            # Sort by rerank score descending
            reranked = sorted(results, key=lambda x: x.get("rerank_score", 0), reverse=True)
            
            # Return top_k results
            if top_k:
                reranked = reranked[:top_k]
            
            return reranked
            
        except Exception as e:
            logger.warning(f"[LanceDB] Reranking failed: {e}. Returning vector search results.")
            return results

    def _default_db_path(self) -> str:
        """Get default database path (see _resolve_default_db_path)."""
        return _resolve_default_db_path()

    def _init_lancedb(self):
        """Initialize LanceDB connection and table."""
        if not LANCEDB_AVAILABLE:
            print("[LanceDB] LanceDB not installed. Run: pip install lancedb")
            return

        try:
            # Create database directory
            os.makedirs(self.db_path, exist_ok=True)

            # Open LanceDB database
            self._db = lancedb.connect(self.db_path)

            schema = self._memories_schema()
            existing_tables = self._db.table_names()

            if self.table_name in existing_tables:
                # Legacy tables with a variable-size vector column cannot
                # serve ANN search on lancedb 0.37 -> rebuild if types drifted.
                self._migrate_table_if_needed(self.table_name, schema)
                self._table = self._db.open_table(self.table_name)
            else:
                # Create table with schema (without index first)
                self._table = self._db.create_table(
                    self.table_name,
                    schema=schema,
                    exist_ok=True
                )

            # ANN index: IVF_PQ needs >= 256 usable vectors to train
            # ("Not enough rows to train PQ") and empty tables are rejected
            # outright, so below the threshold we skip cleanly; search
            # still works as a flat scan without an index.
            row_count = len(self._table)
            if row_count < MIN_ROWS_FOR_VECTOR_INDEX:
                logger.info(
                    f"[LanceDB] Skipping vector index on '{self.table_name}': "
                    f"{row_count} rows < {MIN_ROWS_FOR_VECTOR_INDEX} needed to train"
                )
            else:
                try:
                    index_kwargs = {"metric": "cosine", "vector_column_name": "vector"}
                    if self._embedding_dim % 96 == 0:
                        index_kwargs["num_sub_vectors"] = 96
                    self._table.create_index(**index_kwargs)
                    print(f"[LanceDB] IVF_PQ index created on '{self.table_name}.vector'")
                except Exception as e:
                    if "exist" in str(e).lower():
                        logger.debug(f"[LanceDB] Vector index already present: {e}")
                    else:
                        logger.warning(f"Vector index creation skipped: {e}")

            print(f"[LanceDB] Connected to {self.db_path}")
            print(f"[LanceDB] Table: {self.table_name}, Rows: {len(self._table)}")

        except Exception as e:
            logger.error(f"[LanceDB] Failed to initialize: {e}", exc_info=True)
            self._db = None
            self._table = None

    def _init_embedder(self):
        """Initialize embedding function for vectorization."""
        if not ST_AVAILABLE:
            logger.warning("sentence-transformers not available. Using raw text matching.")
            return

        try:
            # Use the configured embedder (from config.json or default)
            self._local_embedder = SentenceTransformer(self.embedding_model_name)
            logger.info(f"Embedder loaded: {self.embedding_model_name}")
        except Exception as e:
            logger.error(f"Failed to load embedder '{self.embedding_model_name}': {e}")

    def _load_embedding_dim_config(self) -> Optional[int]:
        """Load an explicit embedding_dim override from config.json."""
        config_path = Path(__file__).parent.parent / "config.json"
        if config_path.exists():
            try:
                with open(config_path, "r") as f:
                    config = json.load(f)
                dim = config.get("memory", {}).get("embedding_dim")
                if isinstance(dim, int) and dim > 0:
                    return dim
            except (json.JSONDecodeError, OSError):
                pass
        return None

    def _detect_embedding_dim(self) -> int:
        """
        Resolve the embedding dimension for fixed-size vector columns.

        Precedence: constructor override > dimension reported by a loaded
        embedder > config.json "memory.embedding_dim" > known-model mapping
        > DEFAULT_EMBEDDING_DIM (384, all-MiniLM-L6-v2).

        Returns:
            Embedding dimension used to size fixed-size vector columns
        """
        if self._dim_override:
            return int(self._dim_override)

        embedder = self._local_embedder
        if embedder is not None:
            getter = getattr(embedder, "get_sentence_embedding_dimension", None)
            if callable(getter):
                try:
                    dim = getter()
                    if isinstance(dim, int) and dim > 0:
                        return dim
                except Exception as e:
                    logger.debug(f"Embedding dimension detection failed: {e}")

        # No live embedder (offline mode): fall back to configuration and
        # the known-model mapping so schemas still get a sane fixed size.
        dim = self._load_embedding_dim_config()
        if dim:
            return dim
        return EMBEDDING_MODEL_DIMENSIONS.get(
            self.embedding_model_name, DEFAULT_EMBEDDING_DIM
        )

    def _memories_schema(self, dim: Optional[int] = None):
        """Build the memories table schema with a fixed-size vector column."""
        dim = dim or self._embedding_dim
        return pa.schema([
            pa.field("id", pa.string()),
            pa.field("content", pa.string()),
            pa.field("session_id", pa.string(), nullable=True),
            pa.field("timestamp", pa.string()),
            pa.field("importance", pa.float64()),
            pa.field("tags", pa.list_(pa.string())),
            pa.field("metadata", pa.string()),  # JSON serialized
            pa.field(
                "vector",
                pa.list_(pa.float32(), dim),
                nullable=True,
            ),
        ])

    def _profiles_schema(self, dim: Optional[int] = None):
        """Build the user_profiles schema with a fixed-size key vector."""
        dim = dim or self._embedding_dim
        return pa.schema([
            pa.field("profile_key", pa.string()),
            pa.field("profile_value", pa.string()),
            pa.field("confidence", pa.float64()),
            pa.field("updated_at", pa.string()),
            pa.field("key_vector", pa.list_(pa.float32(), dim), nullable=True),
        ])

    def _migrate_table_if_needed(self, name: str, expected_schema) -> bool:
        """
        Rebuild a legacy table whose column types predate this module's schema.

        lancedb 0.37 rejects variable-size List(Float32) vector columns for
        ANN search ("Data type is not a vector"), so tables created by older
        versions of this file must be recreated against fixed-size vectors.
        Runtime data under data/lancedb is disposable development data: rows
        are salvaged as-is and any vector whose length does not match the
        expected embedding dimension becomes NULL.

        Args:
            name: Table name
            expected_schema: Target pyarrow schema

        Returns:
            True if the table was rebuilt, False if it already matched
        """
        if self._db is None or expected_schema is None:
            return False

        try:
            table = self._db.open_table(name)
        except Exception:
            return False

        needs_rebuild = False
        for f in expected_schema:
            if f.name not in table.schema.names:
                needs_rebuild = True
                break
            if not table.schema.field(f.name).type.equals(f.type):
                needs_rebuild = True
                break
        if not needs_rebuild:
            return False

        try:
            old_rows = table.to_arrow().to_pylist()
            vector_col = "vector" if name == self.table_name else "key_vector"

            salvaged = []
            for r in old_rows:
                row = {}
                for f in expected_schema:
                    val = r.get(f.name)
                    if f.name == vector_col:
                        if not (isinstance(val, list) and len(val) == self._embedding_dim):
                            val = None
                    elif (
                        (pa.types.is_list(f.type) or pa.types.is_fixed_size_list(f.type))
                        and not isinstance(val, list)
                    ):
                        val = []
                    elif f.name == "metadata" and isinstance(val, (dict, list)):
                        val = json.dumps(val)
                    elif f.name in ("importance", "confidence") and val is None:
                        val = 0.5
                    row[f.name] = val
                salvaged.append(row)

            self._db.drop_table(name)
            new_table = self._db.create_table(name, schema=expected_schema)
            if salvaged:
                new_table.add(salvaged)
            logger.warning(
                f"[LanceDB] Migrated table '{name}' to fixed-size vector schema "
                f"(rows salvaged: {len(salvaged)})"
            )
            return True
        except Exception as e:
            logger.error(f"[LanceDB] Migration of table '{name}' failed: {e}", exc_info=True)
            return False

    def _generate_id(self, content: str) -> str:
        """Generate unique memory ID."""
        unique = f"{content}{datetime.now().isoformat()}"
        return hashlib.sha256(unique.encode()).hexdigest()[:16]

    @staticmethod
    def _sql_quote(value: str) -> str:
        """
        Quote a string literal for a LanceDB SQL predicate.

        Doubles embedded single quotes (SQL-standard escaping) so
        user-supplied values cannot break out of the literal.

        Args:
            value: Raw string value to quote

        Returns:
            Single-quoted SQL literal
        """
        return "'" + str(value).replace("'", "''") + "'"

    def _embed_text(self, texts: Union[str, List[str]]) -> np.ndarray:
        """
        Generate embeddings for text.

        Args:
            texts: Single text or list of texts

        Returns:
            Embedding vectors (list of float lists)

        Raises:
            RuntimeError: If no embedding model is loaded. Callers that want
                to store memories without vectors should pass
                auto_embed=False instead of catching this.
        """
        if isinstance(texts, str):
            texts = [texts]

        if self._local_embedder is not None:
            embeddings = self._local_embedder.encode(texts)
            return embeddings.tolist() if hasattr(embeddings, 'tolist') else embeddings

        # No silent random-vector fallback: garbage vectors poison search.
        raise RuntimeError(
            "Embedding model unavailable - install sentence-transformers "
            "or pass auto_embed=False"
        )

    def add_memory(
        self,
        content: str,
        session_id: Optional[str] = None,
        importance: float = 0.5,
        tags: Optional[List[str]] = None,
        metadata: Optional[Dict[str, Any]] = None,
        auto_embed: bool = True,
        memory_id: Optional[str] = None
    ) -> str:
        """
        Add a memory to the vector store.

        Args:
            content: Memory text content
            session_id: Associated session ID
            importance: 0.0-1.0 importance score
            tags: List of tag strings
            metadata: Additional metadata dict
            auto_embed: Whether to auto-generate embedding
            memory_id: Optional explicit id (e.g. deterministic golden-corpus
                fixtures). When omitted, a content+timestamp hash id is
                generated as before.

        Returns:
            Memory ID string
        """
        if self._table is None:
            logger.error("[LanceDB] Table not available. Memory not saved.")
            return None

        memory_id = memory_id or self._generate_id(content)
        timestamp = datetime.now().isoformat()

        # Generate embedding
        vector = None
        if auto_embed and self._local_embedder:
            vector = self._embed_text(content)[0]

        # Serialize metadata
        metadata_str = json.dumps(metadata or {})

        # Create record
        record = {
            "id": memory_id,
            "content": content,
            "session_id": session_id,
            "timestamp": timestamp,
            "importance": float(importance),
            "tags": tags or [],
            "metadata": metadata_str,
            "vector": vector
        }

        try:
            self._table.add([record])
            return memory_id
        except Exception as e:
            logger.error(f"[LanceDB] Failed to add memory: {e}", exc_info=True)
            return None

    def add_memories_batch(self, memories: List[Dict]) -> List[str]:
        """
        Add multiple memories at once (optimized for bulk insert).
        
        Args:
            memories: List of memory dicts with keys:
                - content (required)
                - session_id (optional)
                - importance (optional, default 0.5)
                - tags (optional)
                - metadata (optional)
                
        Returns:
            List of memory IDs
        """
        if self._table is None:
            return []

        if not memories:
            return []

        # Batch embed if possible
        texts = [m.get("content", "") for m in memories]
        vectors = None
        if self._local_embedder:
            vectors = self._embed_text(texts)

        records = []
        for i, mem in enumerate(memories):
            memory_id = self._generate_id(mem.get("content", "") + str(i))
            timestamp = datetime.now().isoformat()

            record = {
                "id": memory_id,
                "content": mem.get("content", ""),
                "session_id": mem.get("session_id"),
                "timestamp": timestamp,
                "importance": float(mem.get("importance", 0.5)),
                "tags": mem.get("tags", []),
                "metadata": json.dumps(mem.get("metadata", {})),
                "vector": vectors[i] if vectors else None
            }
            records.append(record)

        try:
            self._table.add(records)
            return [r["id"] for r in records]
        except Exception as e:
            logger.error(f"[LanceDB] Batch add failed: {e}", exc_info=True)
            return []

    def search(
        self,
        query: str,
        n_results: int = 5,
        session_id: Optional[str] = None,
        min_importance: float = 0.0,
        tags: Optional[List[str]] = None,
        filter_fn: Optional[callable] = None,
        use_rerank: bool = True
    ) -> List[Dict]:
        """
        Semantic vector search for memories with optional reranking.
        
        Args:
            query: Search query text
            n_results: Max number of results
            session_id: Filter by session
            min_importance: Filter by minimum importance
            tags: Filter by tags (any match)
            filter_fn: Custom filter function (takes row dict, returns bool)
            use_rerank: Whether to use BGE reranker for improved relevance (default: True)
            
        Returns:
            List of matching memory dicts with scores
        """
        if self._table is None:
            return []

        # Graceful degradation: with no embedder, fall back to case-insensitive
        # substring matching over content (honours the "raw text matching"
        # notice logged by _init_embedder) instead of failing every query.
        if self._local_embedder is None:
            return self._keyword_search(
                query, n_results, session_id, min_importance, tags, filter_fn
            )

        try:
            # Generate query embedding
            query_embedding = self._embed_text(query)[0]

            # Build WHERE clause
            where_clauses = []
            if session_id:
                where_clauses.append(f"session_id = {self._sql_quote(session_id)}")
            if min_importance > 0:
                where_clauses.append(f"importance >= {float(min_importance)}")
            where = " AND ".join(where_clauses) if where_clauses else None

            # Vector search - get more candidates for reranking
            search_results = self._table.search(
                query=query_embedding,
                vector_column_name="vector"
            ).where(where) if where else self._table.search(
                query=query_embedding,
                vector_column_name="vector"
            )

            # Get 4x candidates for better reranking
            candidates = search_results.limit(n_results * 4).to_list()

            # Post-filter by tags and custom filter
            filtered = []
            for r in candidates:
                # Tag filter
                if tags:
                    mem_tags = r.get("tags", [])
                    if not any(t in mem_tags for t in tags):
                        continue

                # Custom filter
                if filter_fn and not filter_fn(r):
                    continue

                # Parse metadata
                r["metadata"] = json.loads(r.get("metadata", "{}"))

                filtered.append(r)

                if len(filtered) >= n_results * 4:
                    break

            # Apply reranking if enabled and available
            if use_rerank and self._force_rerank and self._reranker and filtered:
                results = self._rerank_results(query, filtered, top_k=n_results)
            else:
                # Fallback: return vector search results without reranking
                results = filtered[:n_results]

            return results

        except Exception as e:
            logger.error(f"[LanceDB] Search failed: {e}", exc_info=True)
            return []

    def _keyword_search(
        self,
        query: str,
        n_results: int,
        session_id: Optional[str],
        min_importance: float,
        tags: Optional[List[str]],
        filter_fn: Optional[callable],
    ) -> List[Dict]:
        """
        Embedder-free fallback search: case-insensitive per-term substring
        matching with naive relevance ranking (results are ordered by how
        many distinct query terms they contain, then by total term
        frequency). Each result carries a "score" field: the fraction of
        query terms matched.

        Args:
            query: Search query text (whitespace-split into terms)
            n_results: Max number of results
            session_id: Filter by session
            min_importance: Filter by minimum importance
            tags: Filter by tags (any match)
            filter_fn: Custom filter function (takes row dict, returns bool)

        Returns:
            List of matching memory dicts ranked by naive relevance
        """
        try:
            terms = [t.strip() for t in query.split() if t.strip()]
            if not terms:
                return []

            arrow = self._table.to_arrow()
            combined = None
            for term in terms:
                mask = pc.match_substring(arrow["content"], term, ignore_case=True)
                combined = mask if combined is None else pc.or_(combined, mask)
            if session_id:
                combined = pc.and_(combined, pc.equal(arrow["session_id"], session_id))
            if min_importance > 0:
                combined = pc.and_(
                    combined, pc.greater_equal(arrow["importance"], float(min_importance))
                )
            rows = arrow.filter(combined).to_pylist()

            lowered_terms = [t.lower() for t in terms]

            def _rank(row: Dict):
                text = row.get("content", "").lower()
                hits = sum(1 for t in lowered_terms if t in text)
                freq = sum(text.count(t) for t in lowered_terms)
                # Score: fraction of distinct terms matched, tie-broken by
                # total occurrences so denser matches rank first.
                row["score"] = hits / len(lowered_terms) if lowered_terms else 0.0
                return (hits, freq)

            filtered = []
            for r in rows:
                if tags and not any(t in r.get("tags", []) for t in tags):
                    continue
                if filter_fn and not filter_fn(r):
                    continue
                r["metadata"] = json.loads(r.get("metadata") or "{}")
                filtered.append(r)

            filtered.sort(key=_rank, reverse=True)
            return filtered[:n_results]
        except Exception as e:
            logger.error(f"[LanceDB] Raw-text search failed: {e}", exc_info=True)
            return []

    def get_memory(self, memory_id: str) -> Optional[Dict]:
        """
        Get a specific memory by ID.

        Args:
            memory_id: Memory ID to look up

        Returns:
            Memory row dict with parsed metadata, or None if not found
        """
        if self._table is None:
            return None

        try:
            # Exact-key lookup via Arrow compute: no SQL string, no vector
            # search (a hex id is not a query vector).
            arrow = self._table.to_arrow()
            mask = pc.equal(arrow["id"], memory_id)
            rows = arrow.filter(mask).to_pylist()

            if not rows:
                return None

            row = rows[0]
            try:
                row["metadata"] = json.loads(row.get("metadata") or "{}")
            except json.JSONDecodeError as e:
                logger.warning(
                    f"[LanceDB] Malformed metadata JSON for memory {memory_id}: {e}"
                )
                row["metadata"] = {}
            return row
        except Exception as e:
            logger.error(f"[LanceDB] Failed to get memory {memory_id}: {e}", exc_info=True)
            return None

    def get_recent_memories(
        self,
        hours: int = 24,
        limit: int = 100,
        session_id: Optional[str] = None
    ) -> List[Dict]:
        """
        Get recent memories within time window, newest first.

        Args:
            hours: Size of the look-back window in hours
            limit: Maximum number of memories to return
            session_id: Optional session filter

        Returns:
            List of memory dicts ordered by timestamp descending
        """
        if self._table is None:
            return []

        try:
            from datetime import timedelta
            cutoff = (datetime.now() - timedelta(hours=hours)).isoformat()

            # Time-windowed fetch via Arrow compute (injection-safe); no
            # dummy-vector search, so ordering below is genuine recency.
            arrow = self._table.to_arrow()
            mask = pc.greater_equal(arrow["timestamp"], cutoff)
            if session_id:
                mask = pc.and_(mask, pc.equal(arrow["session_id"], session_id))
            rows = arrow.filter(mask).to_pylist()

            # Recency contract: newest first regardless of insertion order.
            rows.sort(key=lambda r: r.get("timestamp", ""), reverse=True)
            rows = rows[:max(0, int(limit))]

            for r in rows:
                try:
                    r["metadata"] = json.loads(r.get("metadata") or "{}")
                except json.JSONDecodeError as e:
                    logger.warning(f"[LanceDB] Malformed metadata JSON in recent memories: {e}")
                    r["metadata"] = {}
            return rows
        except Exception as e:
            logger.error(f"[LanceDB] Get recent failed: {e}", exc_info=True)
            return []

    def update_memory(self, memory_id: str, updates: Dict) -> bool:
        """
        Update a memory's fields.

        Only changed columns are written. Rows whose new payload contains an
        empty list column value (e.g. tags=[]) are replaced via delete+add,
        because table.update() cannot build an array from an empty list on
        lancedb 0.37 ("concat requires input of at least one array").

        Args:
            memory_id: Memory ID to update
            updates: Allowed keys: content, importance, tags, metadata

        Returns:
            True if a row was updated, False otherwise
        """
        if self._table is None:
            return False

        try:
            # Get current row
            current = self.get_memory(memory_id)
            if not current:
                return False

            timestamp = datetime.now().isoformat()

            # Merge allowed updates only (get_memory parses metadata back
            # into a dict; keep the stored JSON form for the table)
            changed = {}
            if "content" in updates:
                changed["content"] = str(updates["content"])
            if "importance" in updates:
                changed["importance"] = float(updates["importance"])
            if isinstance(updates.get("tags"), list):
                changed["tags"] = updates["tags"]
            if "metadata" in updates:
                md = updates["metadata"]
                changed["metadata"] = md if isinstance(md, str) else json.dumps(md)

            if not changed:
                logger.warning(f"[LanceDB] No updatable fields in update for {memory_id}")
                return False

            has_empty_list = any(isinstance(v, list) and not v for v in changed.values())

            if has_empty_list:
                # Fallback path: replace the whole row (delete + add), which
                # handles empty list values that table.update() cannot.
                row = {
                    "id": memory_id,
                    "content": str(current.get("content", "")),
                    "session_id": current.get("session_id"),
                    "timestamp": timestamp,
                    "importance": float(current.get("importance", 0.5)),
                    "tags": list(current.get("tags") or []),
                    "metadata": current.get("metadata") if isinstance(current.get("metadata"), str)
                                else json.dumps(current.get("metadata") or {}),
                    "vector": current.get("vector"),
                }
                row.update(changed)
                self._table.delete(f"id = {self._sql_quote(memory_id)}")
                self._table.add([row])
                return True

            result = self._table.update(
                where=f"id = {self._sql_quote(memory_id)}",
                values={**changed, "timestamp": timestamp},
            )
            updated = getattr(result, "rows_updated", 1)
            if not updated:
                logger.warning(f"[LanceDB] Update matched no rows for memory {memory_id}")
                return False
            return True
        except Exception as e:
            logger.error(f"[LanceDB] Update failed for memory {memory_id}: {e}", exc_info=True)
            return False

    def update_importance(self, memory_id: str, importance: float) -> bool:
        """Update memory importance score."""
        return self.update_memory(memory_id, {"importance": importance})

    def delete_memory(self, memory_id: str) -> bool:
        """
        Delete a memory.

        Returns True only when a row actually matched and was deleted; a
        miss on an unknown id returns False so callers (e.g. the MCP
        forget tool) can report an honest result.
        """
        if self._table is None:
            return False

        try:
            arrow = self._table.to_arrow()
            mask = pc.equal(arrow["id"], memory_id)
            exists = arrow.filter(mask).num_rows > 0
            if not exists:
                return False

            self._table.delete(f"id = {self._sql_quote(memory_id)}")
            return True
        except Exception as e:
            logger.error(f"[LanceDB] Delete failed for memory {memory_id}: {e}", exc_info=True)
            return False

    def delete_old_memories(self, days: int = 30, min_importance: float = 0.3) -> int:
        """
        Delete memories older than N days with low importance.

        Args:
            days: Age threshold in days
            min_importance: Only memories with importance strictly below
                this value are deleted

        Returns:
            Number of rows deleted (0 when table unavailable or nothing matched)
        """
        if self._table is None:
            return 0

        try:
            from datetime import timedelta
            cutoff = (datetime.now() - timedelta(days=days)).isoformat()

            # Count victims first via Arrow compute (injection-safe; Table
            # has no .where() on lancedb 0.37).
            arrow = self._table.to_arrow()
            mask = pc.and_(
                pc.less(arrow["timestamp"], cutoff),
                pc.less(arrow["importance"], float(min_importance)),
            )
            count = arrow.filter(mask).num_rows

            if count == 0:
                return 0

            # Same predicate as SQL for the actual delete (verified signature:
            # Table.delete(where: Union[str, Expr])).
            where = (
                f"timestamp < {self._sql_quote(cutoff)} "
                f"AND importance < {float(min_importance)}"
            )
            self._table.delete(where)
            return count
        except Exception as e:
            logger.error(f"[LanceDB] Delete old failed: {e}", exc_info=True)
            return 0

    # ===== User Profile Operations =====

    def set_user_profile(self, key: str, value: str, confidence: float = 0.5) -> bool:
        """
        Set a user profile attribute (exact-key upsert).

        Identity is the exact profile_key, never vector similarity. The
        embedded key is still written for future semantic use when an
        embedder is available; it is optional (schema-legal NULL).

        Args:
            key: Profile attribute name
            value: Profile attribute value
            confidence: 0.0-1.0 confidence score

        Returns:
            True on success
        """
        profile_table = self._get_or_create_table("user_profiles", self._profiles_schema())
        if profile_table is None:
            return False

        updated_at = datetime.now().isoformat()

        try:
            arrow = profile_table.to_arrow()
            mask = pc.equal(arrow["profile_key"], key)
            exists = arrow.filter(mask).num_rows > 0

            if exists:
                result = profile_table.update(
                    where=f"profile_key = {self._sql_quote(key)}",
                    values={
                        "profile_value": value,
                        "confidence": float(confidence),
                        "updated_at": updated_at,
                    },
                )
                if not getattr(result, "rows_updated", 1):
                    # Row vanished between filter and update: fall through to insert.
                    logger.warning(f"[LanceDB] Profile update matched 0 rows for '{key}'")
                    exists = False

            if not exists:
                key_vector = None
                if self._local_embedder is not None:
                    try:
                        key_vector = self._embed_text(key)[0]
                    except Exception as e:
                        logger.warning(f"[LanceDB] Key embedding skipped for '{key}': {e}")
                profile_table.add([{
                    "profile_key": key,
                    "profile_value": value,
                    "confidence": float(confidence),
                    "updated_at": updated_at,
                    "key_vector": key_vector,
                }])
            return True
        except Exception as e:
            logger.error(f"[LanceDB] Set profile failed for '{key}': {e}", exc_info=True)
            return False

    def get_user_profile(self, key: str) -> Optional[Dict]:
        """
        Get a user profile attribute by exact key.

        Args:
            key: Profile attribute name

        Returns:
            Dict with key/value/confidence/updated_at, or None if absent
        """
        profile_table = self._get_or_create_table("user_profiles", self._profiles_schema())
        if profile_table is None:
            return None

        try:
            arrow = profile_table.to_arrow()
            rows = arrow.filter(pc.equal(arrow["profile_key"], key)).to_pylist()

            if rows:
                r = rows[0]
                return {
                    "key": r["profile_key"],
                    "value": r["profile_value"],
                    "confidence": r["confidence"],
                    "updated_at": r["updated_at"],
                }
            return None
        except Exception as e:
            logger.error(f"[LanceDB] Get user profile failed for '{key}': {e}", exc_info=True)
            return None

    def get_all_user_profiles(self) -> Dict[str, Dict]:
        """Get all user profile attributes."""
        profile_table = self._get_or_create_table("user_profiles", self._profiles_schema())
        if profile_table is None:
            return {}

        try:
            # Table has no .to_list() on lancedb 0.37; go via Arrow.
            results = profile_table.to_arrow().to_pylist()
            profiles = {}
            for r in results:
                profiles[r["profile_key"]] = {
                    "value": r["profile_value"],
                    "confidence": r["confidence"],
                    "updated_at": r["updated_at"]
                }
            return profiles
        except Exception as e:
            logger.debug(f"Failed to get all user profiles: {e}")
            return {}

    def _get_or_create_table(self, name: str, schema) -> Optional[Table]:
        """Get or create a named table."""
        if self._db is None:
            return None

        try:
            existing = self._db.table_names()
            if name in existing:
                if schema is not None:
                    # Rebuild legacy layouts before handing out a handle.
                    self._migrate_table_if_needed(name, schema)
                return self._db.open_table(name)
            return self._db.create_table(name, schema=schema, exist_ok=True)
        except Exception as e:
            logger.error(f"[LanceDB] Table {name} get-or-create failed: {e}", exc_info=True)
            return None

    # ===== Utility Methods =====

    def get_stats(self) -> Dict[str, Any]:
        """Get database statistics."""
        stats = {
            "lancedb_available": LANCEDB_AVAILABLE,
            "embedder_available": self._local_embedder is not None,
            "db_path": self.db_path,
            "tables": [],
            "reranker": {
                "available": RERANKER_AVAILABLE,
                "loaded": self._reranker is not None,
                "model": self._reranker_model_name,
                "gpu_enabled": self._has_gpu,
                "force_rerank": self._force_rerank
            }
        }

        if self._db:
            try:
                stats["tables"] = self._db.table_names()
                for table_name in stats["tables"]:
                    try:
                        table = self._db.open_table(table_name)
                        stats[f"table_{table_name}_rows"] = len(table)
                    except Exception as e:
                        logger.debug(f"Could not read table stats for '{table_name}': {e}")
            except Exception as e:
                logger.debug(f"Could not list tables: {e}")

        # Convenience counters used by main.py / tests
        stats["total_memories"] = stats.get(f"table_{self.table_name}_rows", 0)
        stats["total_user_profiles"] = stats.get("table_user_profiles_rows", 0)

        return stats

    def optimize(self):
        """Optimize table indices and compaction."""
        if self._table is None:
            return

        try:
            # Trigger compaction
            self._table.compact_files()
            print("[LanceDB] Table optimized")
        except Exception as e:
            logger.warning(f"[LanceDB] Optimize failed: {e}", exc_info=True)

    def backup(self, backup_path: str = None) -> str:
        """Create a backup of the database."""
        if self._db is None:
            return None

        if backup_path is None:
            backup_path = os.path.join(self.db_path, "..", "backups", datetime.now().strftime("%Y%m%d_%H%M%S"))

        try:
            os.makedirs(backup_path, exist_ok=True)
            
            # Copy entire db directory
            import shutil
            shutil.copytree(self.db_path, os.path.join(backup_path, "lancedb"), dirs_exist_ok=True)
            
            print(f"[LanceDB] Backup created: {backup_path}")
            return backup_path
        except Exception as e:
            logger.error(f"[LanceDB] Backup failed: {e}", exc_info=True)
            return None

    def close(self):
        """Close database connections."""
        # LanceDB doesn't require explicit close
        self._db = None
        self._table = None

    def __len__(self) -> int:
        """Get total memory count."""
        if self._table is None:
            return 0
        return len(self._table)


# User Profile Schema (default layout; see LanceDBVectorStore._profiles_schema
# for the per-instance version with detected embedding dimension)
if PA_ARROW_AVAILABLE:
    USER_PROFILE_SCHEMA = pa.schema([
        pa.field("profile_key", pa.string()),
        pa.field("profile_value", pa.string()),
        pa.field("confidence", pa.float64()),
        pa.field("updated_at", pa.string()),
        pa.field(
            "key_vector",
            pa.list_(pa.float32(), DEFAULT_EMBEDDING_DIM),
            nullable=True,
        ),
    ])
else:
    USER_PROFILE_SCHEMA = None


# Singleton instance
_db_instance = None

def get_vector_db() -> LanceDBVectorStore:
    """Get or create singleton VectorDB instance.

    A previously-closed singleton is re-created on demand: close() nils out
    _db/_table, and handing that zombie back made every later consumer
    silently drop writes ("Table not available").
    """
    global _db_instance
    if _db_instance is None or getattr(_db_instance, "_db", None) is None:
        _db_instance = LanceDBVectorStore()
    return _db_instance


# Backwards compatibility alias for tests and legacy imports
VectorDB = LanceDBVectorStore
