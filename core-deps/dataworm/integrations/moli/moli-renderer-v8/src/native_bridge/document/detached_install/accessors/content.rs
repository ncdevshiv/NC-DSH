use super::super::super::{
    with_detached_tree_reaction_scope,
    write_detached_native_text_content_appending_to_current_reaction_queue,
};

pub(in crate::native_bridge) fn set_detached_text_replacement_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    object: v8::Local<'s, v8::Object>,
    value: &str,
) -> Option<bool> {
    with_detached_tree_reaction_scope(scope, |scope| {
        write_detached_native_text_content_appending_to_current_reaction_queue(scope, object, value)
    })
}
