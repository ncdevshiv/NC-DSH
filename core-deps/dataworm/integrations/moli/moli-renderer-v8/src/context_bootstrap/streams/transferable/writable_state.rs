use super::*;
use moli_streams::transfer::WritableTransferSnapshot;

const STREAM_SLOT: &str = "__moliWritableStreamTransferStream";
const PORT_SLOT: &str = "__moliWritableStreamTransferPort";
const ACTIVE_SLOT: &str = "__moliWritableStreamTransferActive";
const PULL_DEMAND_SLOT: &str = "__moliWritableStreamTransferPullDemand";
const PENDING_WRITE_SLOT: &str = "__moliWritableStreamTransferPendingWrite";
const PENDING_WRITE_CHUNK_INDEX: u32 = 0;
const PENDING_WRITE_PROMISE_INDEX: u32 = 1;

pub(super) fn create<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    stream: v8::Local<'s, v8::Object>,
    port: v8::Local<'s, v8::Object>,
) -> v8::Local<'s, v8::Object> {
    let initial = WritableTransferSnapshot::initial();
    let state = v8::Object::new(scope);
    set_stream_slot_value(scope, state, STREAM_SLOT, stream.into());
    set_stream_slot_value(scope, state, PORT_SLOT, port.into());
    set_stream_slot_bool(scope, state, ACTIVE_SLOT, initial.active());
    set_pull_demand(scope, state, initial.pull_demand());
    set_stream_slot_value(scope, state, PENDING_WRITE_SLOT, v8::null(scope).into());
    state
}

pub(super) fn stream<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    state: v8::Local<'s, v8::Object>,
) -> Option<v8::Local<'s, v8::Object>> {
    stream_slot_object(scope, state, STREAM_SLOT)
}

pub(super) fn port<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    state: v8::Local<'s, v8::Object>,
) -> Option<v8::Local<'s, v8::Object>> {
    stream_slot_object(scope, state, PORT_SLOT)
}

pub(super) fn snapshot<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    state: v8::Local<'s, v8::Object>,
) -> WritableTransferSnapshot {
    WritableTransferSnapshot::new(
        stream_slot_bool(scope, state, ACTIVE_SLOT).unwrap_or(false),
        stream_slot_number(scope, state, PULL_DEMAND_SLOT)
            .map(|value| value as u32)
            .unwrap_or(0),
        pending_write(scope, state).is_some(),
    )
}

pub(super) fn set_pull_demand<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    state: v8::Local<'s, v8::Object>,
    demand: u32,
) {
    set_stream_slot_value(
        scope,
        state,
        PULL_DEMAND_SLOT,
        v8::Integer::new_from_unsigned(scope, demand).into(),
    );
}

pub(super) fn stage_write<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    state: v8::Local<'s, v8::Object>,
    chunk: v8::Local<'s, v8::Value>,
    pending: v8::Local<'s, v8::Object>,
) {
    assert!(
        pending_write(scope, state).is_none(),
        "a transferred writable endpoint must serialize sink writes"
    );
    let entry = v8::Array::new(scope, 2);
    let _ = entry.set_index(scope, PENDING_WRITE_CHUNK_INDEX, chunk);
    let _ = entry.set_index(scope, PENDING_WRITE_PROMISE_INDEX, pending.into());
    set_stream_slot_value(scope, state, PENDING_WRITE_SLOT, entry.into());
}

pub(super) fn take_pending_write<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    state: v8::Local<'s, v8::Object>,
) -> Option<(v8::Local<'s, v8::Value>, v8::Local<'s, v8::Object>)> {
    let value = pending_write(scope, state)?;
    let chunk = value.get_index(scope, PENDING_WRITE_CHUNK_INDEX)?;
    let pending = value
        .get_index(scope, PENDING_WRITE_PROMISE_INDEX)
        .and_then(|value| v8::Local::<v8::Object>::try_from(value).ok())?;
    set_stream_slot_value(scope, state, PENDING_WRITE_SLOT, v8::null(scope).into());
    Some((chunk, pending))
}

fn pending_write<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    state: v8::Local<'s, v8::Object>,
) -> Option<v8::Local<'s, v8::Array>> {
    stream_slot_array(scope, state, PENDING_WRITE_SLOT)
}

pub(super) fn finish<'s>(scope: &mut v8::PinScope<'s, '_>, state: v8::Local<'s, v8::Object>) {
    if !snapshot(scope, state).active() {
        return;
    }
    set_stream_slot_bool(scope, state, ACTIVE_SLOT, false);
    let Some(port) = port(scope, state) else {
        return;
    };
    if let Some(port_id) = message_port_id_from_object(scope, port)
        && let Some(registry) = current_message_port_registry(scope)
    {
        registry.close_message_port(port_id);
    }
    close_message_port_object(scope, port);
}
