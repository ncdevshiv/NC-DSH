//! Disk-root initialization boundaries and retry behavior.

pub mod common;

use std::{
    fs,
    sync::{Arc, Barrier},
    thread,
};

use common::{TempRoot, directory_entries};
use moli_opfs::{Opfs, OpfsBucketKey, OpfsError};

fn bucket(name: &str) -> OpfsBucketKey {
    OpfsBucketKey::new(format!("test:initialization:{name}")).unwrap()
}

#[test]
fn configuring_disk_backend_does_not_touch_the_root() {
    let temp = TempRoot::new("lazy-root");
    let root = temp.path().join("opfs");
    let opfs = Opfs::on_disk(&root).unwrap();

    assert!(
        !root.exists(),
        "constructing the backend must not create its configured root"
    );

    assert!(opfs.ensure_root(&bucket("first-use")).unwrap().is_root());
    assert!(root.is_dir());
    assert_eq!(directory_entries(&root).len(), 1);
}

#[test]
fn failed_disk_root_initialization_can_be_retried() {
    let temp = TempRoot::new("retry-root");
    let root = temp.path().join("opfs");
    fs::write(&root, b"blocks directory creation").unwrap();
    let opfs = Opfs::on_disk(&root).unwrap();
    let bucket = bucket("retry");

    assert!(matches!(
        opfs.ensure_root(&bucket),
        Err(OpfsError::Io {
            operation: "create root directory",
            ..
        })
    ));

    fs::remove_file(&root).unwrap();
    assert!(opfs.ensure_root(&bucket).unwrap().is_root());
    assert!(root.is_dir());
}

#[test]
fn failed_root_recovery_can_be_retried_after_the_obstruction_is_fixed() {
    let temp = TempRoot::new("retry-recovery");
    let root = temp.path().join("opfs");
    let obstructed_bucket = root.join("obstructed-bucket");
    fs::create_dir_all(&obstructed_bucket).unwrap();
    let staging_path = obstructed_bucket.join("staging");
    fs::write(&staging_path, b"not a directory").unwrap();
    let opfs = Opfs::on_disk(&root).unwrap();
    let bucket = bucket("retry-recovery");

    assert!(matches!(
        opfs.ensure_root(&bucket),
        Err(OpfsError::Io {
            operation: "scan writable staging directory",
            ..
        })
    ));
    assert!(
        staging_path.is_file(),
        "failed recovery must leave the obstruction available for diagnosis"
    );

    fs::remove_file(&staging_path).unwrap();
    assert!(
        opfs.ensure_root(&bucket).unwrap().is_root(),
        "a recovery error must not be cached as a terminal initialization result"
    );
    assert_eq!(
        directory_entries(&root).len(),
        2,
        "the repaired pre-existing directory and the requested bucket should remain"
    );
}

#[test]
fn usage_and_clear_are_real_first_operations() {
    let temp = TempRoot::new("usage-and-clear");

    let usage_root = temp.path().join("usage");
    let usage_opfs = Opfs::on_disk(&usage_root).unwrap();
    assert!(!usage_root.exists());
    assert_eq!(usage_opfs.usage(&bucket("usage")).unwrap(), 0);
    assert!(usage_root.is_dir());

    let clear_root = temp.path().join("clear");
    let clear_opfs = Opfs::on_disk(&clear_root).unwrap();
    assert!(!clear_root.exists());
    clear_opfs.clear_bucket(&bucket("clear")).unwrap();
    assert!(clear_root.is_dir());
}

#[test]
fn successful_root_recovery_runs_once_per_backend_lifetime() {
    let temp = TempRoot::new("single-recovery");
    let root = temp.path().join("opfs");
    let first_bucket = bucket("single-recovery-first");
    let second_bucket = bucket("single-recovery-second");
    let opfs = Opfs::on_disk(&root).unwrap();

    assert!(opfs.ensure_root(&first_bucket).unwrap().is_root());
    let first_bucket_directory = directory_entries(&root)
        .into_iter()
        .next()
        .expect("first bucket directory");
    let orphan = first_bucket_directory
        .join("staging")
        .join("late-orphan.stage");
    fs::write(&orphan, b"created after initial recovery").unwrap();

    assert!(opfs.ensure_root(&second_bucket).unwrap().is_root());
    assert!(
        orphan.is_file(),
        "opening another bucket must not repeat root-wide recovery after Ready"
    );

    drop(opfs);
    let reopened = Opfs::on_disk(&root).unwrap();
    assert!(
        orphan.is_file(),
        "constructing the next backend must still defer recovery"
    );
    assert_eq!(reopened.usage(&second_bucket).unwrap(), 0);
    assert!(
        !orphan.exists(),
        "the next backend lifetime must recover the root on its first operation"
    );
}

#[test]
fn concurrent_first_operations_share_one_root_initialization() {
    const THREAD_COUNT: usize = 8;

    let temp = TempRoot::new("concurrent-root");
    let root = temp.path().join("opfs");
    let opfs = Opfs::on_disk(&root).unwrap();
    let barrier = Arc::new(Barrier::new(THREAD_COUNT));
    let threads = (0..THREAD_COUNT)
        .map(|index| {
            let opfs = opfs.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                opfs.ensure_root(&bucket(&format!("concurrent-{index}")))
                    .unwrap()
            })
        })
        .collect::<Vec<_>>();

    for thread in threads {
        assert!(thread.join().unwrap().is_root());
    }
    assert!(root.is_dir());
    assert_eq!(directory_entries(&root).len(), THREAD_COUNT);
}
