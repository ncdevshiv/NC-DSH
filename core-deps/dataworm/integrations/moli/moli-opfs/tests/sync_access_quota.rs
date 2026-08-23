//! Synchronous access handle logical-usage accounting scenarios.

pub mod common;

use common::TempRoot;
use moli_opfs::{Opfs, OpfsBucketKey, SyncAccessMode};

#[test]
fn truncate_growth_and_shrink_charge_only_the_live_file_size() {
    let opfs = Opfs::in_memory();
    let bucket = OpfsBucketKey::new("test:sync-truncate-usage").unwrap();
    let root = opfs.ensure_root(&bucket).unwrap();
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    let baseline = opfs.usage(&bucket).unwrap();
    let handle = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::Readwrite)
        .unwrap();

    opfs.sync_truncate(handle, 100, None).unwrap();
    assert_eq!(opfs.usage(&bucket).unwrap(), baseline + 100);
    assert_eq!(opfs.quota_usage(&bucket).unwrap(), baseline + 100);
    opfs.sync_truncate(handle, 10, None).unwrap();
    assert_eq!(opfs.usage(&bucket).unwrap(), baseline + 10);
    assert_eq!(opfs.quota_usage(&bucket).unwrap(), baseline + 10);

    opfs.close_sync(handle, None).unwrap();
    assert_eq!(opfs.usage(&bucket).unwrap(), baseline + 10);
}

#[test]
fn shrinking_a_reopened_nonempty_file_releases_its_usage() {
    let temp = TempRoot::new("sync-reopen-shrink-usage");
    let bucket = OpfsBucketKey::new("test:sync-reopen-shrink-usage").unwrap();
    let file;
    let usage_before_shrink;
    {
        let opfs = Opfs::on_disk(temp.path()).unwrap();
        let root = opfs.ensure_root(&bucket).unwrap();
        file = opfs.get_file(&bucket, &root, "file", true).unwrap();
        let handle = opfs
            .create_sync_access_handle(&bucket, &file, SyncAccessMode::Readwrite)
            .unwrap();
        opfs.sync_truncate(handle, 100, None).unwrap();
        opfs.close_sync(handle, None).unwrap();
        usage_before_shrink = opfs.usage(&bucket).unwrap();
    }

    {
        let opfs = Opfs::on_disk(temp.path()).unwrap();
        let handle = opfs
            .create_sync_access_handle(&bucket, &file, SyncAccessMode::Readwrite)
            .unwrap();
        opfs.sync_truncate(handle, 0, None).unwrap();
        assert_eq!(opfs.usage(&bucket).unwrap() + 100, usage_before_shrink);
        opfs.close_sync(handle, None).unwrap();
    }

    let reopened = Opfs::on_disk(temp.path()).unwrap();
    assert_eq!(reopened.usage(&bucket).unwrap() + 100, usage_before_shrink);
    assert_eq!(reopened.read_file(&bucket, &file).unwrap().bytes, b"");
}

#[test]
fn sparse_and_in_place_writes_charge_only_file_growth() {
    let opfs = Opfs::in_memory();
    let bucket = OpfsBucketKey::new("test:sync-write-usage").unwrap();
    let root = opfs.ensure_root(&bucket).unwrap();
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    let baseline = opfs.usage(&bucket).unwrap();

    let first = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::Readwrite)
        .unwrap();
    opfs.sync_write(first, &[0; 10], Some(0), None).unwrap();
    assert_eq!(opfs.usage(&bucket).unwrap(), baseline + 10);
    opfs.close_sync(first, None).unwrap();

    let second = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::Readwrite)
        .unwrap();
    opfs.sync_write(second, &[1; 80], Some(20), None).unwrap();
    assert_eq!(opfs.sync_size(second).unwrap(), 100);
    assert_eq!(opfs.usage(&bucket).unwrap(), baseline + 100);
    opfs.close_sync(second, None).unwrap();

    let third = opfs
        .create_sync_access_handle(&bucket, &file, SyncAccessMode::Readwrite)
        .unwrap();
    opfs.sync_write(third, &[2; 20], Some(5), None).unwrap();
    assert_eq!(opfs.sync_size(third).unwrap(), 100);
    assert_eq!(opfs.usage(&bucket).unwrap(), baseline + 100);
    opfs.close_sync(third, None).unwrap();
}
