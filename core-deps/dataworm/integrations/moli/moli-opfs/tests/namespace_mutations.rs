//! Namespace mutation and quota regression scenarios.
//!
//! Source baseline: Chromium `a03603fe9af6`, primarily
//! `sandbox_directory_database_unittest.cc` and
//! `obfuscated_file_util_unittest.cc`.

pub mod common;

use common::{file_text, memory_fixture};
use moli_opfs::{EntryKind, OpfsError};

#[test]
fn missing_parents_and_entry_kind_conflicts_are_rejected() {
    // Chromium: TestMissingParentAddFileInfo, TestAddNameClash, and
    // TestReadDirectoryOnFile.
    let (opfs, bucket, root) = memory_fixture("namespace-kinds");
    let missing_parent = root.child("missing-parent").unwrap();

    assert!(matches!(
        opfs.get_file(&bucket, &missing_parent, "child", true),
        Err(OpfsError::NotFound(_))
    ));

    let file = opfs.get_file(&bucket, &root, "entry", true).unwrap();
    assert_eq!(
        opfs.get_file(&bucket, &root, "entry", true).unwrap(),
        file,
        "non-exclusive create must return the existing same-kind entry"
    );
    assert!(matches!(
        opfs.get_directory(&bucket, &root, "entry", true),
        Err(OpfsError::TypeMismatch {
            expected: EntryKind::Directory,
            actual: EntryKind::File,
            ..
        })
    ));
    assert!(matches!(
        opfs.read_directory(&bucket, &file),
        Err(OpfsError::TypeMismatch {
            expected: EntryKind::Directory,
            actual: EntryKind::File,
            ..
        })
    ));
}

#[test]
fn non_empty_directories_and_the_root_cannot_be_removed() {
    // Chromium: TestRemoveWithChildren and TestDirectoryOps.
    let (opfs, bucket, root) = memory_fixture("remove-tree");
    let directory = opfs
        .get_directory(&bucket, &root, "directory", true)
        .unwrap();
    let child = opfs
        .get_file(&bucket, &directory, "child.bin", true)
        .unwrap();

    assert!(matches!(
        opfs.remove_entry(&bucket, &directory, false),
        Err(OpfsError::DirectoryNotEmpty(_))
    ));
    assert!(matches!(
        opfs.remove_entry(&bucket, &root, true),
        Err(OpfsError::InvalidModification(_))
    ));
    assert!(matches!(
        opfs.move_entry(
            &bucket,
            &root,
            EntryKind::Directory,
            &directory,
            "root",
            None,
        ),
        Err(OpfsError::InvalidModification(_))
    ));

    opfs.remove_entry(&bucket, &child, false).unwrap();
    opfs.remove_entry(&bucket, &directory, false).unwrap();
}

#[test]
fn overwriting_file_move_preserves_source_identity_and_replaces_destination() {
    // Chromium: TestOverwritingMoveFileSuccess and the move variants in
    // TestCopyOrMoveFileSuccess.
    let (opfs, bucket, root) = memory_fixture("overwriting-file-move");
    let source = opfs.get_file(&bucket, &root, "source.bin", true).unwrap();
    let destination = opfs
        .get_file(&bucket, &root, "destination.bin", true)
        .unwrap();
    opfs.write_file(&bucket, &source, b"source bytes", None)
        .unwrap();
    opfs.write_file(&bucket, &destination, b"old destination", None)
        .unwrap();
    let source_snapshot = opfs.read_file(&bucket, &source).unwrap();
    let destination_snapshot = opfs.read_file(&bucket, &destination).unwrap();

    let moved = opfs
        .move_entry(
            &bucket,
            &source,
            EntryKind::File,
            &root,
            "destination.bin",
            None,
        )
        .unwrap();

    assert_eq!(moved, destination);
    assert_eq!(file_text(&opfs, &bucket, &moved), "source bytes");
    assert!(matches!(
        opfs.read_file(&bucket, &source),
        Err(OpfsError::NotFound(_))
    ));
    opfs.validate_file_snapshot(&bucket, &moved, source_snapshot.identity)
        .unwrap();
    assert!(matches!(
        opfs.validate_file_snapshot(&bucket, &moved, destination_snapshot.identity),
        Err(OpfsError::NotFound(_))
    ));
}

#[test]
fn directory_move_replaces_only_an_empty_same_kind_destination() {
    // Chromium: TestRemoveWithChildren and the file-handle move overwrite
    // matrix. Moli extends the same invariant to experimental directory
    // moves.
    let (opfs, bucket, root) = memory_fixture("directory-overwrite");
    let source = opfs.get_directory(&bucket, &root, "source", true).unwrap();
    let payload = opfs
        .get_file(&bucket, &source, "payload.bin", true)
        .unwrap();
    opfs.write_file(&bucket, &payload, b"payload", None)
        .unwrap();
    let destination = opfs
        .get_directory(&bucket, &root, "destination", true)
        .unwrap();

    let moved = opfs
        .move_entry(
            &bucket,
            &source,
            EntryKind::Directory,
            &root,
            "destination",
            None,
        )
        .unwrap();
    assert_eq!(moved, destination);
    assert_eq!(
        file_text(&opfs, &bucket, &moved.child("payload.bin").unwrap()),
        "payload"
    );

    let second_source = opfs
        .get_directory(&bucket, &root, "second-source", true)
        .unwrap();
    let non_empty = opfs
        .get_directory(&bucket, &root, "non-empty", true)
        .unwrap();
    opfs.get_file(&bucket, &non_empty, "child", true).unwrap();
    assert!(matches!(
        opfs.move_entry(
            &bucket,
            &second_source,
            EntryKind::Directory,
            &root,
            "non-empty",
            None,
        ),
        Err(OpfsError::DirectoryNotEmpty(_))
    ));
    assert_eq!(
        opfs.entry_kind(&bucket, &second_source).unwrap(),
        EntryKind::Directory
    );

    let nested = opfs.get_directory(&bucket, &moved, "nested", true).unwrap();
    assert!(matches!(
        opfs.move_entry(
            &bucket,
            &moved,
            EntryKind::Directory,
            &nested,
            "cycle",
            None,
        ),
        Err(OpfsError::InvalidModification(_))
    ));

    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    assert!(matches!(
        opfs.move_entry(&bucket, &file, EntryKind::File, &root, "non-empty", None,),
        Err(OpfsError::TypeMismatch {
            expected: EntryKind::File,
            actual: EntryKind::Directory,
            ..
        })
    ));
}

#[test]
fn rename_quota_failure_rolls_back_the_entire_namespace_change() {
    // Chromium: TestMovePathQuotasWithRename.
    let (opfs, bucket, root) = memory_fixture("rename-quota");
    let source = opfs.get_file(&bucket, &root, "a", true).unwrap();
    opfs.write_file(&bucket, &source, b"bytes", None).unwrap();
    let usage_before = opfs.usage(&bucket).unwrap();
    let destination_name = "a-much-longer-destination-name";

    assert!(matches!(
        opfs.move_entry(
            &bucket,
            &source,
            EntryKind::File,
            &root,
            destination_name,
            Some(usage_before),
        ),
        Err(OpfsError::QuotaExceeded { .. })
    ));
    assert_eq!(file_text(&opfs, &bucket, &source), "bytes");
    assert!(matches!(
        opfs.get_file(&bucket, &root, destination_name, false),
        Err(OpfsError::NotFound(_))
    ));
    assert_eq!(opfs.usage(&bucket).unwrap(), usage_before);

    let moved = opfs
        .move_entry(
            &bucket,
            &source,
            EntryKind::File,
            &root,
            destination_name,
            None,
        )
        .unwrap();
    assert_eq!(file_text(&opfs, &bucket, &moved), "bytes");
    assert!(opfs.usage(&bucket).unwrap() > usage_before);
}

#[test]
fn removing_files_and_subtrees_releases_path_and_content_usage() {
    // Chromium: TestQuotaOnRemove.
    let (opfs, bucket, root) = memory_fixture("remove-quota");
    let standalone = opfs.get_file(&bucket, &root, "standalone", true).unwrap();
    opfs.write_file(&bucket, &standalone, &[1; 340], None)
        .unwrap();
    let directory = opfs.get_directory(&bucket, &root, "dir", true).unwrap();
    let first = opfs.get_file(&bucket, &directory, "first", true).unwrap();
    let second = opfs.get_file(&bucket, &directory, "second", true).unwrap();
    opfs.write_file(&bucket, &first, &[2; 1020], None).unwrap();
    opfs.write_file(&bucket, &second, &[3; 120], None).unwrap();
    let full_usage = opfs.usage(&bucket).unwrap();

    opfs.remove_entry(&bucket, &standalone, false).unwrap();
    let subtree_usage = opfs.usage(&bucket).unwrap();
    assert!(subtree_usage < full_usage);

    opfs.remove_entry(&bucket, &directory, true).unwrap();
    assert_eq!(opfs.usage(&bucket).unwrap(), 0);
}
