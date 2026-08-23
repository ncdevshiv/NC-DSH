//! Public atomic writable stream backend scenarios.

pub mod common;

use common::{file_text, memory_fixture};
use moli_opfs::{OpfsError, WritableCommand, WritableMode};

fn write(data: &[u8], position: Option<u64>) -> WritableCommand {
    WritableCommand::Write {
        data: data.to_vec(),
        position,
    }
}

#[test]
fn writable_without_existing_data_is_atomic_and_replaces_on_close() {
    let (opfs, bucket, root) = memory_fixture("writable-atomic");
    let file = opfs.get_file(&bucket, &root, "document.txt", true).unwrap();
    opfs.write_file(&bucket, &file, b"abcdef", None).unwrap();

    let writer = opfs
        .create_writable(&bucket, &file, false, WritableMode::Siloed)
        .unwrap();
    opfs.writable_command(writer, write(b"XY", None), None)
        .unwrap();

    assert_eq!(
        file_text(&opfs, &bucket, &file),
        "abcdef",
        "the committed target must stay unchanged before close"
    );
    opfs.close_writable(writer, None).unwrap();
    assert_eq!(file_text(&opfs, &bucket, &file), "XY");
    assert!(matches!(
        opfs.close_writable(writer, None),
        Err(OpfsError::InvalidState)
    ));
}

#[test]
fn keep_existing_data_supports_seek_overwrite_and_zero_filled_growth() {
    let (opfs, bucket, root) = memory_fixture("writable-random-access");
    let file = opfs.get_file(&bucket, &root, "random.bin", true).unwrap();
    opfs.write_file(&bucket, &file, b"abcdefgh", None).unwrap();

    let writer = opfs
        .create_writable(&bucket, &file, true, WritableMode::Siloed)
        .unwrap();
    opfs.writable_command(writer, WritableCommand::Seek(2), None)
        .unwrap();
    opfs.writable_command(writer, write(b"XY", None), None)
        .unwrap();
    opfs.writable_command(writer, WritableCommand::Truncate(6), None)
        .unwrap();
    opfs.close_writable(writer, None).unwrap();
    assert_eq!(file_text(&opfs, &bucket, &file), "abXYef");

    let grower = opfs
        .create_writable(&bucket, &file, true, WritableMode::Siloed)
        .unwrap();
    opfs.writable_command(grower, WritableCommand::Truncate(9), None)
        .unwrap();
    opfs.close_writable(grower, None).unwrap();
    assert_eq!(
        opfs.read_file(&bucket, &file).unwrap().bytes,
        b"abXYef\0\0\0"
    );
}

#[test]
fn abort_discards_staging_and_releases_the_file_lock() {
    let (opfs, bucket, root) = memory_fixture("writable-abort");
    let file = opfs.get_file(&bucket, &root, "atomic.txt", true).unwrap();
    opfs.write_file(&bucket, &file, b"committed", None).unwrap();

    let writer = opfs
        .create_writable(&bucket, &file, true, WritableMode::Exclusive)
        .unwrap();
    opfs.writable_command(writer, write(b"discarded", Some(0)), None)
        .unwrap();
    assert!(matches!(
        opfs.create_writable(&bucket, &file, true, WritableMode::Siloed),
        Err(OpfsError::NoModificationAllowed(_))
    ));
    opfs.abort_writable(writer).unwrap();
    assert_eq!(file_text(&opfs, &bucket, &file), "committed");

    let replacement = opfs
        .create_writable(&bucket, &file, false, WritableMode::Exclusive)
        .unwrap();
    opfs.abort_writable(replacement).unwrap();
}

#[test]
fn concurrent_siloed_writers_have_independent_staging_and_last_close_wins() {
    let (opfs, bucket, root) = memory_fixture("writable-concurrency");
    let file = opfs.get_file(&bucket, &root, "shared.txt", true).unwrap();
    opfs.write_file(&bucket, &file, b"base", None).unwrap();

    let first = opfs
        .create_writable(&bucket, &file, true, WritableMode::Siloed)
        .unwrap();
    let second = opfs
        .create_writable(&bucket, &file, true, WritableMode::Siloed)
        .unwrap();
    opfs.writable_command(first, write(b"first", Some(0)), None)
        .unwrap();
    opfs.writable_command(second, write(b"last", Some(0)), None)
        .unwrap();

    opfs.close_writable(first, None).unwrap();
    assert_eq!(file_text(&opfs, &bucket, &file), "first");
    opfs.close_writable(second, None).unwrap();
    assert_eq!(file_text(&opfs, &bucket, &file), "last");
}
