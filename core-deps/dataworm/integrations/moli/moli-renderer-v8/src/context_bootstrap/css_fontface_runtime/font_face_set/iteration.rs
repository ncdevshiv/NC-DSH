use super::*;
use crate::util::serialize_v8_iter_array;
use crate::webidl_iterator::{
    SnapshotWebIdlIteratorKind, invoke_webidl_collection_for_each_callback,
    new_snapshot_webidl_iterator, prepare_webidl_collection_for_each_callback,
};

pub(in crate::context_bootstrap) fn font_face_set_keys_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    apply_pending_stylesheet_source_css_projections(scope);
    let mut values = Vec::new();
    if let Some(faces) = font_face_set_faces_array(scope, args.this()) {
        for index in 0..faces.length() {
            let Some(face) = faces.get_index(scope, index) else {
                continue;
            };
            values.push(face);
        }
    }
    let array = serialize_v8_iter_array(scope, values).unwrap_or_else(|| v8::Array::new(scope, 0));
    if let Some(iterator) =
        new_snapshot_webidl_iterator(scope, array, SnapshotWebIdlIteratorKind::FontFaceSet)
    {
        rv.set(iterator.into());
    } else {
        rv.set_undefined();
    }
}

pub(in crate::context_bootstrap) fn font_face_set_values_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue<'_, v8::Value>,
) {
    font_face_set_keys_callback(scope, args, rv);
}

pub(in crate::context_bootstrap) fn font_face_set_entries_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    apply_pending_stylesheet_source_css_projections(scope);
    let mut entries = Vec::new();
    if let Some(faces) = font_face_set_faces_array(scope, args.this()) {
        for index in 0..faces.length() {
            let Some(face) = faces.get_index(scope, index) else {
                continue;
            };
            entries.push((face, face));
        }
    }
    let array = serialize_v8_iter_array(scope, entries).unwrap_or_else(|| v8::Array::new(scope, 0));
    if let Some(iterator) =
        new_snapshot_webidl_iterator(scope, array, SnapshotWebIdlIteratorKind::FontFaceSet)
    {
        rv.set(iterator.into());
    } else {
        rv.set_undefined();
    }
}

pub(in crate::context_bootstrap) fn font_face_set_for_each_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    apply_pending_stylesheet_source_css_projections(scope);
    let Some(callback) =
        prepare_webidl_collection_for_each_callback(scope, args.get(0), "FontFaceSet forEach")
    else {
        return;
    };
    let this_arg = if args.length() >= 2 && !args.get(1).is_undefined() {
        args.get(1)
    } else {
        v8::undefined(scope).into()
    };
    if let Some(faces) = font_face_set_faces_array(scope, args.this()) {
        for index in 0..faces.length() {
            let Some(face) = faces.get_index(scope, index) else {
                continue;
            };
            if invoke_webidl_collection_for_each_callback(
                scope,
                &callback,
                this_arg,
                face,
                face,
                args.this(),
            )
            .is_none()
            {
                return;
            }
        }
    }
    rv.set_undefined();
}
