use std::collections::VecDeque;

/// A small FIFO queue for document-owned task lanes.
///
/// This is only the queue core. Frame/document currentness, realm selection,
/// and task dispatch stay in the owner adapter that consumes the queue.
#[derive(Debug, Clone)]
pub(crate) struct DocumentTaskQueue<T> {
    tasks: VecDeque<T>,
}

impl<T> Default for DocumentTaskQueue<T> {
    fn default() -> Self {
        Self {
            tasks: VecDeque::new(),
        }
    }
}

impl<T> DocumentTaskQueue<T> {
    pub(crate) fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.tasks.len()
    }

    pub(crate) fn push_back(&mut self, task: T) {
        self.tasks.push_back(task);
    }

    #[cfg(test)]
    pub(crate) fn insert(&mut self, index: usize, task: T) {
        self.tasks.insert(index, task);
    }

    pub(crate) fn pop_front(&mut self) -> Option<T> {
        self.tasks.pop_front()
    }

    pub(crate) fn drain_all(&mut self) -> impl Iterator<Item = T> + '_ {
        self.tasks.drain(..)
    }

    pub(crate) fn clear(&mut self) {
        self.tasks.clear();
    }

    pub(crate) fn retain(&mut self, keep: impl FnMut(&T) -> bool) -> bool {
        let original_len = self.tasks.len();
        self.tasks.retain(keep);
        self.tasks.len() != original_len
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &T> {
        self.tasks.iter()
    }

    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.tasks.iter_mut()
    }
}

impl<T> IntoIterator for DocumentTaskQueue<T> {
    type Item = T;
    type IntoIter = std::collections::vec_deque::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.tasks.into_iter()
    }
}

impl<T: PartialEq> DocumentTaskQueue<T> {
    pub(crate) fn push_unique(&mut self, task: T) -> bool {
        if self.tasks.iter().any(|pending| pending == &task) {
            return false;
        }
        self.tasks.push_back(task);
        true
    }
}
