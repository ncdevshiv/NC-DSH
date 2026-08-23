//! Shared fixtures for the standalone OPFS integration tests.
//!
//! The tests intentionally use the public `moli-opfs` API. They cover
//! the filesystem behavior behind the Web API, while secure-context exposure,
//! WebIDL conversion, Promise settlement, Worker-only interfaces, and
//! DOMException mapping remain renderer/WPT responsibilities.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use moli_opfs::{Opfs, OpfsBucketKey, OpfsPath};

static NEXT_TEMP_ROOT_ID: AtomicU64 = AtomicU64::new(1);

pub struct TempRoot(PathBuf);

impl TempRoot {
    pub fn new(name: &str) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let id = NEXT_TEMP_ROOT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "moli-opfs-test-{name}-{}-{timestamp}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn only_bucket_directory(&self) -> PathBuf {
        let entries = directory_entries(self.path());
        assert_eq!(entries.len(), 1, "expected exactly one disk bucket");
        assert!(entries[0].is_dir());
        entries[0].clone()
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn memory_fixture(name: &str) -> (Opfs, OpfsBucketKey, OpfsPath) {
    let opfs = Opfs::in_memory();
    let bucket = OpfsBucketKey::new(format!("test:{name}")).unwrap();
    let root = opfs.ensure_root(&bucket).unwrap();
    (opfs, bucket, root)
}

pub fn file_text(opfs: &Opfs, bucket: &OpfsBucketKey, path: &OpfsPath) -> String {
    String::from_utf8(opfs.read_file(bucket, path).unwrap().bytes).unwrap()
}

pub fn directory_entries(path: &Path) -> Vec<PathBuf> {
    let mut entries = fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    entries.sort();
    entries
}
