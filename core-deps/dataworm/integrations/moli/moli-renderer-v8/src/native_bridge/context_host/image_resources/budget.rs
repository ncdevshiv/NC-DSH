use std::sync::Arc;

use parking_lot::Mutex;

pub(super) const MAX_QUEUED_IMAGE_DECODE_JOBS: usize = 64;
pub(super) const MAX_QUEUED_IMAGE_ENCODED_BYTES: usize = 256 * 1024 * 1024;
pub(super) const MAX_RETAINED_IMAGE_DECODED_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, Default)]
pub(super) struct SharedImageResourceBudget {
    state: Arc<Mutex<ImageResourceBudgetState>>,
}

#[derive(Default)]
struct ImageResourceBudgetState {
    jobs: usize,
    encoded_bytes: usize,
    decoded_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ImageResourceBudgetError {
    JobLimit,
    EncodedByteLimit,
    DecodedByteLimit,
}

impl SharedImageResourceBudget {
    pub(super) fn admit_job(
        &self,
        encoded_bytes: usize,
    ) -> Result<ImageDecodeJobPermit, ImageResourceBudgetError> {
        let mut state = self.state.lock();
        if state.jobs >= MAX_QUEUED_IMAGE_DECODE_JOBS {
            return Err(ImageResourceBudgetError::JobLimit);
        }
        let next_encoded = state
            .encoded_bytes
            .checked_add(encoded_bytes)
            .ok_or(ImageResourceBudgetError::EncodedByteLimit)?;
        if next_encoded > MAX_QUEUED_IMAGE_ENCODED_BYTES {
            return Err(ImageResourceBudgetError::EncodedByteLimit);
        }
        state.jobs += 1;
        state.encoded_bytes = next_encoded;
        Ok(ImageDecodeJobPermit {
            budget: self.clone(),
            encoded_bytes,
        })
    }

    pub(super) fn reserve_decoded(
        &self,
        decoded_bytes: usize,
    ) -> Result<ImageDecodedBytesPermit, ImageResourceBudgetError> {
        let mut state = self.state.lock();
        let next_decoded = state
            .decoded_bytes
            .checked_add(decoded_bytes)
            .ok_or(ImageResourceBudgetError::DecodedByteLimit)?;
        if next_decoded > MAX_RETAINED_IMAGE_DECODED_BYTES {
            return Err(ImageResourceBudgetError::DecodedByteLimit);
        }
        state.decoded_bytes = next_decoded;
        Ok(ImageDecodedBytesPermit {
            budget: self.clone(),
            decoded_bytes,
        })
    }

    #[cfg(test)]
    pub(super) fn counters(&self) -> (usize, usize, usize) {
        let state = self.state.lock();
        (state.jobs, state.encoded_bytes, state.decoded_bytes)
    }
}

pub(super) struct ImageDecodeJobPermit {
    budget: SharedImageResourceBudget,
    encoded_bytes: usize,
}

impl ImageDecodeJobPermit {
    pub(super) fn release_encoded_bytes(&mut self) {
        if self.encoded_bytes == 0 {
            return;
        }
        let mut state = self.budget.state.lock();
        state.encoded_bytes = state.encoded_bytes.saturating_sub(self.encoded_bytes);
        self.encoded_bytes = 0;
    }
}

impl Drop for ImageDecodeJobPermit {
    fn drop(&mut self) {
        let mut state = self.budget.state.lock();
        state.jobs = state.jobs.saturating_sub(1);
        state.encoded_bytes = state.encoded_bytes.saturating_sub(self.encoded_bytes);
    }
}

pub(super) struct ImageDecodedBytesPermit {
    budget: SharedImageResourceBudget,
    decoded_bytes: usize,
}

impl Drop for ImageDecodedBytesPermit {
    fn drop(&mut self) {
        let mut state = self.budget.state.lock();
        state.decoded_bytes = state.decoded_bytes.saturating_sub(self.decoded_bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_admission_is_bounded_and_drop_releases_every_charge() {
        let budget = SharedImageResourceBudget::default();
        let permits = (0..MAX_QUEUED_IMAGE_DECODE_JOBS)
            .map(|_| budget.admit_job(0).expect("job within the bound"))
            .collect::<Vec<_>>();
        assert!(matches!(
            budget.admit_job(0),
            Err(ImageResourceBudgetError::JobLimit)
        ));
        assert_eq!(budget.counters(), (MAX_QUEUED_IMAGE_DECODE_JOBS, 0, 0));
        drop(permits);

        let encoded = budget
            .admit_job(MAX_QUEUED_IMAGE_ENCODED_BYTES)
            .expect("the exact encoded-byte bound is admitted");
        assert!(matches!(
            budget.admit_job(1),
            Err(ImageResourceBudgetError::EncodedByteLimit)
        ));
        drop(encoded);
        assert_eq!(budget.counters(), (0, 0, 0));
    }

    #[test]
    fn decoded_resource_charge_lives_exactly_as_long_as_its_permit() {
        let budget = SharedImageResourceBudget::default();
        let permit = budget
            .reserve_decoded(MAX_RETAINED_IMAGE_DECODED_BYTES)
            .expect("the exact decoded-byte bound is admitted");
        assert!(matches!(
            budget.reserve_decoded(1),
            Err(ImageResourceBudgetError::DecodedByteLimit)
        ));
        assert_eq!(budget.counters(), (0, 0, MAX_RETAINED_IMAGE_DECODED_BYTES));
        drop(permit);
        assert_eq!(budget.counters(), (0, 0, 0));
    }
}
