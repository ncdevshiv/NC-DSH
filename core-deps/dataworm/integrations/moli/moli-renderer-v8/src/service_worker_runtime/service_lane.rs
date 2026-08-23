use std::collections::VecDeque;

use parking_lot::Mutex;

use super::{
    service::ServiceWorkerRuntimeService, start_completion::ServiceWorkerRuntimeCompletion,
};

#[derive(Default)]
pub(super) struct ServiceWorkerServiceLane {
    events: Mutex<VecDeque<ServiceWorkerServiceLaneEvent>>,
}

enum ServiceWorkerServiceLaneEvent {
    Completion(Box<ServiceWorkerRuntimeCompletion>),
}

impl ServiceWorkerServiceLane {
    pub(super) fn enqueue_completion(&self, completion: ServiceWorkerRuntimeCompletion) {
        self.events
            .lock()
            .push_back(ServiceWorkerServiceLaneEvent::Completion(Box::new(
                completion,
            )));
    }

    pub(super) fn drain(&self) -> usize {
        let events = std::mem::take(&mut *self.events.lock());
        let count = events.len();
        for event in events {
            match event {
                ServiceWorkerServiceLaneEvent::Completion(completion) => completion.complete(),
            }
        }
        count
    }

    pub(super) fn pending_count(&self) -> usize {
        self.events.lock().len()
    }
}

impl ServiceWorkerRuntimeService {
    pub(crate) fn drain_service_lane(&self) -> usize {
        self.service_lane().drain()
    }

    pub(super) fn enqueue_service_lane_completion(
        &self,
        completion: ServiceWorkerRuntimeCompletion,
    ) {
        self.service_lane().enqueue_completion(completion);
    }

    pub(crate) fn pending_service_lane_event_count(&self) -> usize {
        self.service_lane().pending_count()
    }
}
