use std::cmp::Ordering;
use std::sync::Arc;

use markup5ever::{LocalName, Namespace, Prefix, namespace_url, ns};

use crate::{
    Attribute, Document, Dom, Element, NamespaceResolver, Node, ProcessingInstruction, Value,
    evaluate_parsed_xpath, parse,
};
use crate::{Error as EvaluationError, ParserError};

pub type SnapshotNodeId = usize;

#[derive(Clone, Debug)]
pub struct Snapshot {
    nodes: Arc<Vec<SnapshotNodeData>>,
    root: SnapshotNodeId,
}

#[derive(Clone, Debug)]
pub enum SnapshotValue {
    Boolean(bool),
    Number(f64),
    String(String),
    Nodes(Vec<SnapshotNodeId>),
}

#[derive(Clone, Debug)]
pub enum SnapshotXPathEvaluationError {
    Parse(ParserError),
    ContextNodeNotInSnapshot,
    Evaluation(EvaluationError),
}

impl std::fmt::Display for SnapshotXPathEvaluationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(error) => write!(formatter, "XPath parse failed: {error:?}"),
            Self::ContextNodeNotInSnapshot => {
                formatter.write_str("XPath context node is not in snapshot")
            }
            Self::Evaluation(error) => write!(formatter, "XPath evaluation failed: {error:?}"),
        }
    }
}

#[derive(Default)]
pub struct SnapshotBuilder {
    nodes: Vec<SnapshotNodeData>,
}

#[derive(Clone, Debug)]
struct SnapshotNodeData {
    parent: Option<SnapshotNodeId>,
    children: Vec<SnapshotNodeId>,
    order: usize,
    kind: SnapshotNodeKind,
}

#[derive(Clone, Debug)]
enum SnapshotNodeKind {
    Document,
    Element(SnapshotElementData),
    Attribute(SnapshotAttributeData),
    Text(String),
    Comment(String),
}

#[derive(Clone, Debug)]
struct SnapshotElementData {
    prefix: Option<String>,
    namespace: String,
    local_name: String,
    attributes: Vec<SnapshotNodeId>,
    is_html_element_in_html_document: bool,
}

#[derive(Clone, Debug)]
struct SnapshotAttributeData {
    prefix: Option<String>,
    namespace: String,
    local_name: String,
    value: String,
}

#[derive(Clone, Debug)]
pub struct SnapshotNode {
    nodes: Arc<Vec<SnapshotNodeData>>,
    id: SnapshotNodeId,
}

#[derive(Clone, Debug)]
pub struct SnapshotDocument {
    nodes: Arc<Vec<SnapshotNodeData>>,
}

#[derive(Clone, Debug)]
pub struct SnapshotElement {
    node: SnapshotNode,
}

#[derive(Clone, Debug)]
pub struct SnapshotAttribute {
    node: SnapshotNode,
}

#[derive(Clone, Debug)]
pub struct NullNamespaceResolver;

pub struct SnapshotDom;

impl SnapshotBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn append_document(&mut self) -> SnapshotNodeId {
        self.push_node(None, SnapshotNodeKind::Document)
    }

    pub fn append_element(
        &mut self,
        parent: SnapshotNodeId,
        local_name: impl Into<String>,
        prefix: Option<String>,
        namespace: Option<String>,
        is_html_element_in_html_document: bool,
    ) -> SnapshotNodeId {
        let namespace = namespace.unwrap_or_else(|| {
            if is_html_element_in_html_document {
                namespace_url!("http://www.w3.org/1999/xhtml").to_string()
            } else {
                String::new()
            }
        });
        let id = self.push_node(
            Some(parent),
            SnapshotNodeKind::Element(SnapshotElementData {
                prefix,
                namespace,
                local_name: local_name.into(),
                attributes: Vec::new(),
                is_html_element_in_html_document,
            }),
        );
        self.nodes[parent].children.push(id);
        id
    }

    pub fn append_attribute(
        &mut self,
        element: SnapshotNodeId,
        local_name: impl Into<String>,
        value: impl Into<String>,
        prefix: Option<String>,
        namespace: Option<String>,
    ) -> SnapshotNodeId {
        let id = self.push_node(
            Some(element),
            SnapshotNodeKind::Attribute(SnapshotAttributeData {
                prefix,
                namespace: namespace.unwrap_or_default(),
                local_name: local_name.into(),
                value: value.into(),
            }),
        );
        if let SnapshotNodeKind::Element(data) = &mut self.nodes[element].kind {
            data.attributes.push(id);
        }
        id
    }

    pub fn append_text(
        &mut self,
        parent: SnapshotNodeId,
        text: impl Into<String>,
    ) -> SnapshotNodeId {
        let id = self.push_node(Some(parent), SnapshotNodeKind::Text(text.into()));
        self.nodes[parent].children.push(id);
        id
    }

    pub fn append_comment(
        &mut self,
        parent: SnapshotNodeId,
        text: impl Into<String>,
    ) -> SnapshotNodeId {
        let id = self.push_node(Some(parent), SnapshotNodeKind::Comment(text.into()));
        self.nodes[parent].children.push(id);
        id
    }

    pub fn finish(self, root: SnapshotNodeId) -> Snapshot {
        Snapshot {
            nodes: Arc::new(self.nodes),
            root,
        }
    }

    fn push_node(
        &mut self,
        parent: Option<SnapshotNodeId>,
        kind: SnapshotNodeKind,
    ) -> SnapshotNodeId {
        let id = self.nodes.len();
        self.nodes.push(SnapshotNodeData {
            parent,
            children: Vec::new(),
            order: id,
            kind,
        });
        id
    }
}

impl Snapshot {
    pub fn root_id(&self) -> SnapshotNodeId {
        self.root
    }

    pub fn node(&self, id: SnapshotNodeId) -> Option<SnapshotNode> {
        (id < self.nodes.len()).then(|| SnapshotNode {
            nodes: Arc::clone(&self.nodes),
            id,
        })
    }
}

pub fn evaluate_snapshot_xpath(
    snapshot: &Snapshot,
    expression: &str,
    context_node: SnapshotNodeId,
    is_in_html_document: bool,
) -> Result<SnapshotValue, String> {
    evaluate_snapshot_xpath_with_resolver::<NullNamespaceResolver>(
        snapshot,
        expression,
        context_node,
        is_in_html_document,
        None,
    )
}

pub fn evaluate_snapshot_xpath_with_resolver<N>(
    snapshot: &Snapshot,
    expression: &str,
    context_node: SnapshotNodeId,
    is_in_html_document: bool,
    namespace_resolver: Option<N>,
) -> Result<SnapshotValue, String>
where
    N: NamespaceResolver,
{
    evaluate_snapshot_xpath_with_resolver_detailed(
        snapshot,
        expression,
        context_node,
        is_in_html_document,
        namespace_resolver,
    )
    .map_err(|error| error.to_string())
}

pub fn evaluate_snapshot_xpath_with_resolver_detailed<N>(
    snapshot: &Snapshot,
    expression: &str,
    context_node: SnapshotNodeId,
    is_in_html_document: bool,
    namespace_resolver: Option<N>,
) -> Result<SnapshotValue, SnapshotXPathEvaluationError>
where
    N: NamespaceResolver,
{
    let expression = parse(expression, namespace_resolver, is_in_html_document)
        .map_err(SnapshotXPathEvaluationError::Parse)?;
    let context = snapshot
        .node(context_node)
        .ok_or(SnapshotXPathEvaluationError::ContextNodeNotInSnapshot)?;
    let value = evaluate_parsed_xpath::<SnapshotDom>(&expression, context)
        .map_err(SnapshotXPathEvaluationError::Evaluation)?;
    Ok(snapshot_value(value))
}

fn snapshot_value(value: Value<SnapshotNode>) -> SnapshotValue {
    match value {
        Value::Boolean(value) => SnapshotValue::Boolean(value),
        Value::Number(value) => SnapshotValue::Number(value),
        Value::String(value) => SnapshotValue::String(value),
        Value::NodeSet(nodes) => {
            SnapshotValue::Nodes(nodes.into_iter().map(|node| node.id).collect())
        }
    }
}

impl SnapshotNode {
    fn data(&self) -> &SnapshotNodeData {
        &self.nodes[self.id]
    }

    fn text_content_from(id: SnapshotNodeId, nodes: &[SnapshotNodeData], out: &mut String) {
        match &nodes[id].kind {
            SnapshotNodeKind::Text(text)
            | SnapshotNodeKind::Comment(text)
            | SnapshotNodeKind::Attribute(SnapshotAttributeData { value: text, .. }) => {
                out.push_str(text);
            }
            SnapshotNodeKind::Document | SnapshotNodeKind::Element(_) => {
                for child in &nodes[id].children {
                    Self::text_content_from(*child, nodes, out);
                }
            }
        }
    }

    fn child_nodes(&self) -> Vec<Self> {
        self.data()
            .children
            .iter()
            .map(|id| self.with_id(*id))
            .collect()
    }

    fn with_id(&self, id: SnapshotNodeId) -> Self {
        Self {
            nodes: Arc::clone(&self.nodes),
            id,
        }
    }

    fn all_nodes_in_order(&self) -> Vec<Self> {
        (0..self.nodes.len()).map(|id| self.with_id(id)).collect()
    }

    fn is_ancestor_of(&self, other: &Self) -> bool {
        let mut parent = other.data().parent;
        while let Some(parent_id) = parent {
            if parent_id == self.id {
                return true;
            }
            parent = self.nodes[parent_id].parent;
        }
        false
    }

    fn is_attribute(&self) -> bool {
        matches!(self.data().kind, SnapshotNodeKind::Attribute(_))
    }
}

impl PartialEq for SnapshotNode {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && Arc::ptr_eq(&self.nodes, &other.nodes)
    }
}

impl Eq for SnapshotNode {}

impl Dom for SnapshotDom {
    type Node = SnapshotNode;
    type NamespaceResolver = NullNamespaceResolver;
}

impl NamespaceResolver for NullNamespaceResolver {
    fn resolve_namespace_prefix(&self, _prefix: &str) -> Option<String> {
        None
    }
}

impl Node for SnapshotNode {
    type ProcessingInstruction = ();
    type Document = SnapshotDocument;
    type Attribute = SnapshotAttribute;
    type Element = SnapshotElement;
    type Opaque = SnapshotNodeId;

    fn is_comment(&self) -> bool {
        matches!(self.data().kind, SnapshotNodeKind::Comment(_))
    }

    fn is_text(&self) -> bool {
        matches!(self.data().kind, SnapshotNodeKind::Text(_))
    }

    fn text_content(&self) -> String {
        let mut out = String::new();
        Self::text_content_from(self.id, &self.nodes, &mut out);
        out
    }

    fn language(&self) -> Option<String> {
        let mut current = Some(self.clone());
        while let Some(node) = current {
            if let Some(element) = node.as_element() {
                for attribute in element.attributes() {
                    let data = attribute.data();
                    if data.local_name.eq_ignore_ascii_case("lang") {
                        return Some(data.value.clone());
                    }
                }
            }
            current = node.parent();
        }
        None
    }

    fn parent(&self) -> Option<Self> {
        self.data().parent.map(|id| self.with_id(id))
    }

    fn children(&self) -> impl Iterator<Item = Self> {
        self.child_nodes().into_iter()
    }

    fn compare_tree_order(&self, other: &Self) -> Ordering {
        self.data().order.cmp(&other.data().order)
    }

    fn traverse_preorder(&self) -> impl Iterator<Item = Self> {
        let mut out = Vec::new();
        collect_preorder(self.clone(), &mut out);
        out.into_iter()
    }

    fn inclusive_ancestors(&self) -> impl Iterator<Item = Self> {
        let mut out = Vec::new();
        let mut current = Some(self.clone());
        while let Some(node) = current {
            current = node.parent();
            out.push(node);
        }
        out.into_iter()
    }

    fn preceding_nodes(&self) -> impl Iterator<Item = Self> {
        let out = self
            .all_nodes_in_order()
            .into_iter()
            .filter(|node| {
                node.data().order <= self.data().order
                    && !node.is_attribute()
                    && !node.is_ancestor_of(self)
            })
            .rev()
            .collect::<Vec<_>>();
        out.into_iter()
    }

    fn following_nodes(&self) -> impl Iterator<Item = Self> {
        let out = self
            .all_nodes_in_order()
            .into_iter()
            .filter(|node| {
                node.data().order >= self.data().order
                    && !node.is_attribute()
                    && !self.is_ancestor_of(node)
            })
            .collect::<Vec<_>>();
        out.into_iter()
    }

    fn preceding_siblings(&self) -> impl Iterator<Item = Self> {
        let Some(parent) = self.parent() else {
            return Vec::new().into_iter();
        };
        let mut out = parent
            .child_nodes()
            .into_iter()
            .take_while(|node| node.id != self.id)
            .collect::<Vec<_>>();
        out.reverse();
        out.into_iter()
    }

    fn following_siblings(&self) -> impl Iterator<Item = Self> {
        let Some(parent) = self.parent() else {
            return Vec::new().into_iter();
        };
        let mut seen_self = false;
        let out = parent
            .child_nodes()
            .into_iter()
            .filter(|node| {
                if seen_self {
                    true
                } else {
                    seen_self = node.id == self.id;
                    false
                }
            })
            .collect::<Vec<_>>();
        out.into_iter()
    }

    fn owner_document(&self) -> Self::Document {
        SnapshotDocument {
            nodes: Arc::clone(&self.nodes),
        }
    }

    fn to_opaque(&self) -> Self::Opaque {
        self.id
    }

    fn as_processing_instruction(&self) -> Option<Self::ProcessingInstruction> {
        None
    }

    fn as_attribute(&self) -> Option<Self::Attribute> {
        matches!(self.data().kind, SnapshotNodeKind::Attribute(_))
            .then(|| SnapshotAttribute { node: self.clone() })
    }

    fn as_element(&self) -> Option<Self::Element> {
        matches!(self.data().kind, SnapshotNodeKind::Element(_))
            .then(|| SnapshotElement { node: self.clone() })
    }

    fn get_root_node(&self) -> Self {
        let mut current = self.clone();
        while let Some(parent) = current.parent() {
            current = parent;
        }
        current
    }
}

fn collect_preorder(node: SnapshotNode, out: &mut Vec<SnapshotNode>) {
    out.push(node.clone());
    for child in node.child_nodes() {
        collect_preorder(child, out);
    }
}

impl ProcessingInstruction for () {
    fn target(&self) -> String {
        String::new()
    }
}

impl Document for SnapshotDocument {
    type Node = SnapshotNode;

    fn get_elements_with_id(
        &self,
        id: &str,
    ) -> impl Iterator<Item = <Self::Node as Node>::Element> {
        let out = (0..self.nodes.len())
            .filter_map(|node_id| {
                let node = SnapshotNode {
                    nodes: Arc::clone(&self.nodes),
                    id: node_id,
                };
                let element = node.as_element()?;
                let matches_id = element.attributes().any(|attribute| {
                    let data = attribute.data();
                    data.local_name == "id" && data.value == id
                });
                matches_id.then_some(element)
            })
            .collect::<Vec<_>>();
        out.into_iter()
    }
}

impl SnapshotElement {
    fn data(&self) -> &SnapshotElementData {
        let SnapshotNodeKind::Element(data) = &self.node.data().kind else {
            unreachable!("SnapshotElement only wraps element nodes")
        };
        data
    }
}

impl Element for SnapshotElement {
    type Node = SnapshotNode;
    type Attribute = SnapshotAttribute;

    fn as_node(&self) -> Self::Node {
        self.node.clone()
    }

    fn prefix(&self) -> Option<Prefix> {
        self.data().prefix.as_deref().map(Prefix::from)
    }

    fn namespace(&self) -> Namespace {
        if self.data().namespace.is_empty() {
            ns!()
        } else {
            Namespace::from(self.data().namespace.as_str())
        }
    }

    fn local_name(&self) -> LocalName {
        LocalName::from(self.data().local_name.as_str())
    }

    fn attributes(&self) -> impl Iterator<Item = Self::Attribute> {
        let out = self
            .data()
            .attributes
            .iter()
            .map(|id| SnapshotAttribute {
                node: self.node.with_id(*id),
            })
            .collect::<Vec<_>>();
        out.into_iter()
    }

    fn is_html_element_in_html_document(&self) -> bool {
        self.data().is_html_element_in_html_document
    }
}

impl SnapshotAttribute {
    fn data(&self) -> &SnapshotAttributeData {
        let SnapshotNodeKind::Attribute(data) = &self.node.data().kind else {
            unreachable!("SnapshotAttribute only wraps attribute nodes")
        };
        data
    }
}

impl Attribute for SnapshotAttribute {
    type Node = SnapshotNode;

    fn as_node(&self) -> Self::Node {
        self.node.clone()
    }

    fn prefix(&self) -> Option<Prefix> {
        self.data().prefix.as_deref().map(Prefix::from)
    }

    fn namespace(&self) -> Namespace {
        if self.data().namespace.is_empty() {
            ns!()
        } else {
            Namespace::from(self.data().namespace.as_str())
        }
    }

    fn local_name(&self) -> LocalName {
        LocalName::from(self.data().local_name.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_snapshot() -> (Snapshot, SnapshotNodeId) {
        let mut builder = SnapshotBuilder::new();
        let document = builder.append_document();
        let root = builder.append_element(document, "main", None, None, true);
        let first = builder.append_element(root, "div", None, None, true);
        builder.append_attribute(first, "id", "a", None, None);
        builder.append_text(first, "Alpha");
        let second = builder.append_element(root, "div", None, None, true);
        builder.append_attribute(second, "id", "b", None, None);
        builder.append_text(second, "Beta");
        (builder.finish(document), document)
    }

    #[test]
    fn evaluates_element_nodes_in_document_order() {
        let (snapshot, document) = sample_snapshot();
        let value = evaluate_snapshot_xpath(&snapshot, "//div", document, true).unwrap();
        let SnapshotValue::Nodes(nodes) = value else {
            panic!("expected nodes");
        };
        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn evaluates_attribute_predicates_and_scalar_functions() {
        let (snapshot, document) = sample_snapshot();
        let value =
            evaluate_snapshot_xpath(&snapshot, "string(//div[@id='b'])", document, true).unwrap();
        assert!(matches!(value, SnapshotValue::String(value) if value == "Beta"));
    }

    #[test]
    fn html_attribute_name_tests_use_the_owner_element_case_rules() {
        let mut builder = SnapshotBuilder::new();
        let document = builder.append_document();
        let html = builder.append_element(document, "html", None, None, true);
        let div = builder.append_element(html, "div", None, None, true);
        builder.append_attribute(div, "id", "html", None, None);
        let svg = builder.append_element(
            html,
            "svg",
            None,
            Some(namespace_url!("http://www.w3.org/2000/svg").to_string()),
            false,
        );
        let path = builder.append_element(
            svg,
            "path",
            None,
            Some(namespace_url!("http://www.w3.org/2000/svg").to_string()),
            false,
        );
        builder.append_attribute(path, "id", "svg", None, None);
        builder.append_attribute(path, "refX", "value", None, None);
        let snapshot = builder.finish(document);

        let matching_nodes = |expression| {
            let SnapshotValue::Nodes(nodes) =
                evaluate_snapshot_xpath(&snapshot, expression, document, true).unwrap()
            else {
                panic!("expected nodes");
            };
            nodes
        };

        assert_eq!(matching_nodes("//*[@Id]"), vec![div]);
        assert_eq!(matching_nodes("//*[@refX]"), vec![path]);
        assert!(matching_nodes("//*[@refx]").is_empty());
    }

    #[test]
    fn id_function_ignores_empty_id_token_lists() {
        let mut builder = SnapshotBuilder::new();
        let document = builder.append_document();
        let root = builder.append_element(document, "root", None, None, false);
        let element = builder.append_element(root, "item", None, None, false);
        builder.append_attribute(element, "id", "", None, None);
        let snapshot = builder.finish(document);

        for expression in ["id('')", "id('   ')"] {
            let value = evaluate_snapshot_xpath(&snapshot, expression, document, false).unwrap();
            assert!(matches!(value, SnapshotValue::Nodes(nodes) if nodes.is_empty()));
        }
    }
}
