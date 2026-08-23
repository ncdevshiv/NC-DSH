//! Storage backends: key-value, document, and vector store interfaces with
//! a real SQLite adapter (spec §11 memory storage, §4.2 persistence).
//!
//! All interfaces are synchronous-core with `async` wrappers; the SQLite
//! adapter uses a connection behind a mutex (short, non-blocking ops) and is
//! fully tested against real database files.

use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::Connection;

use ai_errors::{AiError, StorageError};

/// A key-value store (arbitrary JSON values keyed by string).
#[async_trait::async_trait]
pub trait KeyValueStore: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<serde_json::Value>, AiError>;
    async fn set(&self, key: &str, value: serde_json::Value) -> Result<(), AiError>;
    async fn delete(&self, key: &str) -> Result<(), AiError>;
    async fn keys(&self) -> Result<Vec<String>, AiError>;
}

/// A document store (named documents with metadata).
#[async_trait::async_trait]
pub trait DocumentStore: Send + Sync {
    async fn put(
        &self,
        id: &str,
        content: &str,
        metadata: serde_json::Value,
    ) -> Result<(), AiError>;
    async fn get(&self, id: &str) -> Result<Option<(String, serde_json::Value)>, AiError>;
    async fn delete(&self, id: &str) -> Result<(), AiError>;
    async fn list(&self) -> Result<Vec<String>, AiError>;
}

/// A vector entry.
#[derive(Debug, Clone)]
pub struct VectorEntry {
    pub id: String,
    pub vector: Vec<f32>,
    pub payload: serde_json::Value,
}

/// A vector store with brute-force similarity search.
#[async_trait::async_trait]
pub trait VectorStore: Send + Sync {
    async fn upsert(&self, entry: VectorEntry) -> Result<(), AiError>;
    async fn search(&self, query: &[f32], top_k: usize)
    -> Result<Vec<(VectorEntry, f32)>, AiError>;
    async fn delete(&self, id: &str) -> Result<(), AiError>;
    async fn len(&self) -> Result<usize, AiError>;

    /// True when the store holds no entries.
    async fn is_empty(&self) -> Result<bool, AiError> {
        Ok(self.len().await? == 0)
    }
}

fn storage_error(backend: &str, message: impl Into<String>) -> AiError {
    AiError::Storage(StorageError::new(backend, message))
}

/// SQLite-backed key-value + document store.
///
/// One database file with two tables (`kv`, `documents`). Writes are
/// transactional per operation.
pub struct SqliteStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteStore {
    /// Opens (or creates) the database at `path`.
    pub fn open(path: &Path) -> Result<Self, AiError> {
        let conn = Connection::open(path).map_err(|e| {
            storage_error(
                "sqlite",
                format!("failed to open `{}`: {e}", path.display()),
            )
        })?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS kv (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                metadata TEXT NOT NULL
            );",
        )
        .map_err(|e| storage_error("sqlite", format!("schema init failed: {e}")))?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Opens an in-memory database (tests, ephemeral use).
    pub fn in_memory() -> Result<Self, AiError> {
        Self::open(Path::new(":memory:"))
    }
}

fn row_to_value(row: &rusqlite::Row<'_>) -> rusqlite::Result<serde_json::Value> {
    let text: String = row.get(0)?;
    serde_json::from_str(&text).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}

#[async_trait::async_trait]
impl KeyValueStore for SqliteStore {
    async fn get(&self, key: &str) -> Result<Option<serde_json::Value>, AiError> {
        let conn = self.conn.lock();

        conn.query_row("SELECT value FROM kv WHERE key = ?1", [key], row_to_value)
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
            .map_err(|e| storage_error("sqlite", format!("kv get failed: {e}")))
    }

    async fn set(&self, key: &str, value: serde_json::Value) -> Result<(), AiError> {
        let conn = self.conn.lock();
        let text = serde_json::to_string(&value)
            .map_err(|e| storage_error("sqlite", format!("value serialization failed: {e}")))?;
        conn.execute(
            "INSERT INTO kv (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![key, text],
        )
        .map_err(|e| storage_error("sqlite", format!("kv set failed: {e}")))?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), AiError> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM kv WHERE key = ?1", [key])
            .map_err(|e| storage_error("sqlite", format!("kv delete failed: {e}")))?;
        Ok(())
    }

    async fn keys(&self) -> Result<Vec<String>, AiError> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare("SELECT key FROM kv ORDER BY key")
            .map_err(|e| storage_error("sqlite", format!("kv keys failed: {e}")))?;
        let keys = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| storage_error("sqlite", format!("kv keys failed: {e}")))?
            .collect::<Result<Vec<String>, _>>()
            .map_err(|e| storage_error("sqlite", format!("kv keys failed: {e}")))?;
        Ok(keys)
    }
}

#[async_trait::async_trait]
impl DocumentStore for SqliteStore {
    async fn put(
        &self,
        id: &str,
        content: &str,
        metadata: serde_json::Value,
    ) -> Result<(), AiError> {
        let conn = self.conn.lock();
        let metadata_text = serde_json::to_string(&metadata)
            .map_err(|e| storage_error("sqlite", format!("metadata serialization failed: {e}")))?;
        conn.execute(
            "INSERT INTO documents (id, content, metadata) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET content = excluded.content, metadata = excluded.metadata",
            rusqlite::params![id, content, metadata_text],
        )
        .map_err(|e| storage_error("sqlite", format!("document put failed: {e}")))?;
        Ok(())
    }

    async fn get(&self, id: &str) -> Result<Option<(String, serde_json::Value)>, AiError> {
        let conn = self.conn.lock();

        conn.query_row(
            "SELECT content, metadata FROM documents WHERE id = ?1",
            [id],
            |row| {
                let content: String = row.get(0)?;
                let metadata_text: String = row.get(1)?;
                let metadata: serde_json::Value =
                    serde_json::from_str(&metadata_text).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                Ok((content, metadata))
            },
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })
        .map_err(|e| storage_error("sqlite", format!("document get failed: {e}")))
    }

    async fn delete(&self, id: &str) -> Result<(), AiError> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM documents WHERE id = ?1", [id])
            .map_err(|e| storage_error("sqlite", format!("document delete failed: {e}")))?;
        Ok(())
    }

    async fn list(&self) -> Result<Vec<String>, AiError> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare("SELECT id FROM documents ORDER BY id")
            .map_err(|e| storage_error("sqlite", format!("document list failed: {e}")))?;
        let ids = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| storage_error("sqlite", format!("document list failed: {e}")))?
            .collect::<Result<Vec<String>, _>>()
            .map_err(|e| storage_error("sqlite", format!("document list failed: {e}")))?;
        Ok(ids)
    }
}

/// An in-process vector store with brute-force cosine search. Bounded by
/// `capacity` to prevent unbounded memory growth.
pub struct InMemoryVectorStore {
    entries: parking_lot::RwLock<Vec<VectorEntry>>,
    capacity: usize,
}

impl InMemoryVectorStore {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: parking_lot::RwLock::new(Vec::new()),
            capacity: capacity.max(1),
        }
    }
}

fn cosine(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        return None;
    }
    Some(dot / (na.sqrt() * nb.sqrt()))
}

#[async_trait::async_trait]
impl VectorStore for InMemoryVectorStore {
    async fn upsert(&self, entry: VectorEntry) -> Result<(), AiError> {
        let mut entries = self.entries.write();
        if let Some(existing) = entries.iter_mut().find(|e| e.id == entry.id) {
            *existing = entry;
        } else {
            if entries.len() >= self.capacity {
                entries.remove(0);
            }
            entries.push(entry);
        }
        Ok(())
    }

    async fn search(
        &self,
        query: &[f32],
        top_k: usize,
    ) -> Result<Vec<(VectorEntry, f32)>, AiError> {
        let entries = self.entries.read();
        let mut scored: Vec<(VectorEntry, f32)> = entries
            .iter()
            .filter_map(|e| cosine(query, &e.vector).map(|score| (e.clone(), score)))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);
        Ok(scored)
    }

    async fn delete(&self, id: &str) -> Result<(), AiError> {
        self.entries.write().retain(|e| e.id != id);
        Ok(())
    }

    async fn len(&self) -> Result<usize, AiError> {
        Ok(self.entries.read().len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> SqliteStore {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let dir = std::env::temp_dir();
        // Unique per call: parallel #[tokio::test]s share one process (and
        // therefore pid), so the old single pid-keyed path made every test
        // race on the same file; combined with deleting it while open this
        // surfaced as "attempt to write a readonly database" on loaded
        // Linux runners.
        let path = dir.join(format!(
            "ai-sdk-test-{}-{}.sqlite",
            std::process::id(),
            NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::remove_file(&path).ok(); // stale leftover of a crashed run
        SqliteStore::open(&path).expect("sqlite opens")
    }

    #[tokio::test]
    async fn kv_roundtrip_and_delete() {
        let store = temp_db();
        KeyValueStore::set(&store, "a", serde_json::json!({"n": 1}))
            .await
            .unwrap();
        assert_eq!(
            KeyValueStore::get(&store, "a").await.unwrap(),
            Some(serde_json::json!({"n": 1}))
        );
        assert!(
            KeyValueStore::get(&store, "missing")
                .await
                .unwrap()
                .is_none()
        );
        KeyValueStore::delete(&store, "a").await.unwrap();
        assert!(KeyValueStore::get(&store, "a").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn kv_upsert_overwrites() {
        let store = temp_db();
        KeyValueStore::set(&store, "k", serde_json::json!(1))
            .await
            .unwrap();
        KeyValueStore::set(&store, "k", serde_json::json!(2))
            .await
            .unwrap();
        assert_eq!(
            KeyValueStore::get(&store, "k").await.unwrap(),
            Some(serde_json::json!(2))
        );
        assert_eq!(store.keys().await.unwrap(), vec!["k".to_string()]);
    }

    #[tokio::test]
    async fn documents_roundtrip_with_metadata() {
        let store = temp_db();
        store
            .put("d1", "hello world", serde_json::json!({"tag": "greeting"}))
            .await
            .unwrap();
        let (content, metadata) = DocumentStore::get(&store, "d1").await.unwrap().unwrap();
        assert_eq!(content, "hello world");
        assert_eq!(metadata["tag"], "greeting");
        assert_eq!(store.list().await.unwrap(), vec!["d1".to_string()]);
        DocumentStore::delete(&store, "d1").await.unwrap();
        assert!(DocumentStore::get(&store, "d1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn vector_store_ranks_by_similarity() {
        let store = InMemoryVectorStore::new(100);
        store
            .upsert(VectorEntry {
                id: "cat".into(),
                vector: vec![1.0, 0.0],
                payload: serde_json::json!("feline"),
            })
            .await
            .unwrap();
        store
            .upsert(VectorEntry {
                id: "dog".into(),
                vector: vec![0.0, 1.0],
                payload: serde_json::json!("canine"),
            })
            .await
            .unwrap();

        let results = store.search(&[0.9, 0.1], 2).await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0.id, "cat", "closest vector first");
        assert!(results[0].1 > results[1].1);
        assert_eq!(store.len().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn vector_store_bounds_capacity() {
        let store = InMemoryVectorStore::new(2);
        for i in 0..4 {
            store
                .upsert(VectorEntry {
                    id: format!("v{i}"),
                    vector: vec![i as f32, 0.0],
                    payload: serde_json::Value::Null,
                })
                .await
                .unwrap();
        }
        assert_eq!(store.len().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn in_memory_sqlite_works() {
        let store = SqliteStore::in_memory().unwrap();
        KeyValueStore::set(&store, "x", serde_json::json!("y"))
            .await
            .unwrap();
        assert_eq!(
            KeyValueStore::get(&store, "x").await.unwrap(),
            Some(serde_json::json!("y"))
        );
    }
}
