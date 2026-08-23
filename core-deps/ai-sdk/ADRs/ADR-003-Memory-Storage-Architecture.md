# Architecture Decision Record (ADR) 003: Memory and Storage Architecture

**Status:** Proposed  
**Date:** February 17, 2026  
**Deciders:** Architecture Team, Backend Engineers  

---

## Context

We need to design the memory and storage architecture for the AI SDK. The PRD specifies a 4-tier memory system (working, short-term, long-term, semantic) with compaction. This decision impacts scalability, cost, and performance.

## Decision Drivers

1. **Performance** - Fast reads/writes for real-time agents
2. **Scalability** - Support millions of conversations
3. **Cost** - Minimize storage costs
4. **Persistence** - Data durability requirements
5. **Search** - Semantic search capabilities
6. **Compaction** - Automatic summarization and archival
7. **Privacy** - Data residency and encryption

## Considered Options

### Option 1: Single Database (PostgreSQL with pgvector)

**Architecture:**
- PostgreSQL for all storage
- pgvector extension for embeddings
- JSONB for flexible schemas

**Pros:**
- ✅ Single database to manage
- ✅ ACID compliance
- ✅ Good for relational data
- ✅ pgvector enables vector search
- ✅ JSONB for flexible memory structures

**Cons:**
- ❌ Not optimal for session/caching (Redis better)
- ❌ Vector search slower than dedicated vector DB
- ❌ Harder to scale horizontally
- ❌ All data in one place (blast radius)

---

### Option 2: Polyglot Persistence (Specialized Stores)

**Architecture:**
```
Working Memory:     Redis (fast, ephemeral)
Short-term Memory:  Redis (with persistence)
Long-term Memory:   PostgreSQL (reliable, relational)
Semantic Memory:    Pinecone/Weaviate (vector search)
Graph Memory:       Neo4j (optional, for GraphRAG)
Cache:              Redis/Memcached
```

**Pros:**
- ✅ Each store optimized for its use case
- ✅ Best performance for each tier
- ✅ Can scale independently
- ✅ Specialized vector search

**Cons:**
- ❌ More complexity to manage
- ❌ Multiple connection pools
- ❌ Consistency challenges across stores
- ❌ Higher operational overhead

---

### Option 3: Unified Interface with Pluggable Backends (Recommended)

**Architecture:**
```typescript
// Unified interface
interface MemoryStore {
  get(key: string): Promise<Memory>
  set(key: string, value: Memory): Promise<void>
  search(query: string): Promise<Memory[]>
  compact(): Promise<void>
}

// Pluggable implementations
class RedisMemoryStore implements MemoryStore { }
class PostgresMemoryStore implements MemoryStore { }
class PineconeVectorStore implements MemoryStore { }
```

**Default Stack:**
- **Tier 1 (Working):** In-memory + optional Redis
- **Tier 2 (Short-term):** Redis with persistence
- **Tier 3 (Long-term):** PostgreSQL
- **Tier 4 (Semantic):** Pinecone/Weaviate/ChromaDB
- **Cache:** Redis

**Pros:**
- ✅ Abstraction allows swapping backends
- ✅ Can start simple (PostgreSQL only), add specialized stores later
- ✅ Users choose their infrastructure
- ✅ Easier testing (can use in-memory for tests)

**Cons:**
- ⚠️ More abstraction layers
- ⚠️ Need to maintain multiple adapters

---

## Recommendation

**Option 3: Unified Interface with Pluggable Backends**

**Rationale:**
1. **Flexibility**: Users can choose their infrastructure
2. **Progressive Enhancement**: Start simple, add specialized stores as needed
3. **Testing**: Easy to mock/test with in-memory implementations
4. **Future-proof**: Can add new storage backends without breaking changes

**Default Production Stack:**

| Tier | Technology | Purpose |
|------|------------|---------|
| Working | In-memory / Redis | Session state, fast access |
| Short-term | Redis (persistent) | Recent conversations, cache |
| Long-term | PostgreSQL | Persistent history, metadata |
| Semantic | Pinecone/Weaviate | Vector search, embeddings |
| Cache | Redis | Tool results, LLM responses |

**Alternative Stacks:**

**Simple/Low-cost:**
- PostgreSQL only (with pgvector)

**Serverless/Edge:**
- Redis: Upstash
- PostgreSQL: Supabase/Neon
- Vector: Pinecone serverless

**Enterprise:**
- Redis: AWS ElastiCache
- PostgreSQL: AWS RDS/Aurora
- Vector: Weaviate Enterprise

## Consequences

### Positive
- Flexible infrastructure choices
- Can optimize cost/performance per tier
- Easy to test and develop locally
- Can migrate backends without code changes

### Negative
- More complex initial setup
- Need to understand multiple technologies
- Potential consistency issues across stores

## Implementation Details

### Memory Interface Design:

```typescript
interface MemoryManager {
  // Working memory (session-scoped)
  working: MemoryStore
  
  // Short-term (24h default)
  shortTerm: MemoryStore
  
  // Long-term (persistent)
  longTerm: MemoryStore
  
  // Semantic (vector search)
  semantic: VectorStore
  
  // Compaction logic
  compact(): Promise<void>
}
```

### Compaction Strategy:

1. **Trigger**: When context approaches token limit
2. **Action**: Summarize older messages
3. **Storage**: Save summary to long-term memory
4. **Removal**: Clear from working memory
5. **Frequency**: Configurable (every N messages or time-based)

---

**Decision Status:** Proposed
