use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn write_file_atomically(path: &Path, bytes: &[u8], label: &str) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {label} dir `{}`", parent.display()))?;
    }

    let tmp_path = unique_temp_path(path);
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);

    let mut file = options
        .open(&tmp_path)
        .with_context(|| format!("failed to create {label} `{}`", tmp_path.display()))?;
    let write_result = (|| -> Result<()> {
        file.write_all(bytes)
            .with_context(|| format!("failed to write {label} `{}`", tmp_path.display()))?;
        file.flush()
            .with_context(|| format!("failed to flush {label} `{}`", tmp_path.display()))?;
        Ok(())
    })();
    drop(file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&tmp_path);
        return Err(error);
    }

    if let Err(error) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(error).with_context(|| {
            format!(
                "failed to replace {label} `{}` with `{}`",
                path.display(),
                tmp_path.display()
            )
        });
    }
    Ok(())
}

fn unique_temp_path(path: &Path) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .map(OsString::from)
        .unwrap_or_else(|| OsString::from("cookie-cache-write"));
    let mut tmp_name = OsString::from(".");
    tmp_name.push(file_name);
    tmp_name.push(format!(".{}.{}.{}.tmp", std::process::id(), nonce, counter));
    path.with_file_name(tmp_name)
}
