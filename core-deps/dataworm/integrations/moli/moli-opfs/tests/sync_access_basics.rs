//! Public synchronous access handle backend scenarios.

pub mod common;

use common::{file_text, memory_fixture};
use moli_opfs::{OpfsError, SyncAccessMode, WritableMode};

#[test]
fn sync_access_supports_offsets_cursor_truncate_flush_and_close() {
    let (opfs, bucket, root) = memory_fixture("sync-walkthrough");
    let file = opfs.get_file(&bucket, &root, "database.bin", true).unwrap();
    let handle = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::Readwrite)
        .unwrap();

    assert_eq!(opfs.sync_write(handle, b"Hello ", None, None).unwrap(), 6);
    assert_eq!(opfs.sync_write(handle, b"World", None, None).unwrap(), 5);
    assert_eq!(opfs.sync_size(handle).unwrap(), 11);
    assert_eq!(
        opfs.sync_read(handle, 5, Some(0)).unwrap(),
        b"Hello".to_vec()
    );
    assert_eq!(
        opfs.sync_read(handle, 32, None).unwrap(),
        b" World".to_vec(),
        "an explicit read updates the cursor used by the next implicit read"
    );

    opfs.sync_truncate(handle, 7, None).unwrap();
    assert_eq!(opfs.sync_write(handle, b"!", None, None).unwrap(), 1);
    assert_eq!(opfs.sync_size(handle).unwrap(), 8);
    opfs.flush_sync(handle, None).unwrap();
    assert_eq!(file_text(&opfs, &bucket, &file), "Hello W!");

    opfs.close_sync(handle, None).unwrap();
    assert!(matches!(
        opfs.sync_size(handle),
        Err(OpfsError::InvalidState)
    ));
}

#[test]
fn sync_write_beyond_end_zero_fills_the_gap() {
    let (opfs, bucket, root) = memory_fixture("sync-zero-fill");
    let file = opfs.get_file(&bucket, &root, "pages.bin", true).unwrap();
    let handle = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::Readwrite)
        .unwrap();

    assert_eq!(opfs.sync_write(handle, b"Z", Some(3), None).unwrap(), 1);
    assert_eq!(opfs.sync_read(handle, 8, Some(0)).unwrap(), b"\0\0\0Z");
    opfs.flush_sync(handle, None).unwrap();
    opfs.close_sync(handle, None).unwrap();
    assert_eq!(opfs.read_file(&bucket, &file).unwrap().bytes, b"\0\0\0Z");
}

#[test]
fn readwrite_sync_access_is_exclusive_until_close() {
    let (opfs, bucket, root) = memory_fixture("sync-exclusive-lock");
    let file = opfs.get_file(&bucket, &root, "locked.bin", true).unwrap();
    let handle = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::Readwrite)
        .unwrap();

    assert!(matches!(
        opfs.create_sync_access_handle(&bucket, &file, SyncAccessMode::Readwrite),
        Err(OpfsError::NoModificationAllowed(_))
    ));
    assert!(matches!(
        opfs.create_writable(&bucket, &file, true, WritableMode::Siloed),
        Err(OpfsError::NoModificationAllowed(_))
    ));
    opfs.close_sync(handle, None).unwrap();

    let writer = opfs
        .create_writable(&bucket, &file, false, WritableMode::Exclusive)
        .unwrap();
    opfs.abort_writable(writer).unwrap();
}

#[test]
fn shared_sync_modes_enforce_mode_compatibility_and_write_permission() {
    let (opfs, bucket, root) = memory_fixture("sync-shared-modes");
    let file = opfs.get_file(&bucket, &root, "shared.bin", true).unwrap();
    opfs.write_file(&bucket, &file, b"readable", None).unwrap();

    let first = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadOnly)
        .unwrap();
    let second = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadOnly)
        .unwrap();
    assert_eq!(opfs.sync_read(first, 8, Some(0)).unwrap(), b"readable");
    assert!(matches!(
        opfs.sync_write(first, b"x", Some(0), None),
        Err(OpfsError::NoModificationAllowed(_))
    ));
    assert!(matches!(
        opfs.sync_truncate(first, 0, None),
        Err(OpfsError::NoModificationAllowed(_))
    ));
    assert!(matches!(
        opfs.flush_sync(first, None),
        Err(OpfsError::NoModificationAllowed(_))
    ));
    assert!(matches!(
        opfs.create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe),
        Err(OpfsError::NoModificationAllowed(_))
    ));
    opfs.close_sync(first, None).unwrap();
    opfs.close_sync(second, None).unwrap();

    let unsafe_first = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
        .unwrap();
    let unsafe_second = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
        .unwrap();
    opfs.close_sync(unsafe_first, None).unwrap();
    opfs.close_sync(unsafe_second, None).unwrap();
}
