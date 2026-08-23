use std::collections::HashSet;

use moli_webidl_callback::{PreparedWebIdlCallbackInterface, WebIdlCallbackInterface};

use crate::native_bridge::{EventCallbackId, PreparedEventCallback};
use crate::types::MessagePortId;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct MessagePortEventListenerId(u64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MessagePortEventListenerSnapshot {
    pub(crate) id: MessagePortEventListenerId,
    pub(crate) order: f64,
}

pub(crate) enum PreparedMessagePortEventListenerCallback {
    Window(PreparedEventCallback),
    Worker(PreparedWebIdlCallbackInterface),
}

pub(crate) struct PreparedMessagePortEventListener {
    pub(crate) callback: PreparedMessagePortEventListenerCallback,
    pub(crate) passive: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct ClaimedWindowMessagePortEventListener {
    pub(crate) callback_id: EventCallbackId,
    pub(crate) passive: bool,
    pub(crate) removed_once: bool,
}

struct MessagePortEventListenerRecord<C> {
    id: MessagePortEventListenerId,
    event_type: String,
    order: f64,
    callback: C,
    capture: bool,
    once: bool,
    passive: bool,
}

struct MessagePortEventListenerRecords<C> {
    next_id: u64,
    records: Vec<MessagePortEventListenerRecord<C>>,
}

impl<C> Default for MessagePortEventListenerRecords<C> {
    fn default() -> Self {
        Self {
            next_id: 0,
            records: Vec::new(),
        }
    }
}

impl<C> MessagePortEventListenerRecords<C> {
    fn allocate_id(&mut self) -> MessagePortEventListenerId {
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("MessagePort EventListener id space exhausted");
        MessagePortEventListenerId(self.next_id)
    }

    fn snapshots(&self, event_type: &str) -> Vec<MessagePortEventListenerSnapshot> {
        self.records
            .iter()
            .filter(|record| record.event_type == event_type)
            .map(|record| MessagePortEventListenerSnapshot {
                id: record.id,
                order: record.order,
            })
            .collect()
    }

    fn remove_by_id(
        &mut self,
        listener_id: MessagePortEventListenerId,
    ) -> Option<MessagePortEventListenerRecord<C>> {
        let index = self
            .records
            .iter()
            .position(|record| record.id == listener_id)?;
        Some(self.records.remove(index))
    }
}

/// Callback residence for one exact MessagePort wrapper.
///
/// Window wrappers store callback ids owned by the central exact-execution-
/// context callback registry. A distinct type prevents Window code from
/// accidentally storing worker-local callback roots or invoking worker claim
/// semantics.
#[derive(Default)]
pub(crate) struct WindowMessagePortEventListenerRegistry {
    records: MessagePortEventListenerRecords<EventCallbackId>,
}

impl WindowMessagePortEventListenerRegistry {
    pub(crate) fn snapshots(&self, event_type: &str) -> Vec<MessagePortEventListenerSnapshot> {
        self.records.snapshots(event_type)
    }

    pub(crate) fn callback_ids(&self, event_type: &str, capture: bool) -> Vec<EventCallbackId> {
        self.records
            .records
            .iter()
            .filter(|record| record.event_type == event_type && record.capture == capture)
            .map(|record| record.callback)
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn insert(
        &mut self,
        event_type: String,
        order: f64,
        callback_id: EventCallbackId,
        capture: bool,
        once: bool,
        passive: bool,
    ) -> MessagePortEventListenerId {
        let id = self.records.allocate_id();
        self.records.records.push(MessagePortEventListenerRecord {
            id,
            event_type,
            order,
            callback: callback_id,
            capture,
            once,
            passive,
        });
        id
    }

    pub(crate) fn remove_matching(
        &mut self,
        event_type: &str,
        callback_id: EventCallbackId,
        capture: bool,
    ) -> Option<MessagePortEventListenerId> {
        let index = self.records.records.iter().position(|record| {
            record.event_type == event_type
                && record.capture == capture
                && record.callback == callback_id
        })?;
        Some(self.records.records.remove(index).id)
    }

    pub(crate) fn claim(
        &mut self,
        listener_id: MessagePortEventListenerId,
    ) -> Option<ClaimedWindowMessagePortEventListener> {
        let record = self
            .records
            .records
            .iter()
            .find(|record| record.id == listener_id)?;
        let claimed = ClaimedWindowMessagePortEventListener {
            callback_id: record.callback,
            passive: record.passive,
            removed_once: record.once,
        };
        if record.once {
            self.records.remove_by_id(listener_id);
        }
        Some(claimed)
    }

    pub(crate) fn remove_listener_id(
        &mut self,
        listener_id: MessagePortEventListenerId,
    ) -> Option<EventCallbackId> {
        self.records
            .remove_by_id(listener_id)
            .map(|record| record.callback)
    }

    pub(crate) fn remove_callback_ids(
        &mut self,
        callback_ids: &HashSet<EventCallbackId>,
    ) -> Vec<MessagePortEventListenerId> {
        let mut removed = Vec::new();
        self.records.records.retain(|record| {
            let should_remove = callback_ids.contains(&record.callback);
            if should_remove {
                removed.push(record.id);
            }
            !should_remove
        });
        removed
    }

    pub(crate) fn take_listener_callbacks(
        &mut self,
    ) -> Vec<(MessagePortEventListenerId, EventCallbackId)> {
        std::mem::take(&mut self.records.records)
            .into_iter()
            .map(|record| (record.id, record.callback))
            .collect()
    }
}

/// Callback residence for one exact worker-owned MessagePort wrapper.
///
/// The typed callbacks and their captured contexts retire with the worker run,
/// so this store does not import Window execution-context ids or the central
/// Window callback registry.
#[derive(Default)]
pub(crate) struct WorkerMessagePortEventListenerRegistry {
    records: MessagePortEventListenerRecords<WebIdlCallbackInterface>,
}

impl WorkerMessagePortEventListenerRegistry {
    pub(crate) fn snapshots(&self, event_type: &str) -> Vec<MessagePortEventListenerSnapshot> {
        self.records.snapshots(event_type)
    }

    pub(crate) fn remove_listener_id(&mut self, listener_id: MessagePortEventListenerId) -> bool {
        self.records.remove_by_id(listener_id).is_some()
    }

    pub(crate) fn take_listener_ids(&mut self) -> Vec<MessagePortEventListenerId> {
        std::mem::take(&mut self.records.records)
            .into_iter()
            .map(|record| record.id)
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn register(
        &mut self,
        scope: &mut v8::PinScope<'_, '_>,
        event_type: String,
        order: f64,
        callback: WebIdlCallbackInterface,
        capture: bool,
        once: bool,
        passive: bool,
    ) -> Option<MessagePortEventListenerId> {
        let callback_value = callback.value(scope);
        let Ok(callback_object) = v8::Local::<v8::Object>::try_from(callback_value) else {
            return None;
        };
        if self.records.records.iter().any(|record| {
            record.event_type == event_type
                && record.capture == capture
                && record.callback.matches(scope, callback_object)
        }) {
            return None;
        }
        let id = self.records.allocate_id();
        self.records.records.push(MessagePortEventListenerRecord {
            id,
            event_type,
            order,
            callback,
            capture,
            once,
            passive,
        });
        Some(id)
    }

    pub(crate) fn remove_matching(
        &mut self,
        scope: &mut v8::PinScope<'_, '_>,
        event_type: &str,
        callback: &WebIdlCallbackInterface,
        capture: bool,
    ) -> Option<MessagePortEventListenerId> {
        let callback_value = callback.value(scope);
        let Ok(callback_object) = v8::Local::<v8::Object>::try_from(callback_value) else {
            return None;
        };
        let index = self.records.records.iter().position(|record| {
            record.event_type == event_type
                && record.capture == capture
                && record.callback.matches(scope, callback_object)
        })?;
        Some(self.records.records.remove(index).id)
    }

    pub(crate) fn claim(
        &mut self,
        scope: &mut v8::PinScope<'_, '_>,
        listener_id: MessagePortEventListenerId,
    ) -> Option<(PreparedMessagePortEventListener, bool)> {
        let record = self
            .records
            .records
            .iter()
            .find(|record| record.id == listener_id)?;
        let callback = record.callback.prepare(scope);
        let passive = record.passive;
        let removed_once = record.once;
        if removed_once {
            self.records.remove_by_id(listener_id);
        }
        Some((
            PreparedMessagePortEventListener {
                callback: PreparedMessagePortEventListenerCallback::Worker(callback),
                passive,
            },
            removed_once,
        ))
    }
}

pub(in crate::context_bootstrap::message_ports) fn register_message_port_event_listener<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    port: v8::Local<'s, v8::Object>,
    event_type: String,
    callback: WebIdlCallbackInterface,
    options: crate::webidl::EventListenerOptions,
) -> Option<MessagePortEventListenerId> {
    let port_id = super::message_port_id_from_object(scope, port)?;
    let order = super::next_message_port_listener_order(scope, port);
    if let Some(host_ptr) = crate::util::context_host_ptr_from_global_bridge(scope) {
        return unsafe { &mut *host_ptr }.register_message_port_event_listener(
            scope, port_id, event_type, order, callback, options,
        );
    }
    crate::worker::register_worker_message_port_event_listener(
        scope, port_id, event_type, order, callback, options,
    )
}

pub(in crate::context_bootstrap::message_ports) fn remove_message_port_event_listener<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    port: v8::Local<'s, v8::Object>,
    event_type: &str,
    callback: &WebIdlCallbackInterface,
    capture: bool,
) -> bool {
    let Some(port_id) = super::message_port_id_from_object(scope, port) else {
        return false;
    };
    if let Some(host_ptr) = crate::util::context_host_ptr_from_global_bridge(scope) {
        return unsafe { &mut *host_ptr }
            .remove_message_port_event_listener(scope, port_id, event_type, callback, capture);
    }
    crate::worker::remove_worker_message_port_event_listener(
        scope, port_id, event_type, callback, capture,
    )
}

pub(in crate::context_bootstrap::message_ports) fn remove_message_port_event_listener_by_id(
    scope: &mut v8::PinScope<'_, '_>,
    port_id: MessagePortId,
    listener_id: MessagePortEventListenerId,
) -> bool {
    if let Some(host_ptr) = crate::util::context_host_ptr_from_global_bridge(scope) {
        return unsafe { &mut *host_ptr }
            .remove_message_port_event_listener_by_id(port_id, listener_id);
    }
    crate::worker::remove_worker_message_port_event_listener_by_id(scope, port_id, listener_id)
}

pub(in crate::context_bootstrap::message_ports) fn message_port_event_listener_snapshots(
    scope: &mut v8::PinScope<'_, '_>,
    port_id: MessagePortId,
    event_type: &str,
) -> Vec<MessagePortEventListenerSnapshot> {
    if let Some(host_ptr) = crate::util::context_host_ptr_from_global_bridge(scope) {
        return unsafe { &*host_ptr }.message_port_event_listener_snapshots(port_id, event_type);
    }
    crate::worker::worker_message_port_event_listener_snapshots(scope, port_id, event_type)
}

pub(in crate::context_bootstrap::message_ports) fn claim_message_port_event_listener(
    scope: &mut v8::PinScope<'_, '_>,
    port_id: MessagePortId,
    listener_id: MessagePortEventListenerId,
) -> Option<PreparedMessagePortEventListener> {
    if let Some(host_ptr) = crate::util::context_host_ptr_from_global_bridge(scope) {
        return unsafe { &mut *host_ptr }.claim_message_port_event_listener(
            scope,
            port_id,
            listener_id,
        );
    }
    crate::worker::claim_worker_message_port_event_listener(scope, port_id, listener_id)
}
