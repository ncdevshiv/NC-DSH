use super::*;
use crate::document_runtime::DomHandle;
use crate::native_bridge::callback_value_dom_handle;

pub(in crate::context_bootstrap) fn selection_composed_boundary_order<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    a_node: v8::Local<'s, v8::Object>,
    a_offset: u32,
    b_node: v8::Local<'s, v8::Object>,
    b_offset: u32,
) -> Option<std::cmp::Ordering> {
    let a_handle = callback_value_dom_handle(scope, a_node.into())?;
    let b_handle = callback_value_dom_handle(scope, b_node.into())?;
    let host_ptr = context_host_ptr_from_global_bridge(scope)?;
    selection_composed_boundary_order_handles(
        unsafe { &*host_ptr }.dom_host(),
        a_handle,
        a_offset,
        b_handle,
        b_offset,
    )
}

fn selection_composed_boundary_order_handles(
    dom_host: &moli_dom::native::DomHost,
    a_container: DomHandle,
    a_offset: u32,
    b_container: DomHandle,
    b_offset: u32,
) -> Option<std::cmp::Ordering> {
    if a_container == b_container {
        return Some(a_offset.cmp(&b_offset));
    }

    let a_chain = selection_composed_handle_chain(dom_host, a_container);
    let b_chain = selection_composed_handle_chain(dom_host, b_container);
    if a_chain.last()? != b_chain.last()? {
        return None;
    }

    for i in 1..a_chain.len() {
        if a_chain[i] == b_container {
            let child = a_chain[i - 1];
            let index = selection_composed_child_index(dom_host, b_container, child)?;
            return Some(if usize::try_from(b_offset).ok()? <= index {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Less
            });
        }
    }

    for i in 1..b_chain.len() {
        if b_chain[i] == a_container {
            let child = b_chain[i - 1];
            let index = selection_composed_child_index(dom_host, a_container, child)?;
            return Some(if index < usize::try_from(a_offset).ok()? {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Less
            });
        }
    }

    let mut ai = a_chain.len();
    let mut bi = b_chain.len();
    while ai > 0 && bi > 0 && a_chain[ai - 1] == b_chain[bi - 1] {
        ai -= 1;
        bi -= 1;
    }
    if ai == 0 || bi == 0 {
        return None;
    }
    let lca = a_chain[ai];
    let a_child = a_chain[ai - 1];
    let b_child = b_chain[bi - 1];
    let a_index = selection_composed_child_index(dom_host, lca, a_child)?;
    let b_index = selection_composed_child_index(dom_host, lca, b_child)?;
    Some(a_index.cmp(&b_index))
}

fn selection_composed_handle_chain(
    dom_host: &moli_dom::native::DomHost,
    mut handle: DomHandle,
) -> Vec<DomHandle> {
    let mut chain = vec![handle];
    loop {
        if let Some(parent) = dom_host.node(handle).and_then(|node| node.parent_node()) {
            handle = parent;
            chain.push(handle);
            continue;
        }
        if dom_host.is_shadow_root(handle)
            && let Some(host) = dom_host.shadow_root_host(handle)
        {
            handle = host;
            chain.push(handle);
            continue;
        }
        return chain;
    }
}

fn selection_composed_child_index(
    dom_host: &moli_dom::native::DomHost,
    parent: DomHandle,
    child: DomHandle,
) -> Option<usize> {
    if dom_host.is_shadow_root(child) && dom_host.shadow_root_host(child) == Some(parent) {
        return Some(0);
    }
    let index = dom_host.child_index(parent, child)?;
    Some(index + usize::from(dom_host.shadow_root_handle(parent).is_some()))
}
