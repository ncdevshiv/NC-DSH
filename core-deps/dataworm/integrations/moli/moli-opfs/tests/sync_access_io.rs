//! Synchronous access handle direct-file IO and lifecycle scenarios.

pub mod common;

use common::memory_fixture;
use moli_opfs::{OpfsError, SyncAccessMode};

#[test]
fn size_tracks_overwrite_and_sparse_growth() {
    let (opfs, bucket, root) = memory_fixture("sync-size-growth");
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    let handle = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::Readwrite)
        .unwrap();
    let bytes = [96, 97, 98, 99];

    assert_eq!(opfs.sync_size(handle).unwrap(), 0);
    assert_eq!(opfs.sync_write(handle, &bytes, Some(0), None).unwrap(), 4);
    assert_eq!(opfs.sync_size(handle).unwrap(), 4);
    assert_eq!(opfs.sync_write(handle, &bytes, Some(3), None).unwrap(), 4);
    assert_eq!(opfs.sync_size(handle).unwrap(), 7);
    assert_eq!(opfs.sync_write(handle, &bytes, Some(10), None).unwrap(), 4);
    assert_eq!(opfs.sync_size(handle).unwrap(), 14);
    assert_eq!(
        opfs.sync_read(handle, 14, Some(0)).unwrap(),
        [96, 97, 98, 96, 97, 98, 99, 0, 0, 0, 96, 97, 98, 99]
    );

    opfs.close_sync(handle, None).unwrap();
}

#[test]
fn larger_and_smaller_overwrites_keep_direct_file_tail() {
    let (opfs, bucket, root) = memory_fixture("sync-overwrite-size");
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    let handle = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::Readwrite)
        .unwrap();

    opfs.sync_write(handle, b"Hello", Some(0), None).unwrap();
    opfs.sync_write(handle, b"Longer Text", Some(0), None)
        .unwrap();
    assert_eq!(opfs.sync_read(handle, 11, Some(0)).unwrap(), b"Longer Text");

    opfs.sync_write(handle, b"Hello World", Some(0), None)
        .unwrap();
    opfs.sync_write(handle, b"foobar", Some(0), None).unwrap();
    assert_eq!(opfs.sync_size(handle).unwrap(), 11);
    assert_eq!(opfs.sync_read(handle, 11, Some(0)).unwrap(), b"foobarWorld");

    opfs.close_sync(handle, None).unwrap();
}

#[test]
fn truncate_shrink_and_growth_zero_fill_and_update_the_owning_cursor() {
    let (opfs, bucket, root) = memory_fixture("sync-truncate-cursor");
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    let handle = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::Readwrite)
        .unwrap();

    opfs.sync_write(handle, &[96, 97, 98, 99], Some(0), None)
        .unwrap();
    opfs.sync_truncate(handle, 2, None).unwrap();
    assert!(opfs.sync_read(handle, 256, None).unwrap().is_empty());
    opfs.sync_write(handle, &[100, 101, 102, 103], None, None)
        .unwrap();
    assert_eq!(
        opfs.sync_read(handle, 256, Some(0)).unwrap(),
        [96, 97, 100, 101, 102, 103]
    );

    opfs.sync_truncate(handle, 10, None).unwrap();
    opfs.sync_write(handle, &[110, 111], None, None).unwrap();
    assert_eq!(
        opfs.sync_read(handle, 256, Some(0)).unwrap(),
        [96, 97, 100, 101, 102, 103, 110, 111, 0, 0]
    );

    opfs.close_sync(handle, None).unwrap();
}

#[test]
fn clean_and_dirty_flushes_preserve_the_current_snapshot_identity() {
    let (opfs, bucket, root) = memory_fixture("sync-flush-identity");
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    opfs.write_file(&bucket, &file, b"initial", None).unwrap();
    let initial = opfs.read_file(&bucket, &file).unwrap();
    let handle = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::Readwrite)
        .unwrap();

    assert_eq!(opfs.sync_size(handle).unwrap(), 7);
    opfs.flush_sync(handle, None).unwrap();
    opfs.validate_file_snapshot(&bucket, &file, initial.identity)
        .unwrap();

    opfs.sync_write(handle, b"X", Some(0), None).unwrap();
    let modified = opfs.read_file(&bucket, &file).unwrap();
    opfs.flush_sync(handle, None).unwrap();
    opfs.validate_file_snapshot(&bucket, &file, modified.identity)
        .unwrap();
    assert_eq!(opfs.sync_read(handle, 7, Some(0)).unwrap(), b"Xnitial");

    opfs.close_sync(handle, None).unwrap();
}

#[test]
fn closed_handle_rejects_every_backend_operation() {
    let (opfs, bucket, root) = memory_fixture("sync-closed-operations");
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    let handle = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::Readwrite)
        .unwrap();
    opfs.close_sync(handle, None).unwrap();

    for result in [
        opfs.sync_read(handle, 1, Some(0)).map(|_| ()),
        opfs.sync_write(handle, b"X", Some(0), None).map(|_| ()),
        opfs.sync_truncate(handle, 1, None),
        opfs.flush_sync(handle, None),
        opfs.sync_size(handle).map(|_| ()),
        opfs.close_sync(handle, None),
    ] {
        assert!(matches!(result, Err(OpfsError::InvalidState)));
    }
}
