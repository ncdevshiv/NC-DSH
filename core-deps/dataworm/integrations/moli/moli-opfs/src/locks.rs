use std::collections::BTreeMap;

use crate::{OpfsBucketKey, OpfsError, OpfsPath, OpfsResult};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LockMode {
    SiloedWriter,
    SyncReadOnly,
    SyncReadwriteUnsafe,
    Exclusive,
}

#[derive(Clone, Debug)]
struct LockRecord {
    bucket: OpfsBucketKey,
    path: OpfsPath,
    mode: LockMode,
}

#[derive(Debug, Default)]
pub(crate) struct LockTable {
    records: BTreeMap<u64, Vec<LockRecord>>,
}

impl LockTable {
    pub(crate) fn acquire(
        &mut self,
        owner: u64,
        bucket: &OpfsBucketKey,
        path: &OpfsPath,
        mode: LockMode,
    ) -> OpfsResult<()> {
        self.acquire_many(owner, bucket, std::slice::from_ref(path), mode)
    }

    pub(crate) fn acquire_many(
        &mut self,
        owner: u64,
        bucket: &OpfsBucketKey,
        paths: &[OpfsPath],
        mode: LockMode,
    ) -> OpfsResult<()> {
        for path in paths {
            if self.records.iter().any(|(record_owner, records)| {
                *record_owner != owner
                    && records.iter().any(|record| {
                        record.bucket == *bucket
                            && paths_overlap(&record.path, path)
                            && modes_conflict(record.mode, mode)
                    })
            }) {
                return Err(OpfsError::NoModificationAllowed(path.display()));
            }
        }
        self.records
            .entry(owner)
            .or_default()
            .extend(paths.iter().cloned().map(|path| LockRecord {
                bucket: bucket.clone(),
                path,
                mode,
            }));
        Ok(())
    }

    pub(crate) fn release(&mut self, owner: u64) {
        self.records.remove(&owner);
    }

    pub(crate) fn release_bucket(&mut self, bucket: &OpfsBucketKey) {
        self.records.retain(|_, records| {
            records.retain(|record| record.bucket != *bucket);
            !records.is_empty()
        });
    }
}

fn paths_overlap(left: &OpfsPath, right: &OpfsPath) -> bool {
    left.is_ancestor_of_or_equal(right) || right.is_ancestor_of_or_equal(left)
}

fn modes_conflict(left: LockMode, right: LockMode) -> bool {
    !matches!(
        (left, right),
        (LockMode::SiloedWriter, LockMode::SiloedWriter)
            | (LockMode::SyncReadOnly, LockMode::SyncReadOnly)
            | (LockMode::SyncReadwriteUnsafe, LockMode::SyncReadwriteUnsafe)
    )
}

#[cfg(test)]
mod tests {
    use super::{LockMode, LockTable};
    use crate::{OpfsBucketKey, OpfsPath};

    #[test]
    fn identical_shared_modes_coexist_but_cross_modes_and_ancestors_conflict() {
        let bucket = OpfsBucketKey::new("bucket").unwrap();
        let directory = OpfsPath::root().child("dir").unwrap();
        let file = directory.child("file").unwrap();
        let sibling = directory.child("sibling").unwrap();
        let shared_modes = [
            LockMode::SiloedWriter,
            LockMode::SyncReadOnly,
            LockMode::SyncReadwriteUnsafe,
        ];

        for (index, mode) in shared_modes.into_iter().enumerate() {
            let mut locks = LockTable::default();
            let first_owner = 1 + index as u64 * 10;
            locks.acquire(first_owner, &bucket, &file, mode).unwrap();
            locks
                .acquire(first_owner + 1, &bucket, &file, mode)
                .unwrap();
            assert!(
                locks
                    .acquire(first_owner + 2, &bucket, &file, LockMode::Exclusive)
                    .is_err()
            );
            assert!(
                locks
                    .acquire(first_owner + 2, &bucket, &directory, LockMode::Exclusive)
                    .is_err()
            );
            locks
                .acquire(first_owner + 3, &bucket, &sibling, LockMode::Exclusive)
                .unwrap();

            for other_mode in shared_modes {
                if other_mode != mode {
                    assert!(
                        locks
                            .acquire(first_owner + 4, &bucket, &file, other_mode)
                            .is_err()
                    );
                }
            }
        }
    }
}
