use std::collections::VecDeque;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

#[derive(Debug)]
pub struct OwnerWakeQueue<T> {
    buffered: VecDeque<T>,
    tx: UnboundedSender<T>,
    rx: UnboundedReceiver<T>,
}

impl<T> OwnerWakeQueue<T> {
    pub fn new() -> Self {
        let (tx, rx) = unbounded_channel();
        Self {
            buffered: VecDeque::new(),
            tx,
            rx,
        }
    }

    pub fn sender(&self) -> UnboundedSender<T> {
        self.tx.clone()
    }

    pub fn try_drain_into(&mut self, dst: &mut VecDeque<T>) {
        self.drain_incoming();
        dst.append(&mut self.buffered);
    }

    pub fn pop_front(&mut self) -> Option<T> {
        self.drain_incoming();
        self.buffered.pop_front()
    }

    #[cfg(test)]
    pub fn clear(&mut self) {
        self.drain_incoming();
        self.buffered.clear();
    }

    fn drain_incoming(&mut self) {
        while let Ok(item) = self.rx.try_recv() {
            self.buffered.push_back(item);
        }
    }

    pub async fn recv(&mut self) -> Option<T> {
        if let Some(item) = self.pop_front() {
            return Some(item);
        }
        self.rx.recv().await
    }
}

impl<T> Default for OwnerWakeQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::OwnerWakeQueue;
    use std::collections::VecDeque;

    #[test]
    fn try_drain_into_collects_all_ready_items() {
        let mut queue = OwnerWakeQueue::new();
        let sender = queue.sender();
        sender.send(1).unwrap();
        sender.send(2).unwrap();
        let mut drained = VecDeque::new();
        queue.try_drain_into(&mut drained);
        assert_eq!(drained.into_iter().collect::<Vec<_>>(), vec![1, 2]);
    }

    #[test]
    fn clear_discards_buffered_and_incoming_items() {
        let mut queue = OwnerWakeQueue::new();
        let sender = queue.sender();
        sender.send(1).unwrap();
        sender.send(2).unwrap();

        queue.clear();

        assert_eq!(queue.pop_front(), None);
    }
}
