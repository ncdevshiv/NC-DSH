use super::super::*;
use super::live_dom::{LiveXPathResultNode, LiveXPathValue, evaluate_live_xpath};
use super::resolver::V8XPathNamespaceResolver;
use super::result::{
    XPathIteratorMutationState, build_xpath_nodes_result, build_xpath_scalar_result,
};
use super::snapshot::{build_xpath_snapshot, xpath_context_node_id};
use crate::native_bridge::document::{
    detached_node_type, detached_tree_query_version, detached_tree_root_object,
    live_get_attribute_node_ns_object, live_get_attribute_node_object,
};
use moli_xpath::{
    ParserError, SnapshotValue, SnapshotXPathEvaluationError,
    evaluate_snapshot_xpath_with_resolver_detailed,
};

use super::XPathEvaluationError;

pub(super) fn evaluate_xpath_over_live_dom<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    runtime_ptr: *mut JsContextHost,
    expression: &str,
    context_handle: DomHandle,
    namespace_resolver: Option<V8XPathNamespaceResolver<'s, 'i>>,
    requested_result_type: u32,
) -> Result<Option<v8::Local<'s, v8::Object>>, XPathEvaluationError> {
    let runtime = unsafe { &*runtime_ptr };
    let document_handle = runtime.dom_host().document_handle();
    let is_in_html_document = runtime
        .dom_host()
        .node(document_handle)
        .and_then(crate::dom::native::Node::as_document)
        .is_some_and(|document| document.is_html_document());
    let value = evaluate_live_xpath(
        runtime.dom_host(),
        expression,
        context_handle,
        is_in_html_document,
        namespace_resolver,
    )?;

    match value {
        LiveXPathValue::Nodes(handles) => {
            let mut resolved = Vec::with_capacity(handles.len());
            for node in handles {
                let Some(node) = live_xpath_result_node_object(scope, runtime_ptr, node) else {
                    continue;
                };
                resolved.push(node);
            }
            let baseline_query_version = runtime.dom_host().query_version();
            Ok(build_xpath_nodes_result(
                scope,
                &resolved,
                requested_result_type,
                Some(XPathIteratorMutationState::Live {
                    runtime_ptr,
                    query_version: baseline_query_version,
                }),
            ))
        }
        LiveXPathValue::Scalar(value) => Ok(build_xpath_scalar_result(
            scope,
            value,
            requested_result_type,
        )),
    }
}

fn live_xpath_result_node_object<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    runtime_ptr: *mut JsContextHost,
    node: LiveXPathResultNode,
) -> Option<v8::Local<'s, v8::Object>> {
    match node {
        LiveXPathResultNode::Node(handle) => {
            let runtime = unsafe { &mut *runtime_ptr };
            runtime
                .native_bridge_mut()
                .wrap_handle(scope, runtime_ptr, handle)
        }
        LiveXPathResultNode::Attribute { owner, index } => {
            let (name, namespace_uri, local_name) = {
                let runtime = unsafe { &*runtime_ptr };
                let attribute = runtime
                    .dom_host()
                    .node(owner)?
                    .as_element()?
                    .attributes()
                    .get(index)?;
                (
                    attribute.name(),
                    (!attribute.namespace().is_empty()).then(|| attribute.namespace().to_owned()),
                    attribute.local_name().to_owned(),
                )
            };
            let runtime = unsafe { &mut *runtime_ptr };
            let owner = runtime
                .native_bridge_mut()
                .wrap_handle(scope, runtime_ptr, owner)?;
            if let Some(namespace_uri) = namespace_uri.as_deref() {
                live_get_attribute_node_ns_object(scope, owner, Some(namespace_uri), &local_name)
            } else {
                live_get_attribute_node_object(scope, owner, &name)
            }
        }
    }
}

pub(super) fn evaluate_xpath_over_object_tree<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    root: v8::Local<'s, v8::Object>,
    expression: &str,
    context_node: Option<v8::Local<'s, v8::Object>>,
    namespace_resolver: Option<V8XPathNamespaceResolver<'s, 'i>>,
    requested_result_type: u32,
) -> Result<Option<v8::Local<'s, v8::Object>>, XPathEvaluationError> {
    let root_node_type = detached_node_type(scope, root).unwrap_or_default();
    if root_node_type != 9 && root_node_type != 1 {
        return Ok(None);
    }
    let iterator_mutation_state =
        object_tree_xpath_iterator_mutation_state(scope, context_node, root);

    let Some(snapshot) = build_xpath_snapshot(scope, root) else {
        return Ok(None);
    };
    let context = context_node.unwrap_or(root);
    let Some(context_id) = xpath_context_node_id(scope, context, &snapshot) else {
        return Ok(build_xpath_nodes_result(
            scope,
            &[],
            requested_result_type,
            iterator_mutation_state,
        ));
    };

    let value = evaluate_snapshot_xpath_with_resolver_detailed::<V8XPathNamespaceResolver<'s, 'i>>(
        &snapshot.snapshot,
        expression,
        context_id,
        true,
        namespace_resolver,
    )
    .map_err(|error| match error {
        SnapshotXPathEvaluationError::Parse(ParserError::FailedToResolveNamespacePrefix) => {
            XPathEvaluationError::Namespace
        }
        _ => XPathEvaluationError::InvalidExpression,
    })?;

    match value {
        SnapshotValue::Nodes(nodes) => {
            let mut resolved = Vec::new();
            for node_id in nodes {
                if let Some(Some(original)) = snapshot.original_nodes.get(node_id) {
                    resolved.push(*original);
                }
            }
            Ok(build_xpath_nodes_result(
                scope,
                &resolved,
                requested_result_type,
                iterator_mutation_state,
            ))
        }
        value => Ok(build_xpath_scalar_result(
            scope,
            value,
            requested_result_type,
        )),
    }
}

fn object_tree_xpath_iterator_mutation_state<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    context_node: Option<v8::Local<'s, v8::Object>>,
    root: v8::Local<'s, v8::Object>,
) -> Option<XPathIteratorMutationState<'s>> {
    let candidate = context_node.unwrap_or(root);
    let root = match detached_tree_root_object(scope, candidate) {
        Some(root) => root,
        None => detached_tree_root_object(scope, root)?,
    };
    let query_version = detached_tree_query_version(scope, root)?;
    Some(XPathIteratorMutationState::ObjectTree {
        root,
        query_version,
    })
}
