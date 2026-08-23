use std::collections::VecDeque;

use moli_owner_queue::OwnerTaskSource;

/// A posted task source for document-owned lanes.
///
/// This wraps the existing owner wake queue so asynchronous producers can still
/// post work into the owner loop. It gives renderer code a document-task-lane
/// boundary without making the lane a child-frame-specific container.
#[derive(Debug)]
pub(crate) struct DocumentPostedTaskSource<T> {
    source: OwnerTaskSource<T>,
}

impl<T> Default for DocumentPostedTaskSource<T> {
    fn default() -> Self {
        Self {
            source: OwnerTaskSource::new(),
        }
    }
}

impl<T> DocumentPostedTaskSource<T> {
    pub(crate) fn post(&self, task: T) {
        self.source
            .sender()
            .send(task)
            .expect("document posted task source should stay open");
    }

    pub(crate) fn update_ready_tasks<R>(
        &mut self,
        update: impl FnOnce(&mut VecDeque<T>) -> R,
    ) -> R {
        self.source.with_tasks_mut(update)
    }

    pub(crate) fn pop_front(&mut self) -> Option<T> {
        self.source.pop_front()
    }

    #[cfg(test)]
    pub(crate) fn is_empty_local_only(&self) -> bool {
        self.source.is_empty_local_only()
    }

    #[cfg(test)]
    pub(crate) fn drain_posted_for_testing(&mut self) {
        let _ = self.source.is_empty();
    }
}
