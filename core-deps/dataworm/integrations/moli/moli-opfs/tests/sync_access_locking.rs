//! Sync access handle lock, cursor, and capacity regression scenarios.
//!
//! Source baseline: Chromium `a03603fe9af6`, primarily
//! `file_system_access_file_handle_impl_unittest.cc` and Blink's regular-file
//! delegate used by `FileSystemSyncAccessHandle`.

pub mod common;

use common::memory_fixture;
use moli_opfs::{
    EntryKind, Opfs, OpfsBucketKey, OpfsError, OpfsPath, SyncAccessMode, WritableMode,
};

#[test]
fn every_sync_access_lock_mode_has_the_expected_compatibility_matrix() {
    // Chromium: OpenAccessHandleLockModes_Readwrite,
    // OpenAccessHandleLockModes_ReadOnly, and
    // OpenAccessHandleLockModes_ReadwriteUnsafe.
    let (opfs, bucket, root) = memory_fixture("sync-lock-matrix");
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();

    let readwrite = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::Readwrite)
        .unwrap();
    for mode in [
        SyncAccessMode::Readwrite,
        SyncAccessMode::ReadOnly,
        SyncAccessMode::ReadwriteUnsafe,
    ] {
        assert!(matches!(
            opfs.create_sync_access_handle(&bucket, &file, mode),
            Err(OpfsError::NoModificationAllowed(_))
        ));
    }
    opfs.close_sync(readwrite, None).unwrap();

    let read_only = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadOnly)
        .unwrap();
    let second_read_only = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadOnly)
        .unwrap();
    for mode in [SyncAccessMode::Readwrite, SyncAccessMode::ReadwriteUnsafe] {
        assert!(matches!(
            opfs.create_sync_access_handle(&bucket, &file, mode),
            Err(OpfsError::NoModificationAllowed(_))
        ));
    }
    opfs.close_sync(read_only, None).unwrap();
    opfs.close_sync(second_read_only, None).unwrap();

    let unsafe_handle = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
        .unwrap();
    let second_unsafe_handle = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
        .unwrap();
    for mode in [SyncAccessMode::Readwrite, SyncAccessMode::ReadOnly] {
        assert!(matches!(
            opfs.create_sync_access_handle(&bucket, &file, mode),
            Err(OpfsError::NoModificationAllowed(_))
        ));
    }
    opfs.close_sync(unsafe_handle, None).unwrap();
    opfs.close_sync(second_unsafe_handle, None).unwrap();
}

#[test]
fn writable_and_sync_access_locks_conflict_only_on_the_same_file() {
    // Chromium: SiloedMode/ExclusiveMode combined with the access-handle lock
    // mode tests.
    let (opfs, bucket, root) = memory_fixture("cross-primitive-locks");
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    let sibling = opfs.get_file(&bucket, &root, "sibling", true).unwrap();
    let writer = opfs
        .create_writable(&bucket, &file, false, WritableMode::Siloed)
        .unwrap();

    for mode in [
        SyncAccessMode::Readwrite,
        SyncAccessMode::ReadOnly,
        SyncAccessMode::ReadwriteUnsafe,
    ] {
        assert!(matches!(
            opfs.create_sync_access_handle(&bucket, &file, mode),
            Err(OpfsError::NoModificationAllowed(_))
        ));
    }
    let sibling_sync = opfs
        .create_sync_access_handle(&bucket, &sibling, SyncAccessMode::Readwrite)
        .unwrap();
    opfs.close_sync(sibling_sync, None).unwrap();
    opfs.abort_writable(writer).unwrap();

    let read_only = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadOnly)
        .unwrap();
    for mode in [WritableMode::Siloed, WritableMode::Exclusive] {
        assert!(matches!(
            opfs.create_writable(&bucket, &file, false, mode),
            Err(OpfsError::NoModificationAllowed(_))
        ));
    }
    opfs.close_sync(read_only, None).unwrap();
}

#[test]
fn explicit_eof_read_moves_the_cursor_used_by_the_next_write() {
    // Chromium's FileSystemSyncAccessHandle read/write implementation updates
    // the cursor to `offset + bytes_processed`, including a zero-byte EOF read.
    let (opfs, bucket, root) = memory_fixture("sync-cursor");
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    opfs.write_file(&bucket, &file, b"abcdef", None).unwrap();
    let handle = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::Readwrite)
        .unwrap();

    assert_eq!(opfs.sync_read(handle, 2, Some(2)).unwrap(), b"cd");
    assert_eq!(opfs.sync_read(handle, 10, None).unwrap(), b"ef");
    assert!(opfs.sync_read(handle, 4, Some(8)).unwrap().is_empty());
    assert_eq!(opfs.sync_write(handle, b"Z", None, None).unwrap(), 1);
    opfs.flush_sync(handle, None).unwrap();
    opfs.close_sync(handle, None).unwrap();

    assert_eq!(
        opfs.read_file(&bucket, &file).unwrap().bytes,
        b"abcdef\0\0Z"
    );
}

#[test]
fn sync_capacity_failure_does_not_partially_modify_the_session() {
    // Chromium's regular-file delegate requests capacity for the complete
    // extension before issuing the host write.
    let (opfs, bucket, root) = memory_fixture("sync-capacity");
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    opfs.write_file(&bucket, &file, b"abc", None).unwrap();
    let quota = opfs.usage(&bucket).unwrap();
    let handle = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::Readwrite)
        .unwrap();

    assert!(matches!(
        opfs.sync_write(handle, b"grow", Some(3), Some(quota)),
        Err(OpfsError::QuotaExceeded { .. })
    ));
    assert_eq!(opfs.sync_size(handle).unwrap(), 3);
    assert_eq!(opfs.sync_read(handle, 3, Some(0)).unwrap(), b"abc");

    assert_eq!(
        opfs.sync_write(handle, b"Z", Some(1), Some(quota)).unwrap(),
        1
    );
    opfs.flush_sync(handle, Some(quota)).unwrap();
    opfs.close_sync(handle, Some(quota)).unwrap();
    assert_eq!(opfs.read_file(&bucket, &file).unwrap().bytes, b"aZc");
}

#[test]
fn failed_sync_open_releases_its_provisional_path_lock() {
    // Chromium: OpenAccessHandle failure paths must not retain the lock taken
    // before the backing file is opened.
    let (opfs, bucket, root) = memory_fixture("sync-open-failure");
    let directory = opfs
        .get_directory(&bucket, &root, "directory", true)
        .unwrap();
    assert!(matches!(
        opfs.create_sync_access_handle(&bucket, &directory, SyncAccessMode::Readwrite),
        Err(OpfsError::TypeMismatch {
            expected: EntryKind::File,
            actual: EntryKind::Directory,
            ..
        })
    ));
    opfs.remove_entry(&bucket, &directory, false).unwrap();

    let missing = root.child("missing").unwrap();
    assert!(matches!(
        opfs.create_sync_access_handle(&bucket, &missing, SyncAccessMode::Readwrite),
        Err(OpfsError::NotFound(_))
    ));
    opfs.get_file(&bucket, &root, "missing", true).unwrap();
}

#[test]
fn readwrite_unsafe_handles_observe_the_same_live_file() {
    let (opfs, bucket, root) = memory_fixture("unsafe-shared-backing");
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    opfs.write_file(&bucket, &file, b"abcd", None).unwrap();
    let old_snapshot = opfs.read_file(&bucket, &file).unwrap();
    let first = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
        .unwrap();
    let second = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
        .unwrap();

    assert_eq!(opfs.sync_write(first, b"X", Some(0), None).unwrap(), 1);
    assert_eq!(opfs.sync_read(second, 4, Some(0)).unwrap(), b"Xbcd");
    assert_eq!(opfs.sync_write(second, b"Y", Some(3), None).unwrap(), 1);
    assert_eq!(opfs.read_file(&bucket, &file).unwrap().bytes, b"XbcY");
    assert!(matches!(
        opfs.validate_file_snapshot(&bucket, &file, old_snapshot.identity),
        Err(OpfsError::NotFound(_))
    ));

    opfs.sync_truncate(first, 6, None).unwrap();
    assert_eq!(opfs.sync_size(second).unwrap(), 6);
    assert_eq!(opfs.sync_read(second, 6, Some(0)).unwrap(), b"XbcY\0\0");
    opfs.close_sync(first, None).unwrap();

    assert_eq!(opfs.sync_write(second, b"Z", Some(5), None).unwrap(), 1);
    opfs.close_sync(second, None).unwrap();
    assert_eq!(opfs.read_file(&bucket, &file).unwrap().bytes, b"XbcY\0Z");
}

#[test]
fn identical_virtual_paths_in_different_buckets_do_not_share_locks_or_bytes() {
    // Chromium: FileSystemAccessLockManagerTest.SandboxedFilesDifferentBucket.
    let opfs = Opfs::in_memory();
    let first_bucket = OpfsBucketKey::new("test:unsafe:first-bucket").unwrap();
    let second_bucket = OpfsBucketKey::new("test:unsafe:second-bucket").unwrap();
    let root = OpfsPath::root();
    let first_file = opfs
        .get_file(&first_bucket, &root, "same-path", true)
        .unwrap();
    let second_file = opfs
        .get_file(&second_bucket, &root, "same-path", true)
        .unwrap();
    let first = opfs
        .create_sync_access_handle(&first_bucket, &first_file, SyncAccessMode::ReadwriteUnsafe)
        .unwrap();
    let second = opfs
        .create_sync_access_handle(
            &second_bucket,
            &second_file,
            SyncAccessMode::ReadwriteUnsafe,
        )
        .unwrap();

    opfs.sync_write(first, b"first", Some(0), None).unwrap();
    opfs.sync_write(second, b"second", Some(0), None).unwrap();
    assert_eq!(opfs.sync_read(first, 16, Some(0)).unwrap(), b"first");
    assert_eq!(opfs.sync_read(second, 16, Some(0)).unwrap(), b"second");

    opfs.close_sync(first, None).unwrap();
    opfs.close_sync(second, None).unwrap();
}

#[test]
fn unsafe_child_handle_blocks_ancestor_mutation_but_not_a_sibling() {
    // Chromium: FileSystemAccessLockManagerTest.AncestorLocksSandboxed.
    let (opfs, bucket, root) = memory_fixture("unsafe-ancestor-lock");
    let directory = opfs
        .get_directory(&bucket, &root, "directory", true)
        .unwrap();
    let file = opfs.get_file(&bucket, &directory, "file", true).unwrap();
    let sibling = opfs.get_file(&bucket, &root, "sibling", true).unwrap();
    let handle = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
        .unwrap();

    assert!(matches!(
        opfs.remove_entry(&bucket, &directory, true),
        Err(OpfsError::NoModificationAllowed(_))
    ));
    assert!(matches!(
        opfs.move_entry(
            &bucket,
            &directory,
            EntryKind::Directory,
            &root,
            "moved-directory",
            None,
        ),
        Err(OpfsError::NoModificationAllowed(_))
    ));
    let moved_sibling = opfs
        .move_entry(
            &bucket,
            &sibling,
            EntryKind::File,
            &root,
            "moved-sibling",
            None,
        )
        .unwrap();
    assert_eq!(moved_sibling, root.child("moved-sibling").unwrap());

    opfs.close_sync(handle, None).unwrap();
    opfs.remove_entry(&bucket, &directory, true).unwrap();
}

#[test]
fn different_files_allow_every_pair_of_sync_modes() {
    let (opfs, bucket, root) = memory_fixture("sync-modes-different-files");
    let first_file = opfs.get_file(&bucket, &root, "first", true).unwrap();
    let second_file = opfs.get_file(&bucket, &root, "second", true).unwrap();
    let modes = [
        SyncAccessMode::Readwrite,
        SyncAccessMode::ReadOnly,
        SyncAccessMode::ReadwriteUnsafe,
    ];

    for first_mode in modes {
        for second_mode in modes {
            let first = opfs
                .create_sync_access_handle(&bucket, &first_file, first_mode)
                .unwrap();
            let second = opfs
                .create_sync_access_handle(&bucket, &second_file, second_mode)
                .unwrap();
            opfs.close_sync(first, None).unwrap();
            opfs.close_sync(second, None).unwrap();
        }
    }
}

#[test]
fn exclusive_release_allows_every_mode_to_reacquire_the_file() {
    let (opfs, bucket, root) = memory_fixture("sync-exclusive-release");
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();

    for next_mode in [
        SyncAccessMode::Readwrite,
        SyncAccessMode::ReadOnly,
        SyncAccessMode::ReadwriteUnsafe,
    ] {
        let exclusive = opfs
            .create_sync_access_handle(&bucket, &file, SyncAccessMode::Readwrite)
            .unwrap();
        opfs.close_sync(exclusive, None).unwrap();
        let next = opfs
            .create_sync_access_handle(&bucket, &file, next_mode)
            .unwrap();
        opfs.close_sync(next, None).unwrap();
    }
}

#[test]
fn incompatible_mode_waits_for_the_last_shared_handle_to_close() {
    for shared_mode in [SyncAccessMode::ReadOnly, SyncAccessMode::ReadwriteUnsafe] {
        let (opfs, bucket, root) = memory_fixture(match shared_mode {
            SyncAccessMode::ReadOnly => "sync-read-only-last-close",
            SyncAccessMode::ReadwriteUnsafe => "sync-unsafe-last-close",
            SyncAccessMode::Readwrite => unreachable!(),
        });
        let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
        let first = opfs
            .create_sync_access_handle(&bucket, &file, shared_mode)
            .unwrap();
        let second = opfs
            .create_sync_access_handle(&bucket, &file, shared_mode)
            .unwrap();

        opfs.close_sync(first, None).unwrap();
        assert!(matches!(
            opfs.create_sync_access_handle(&bucket, &file, SyncAccessMode::Readwrite),
            Err(OpfsError::NoModificationAllowed(_))
        ));
        opfs.close_sync(second, None).unwrap();

        let exclusive = opfs
            .create_sync_access_handle(&bucket, &file, SyncAccessMode::Readwrite)
            .unwrap();
        opfs.close_sync(exclusive, None).unwrap();
    }
}

#[test]
fn same_named_descendant_still_overlaps_its_ancestor_lock() {
    let (opfs, bucket, root) = memory_fixture("sync-same-name-ancestor");
    let directory = opfs.get_directory(&bucket, &root, "same", true).unwrap();
    let file = opfs.get_file(&bucket, &directory, "same", true).unwrap();
    let handle = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadOnly)
        .unwrap();

    assert!(matches!(
        opfs.remove_entry(&bucket, &directory, true),
        Err(OpfsError::NoModificationAllowed(_))
    ));
    opfs.close_sync(handle, None).unwrap();
    opfs.remove_entry(&bucket, &directory, true).unwrap();
}
