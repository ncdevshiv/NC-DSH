use super::*;
use crate::native_bridge::throw_dom_exception;

pub(in crate::context_bootstrap) fn range_insert_node_at_boundary<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    range: v8::Local<'s, v8::Object>,
    container: v8::Local<'s, v8::Object>,
    offset: u32,
    new_node: v8::Local<'s, v8::Object>,
) -> Option<()> {
    let container_handle = node_handle_for_tree_op(scope, container)?;
    let (parent, _) = range_insert_target(scope, container_handle, offset, None)?;
    let document_handle = document_handle_for_node_handle_or_self(scope, parent);
    let Some(new_node_handle) = node_handle_for_range_insert(scope, document_handle, new_node)
    else {
        crate::util::throw_type_error(scope, "Range.insertNode requires a Node");
        return None;
    };
    range_insert_node_handle_at_boundary(scope, range, container_handle, offset, new_node_handle)
}

pub(in crate::context_bootstrap::range_algorithms) fn range_insert_node_handle_at_boundary<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    range: v8::Local<'s, v8::Object>,
    container_handle: DomHandle,
    offset: u32,
    new_node_handle: DomHandle,
) -> Option<()> {
    let (parent, mut reference) =
        range_insert_target(scope, container_handle, offset, Some(new_node_handle))?;
    let container_type = node_type_for_handle(scope, container_handle)?;
    let start_is_character_data = matches!(container_type, NodeType::Text | NodeType::CDataSection);

    validate_pre_insert_internal_handle(scope, parent, new_node_handle, reference, &[])?;

    if start_is_character_data {
        reference = Some(split_text_internal_handle(scope, container_handle, offset)?);
    }
    if reference == Some(new_node_handle) {
        reference = next_sibling_handle(scope, new_node_handle);
    }

    let new_offset =
        prospective_child_index_after_removal_handle(scope, parent, reference, new_node_handle)?
            .saturating_add(range_inserted_node_length_handle(scope, new_node_handle)?);

    range_insert_move_internal_handle(scope, parent, new_node_handle, reference).or_else(|| {
        throw_range_insert_hierarchy_error(scope);
        None
    })?;

    let collapsed_after_insert = native_range_record_is_collapsed(scope, range)
        .unwrap_or_else(|| range_is_collapsed(scope, range));
    if collapsed_after_insert {
        let parent = node_wrapper_for_handle_prefer_paired(scope, parent)?;
        set_range_boundary(scope, range, RangeBoundarySide::End, parent, new_offset);
    }
    Some(())
}

fn native_range_record_is_collapsed<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    range: v8::Local<'s, v8::Object>,
) -> Option<bool> {
    let boundaries = native_range_boundary_handles(scope, range)?;
    Some(
        boundaries.start.container == boundaries.end.container
            && boundaries.start.offset == boundaries.end.offset,
    )
}

fn range_insert_target(
    scope: &mut v8::PinScope<'_, '_>,
    container_handle: DomHandle,
    offset: u32,
    new_node_handle: Option<DomHandle>,
) -> Option<(DomHandle, Option<DomHandle>)> {
    let container_type = node_type_for_handle(scope, container_handle)?;
    if matches!(
        container_type,
        NodeType::ProcessingInstruction | NodeType::Comment
    ) || new_node_handle == Some(container_handle)
    {
        throw_range_insert_hierarchy_error(scope);
        return None;
    }

    let start_is_character_data = matches!(container_type, NodeType::Text | NodeType::CDataSection);
    if start_is_character_data && parent_handle(scope, container_handle).is_none() {
        throw_range_insert_hierarchy_error(scope);
        return None;
    }

    let reference = if start_is_character_data {
        Some(container_handle)
    } else {
        child_handle_at_offset_optional(scope, container_handle, offset)?
    };
    let parent = match reference {
        Some(reference) => parent_handle(scope, reference),
        None => Some(container_handle),
    };
    let Some(parent) = parent else {
        throw_range_insert_hierarchy_error(scope);
        return None;
    };
    Some((parent, reference))
}

fn throw_range_insert_hierarchy_error(scope: &mut v8::PinScope<'_, '_>) {
    throw_dom_exception(scope, "HierarchyRequestError", 3, "Hierarchy Error");
}

pub(in crate::context_bootstrap::range_algorithms) fn range_select_node_handle<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    range: v8::Local<'s, v8::Object>,
    node: DomHandle,
) -> Option<()> {
    let parent = parent_handle(scope, node)?;
    let index = child_index_handle(scope, parent, node)?;
    let parent = node_wrapper_for_handle_prefer_paired(scope, parent)?;
    set_range_boundary(scope, range, RangeBoundarySide::Start, parent, index);
    set_range_boundary(scope, range, RangeBoundarySide::End, parent, index + 1);
    Some(())
}
