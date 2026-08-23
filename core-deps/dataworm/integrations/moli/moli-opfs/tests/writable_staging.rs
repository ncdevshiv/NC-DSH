//! Writable staging, cursor, lock, and quota regression scenarios.
//!
//! Source baseline: Chromium `a03603fe9af6`, primarily
//! `file_system_access_file_writer_impl_{unit,browser}test.cc` and
//! `file_system_access_file_handle_impl_unittest.cc`.

pub mod common;

use std::{collections::BTreeSet, sync::Arc, sync::Barrier, thread};

use common::{TempRoot, directory_entries, file_text, memory_fixture};
use moli_opfs::{Opfs, OpfsBucketKey, OpfsError, WritableCommand, WritableMode};

fn write(data: &[u8], position: Option<u64>) -> WritableCommand {
    WritableCommand::Write {
        data: data.to_vec(),
        position,
    }
}

#[test]
fn empty_write_to_a_fresh_swap_commits_an_empty_file() {
    // Chromium: WriteValidEmptyString and
    // CreateWriterNoKeepExistingWithEmptyFile.
    let (opfs, bucket, root) = memory_fixture("empty-write");
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    opfs.write_file(&bucket, &file, b"very long old contents", None)
        .unwrap();
    let writer = opfs
        .create_writable(&bucket, &file, false, WritableMode::Siloed)
        .unwrap();

    opfs.writable_command(writer, write(b"", None), None)
        .unwrap();
    assert_eq!(file_text(&opfs, &bucket, &file), "very long old contents");
    opfs.close_writable(writer, None).unwrap();
    assert!(opfs.read_file(&bucket, &file).unwrap().bytes.is_empty());
}

#[test]
fn positioned_writes_overwrite_in_place_and_zero_fill_past_end() {
    // Chromium: WriteWithOffsetInFile and WriteWithOffsetPastFile.
    let (opfs, bucket, root) = memory_fixture("positioned-writes");
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    let writer = opfs
        .create_writable(&bucket, &file, false, WritableMode::Siloed)
        .unwrap();
    opfs.writable_command(writer, write(b"1234567890", None), None)
        .unwrap();
    opfs.writable_command(writer, write(b"abc", Some(4)), None)
        .unwrap();
    opfs.close_writable(writer, None).unwrap();
    assert_eq!(file_text(&opfs, &bucket, &file), "1234abc890");

    let sparse_writer = opfs
        .create_writable(&bucket, &file, false, WritableMode::Siloed)
        .unwrap();
    opfs.writable_command(sparse_writer, write(b"abc", Some(4)), None)
        .unwrap();
    opfs.close_writable(sparse_writer, None).unwrap();
    assert_eq!(
        opfs.read_file(&bucket, &file).unwrap().bytes,
        b"\0\0\0\0abc"
    );
}

#[test]
fn explicit_write_position_after_seek_becomes_the_new_cursor() {
    // Chromium: WriteOffsetAndSeekInSameWritable (crbug.com/1427819).
    let (opfs, bucket, root) = memory_fixture("write-cursor");
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    let writer = opfs
        .create_writable(&bucket, &file, false, WritableMode::Siloed)
        .unwrap();

    opfs.writable_command(writer, write(b"abcdefgh", None), None)
        .unwrap();
    opfs.writable_command(writer, WritableCommand::Seek(0), None)
        .unwrap();
    opfs.writable_command(writer, write(b"123", Some(3)), None)
        .unwrap();
    opfs.writable_command(writer, write(b"4", None), None)
        .unwrap();
    opfs.close_writable(writer, None).unwrap();

    assert_eq!(file_text(&opfs, &bucket, &file), "abc1234h");
}

#[test]
fn keep_existing_data_captures_an_independent_point_in_time_copy() {
    // Chromium: KeepExistingDataHasPreviousContent plus the unique-swap-file
    // tests. Each siloed writer starts from its own copy.
    let (opfs, bucket, root) = memory_fixture("keep-existing-copy");
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    opfs.write_file(&bucket, &file, b"fooks", None).unwrap();

    let older_copy = opfs
        .create_writable(&bucket, &file, true, WritableMode::Siloed)
        .unwrap();
    let newer_writer = opfs
        .create_writable(&bucket, &file, false, WritableMode::Siloed)
        .unwrap();
    opfs.writable_command(newer_writer, write(b"newer", None), None)
        .unwrap();
    opfs.close_writable(newer_writer, None).unwrap();
    assert_eq!(file_text(&opfs, &bucket, &file), "newer");

    opfs.writable_command(older_copy, write(b"bar", Some(0)), None)
        .unwrap();
    opfs.close_writable(older_copy, None).unwrap();
    assert_eq!(file_text(&opfs, &bucket, &file), "barks");
}

#[test]
fn siloed_and_exclusive_writer_lock_modes_have_the_expected_matrix() {
    // Chromium: SiloedMode and ExclusiveMode.
    let (opfs, bucket, root) = memory_fixture("writer-lock-modes");
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    let sibling = opfs.get_file(&bucket, &root, "sibling", true).unwrap();

    let first_siloed = opfs
        .create_writable(&bucket, &file, false, WritableMode::Siloed)
        .unwrap();
    let second_siloed = opfs
        .create_writable(&bucket, &file, false, WritableMode::Siloed)
        .unwrap();
    assert!(matches!(
        opfs.create_writable(&bucket, &file, false, WritableMode::Exclusive),
        Err(OpfsError::NoModificationAllowed(_))
    ));
    let sibling_exclusive = opfs
        .create_writable(&bucket, &sibling, false, WritableMode::Exclusive)
        .unwrap();
    opfs.abort_writable(sibling_exclusive).unwrap();
    opfs.abort_writable(first_siloed).unwrap();
    opfs.abort_writable(second_siloed).unwrap();

    let exclusive = opfs
        .create_writable(&bucket, &file, false, WritableMode::Exclusive)
        .unwrap();
    for mode in [WritableMode::Siloed, WritableMode::Exclusive] {
        assert!(matches!(
            opfs.create_writable(&bucket, &file, false, mode),
            Err(OpfsError::NoModificationAllowed(_))
        ));
    }
    opfs.abort_writable(exclusive).unwrap();
}

#[test]
fn quota_error_never_reaches_the_swap_and_close_commits_its_unchanged_state() {
    // Chromium: FileSystemAccessSandboxedFileWriterImplTest.QuotaError.
    let (opfs, bucket, root) = memory_fixture("writer-quota");
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    opfs.write_file(&bucket, &file, b"committed", None).unwrap();
    let quota = opfs.usage(&bucket).unwrap();
    let writer = opfs
        .create_writable(&bucket, &file, false, WritableMode::Siloed)
        .unwrap();

    assert!(matches!(
        opfs.writable_command(writer, write(b"0123456789", None), Some(quota)),
        Err(OpfsError::QuotaExceeded { .. })
    ));
    assert_eq!(file_text(&opfs, &bucket, &file), "committed");
    opfs.close_writable(writer, Some(quota)).unwrap();
    assert!(opfs.read_file(&bucket, &file).unwrap().bytes.is_empty());
}

#[test]
fn racing_disk_writers_receive_unique_staging_files_and_all_clean_up() {
    // Chromium: EachWriterHasUniqueSwapFileRacy and
    // EachWriterHasUniqueSwapFileRacyKeepExistingData.
    const WRITER_COUNT: usize = 8;

    let temp = TempRoot::new("racing-swap-files");
    let opfs = Opfs::on_disk(temp.path()).unwrap();
    let bucket = OpfsBucketKey::new("test:racing-swap-files").unwrap();
    let root = opfs.ensure_root(&bucket).unwrap();
    let file = opfs.get_file(&bucket, &root, "file", true).unwrap();
    opfs.write_file(&bucket, &file, b"seed", None).unwrap();
    let barrier = Arc::new(Barrier::new(WRITER_COUNT));

    let threads = (0..WRITER_COUNT)
        .map(|index| {
            let worker = opfs.clone();
            let worker_bucket = bucket.clone();
            let worker_file = file.clone();
            let worker_barrier = barrier.clone();
            thread::spawn(move || {
                worker_barrier.wait();
                worker
                    .create_writable(
                        &worker_bucket,
                        &worker_file,
                        index % 2 == 0,
                        WritableMode::Siloed,
                    )
                    .unwrap()
            })
        })
        .collect::<Vec<_>>();
    let writers = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect::<Vec<_>>();

    assert_eq!(
        writers
            .iter()
            .map(|writer| writer.get())
            .collect::<BTreeSet<_>>()
            .len(),
        WRITER_COUNT
    );
    let staging_directory = temp.only_bucket_directory().join("staging");
    assert_eq!(directory_entries(&staging_directory).len(), WRITER_COUNT);

    for writer in writers {
        opfs.abort_writable(writer).unwrap();
    }
    assert!(directory_entries(&staging_directory).is_empty());
    assert_eq!(file_text(&opfs, &bucket, &file), "seed");
}
