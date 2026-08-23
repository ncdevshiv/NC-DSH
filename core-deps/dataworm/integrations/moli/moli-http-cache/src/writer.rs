use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
};

use anyhow::Result;

use crate::{
    eviction::prune_cache_to_max_bytes,
    metadata::{HttpCacheEntryMetadata, read_published_body_file, write_metadata_file},
};

/// Streaming writer for one cache entry body.
#[derive(Debug)]
pub struct HttpCacheBodyWriter {
    pub(crate) entry_dir: PathBuf,
    pub(crate) root: PathBuf,
    pub(crate) body_path: PathBuf,
    pub(crate) body_name: String,
    pub(crate) file: Option<File>,
    pub(crate) max_bytes: Option<u64>,
    pub(crate) body_bytes_written: u64,
    pub(crate) finished: bool,
}

impl HttpCacheBodyWriter {
    /// Appends body bytes to an unpublished body stream.
    pub fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        if self.body_write_would_exceed_store_limit(bytes.len()) {
            return Err(io::Error::other(
                "HTTP cache entry body exceeds configured cache size limit",
            ));
        }
        if let Some(file) = self.file.as_mut() {
            file.write_all(bytes)?;
        }
        self.body_bytes_written = self.body_bytes_written.saturating_add(bytes.len() as u64);
        Ok(())
    }

    fn body_write_would_exceed_store_limit(&self, next_bytes: usize) -> bool {
        self.max_bytes.is_some_and(|max_bytes| {
            self.body_bytes_written.saturating_add(next_bytes as u64) > max_bytes
        })
    }

    /// Publishes the body and metadata atomically enough for cache readers.
    ///
    /// The metadata file is renamed last. Readers only consider entries with a
    /// valid metadata file, so crashes during body writes leave an ignored
    /// unpublished body stream rather than a readable partial response.
    ///
    /// Body files use unique names and are written directly in the entry
    /// directory. Finishing a writer only publishes metadata that points to the
    /// completed body; this avoids a body rename on the commit path while still
    /// keeping partial bodies invisible to readers.
    ///
    /// Only the previously published body is removed after metadata commit. A
    /// broad directory sweep would be unsafe because another writer may have an
    /// unpublished unique body in progress for the same cache key.
    pub fn finish(mut self, metadata: HttpCacheEntryMetadata) -> Result<()> {
        if let Some(mut file) = self.file.take() {
            // HTTP cache contents are disposable. Flush userspace buffering so
            // metadata publication follows the body write, but do not make
            // network response completion wait for filesystem durability.
            file.flush()?;
        }

        let previous_body_name = read_published_body_file(&self.entry_dir);
        let metadata = metadata.with_body_file(self.body_name.clone());
        write_metadata_file(&self.entry_dir, &metadata)?;

        remove_replaced_body_file(
            &self.entry_dir,
            previous_body_name.as_deref(),
            &self.body_name,
        );
        if let Some(max_bytes) = self.max_bytes {
            prune_cache_to_max_bytes(&self.root, max_bytes, &self.entry_dir);
        }

        self.finished = true;
        Ok(())
    }
}

impl Drop for HttpCacheBodyWriter {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.file.take();
            let _ = fs::remove_file(&self.body_path);
            let _ = fs::remove_dir(&self.entry_dir);
        }
    }
}

fn remove_replaced_body_file(
    entry_dir: &Path,
    replaced_body_name: Option<&str>,
    live_body_name: &str,
) {
    let Some(replaced_body_name) = replaced_body_name else {
        return;
    };
    if replaced_body_name != live_body_name {
        let _ = fs::remove_file(entry_dir.join(replaced_body_name));
    }
}
