//! Shared `readwrite-unsafe` byte, cursor, quota, and concurrency scenarios.
//!
//! The cases transpose Chromium's regular-file delegate and vendored
//! `FileSystemSyncAccessHandle` WPT semantics onto multiple compatible handles.

pub mod common;

use std::{
    sync::{Arc, Barrier},
    thread,
};

use common::memory_fixture;
use moli_opfs::{EntryKind, OpfsError, SyncAccessMode};

#[test]
fn unsafe_handles_share_read_write_size_and_sparse_file_semantics() {
    let (opfs, bucket, root) = memory_fixture("unsafe-read-write");
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    let first = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
        .unwrap();
    let second = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
        .unwrap();

    assert!(opfs.sync_read(second, 24, Some(0)).unwrap().is_empty());
    assert_eq!(
        opfs.sync_write(first, b"Hello Storage Foundation", Some(0), None)
            .unwrap(),
        24
    );
    assert_eq!(opfs.sync_size(second).unwrap(), 24);
    assert_eq!(opfs.sync_read(second, 7, Some(6)).unwrap(), b"Storage");

    assert_eq!(
        opfs.sync_write(second, b"Longer Text", Some(0), None)
            .unwrap(),
        11
    );
    assert_eq!(
        opfs.sync_read(first, 24, Some(0)).unwrap(),
        b"Longer Textge Foundation"
    );
    assert_eq!(opfs.sync_write(first, b"foobar", Some(0), None).unwrap(), 6);
    assert_eq!(
        opfs.sync_read(second, 24, Some(0)).unwrap(),
        b"foobar Textge Foundation"
    );

    opfs.sync_truncate(first, 0, None).unwrap();
    assert_eq!(opfs.sync_write(second, &[17], Some(5), None).unwrap(), 1);
    assert_eq!(opfs.sync_size(first).unwrap(), 6);
    assert_eq!(
        opfs.sync_read(first, 8, Some(0)).unwrap(),
        [0, 0, 0, 0, 0, 17]
    );

    opfs.close_sync(first, None).unwrap();
    opfs.close_sync(second, None).unwrap();
}

#[test]
fn unsafe_handles_keep_independent_cursors_over_shared_bytes() {
    let (opfs, bucket, root) = memory_fixture("unsafe-independent-cursors");
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    opfs.write_file(&bucket, &file, b"abcdef", None).unwrap();
    let first = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
        .unwrap();
    let second = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
        .unwrap();

    assert_eq!(opfs.sync_read(first, 2, Some(1)).unwrap(), b"bc");
    assert_eq!(opfs.sync_read(second, 1, Some(5)).unwrap(), b"f");
    assert_eq!(opfs.sync_write(first, b"X", None, None).unwrap(), 1);
    assert_eq!(opfs.sync_write(second, b"Y", None, None).unwrap(), 1);
    assert_eq!(opfs.sync_read(first, 16, Some(0)).unwrap(), b"abcXefY");

    opfs.close_sync(first, None).unwrap();
    opfs.close_sync(second, None).unwrap();
}

#[test]
fn unsafe_peer_truncate_changes_length_without_retargeting_other_cursor() {
    let (opfs, bucket, root) = memory_fixture("unsafe-peer-truncate");
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    opfs.write_file(&bucket, &file, b"abcdef", None).unwrap();
    let truncating = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
        .unwrap();
    let peer = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
        .unwrap();

    assert_eq!(opfs.sync_read(truncating, 2, Some(4)).unwrap(), b"ef");
    assert_eq!(opfs.sync_read(peer, 1, Some(5)).unwrap(), b"f");
    opfs.sync_truncate(truncating, 2, None).unwrap();
    assert_eq!(opfs.sync_write(truncating, b"X", None, None).unwrap(), 1);
    assert_eq!(opfs.sync_write(peer, b"Y", None, None).unwrap(), 1);
    assert_eq!(opfs.sync_size(truncating).unwrap(), 7);
    assert_eq!(opfs.sync_read(peer, 16, Some(0)).unwrap(), b"abX\0\0\0Y");

    opfs.close_sync(truncating, None).unwrap();
    opfs.close_sync(peer, None).unwrap();
}

#[test]
fn unsafe_empty_write_repositions_only_its_cursor_without_growing_the_file() {
    let (opfs, bucket, root) = memory_fixture("unsafe-empty-write");
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    opfs.write_file(&bucket, &file, b"abc", None).unwrap();
    let first = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
        .unwrap();
    let second = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
        .unwrap();

    assert_eq!(opfs.sync_write(first, b"", Some(5), None).unwrap(), 0);
    assert_eq!(opfs.sync_size(second).unwrap(), 3);
    assert_eq!(opfs.sync_write(second, b"Y", None, None).unwrap(), 1);
    assert_eq!(opfs.sync_write(first, b"X", None, None).unwrap(), 1);
    assert_eq!(opfs.sync_read(second, 16, Some(0)).unwrap(), b"Ybc\0\0X");

    opfs.close_sync(first, None).unwrap();
    opfs.close_sync(second, None).unwrap();
}

#[test]
fn unsafe_quota_failure_preserves_shared_bytes_size_and_failing_cursor() {
    let (opfs, bucket, root) = memory_fixture("unsafe-quota-atomicity");
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    opfs.write_file(&bucket, &file, b"abc", None).unwrap();
    let quota = opfs.usage(&bucket).unwrap();
    let first = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
        .unwrap();
    let second = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
        .unwrap();

    assert_eq!(opfs.sync_read(first, 1, Some(1)).unwrap(), b"b");
    assert!(matches!(
        opfs.sync_write(first, b"grow", Some(3), Some(quota)),
        Err(OpfsError::QuotaExceeded { .. })
    ));
    assert!(matches!(
        opfs.sync_truncate(first, 4, Some(quota)),
        Err(OpfsError::QuotaExceeded { .. })
    ));
    assert_eq!(opfs.sync_size(second).unwrap(), 3);
    assert_eq!(opfs.sync_read(second, 3, Some(0)).unwrap(), b"abc");

    assert_eq!(
        opfs.sync_write(first, b"Z", None, Some(quota)).unwrap(),
        1,
        "failed explicit operations must leave the cursor at offset 2"
    );
    assert_eq!(opfs.sync_read(second, 3, Some(0)).unwrap(), b"abZ");

    opfs.close_sync(first, Some(quota)).unwrap();
    opfs.close_sync(second, Some(quota)).unwrap();
}

#[test]
fn closing_one_unsafe_handle_keeps_the_peer_live_and_the_path_locked() {
    let (opfs, bucket, root) = memory_fixture("unsafe-partial-close");
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    let first = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
        .unwrap();
    let second = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
        .unwrap();

    assert_eq!(opfs.sync_write(first, b"A", Some(0), None).unwrap(), 1);
    opfs.close_sync(first, None).unwrap();
    assert!(matches!(
        opfs.sync_read(first, 1, Some(0)),
        Err(OpfsError::InvalidState)
    ));
    assert_eq!(opfs.sync_write(second, b"B", Some(1), None).unwrap(), 1);
    assert!(matches!(
        opfs.move_entry(&bucket, &file, EntryKind::File, &root, "moved", None,),
        Err(OpfsError::NoModificationAllowed(_))
    ));

    opfs.close_sync(second, None).unwrap();
    let moved = opfs
        .move_entry(&bucket, &file, EntryKind::File, &root, "moved", None)
        .unwrap();
    assert_eq!(opfs.read_file(&bucket, &moved).unwrap().bytes, b"AB");
}

#[test]
fn concurrent_unsafe_handles_merge_disjoint_writes_into_one_file() {
    const HANDLE_COUNT: usize = 8;
    const BYTES_PER_HANDLE: usize = 4;

    let (opfs, bucket, root) = memory_fixture("unsafe-concurrent-writes");
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    let opened = Arc::new(Barrier::new(HANDLE_COUNT));
    let written = Arc::new(Barrier::new(HANDLE_COUNT));
    let threads = (0..HANDLE_COUNT)
        .map(|index| {
            let opfs = opfs.clone();
            let bucket = bucket.clone();
            let file = file.clone();
            let opened = opened.clone();
            let written = written.clone();
            thread::spawn(move || {
                let handle = opfs
                    .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
                    .unwrap();
                opened.wait();
                let bytes = vec![b'A' + u8::try_from(index).unwrap(); BYTES_PER_HANDLE];
                assert_eq!(
                    opfs.sync_write(
                        handle,
                        &bytes,
                        Some(u64::try_from(index * BYTES_PER_HANDLE).unwrap()),
                        None,
                    )
                    .unwrap(),
                    BYTES_PER_HANDLE
                );
                written.wait();
                opfs.close_sync(handle, None).unwrap();
            })
        })
        .collect::<Vec<_>>();
    for thread in threads {
        thread.join().unwrap();
    }

    let expected = (0..HANDLE_COUNT)
        .flat_map(|index| {
            std::iter::repeat_n(b'A' + u8::try_from(index).unwrap(), BYTES_PER_HANDLE)
        })
        .collect::<Vec<_>>();
    assert_eq!(opfs.read_file(&bucket, &file).unwrap().bytes, expected);
}

#[test]
fn sync_operations_reject_offsets_beyond_the_signed_file_limit() {
    let (opfs, bucket, root) = memory_fixture("sync-offset-limit");
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    opfs.write_file(&bucket, &file, b"abc", None).unwrap();
    let handle = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
        .unwrap();
    let too_large = (i64::MAX as u64) + 1;

    for result in [
        opfs.sync_read(handle, 0, Some(too_large)).map(|_| ()),
        opfs.sync_write(handle, b"", Some(too_large), None)
            .map(|_| ()),
        opfs.sync_truncate(handle, too_large, None),
    ] {
        assert!(matches!(result, Err(OpfsError::InvalidModification(_))));
    }
    assert_eq!(
        opfs.sync_write(handle, b"", Some(i64::MAX as u64), None)
            .unwrap(),
        0
    );
    for _ in 0..2 {
        assert!(matches!(
            opfs.sync_write(handle, b"Z", None, None),
            Err(OpfsError::QuotaExceeded { .. })
        ));
    }
    assert_eq!(opfs.sync_size(handle).unwrap(), 3);
    assert_eq!(opfs.sync_write(handle, b"", Some(0), None).unwrap(), 0);
    assert_eq!(opfs.sync_write(handle, b"Z", None, None).unwrap(), 1);
    opfs.close_sync(handle, None).unwrap();
    assert_eq!(opfs.read_file(&bucket, &file).unwrap().bytes, b"Zbc");
}
