use crate::structured_clone::V8StructuredClonePayload;

use super::{
    events::{ServiceWorkerNotificationAction, ServiceWorkerNotificationMetadata},
    ids::{ServiceWorkerEventId, ServiceWorkerRegistrationId},
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct ServiceWorkerSyncRegistrationRecord {
    pub(super) failed_attempts: u8,
    pub(super) dispatch_state: ServiceWorkerTagDispatchState,
}

impl ServiceWorkerSyncRegistrationRecord {
    pub(super) fn active(event_id: ServiceWorkerEventId) -> Self {
        Self {
            failed_attempts: 0,
            dispatch_state: ServiceWorkerTagDispatchState::Active {
                event_id,
                refire_after_finish: false,
            },
        }
    }

    pub(super) fn mark_active(&mut self, event_id: ServiceWorkerEventId) {
        self.dispatch_state = ServiceWorkerTagDispatchState::Active {
            event_id,
            refire_after_finish: false,
        };
    }

    pub(super) fn mark_refire_after_finish_if_active(&mut self) -> bool {
        match &mut self.dispatch_state {
            ServiceWorkerTagDispatchState::Idle => false,
            ServiceWorkerTagDispatchState::Active {
                refire_after_finish,
                ..
            } => {
                *refire_after_finish = true;
                true
            }
        }
    }

    pub(super) fn is_idle(&self) -> bool {
        matches!(self.dispatch_state, ServiceWorkerTagDispatchState::Idle)
    }

    pub(super) fn finish_active_dispatch(
        &mut self,
        event_id: ServiceWorkerEventId,
    ) -> Option<bool> {
        match &mut self.dispatch_state {
            ServiceWorkerTagDispatchState::Active {
                event_id: active_event_id,
                refire_after_finish,
            } if *active_event_id == event_id => {
                let refire_after_finish = *refire_after_finish;
                self.dispatch_state = ServiceWorkerTagDispatchState::Idle;
                Some(refire_after_finish)
            }
            ServiceWorkerTagDispatchState::Idle | ServiceWorkerTagDispatchState::Active { .. } => {
                None
            }
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) enum ServiceWorkerTagDispatchState {
    #[default]
    Idle,
    Active {
        event_id: ServiceWorkerEventId,
        refire_after_finish: bool,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct ServiceWorkerPeriodicSyncRegistrationRecord {
    pub(super) min_interval_ms: u64,
    pub(super) dispatch_state: ServiceWorkerTagDispatchState,
}

impl ServiceWorkerPeriodicSyncRegistrationRecord {
    pub(super) fn new(min_interval_ms: u64) -> Self {
        Self {
            min_interval_ms,
            dispatch_state: ServiceWorkerTagDispatchState::Idle,
        }
    }

    pub(super) fn update_min_interval(&mut self, min_interval_ms: u64) {
        self.min_interval_ms = min_interval_ms;
    }

    pub(super) fn mark_active(&mut self, event_id: ServiceWorkerEventId) {
        self.dispatch_state = ServiceWorkerTagDispatchState::Active {
            event_id,
            refire_after_finish: false,
        };
    }

    pub(super) fn mark_refire_after_finish_if_active(&mut self) -> bool {
        match &mut self.dispatch_state {
            ServiceWorkerTagDispatchState::Idle => false,
            ServiceWorkerTagDispatchState::Active {
                refire_after_finish,
                ..
            } => {
                *refire_after_finish = true;
                true
            }
        }
    }

    pub(super) fn finish_active_dispatch(
        &mut self,
        event_id: ServiceWorkerEventId,
    ) -> Option<bool> {
        match &mut self.dispatch_state {
            ServiceWorkerTagDispatchState::Active {
                event_id: active_event_id,
                refire_after_finish,
            } if *active_event_id == event_id => {
                let refire_after_finish = *refire_after_finish;
                self.dispatch_state = ServiceWorkerTagDispatchState::Idle;
                Some(refire_after_finish)
            }
            ServiceWorkerTagDispatchState::Idle | ServiceWorkerTagDispatchState::Active { .. } => {
                None
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct ServiceWorkerNotificationRecord {
    pub(super) id: u64,
    pub(super) registration_id: ServiceWorkerRegistrationId,
    pub(super) title: String,
    pub(super) tag: String,
    pub(super) metadata: ServiceWorkerNotificationMetadata,
    pub(super) actions: Vec<ServiceWorkerNotificationAction>,
    pub(super) data: V8StructuredClonePayload,
}
