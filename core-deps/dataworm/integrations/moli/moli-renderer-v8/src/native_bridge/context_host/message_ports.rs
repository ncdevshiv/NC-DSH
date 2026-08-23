use super::*;
use crate::context_bootstrap::MessagePortEventListenerId;
use crate::types::MessagePortId;
use moli_webidl_callback::WebIdlCallbackInterface;

impl JsContextHost {
    pub(crate) fn message_port_registry(&self) -> SharedMessagePortRegistry {
        self.message_port_registry.clone()
    }

    pub(crate) fn close_owned_message_ports(&mut self) {
        let port_ids = self
            .message_port_wrappers
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for port_id in port_ids {
            self.retire_message_port(port_id);
        }
    }

    pub(crate) fn register_message_port_wrapper(
        &mut self,
        scope: &mut v8::PinScope<'_, '_>,
        port_id: MessagePortId,
        port: v8::Local<'_, v8::Object>,
        identity: WindowExecutionContextIdentity,
    ) {
        let previous = self.message_port_wrappers.insert(
            port_id,
            MessagePortWrapperEntry {
                identity,
                context: v8::Global::new(scope, scope.get_current_context()),
                wrapper: v8::Global::new(scope, port),
                listeners:
                    crate::context_bootstrap::WindowMessagePortEventListenerRegistry::default(),
            },
        );
        if let Some(mut previous) = previous {
            tracing::warn!(
                port_id,
                previous_owner = ?previous.identity,
                current_owner = ?self
                    .message_port_wrappers
                    .get(&port_id)
                    .map(|entry| entry.identity),
                "replaced MessagePort wrapper without a transfer detach"
            );
            self.release_message_port_wrapper_callbacks(port_id, &mut previous);
        }
    }

    pub(crate) fn forget_message_port_wrapper(&mut self, port_id: MessagePortId) {
        if let Some(mut entry) = self.message_port_wrappers.remove(&port_id) {
            self.release_message_port_wrapper_callbacks(port_id, &mut entry);
        }
    }

    pub(crate) fn message_port_wrapper<'s>(
        &mut self,
        scope: &mut v8::PinScope<'s, '_>,
        port_id: MessagePortId,
    ) -> Option<v8::Local<'s, v8::Object>> {
        self.message_port_wrappers
            .get(&port_id)
            .map(|entry| v8::Local::new(scope, &entry.wrapper))
    }

    pub(crate) fn message_port_dispatch_target<'s>(
        &mut self,
        scope: &mut v8::PinScope<'s, '_>,
        port_id: MessagePortId,
    ) -> Option<(
        OwnerDispatchScope,
        RuntimeObservableContextToken,
        v8::Local<'s, v8::Context>,
        v8::Local<'s, v8::Object>,
    )> {
        let stale_owner = self
            .message_port_wrappers
            .get(&port_id)
            .map(|entry| entry.identity)
            .filter(|identity| !self.window_execution_context_identity_is_current(*identity));
        if let Some(identity) = stale_owner {
            self.retire_message_port(port_id);
            tracing::debug!(
                port_id,
                ?identity,
                "closed MessagePort for retired execution context"
            );
            return None;
        }
        let entry = self.message_port_wrappers.get(&port_id)?;
        Some((
            entry.identity.dispatch_scope(),
            entry.identity.realm_token(),
            v8::Local::new(scope, &entry.context),
            v8::Local::new(scope, &entry.wrapper),
        ))
    }

    pub(crate) fn message_port_execution_context_identity(
        &self,
        port_id: MessagePortId,
    ) -> Option<WindowExecutionContextIdentity> {
        self.message_port_wrappers
            .get(&port_id)
            .map(|entry| entry.identity)
    }

    /// Resolve a wrapper only after the Page arbiter has matched the exact
    /// attachment identity captured by the selected task.
    pub(crate) fn authorized_message_port_dispatch_target<'s>(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
        port_id: MessagePortId,
        expected: WindowExecutionContextIdentity,
    ) -> Option<(
        OwnerDispatchScope,
        RuntimeObservableContextToken,
        v8::Local<'s, v8::Context>,
        v8::Local<'s, v8::Object>,
    )> {
        let entry = self.message_port_wrappers.get(&port_id)?;
        if entry.identity != expected {
            return None;
        }
        Some((
            entry.identity.dispatch_scope(),
            entry.identity.realm_token(),
            v8::Local::new(scope, &entry.context),
            v8::Local::new(scope, &entry.wrapper),
        ))
    }

    pub(crate) fn retire_message_ports_for_execution_context_owner(
        &mut self,
        owner: WindowExecutionContextOwner,
    ) -> usize {
        let port_ids = self
            .message_port_wrappers
            .iter()
            .filter_map(|(port_id, entry)| (entry.identity.owner() == owner).then_some(*port_id))
            .collect::<Vec<_>>();
        let retired_count = port_ids.len();
        for port_id in port_ids {
            self.retire_message_port(port_id);
        }
        retired_count
    }

    pub(crate) fn retire_message_ports_for_context_token(
        &mut self,
        context_token: RuntimeObservableContextToken,
    ) -> usize {
        let port_ids = self
            .message_port_wrappers
            .iter()
            .filter_map(|(port_id, entry)| {
                (entry.identity.realm_token() == context_token).then_some(*port_id)
            })
            .collect::<Vec<_>>();
        let retired_count = port_ids.len();
        for port_id in port_ids {
            self.retire_message_port(port_id);
        }
        retired_count
    }

    pub(crate) fn retire_message_port(&mut self, port_id: MessagePortId) -> bool {
        let Some(mut entry) = self.message_port_wrappers.remove(&port_id) else {
            return false;
        };
        self.release_message_port_wrapper_callbacks(port_id, &mut entry);
        self.message_port_registry.close_message_port(port_id);
        true
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn register_message_port_event_listener(
        &mut self,
        scope: &mut v8::PinScope<'_, '_>,
        port_id: MessagePortId,
        event_type: String,
        order: f64,
        callback: WebIdlCallbackInterface,
        options: crate::webidl::EventListenerOptions,
    ) -> Option<MessagePortEventListenerId> {
        let entry = self.message_port_wrappers.get(&port_id)?;
        let callback_value = callback.value(scope);
        let Ok(callback_object) = v8::Local::<v8::Object>::try_from(callback_value) else {
            return None;
        };
        if entry
            .listeners
            .callback_ids(&event_type, options.capture)
            .into_iter()
            .any(|callback_id| self.event_callback_matches(scope, callback_id, callback_object))
        {
            return None;
        }

        let callback_id = self.register_webidl_event_callback(scope, callback);
        let Some(entry) = self.message_port_wrappers.get_mut(&port_id) else {
            self.release_event_callback(callback_id);
            return None;
        };
        Some(entry.listeners.insert(
            event_type,
            order,
            callback_id,
            options.capture,
            options.once,
            options.passive.unwrap_or(false),
        ))
    }

    pub(crate) fn remove_message_port_event_listener(
        &mut self,
        scope: &mut v8::PinScope<'_, '_>,
        port_id: MessagePortId,
        event_type: &str,
        callback: &WebIdlCallbackInterface,
        capture: bool,
    ) -> bool {
        let Some(entry) = self.message_port_wrappers.get(&port_id) else {
            return false;
        };
        let callback_value = callback.value(scope);
        let Ok(callback_object) = v8::Local::<v8::Object>::try_from(callback_value) else {
            return false;
        };
        let Some(callback_id) = entry
            .listeners
            .callback_ids(event_type, capture)
            .into_iter()
            .find(|callback_id| self.event_callback_matches(scope, *callback_id, callback_object))
        else {
            return false;
        };
        let removed_listener_id = self
            .message_port_wrappers
            .get_mut(&port_id)
            .and_then(|entry| {
                entry
                    .listeners
                    .remove_matching(event_type, callback_id, capture)
            });
        let Some(listener_id) = removed_listener_id else {
            return false;
        };
        self.unregister_abort_message_port_listener(port_id, listener_id);
        self.release_event_callback(callback_id);
        true
    }

    pub(crate) fn remove_message_port_event_listener_by_id(
        &mut self,
        port_id: MessagePortId,
        listener_id: MessagePortEventListenerId,
    ) -> bool {
        let Some(callback_id) = self.take_message_port_event_listener_by_id(port_id, listener_id)
        else {
            return false;
        };
        self.unregister_abort_message_port_listener(port_id, listener_id);
        self.release_event_callback(callback_id);
        true
    }

    pub(in crate::native_bridge) fn remove_message_port_event_listener_after_signal_abort(
        &mut self,
        port_id: MessagePortId,
        listener_id: MessagePortEventListenerId,
    ) {
        let Some(callback_id) = self.take_message_port_event_listener_by_id(port_id, listener_id)
        else {
            return;
        };
        // The abort owner already consumed this exact signal link. Do not
        // re-enter AbortStore through the host while AbortStore::abort_signal
        // holds its exclusive execution boundary.
        self.release_event_callback(callback_id);
    }

    fn take_message_port_event_listener_by_id(
        &mut self,
        port_id: MessagePortId,
        listener_id: MessagePortEventListenerId,
    ) -> Option<EventCallbackId> {
        self.message_port_wrappers
            .get_mut(&port_id)?
            .listeners
            .remove_listener_id(listener_id)
    }

    pub(crate) fn message_port_event_listener_snapshots(
        &self,
        port_id: MessagePortId,
        event_type: &str,
    ) -> Vec<crate::context_bootstrap::MessagePortEventListenerSnapshot> {
        self.message_port_wrappers
            .get(&port_id)
            .map(|entry| entry.listeners.snapshots(event_type))
            .unwrap_or_default()
    }

    pub(crate) fn claim_message_port_event_listener(
        &mut self,
        scope: &mut v8::PinScope<'_, '_>,
        port_id: MessagePortId,
        listener_id: crate::context_bootstrap::MessagePortEventListenerId,
    ) -> Option<crate::context_bootstrap::PreparedMessagePortEventListener> {
        let claimed = self
            .message_port_wrappers
            .get_mut(&port_id)?
            .listeners
            .claim(listener_id)?;
        let prepared = self.prepare_event_callback(scope, claimed.callback_id);
        if claimed.removed_once || prepared.is_none() {
            if !claimed.removed_once {
                let _ = self
                    .message_port_wrappers
                    .get_mut(&port_id)
                    .and_then(|entry| entry.listeners.remove_listener_id(listener_id));
            }
            self.unregister_abort_message_port_listener(port_id, listener_id);
            self.release_event_callback(claimed.callback_id);
        }
        prepared.map(
            |callback| crate::context_bootstrap::PreparedMessagePortEventListener {
                callback:
                    crate::context_bootstrap::PreparedMessagePortEventListenerCallback::Window(
                        callback,
                    ),
                passive: claimed.passive,
            },
        )
    }

    pub(in crate::native_bridge::context_host) fn remove_message_port_event_callbacks(
        &mut self,
        callback_ids: &std::collections::HashSet<EventCallbackId>,
    ) {
        let mut removed_listener_ids = Vec::new();
        for (port_id, entry) in &mut self.message_port_wrappers {
            for listener_id in entry.listeners.remove_callback_ids(callback_ids) {
                removed_listener_ids.push((*port_id, listener_id));
            }
        }
        for (port_id, listener_id) in removed_listener_ids {
            self.unregister_abort_message_port_listener(port_id, listener_id);
        }
    }

    fn release_message_port_wrapper_callbacks(
        &mut self,
        port_id: MessagePortId,
        entry: &mut MessagePortWrapperEntry,
    ) {
        for (listener_id, callback_id) in entry.listeners.take_listener_callbacks() {
            self.unregister_abort_message_port_listener(port_id, listener_id);
            self.release_event_callback(callback_id);
        }
    }

    #[cfg(test)]
    pub(crate) fn message_port_execution_context_owners_for_test(
        &self,
    ) -> Vec<(
        MessagePortId,
        WindowExecutionContextOwner,
        RuntimeObservableContextToken,
    )> {
        let mut owners = self
            .message_port_wrappers
            .iter()
            .map(|(port_id, entry)| {
                (
                    *port_id,
                    entry.identity.owner(),
                    entry.identity.realm_token(),
                )
            })
            .collect::<Vec<_>>();
        owners.sort_by_key(|(port_id, _, _)| *port_id);
        owners
    }
}
