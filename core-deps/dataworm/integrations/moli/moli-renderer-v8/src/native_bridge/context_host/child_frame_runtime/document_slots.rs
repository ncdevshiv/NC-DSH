use crate::context_bootstrap::{
    sync_document_location_runtime_state_from_window, sync_selection_owner_document_for_window,
};
use crate::native_bridge::document::set_document_associated_window;

pub(in crate::native_bridge::context_host::child_frame_runtime) fn sync_child_document_window_slots<
    's,
>(
    scope: &mut v8::PinScope<'s, '_>,
    document: v8::Local<'s, v8::Object>,
    window: v8::Local<'s, v8::Object>,
    sync_location_state: bool,
) {
    set_document_associated_window(scope, document, window);
    sync_selection_owner_document_for_window(scope, window, document);
    if sync_location_state {
        sync_document_location_runtime_state_from_window(scope, document, window);
    }
}
