use super::*;

#[derive(Clone, Copy)]
enum RangeAttribute {
    StartContainer,
    StartOffset,
    EndContainer,
    EndOffset,
    Collapsed,
    CommonAncestorContainer,
}

const RANGE_ATTRIBUTES: &[RangeAttribute] = &[
    RangeAttribute::StartContainer,
    RangeAttribute::StartOffset,
    RangeAttribute::EndContainer,
    RangeAttribute::EndOffset,
    RangeAttribute::Collapsed,
    RangeAttribute::CommonAncestorContainer,
];

pub(super) fn range_attribute_getter_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(attribute) = callback_data_item(scope, &args, RANGE_ATTRIBUTES, "Range attributes")
    else {
        rv.set_undefined();
        return;
    };
    rv.set(range_attribute_value(scope, args.this(), attribute));
}

fn range_attribute_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    range: v8::Local<'s, v8::Object>,
    attribute: RangeAttribute,
) -> v8::Local<'s, v8::Value> {
    match attribute {
        RangeAttribute::StartContainer => {
            range_boundary_container_object(scope, range, RangeBoundarySide::Start)
                .map(|container| container.into())
                .unwrap_or_else(|| v8::undefined(scope).into())
        }
        RangeAttribute::StartOffset => {
            let offset = range_boundary_offset(scope, range, RangeBoundarySide::Start);
            v8::Number::new(scope, offset).into()
        }
        RangeAttribute::EndContainer => {
            range_boundary_container_object(scope, range, RangeBoundarySide::End)
                .map(|container| container.into())
                .unwrap_or_else(|| v8::undefined(scope).into())
        }
        RangeAttribute::EndOffset => {
            let offset = range_boundary_offset(scope, range, RangeBoundarySide::End);
            v8::Number::new(scope, offset).into()
        }
        RangeAttribute::Collapsed => {
            let collapsed = range_is_collapsed(scope, range);
            v8::Boolean::new(scope, collapsed).into()
        }
        RangeAttribute::CommonAncestorContainer => range_common_ancestor_container(scope, range)
            .map(|container| container.into())
            .unwrap_or_else(|| v8::undefined(scope).into()),
    }
}
