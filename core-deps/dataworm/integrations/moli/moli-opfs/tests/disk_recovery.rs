//! Disk staging, recovery, and catalog-integrity regression scenarios.
//!
//! Source baseline: Chromium `a03603fe9af6`, primarily
//! `file_system_access_file_writer_impl_browsertest.cc`,
//! `obfuscated_file_util_unittest.cc`, and
//! `sandbox_directory_database_unittest.cc`.

pub mod common;

use std::{
    fs,
    sync::{Arc, Barrier},
    thread,
};

use common::{TempRoot, directory_entries, file_text};
use moli_opfs::{Opfs, OpfsBucketKey, OpfsError, SyncAccessMode, WritableCommand, WritableMode};
use serde_json::Value;

fn disk_bucket(name: &str) -> OpfsBucketKey {
    OpfsBucketKey::new(format!("test:disk:{name}")).unwrap()
}

#[test]
fn disk_writable_keeps_the_target_old_until_swap_promotion() {
    // Chromium: ContentsWrittenToSwapFileFirst and WriterDestroyedAfterClose.
    let temp = TempRoot::new("disk-swap-promotion");
    let opfs = Opfs::on_disk(temp.path()).unwrap();
    let bucket = disk_bucket("swap-promotion");
    let root = opfs.ensure_root(&bucket).unwrap();
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    opfs.write_file(&bucket, &file, b"old contents", None)
        .unwrap();
    let bucket_directory = temp.only_bucket_directory();
    let staging_directory = bucket_directory.join("staging");
    let writer = opfs
        .create_writable(&bucket, &file, false, WritableMode::Siloed)
        .unwrap();

    opfs.writable_command(
        writer,
        WritableCommand::Write {
            data: b"new contents".to_vec(),
            position: None,
        },
        None,
    )
    .unwrap();
    assert_eq!(file_text(&opfs, &bucket, &file), "old contents");
    assert_eq!(directory_entries(&staging_directory).len(), 1);

    opfs.close_writable(writer, None).unwrap();
    assert_eq!(file_text(&opfs, &bucket, &file), "new contents");
    assert!(directory_entries(&staging_directory).is_empty());
    assert_eq!(
        directory_entries(&bucket_directory.join("contents")).len(),
        1
    );

    drop(opfs);
    let reopened = Opfs::on_disk(temp.path()).unwrap();
    assert_eq!(file_text(&reopened, &bucket, &file), "new contents");
}

#[test]
fn restart_collects_orphan_staging_and_unreferenced_backing_files() {
    // Chromium's ObfuscatedFileUtil recovery rule prefers an intact catalog;
    // a loose backing file with no reference is safe to delete.
    let temp = TempRoot::new("disk-orphan-gc");
    let bucket = disk_bucket("orphan-gc");
    let file;
    let bucket_directory;
    {
        let opfs = Opfs::on_disk(temp.path()).unwrap();
        let root = opfs.ensure_root(&bucket).unwrap();
        file = opfs.get_file(&bucket, &root, "file", true).unwrap();
        opfs.write_file(&bucket, &file, b"kept", None).unwrap();
        bucket_directory = temp.only_bucket_directory();
    }

    let staging_directory = bucket_directory.join("staging");
    fs::write(staging_directory.join("writer-deadbeef.stage"), b"orphan").unwrap();
    let nested_staging = staging_directory.join("orphan-directory");
    fs::create_dir(&nested_staging).unwrap();
    fs::write(nested_staging.join("payload"), b"orphan").unwrap();
    let orphan_content = bucket_directory
        .join("contents")
        .join("ffffffffffffffff.bin");
    fs::write(&orphan_content, b"orphan").unwrap();

    let reopened = Opfs::on_disk(temp.path()).unwrap();
    assert_eq!(
        directory_entries(&staging_directory).len(),
        2,
        "configuring a reopened backend must defer recovery"
    );
    assert!(orphan_content.exists());
    assert_eq!(file_text(&reopened, &bucket, &file), "kept");
    assert!(directory_entries(&staging_directory).is_empty());
    assert!(!orphan_content.exists());
    assert_eq!(
        directory_entries(&bucket_directory.join("contents")).len(),
        1
    );
}

#[test]
fn referenced_backing_size_mismatch_rejects_bucket_reopen() {
    // Chromium: consistency checks distinguish referenced backing data from
    // harmless orphan files. Moli refuses to expose a catalog entry whose
    // declared size no longer matches its backing file.
    let temp = TempRoot::new("disk-backing-size");
    let bucket = disk_bucket("backing-size");
    let bucket_directory;
    {
        let opfs = Opfs::on_disk(temp.path()).unwrap();
        let root = opfs.ensure_root(&bucket).unwrap();
        let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
        opfs.write_file(&bucket, &file, b"expected size", None)
            .unwrap();
        bucket_directory = temp.only_bucket_directory();
    }

    let contents = directory_entries(&bucket_directory.join("contents"));
    assert_eq!(contents.len(), 1);
    fs::write(&contents[0], b"x").unwrap();

    let reopened = Opfs::on_disk(temp.path()).unwrap();
    assert!(matches!(
        reopened.ensure_root(&bucket),
        Err(OpfsError::CorruptCatalog(_))
    ));
}

#[test]
fn catalog_parent_cycle_is_rejected_before_namespace_exposure() {
    // Chromium: TestConsistencyCheck_DirectoryLoop.
    let temp = TempRoot::new("disk-parent-cycle");
    let bucket = disk_bucket("parent-cycle");
    let bucket_directory;
    {
        let opfs = Opfs::on_disk(temp.path()).unwrap();
        let root = opfs.ensure_root(&bucket).unwrap();
        let parent = opfs.get_directory(&bucket, &root, "parent", true).unwrap();
        opfs.get_directory(&bucket, &parent, "child", true).unwrap();
        bucket_directory = temp.only_bucket_directory();
    }

    let catalog_path = bucket_directory.join("catalog.json");
    let mut catalog: Value = serde_json::from_slice(&fs::read(&catalog_path).unwrap()).unwrap();
    let entries = catalog["entries"].as_object_mut().unwrap();
    let parent_id = entries
        .iter()
        .find_map(|(id, entry)| (entry["name"] == "parent").then_some(id.clone()))
        .unwrap();
    let child_id = entries
        .iter()
        .find_map(|(id, entry)| (entry["name"] == "child").then_some(id.clone()))
        .unwrap();
    entries.get_mut(&parent_id).unwrap()["parentId"] =
        Value::from(child_id.parse::<u64>().unwrap());
    fs::write(&catalog_path, serde_json::to_vec_pretty(&catalog).unwrap()).unwrap();

    let reopened = Opfs::on_disk(temp.path()).unwrap();
    assert!(matches!(
        reopened.ensure_root(&bucket),
        Err(OpfsError::CorruptCatalog(message)) if message.contains("parent cycle")
    ));
}

#[test]
fn restart_recovers_an_unclosed_live_sync_backing() {
    let temp = TempRoot::new("disk-sync-recovery");
    let bucket = disk_bucket("sync-recovery");
    let file;
    let old_identity;
    let bucket_directory;
    {
        let opfs = Opfs::on_disk(temp.path()).unwrap();
        let root = opfs.ensure_root(&bucket).unwrap();
        file = opfs.get_file(&bucket, &root, "file", true).unwrap();
        opfs.write_file(&bucket, &file, b"abcd", None).unwrap();
        old_identity = opfs.read_file(&bucket, &file).unwrap().identity;
        bucket_directory = temp.only_bucket_directory();
        let handle = opfs
            .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
            .unwrap();

        opfs.sync_write(handle, b"XY", Some(4), None).unwrap();
        assert_eq!(opfs.read_file(&bucket, &file).unwrap().bytes, b"abcdXY");
        assert_eq!(directory_entries(&bucket_directory.join("sync")).len(), 1);
        // Dropping the backend models a process exiting without close()/flush().
    }

    assert_eq!(directory_entries(&bucket_directory.join("sync")).len(), 1);
    let reopened = Opfs::on_disk(temp.path()).unwrap();
    assert!(reopened.ensure_root(&bucket).unwrap().is_root());
    assert_eq!(reopened.read_file(&bucket, &file).unwrap().bytes, b"abcdXY");
    assert!(matches!(
        reopened.validate_file_snapshot(&bucket, &file, old_identity),
        Err(OpfsError::NotFound(_))
    ));
    assert!(directory_entries(&bucket_directory.join("sync")).is_empty());

    drop(reopened);
    let reopened_again = Opfs::on_disk(temp.path()).unwrap();
    assert_eq!(
        reopened_again.read_file(&bucket, &file).unwrap().bytes,
        b"abcdXY"
    );
}

#[test]
fn clean_sync_close_persists_catalog_and_removes_recovery_marker() {
    let temp = TempRoot::new("disk-sync-clean-close");
    let bucket = disk_bucket("sync-clean-close");
    let file;
    let bucket_directory;
    {
        let opfs = Opfs::on_disk(temp.path()).unwrap();
        let root = opfs.ensure_root(&bucket).unwrap();
        file = opfs.get_file(&bucket, &root, "file", true).unwrap();
        bucket_directory = temp.only_bucket_directory();
        let handle = opfs
            .create_sync_access_handle(&bucket, &file, SyncAccessMode::Readwrite)
            .unwrap();
        opfs.sync_write(handle, b"durable", Some(0), None).unwrap();
        assert_eq!(directory_entries(&bucket_directory.join("sync")).len(), 1);
        opfs.close_sync(handle, None).unwrap();
        assert!(directory_entries(&bucket_directory.join("sync")).is_empty());
    }

    let reopened = Opfs::on_disk(temp.path()).unwrap();
    assert_eq!(
        reopened.read_file(&bucket, &file).unwrap().bytes,
        b"durable"
    );
    assert!(directory_entries(&bucket_directory.join("sync")).is_empty());
}

#[test]
fn unsafe_disk_handles_share_one_open_backing_until_last_close() {
    let temp = TempRoot::new("disk-sync-unsafe-shared");
    let bucket = disk_bucket("sync-unsafe-shared");
    let file;
    let bucket_directory;
    {
        let opfs = Opfs::on_disk(temp.path()).unwrap();
        let root = opfs.ensure_root(&bucket).unwrap();
        file = opfs.get_file(&bucket, &root, "file", true).unwrap();
        opfs.write_file(&bucket, &file, b"abcd", None).unwrap();
        bucket_directory = temp.only_bucket_directory();
        let first = opfs
            .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
            .unwrap();
        let second = opfs
            .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
            .unwrap();

        assert!(directory_entries(&bucket_directory.join("sync")).is_empty());
        opfs.sync_write(first, b"X", Some(0), None).unwrap();
        opfs.sync_write(second, b"Y", Some(3), None).unwrap();
        assert_eq!(directory_entries(&bucket_directory.join("sync")).len(), 1);
        assert_eq!(opfs.sync_read(first, 4, Some(0)).unwrap(), b"XbcY");
        assert_eq!(opfs.read_file(&bucket, &file).unwrap().bytes, b"XbcY");

        opfs.close_sync(first, None).unwrap();
        assert_eq!(directory_entries(&bucket_directory.join("sync")).len(), 1);
        opfs.flush_sync(second, None).unwrap();
        opfs.close_sync(second, None).unwrap();
        assert!(directory_entries(&bucket_directory.join("sync")).is_empty());
    }

    let reopened = Opfs::on_disk(temp.path()).unwrap();
    assert_eq!(reopened.read_file(&bucket, &file).unwrap().bytes, b"XbcY");
}

#[test]
fn opening_unsafe_handles_without_mutation_does_not_create_recovery_work() {
    let temp = TempRoot::new("disk-sync-clean-open");
    let bucket = disk_bucket("sync-clean-open");
    let file;
    let identity;
    let bucket_directory;
    {
        let opfs = Opfs::on_disk(temp.path()).unwrap();
        let root = opfs.ensure_root(&bucket).unwrap();
        file = opfs.get_file(&bucket, &root, "file", true).unwrap();
        opfs.write_file(&bucket, &file, b"unchanged", None).unwrap();
        identity = opfs.read_file(&bucket, &file).unwrap().identity;
        bucket_directory = temp.only_bucket_directory();

        let first = opfs
            .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
            .unwrap();
        let second = opfs
            .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
            .unwrap();
        assert!(directory_entries(&bucket_directory.join("sync")).is_empty());
        opfs.flush_sync(first, None).unwrap();
        assert!(directory_entries(&bucket_directory.join("sync")).is_empty());
        assert_eq!(opfs.sync_size(first).unwrap(), 9);
        assert_eq!(opfs.sync_read(second, 9, Some(0)).unwrap(), b"unchanged");
        // Drop without close to model a process exit after a read-only session.
    }

    let reopened = Opfs::on_disk(temp.path()).unwrap();
    reopened
        .validate_file_snapshot(&bucket, &file, identity)
        .unwrap();
    assert_eq!(
        reopened.read_file(&bucket, &file).unwrap().bytes,
        b"unchanged"
    );
    assert!(directory_entries(&bucket_directory.join("sync")).is_empty());
}

#[test]
fn flushing_either_unsafe_handle_checkpoints_the_combined_live_version() {
    let temp = TempRoot::new("disk-sync-shared-flush");
    let bucket = disk_bucket("sync-shared-flush");
    let file;
    let flushed_identity;
    let bucket_directory;
    {
        let opfs = Opfs::on_disk(temp.path()).unwrap();
        let root = opfs.ensure_root(&bucket).unwrap();
        file = opfs.get_file(&bucket, &root, "file", true).unwrap();
        opfs.write_file(&bucket, &file, b"abcd", None).unwrap();
        bucket_directory = temp.only_bucket_directory();
        let first = opfs
            .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
            .unwrap();
        let second = opfs
            .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
            .unwrap();

        opfs.sync_write(first, b"X", Some(0), None).unwrap();
        opfs.sync_truncate(second, 6, None).unwrap();
        opfs.sync_write(second, b"Z", Some(5), None).unwrap();
        flushed_identity = opfs.read_file(&bucket, &file).unwrap().identity;
        assert_eq!(directory_entries(&bucket_directory.join("sync")).len(), 1);

        opfs.flush_sync(first, None).unwrap();
        assert!(directory_entries(&bucket_directory.join("sync")).is_empty());
        assert_eq!(opfs.sync_read(second, 6, Some(0)).unwrap(), b"Xbcd\0Z");
        // A process exit after flush must not replay a clean mutation marker.
    }

    let reopened = Opfs::on_disk(temp.path()).unwrap();
    assert_eq!(
        reopened.read_file(&bucket, &file).unwrap().bytes,
        b"Xbcd\0Z"
    );
    reopened
        .validate_file_snapshot(&bucket, &file, flushed_identity)
        .unwrap();
    assert!(directory_entries(&bucket_directory.join("sync")).is_empty());
}

#[test]
fn peer_mutation_after_flush_rearms_recovery_and_persists_on_last_close() {
    let temp = TempRoot::new("disk-sync-rearm-after-flush");
    let bucket = disk_bucket("sync-rearm-after-flush");
    let file;
    let flushed_identity;
    let final_identity;
    let bucket_directory;
    {
        let opfs = Opfs::on_disk(temp.path()).unwrap();
        let root = opfs.ensure_root(&bucket).unwrap();
        file = opfs.get_file(&bucket, &root, "file", true).unwrap();
        opfs.write_file(&bucket, &file, b"abcd", None).unwrap();
        bucket_directory = temp.only_bucket_directory();
        let first = opfs
            .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
            .unwrap();
        let second = opfs
            .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
            .unwrap();

        opfs.sync_write(first, b"X", Some(0), None).unwrap();
        opfs.flush_sync(first, None).unwrap();
        flushed_identity = opfs.read_file(&bucket, &file).unwrap().identity;
        assert!(directory_entries(&bucket_directory.join("sync")).is_empty());

        opfs.sync_write(second, b"Y", Some(1), None).unwrap();
        final_identity = opfs.read_file(&bucket, &file).unwrap().identity;
        assert_ne!(flushed_identity, final_identity);
        assert_eq!(directory_entries(&bucket_directory.join("sync")).len(), 1);
        opfs.close_sync(first, None).unwrap();
        assert_eq!(directory_entries(&bucket_directory.join("sync")).len(), 1);
        opfs.close_sync(second, None).unwrap();
        assert!(directory_entries(&bucket_directory.join("sync")).is_empty());
    }

    let reopened = Opfs::on_disk(temp.path()).unwrap();
    assert_eq!(reopened.read_file(&bucket, &file).unwrap().bytes, b"XYcd");
    reopened
        .validate_file_snapshot(&bucket, &file, final_identity)
        .unwrap();
    assert!(matches!(
        reopened.validate_file_snapshot(&bucket, &file, flushed_identity),
        Err(OpfsError::NotFound(_))
    ));
}

#[test]
fn crash_recovery_never_resurrects_an_intermediate_shared_snapshot() {
    let temp = TempRoot::new("disk-sync-snapshot-recovery");
    let bucket = disk_bucket("sync-snapshot-recovery");
    let file;
    let intermediate_identity;
    let final_unflushed_identity;
    let bucket_directory;
    {
        let opfs = Opfs::on_disk(temp.path()).unwrap();
        let root = opfs.ensure_root(&bucket).unwrap();
        file = opfs.get_file(&bucket, &root, "file", true).unwrap();
        opfs.write_file(&bucket, &file, b"abcd", None).unwrap();
        bucket_directory = temp.only_bucket_directory();
        let first = opfs
            .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
            .unwrap();
        let second = opfs
            .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
            .unwrap();

        opfs.sync_write(first, b"X", Some(0), None).unwrap();
        intermediate_identity = opfs.read_file(&bucket, &file).unwrap().identity;
        opfs.sync_write(second, b"Y", Some(1), None).unwrap();
        final_unflushed_identity = opfs.read_file(&bucket, &file).unwrap().identity;
        assert_ne!(intermediate_identity, final_unflushed_identity);
        assert_eq!(directory_entries(&bucket_directory.join("sync")).len(), 1);
        // Neither uncheckpointed identity may become valid after crash recovery.
    }

    let reopened = Opfs::on_disk(temp.path()).unwrap();
    assert_eq!(reopened.read_file(&bucket, &file).unwrap().bytes, b"XYcd");
    for identity in [intermediate_identity, final_unflushed_identity] {
        assert!(matches!(
            reopened.validate_file_snapshot(&bucket, &file, identity),
            Err(OpfsError::NotFound(_))
        ));
    }
    assert!(directory_entries(&bucket_directory.join("sync")).is_empty());
}

#[test]
fn concurrent_disk_unsafe_handles_share_one_marker_and_merge_disjoint_writes() {
    const HANDLE_COUNT: usize = 8;
    const BYTES_PER_HANDLE: usize = 4;

    let temp = TempRoot::new("disk-sync-concurrent-shared");
    let opfs = Opfs::on_disk(temp.path()).unwrap();
    let bucket = disk_bucket("sync-concurrent-shared");
    let root = opfs.ensure_root(&bucket).unwrap();
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    let bucket_directory = temp.only_bucket_directory();
    let all_open = Arc::new(Barrier::new(HANDLE_COUNT + 1));
    let start_writes = Arc::new(Barrier::new(HANDLE_COUNT + 1));
    let all_written = Arc::new(Barrier::new(HANDLE_COUNT + 1));
    let start_close = Arc::new(Barrier::new(HANDLE_COUNT + 1));

    let threads = (0..HANDLE_COUNT)
        .map(|index| {
            let opfs = opfs.clone();
            let bucket = bucket.clone();
            let file = file.clone();
            let all_open = all_open.clone();
            let start_writes = start_writes.clone();
            let all_written = all_written.clone();
            let start_close = start_close.clone();
            thread::spawn(move || {
                let handle = opfs
                    .create_sync_access_handle(&bucket, &file, SyncAccessMode::ReadwriteUnsafe)
                    .unwrap();
                all_open.wait();
                start_writes.wait();
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
                all_written.wait();
                start_close.wait();
                opfs.close_sync(handle, None).unwrap();
            })
        })
        .collect::<Vec<_>>();

    all_open.wait();
    assert!(directory_entries(&bucket_directory.join("sync")).is_empty());
    start_writes.wait();
    all_written.wait();
    assert_eq!(directory_entries(&bucket_directory.join("sync")).len(), 1);
    start_close.wait();
    for thread in threads {
        thread.join().unwrap();
    }
    assert!(directory_entries(&bucket_directory.join("sync")).is_empty());

    let expected = (0..HANDLE_COUNT)
        .flat_map(|index| {
            std::iter::repeat_n(b'A' + u8::try_from(index).unwrap(), BYTES_PER_HANDLE)
        })
        .collect::<Vec<_>>();
    assert_eq!(opfs.read_file(&bucket, &file).unwrap().bytes, expected);
    drop(opfs);
    let reopened = Opfs::on_disk(temp.path()).unwrap();
    assert_eq!(reopened.read_file(&bucket, &file).unwrap().bytes, expected);
}
