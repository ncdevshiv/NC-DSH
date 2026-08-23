//! Public namespace, handle, snapshot, move, and remove scenarios.

pub mod common;

use common::{file_text, memory_fixture};
use moli_opfs::{DirectoryEntry, EntryKind, OpfsError, OpfsPath};

#[test]
fn first_use_creates_a_nested_tree_and_resolves_handles() {
    let (opfs, bucket, root) = memory_fixture("first-use");
    assert_eq!(root, OpfsPath::root());
    assert_eq!(opfs.ensure_root(&bucket).unwrap(), root);

    let projects = opfs
        .get_directory(&bucket, &root, "projects", true)
        .unwrap();
    let moli = opfs
        .get_directory(&bucket, &projects, "moli", true)
        .unwrap();
    let cache = opfs.get_directory(&bucket, &moli, "cache", true).unwrap();
    let config = opfs.get_file(&bucket, &moli, "config.json", true).unwrap();
    opfs.write_file(&bucket, &config, br#"{"version":1}"#, None)
        .unwrap();

    assert_eq!(
        opfs.entry_kind(&bucket, &moli).unwrap(),
        EntryKind::Directory
    );
    assert_eq!(opfs.entry_kind(&bucket, &config).unwrap(), EntryKind::File);
    assert_eq!(
        opfs.get_file(&bucket, &moli, "config.json", false).unwrap(),
        config
    );
    assert_eq!(file_text(&opfs, &bucket, &config), r#"{"version":1}"#);
    assert_eq!(
        opfs.read_directory(&bucket, &moli).unwrap(),
        vec![
            DirectoryEntry {
                name: "cache".to_owned(),
                kind: EntryKind::Directory,
            },
            DirectoryEntry {
                name: "config.json".to_owned(),
                kind: EntryKind::File,
            },
        ]
    );
    assert_eq!(
        opfs.resolve(&bucket, &root, &config).unwrap(),
        Some(vec![
            "projects".to_owned(),
            "moli".to_owned(),
            "config.json".to_owned(),
        ])
    );
    assert_eq!(
        opfs.resolve(&bucket, &projects, &config).unwrap(),
        Some(vec!["moli".to_owned(), "config.json".to_owned()])
    );
    assert_eq!(opfs.resolve(&bucket, &cache, &config).unwrap(), None);
}

#[test]
fn lookup_validates_names_existence_and_entry_kind() {
    let (opfs, bucket, root) = memory_fixture("lookup-errors");

    assert!(matches!(
        opfs.get_file(&bucket, &root, "missing.txt", false),
        Err(OpfsError::NotFound(_))
    ));
    opfs.get_directory(&bucket, &root, "same-name", true)
        .unwrap();
    assert!(matches!(
        opfs.get_file(&bucket, &root, "same-name", false),
        Err(OpfsError::TypeMismatch {
            expected: EntryKind::File,
            actual: EntryKind::Directory,
            ..
        })
    ));
    for invalid in ["", ".", "..", "nested/name", "nested\\name"] {
        assert!(matches!(
            opfs.get_file(&bucket, &root, invalid, true),
            Err(OpfsError::InvalidName(_))
        ));
    }
}

#[test]
fn file_reads_are_immutable_snapshots_of_a_committed_version() {
    let (opfs, bucket, root) = memory_fixture("file-snapshots");
    let file = opfs.get_file(&bucket, &root, "document.txt", true).unwrap();
    opfs.write_file(&bucket, &file, b"version one", None)
        .unwrap();
    let first = opfs.read_file(&bucket, &file).unwrap();

    opfs.write_file(&bucket, &file, b"version two", None)
        .unwrap();
    let second = opfs.read_file(&bucket, &file).unwrap();

    assert_eq!(first.name, "document.txt");
    assert_eq!(first.bytes, b"version one");
    assert_eq!(first.size(), 11);
    assert_eq!(second.bytes, b"version two");
    assert_ne!(first.identity, second.identity);
    assert!(matches!(
        opfs.validate_file_snapshot(&bucket, &file, first.identity),
        Err(OpfsError::NotFound(_))
    ));
    opfs.validate_file_snapshot(&bucket, &file, second.identity)
        .unwrap();
}

#[test]
fn move_and_recursive_remove_preserve_filesystem_tree_rules() {
    let (opfs, bucket, root) = memory_fixture("move-remove");
    let project = opfs.get_directory(&bucket, &root, "project", true).unwrap();
    let file = opfs
        .get_file(&bucket, &project, "config.json", true)
        .unwrap();
    opfs.write_file(&bucket, &file, b"configuration", None)
        .unwrap();

    let moved = opfs
        .move_entry(
            &bucket,
            &file,
            EntryKind::File,
            &root,
            "settings.json",
            None,
        )
        .unwrap();
    assert_eq!(moved.display(), "/settings.json");
    assert_eq!(file_text(&opfs, &bucket, &moved), "configuration");
    assert!(matches!(
        opfs.read_file(&bucket, &file),
        Err(OpfsError::NotFound(_))
    ));

    opfs.get_file(&bucket, &project, "child.txt", true).unwrap();
    assert!(matches!(
        opfs.remove_entry(&bucket, &project, false),
        Err(OpfsError::DirectoryNotEmpty(_))
    ));
    opfs.remove_entry(&bucket, &project, true).unwrap();
    assert!(matches!(
        opfs.remove_entry(&bucket, &project, true),
        Err(OpfsError::NotFound(_))
    ));
}
