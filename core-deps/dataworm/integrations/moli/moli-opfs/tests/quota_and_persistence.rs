//! Public bucket isolation, quota, clear, and disk reopen scenarios.

pub mod common;

use std::fs;

use common::{TempRoot, file_text, memory_fixture};
use moli_opfs::{Opfs, OpfsBucketKey, OpfsError, SyncAccessMode, WritableCommand, WritableMode};

#[test]
fn buckets_are_isolated_even_when_virtual_paths_match() {
    let opfs = Opfs::in_memory();
    let first_bucket = OpfsBucketKey::new("test:first-origin").unwrap();
    let second_bucket = OpfsBucketKey::new("test:second-origin").unwrap();
    let first_root = opfs.ensure_root(&first_bucket).unwrap();
    let second_root = opfs.ensure_root(&second_bucket).unwrap();
    let first_file = opfs
        .get_file(&first_bucket, &first_root, "settings.json", true)
        .unwrap();
    let second_file = opfs
        .get_file(&second_bucket, &second_root, "settings.json", true)
        .unwrap();

    opfs.write_file(&first_bucket, &first_file, b"first", None)
        .unwrap();
    opfs.write_file(&second_bucket, &second_file, b"second", None)
        .unwrap();

    assert_eq!(file_text(&opfs, &first_bucket, &first_file), "first");
    assert_eq!(file_text(&opfs, &second_bucket, &second_file), "second");
}

#[test]
fn quota_rejection_preserves_the_committed_file() {
    let (opfs, bucket, root) = memory_fixture("quota");
    let file = opfs.get_file(&bucket, &root, "limited.bin", true).unwrap();
    opfs.write_file(&bucket, &file, b"old", None).unwrap();
    let quota = opfs.usage(&bucket).unwrap();
    let writer = opfs
        .create_writable(&bucket, &file, false, WritableMode::Siloed)
        .unwrap();

    assert!(matches!(
        opfs.writable_command(
            writer,
            WritableCommand::Write {
                data: b"larger".to_vec(),
                position: None,
            },
            Some(quota),
        ),
        Err(OpfsError::QuotaExceeded { .. })
    ));
    opfs.abort_writable(writer).unwrap();
    assert_eq!(file_text(&opfs, &bucket, &file), "old");
    assert_eq!(opfs.usage(&bucket).unwrap(), quota);
}

#[test]
fn disk_backend_reopens_data_under_an_opaque_bucket_directory() {
    let temp = TempRoot::new("disk-reopen");
    let bucket = OpfsBucketKey::new(
        "storage-key:v1;origin=https://app.example;top-level-site=https://app.example",
    )
    .unwrap();
    let file;
    {
        let opfs = Opfs::on_disk(temp.path()).unwrap();
        let root = opfs.ensure_root(&bucket).unwrap();
        let directory = opfs.get_directory(&bucket, &root, "项目", true).unwrap();
        file = opfs
            .get_file(&bucket, &directory, "数据库.bin", true)
            .unwrap();
        opfs.write_file(&bucket, &file, b"persistent bytes", None)
            .unwrap();
    }

    let opfs = Opfs::on_disk(temp.path()).unwrap();
    assert_eq!(
        opfs.read_file(&bucket, &file).unwrap().bytes,
        b"persistent bytes"
    );
    let host_names = fs::read_dir(temp.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(host_names.iter().any(|name| name.len() == 64));
    assert!(host_names.iter().all(|name| !name.contains("app.example")));
}

#[test]
fn clearing_a_bucket_removes_entries_and_revokes_active_sessions() {
    let (opfs, bucket, root) = memory_fixture("clear");
    let sync_file = opfs.get_file(&bucket, &root, "sync.bin", true).unwrap();
    let writable_file = opfs.get_file(&bucket, &root, "writable.bin", true).unwrap();
    let sync = opfs
        .create_sync_access_handle(&bucket, &sync_file, SyncAccessMode::Readwrite)
        .unwrap();
    let writer = opfs
        .create_writable(&bucket, &writable_file, false, WritableMode::Exclusive)
        .unwrap();

    opfs.clear_bucket(&bucket).unwrap();

    assert!(opfs.read_directory(&bucket, &root).unwrap().is_empty());
    assert!(matches!(opfs.sync_size(sync), Err(OpfsError::InvalidState)));
    assert!(matches!(
        opfs.writable_command(writer, WritableCommand::Truncate(1), None),
        Err(OpfsError::InvalidState)
    ));
    assert_eq!(opfs.usage(&bucket).unwrap(), 0);
}
