//! The expander: one LLM call turns a clarified prompt into a hierarchical
//! task tree.
//!
//! [`LlmPlanner::expand_tree`] (the implementation behind
//! [`Planner::expand`](crate::planner::Planner::expand)) asks the model for a
//! single JSON object describing up to three levels of hierarchy:
//!
//! ```json
//! {"categories": [
//!   {"title": "Backend", "brief": "…",
//!    "children": [
//!      {"title": "API", "brief": "…",
//!       "children": [
//!         {"title": "Route /health", "brief": "self-contained instructions…"}
//!       ]}
//!    ]}
//! ]}
//! ```
//!
//! # JSON contract
//!
//! - Root object with one required key `categories` (array; an empty or
//!   unusable array triggers retry → fallback, see below).
//! - Every node: `title` (string) and `brief` (string), optional `children`
//!   (same shape). Levels map to root categories → subcategories → leaves.
//! - Leaf briefs MUST be **self-contained**: a fresh agent must be able to
//!   execute them with no other context. The instruction demands it and the
//!   orchestrator relies on it.
//! - No dependencies are requested or inserted; ordering flows from the
//!   returned depth-first leaf sequence.
//!
//! # Defensive parsing, retry, and failure policy
//!
//! Identical machinery to the clarifier: code fences stripped, first
//! balanced `{...}` block extracted, tolerant field reading (missing titles
//! become `(untitled)`, missing briefs empty — nodes are never dropped).
//! A response yielding no usable plan is a *parse failure* and is retried
//! [`LlmPlannerConfig::parse_retries`] extra times. If every attempt fails,
//! the fail-safe kicks in: ONE leaf whose brief is the whole clarified
//! prompt verbatim, so orchestration can always proceed. Transport errors
//! from the model propagate unchanged; they are never masked.
//!
//! Two hard guards round out the contract:
//!
//! - **Budget** — a plan proposing more than
//!   [`LlmPlannerConfig::max_nodes`] total nodes (categories +
//!   subcategories + leaves) is rejected with [`AiError::Validation`]
//!   carrying an [`ExpansionTooLarge`] source listing both counts; nothing
//!   is inserted into the tree.
//! - **Leaf selection** — only childless nodes count as leaves and only
//!   their ids are returned, in depth-first order. A category with children
//!   is internal scaffolding; a childless category IS its own leaf (ragged
//!   hierarchies are tolerated at any depth).

use ai_errors::{AiError, ValidationError};
use serde_json::Value;

use crate::clarifier::{LlmPlanner, extract_json_object};
use crate::tree::{TaskId, TaskTree};

/// Typed cause attached to budget rejections: the expansion proposed
/// `proposed` nodes against a configured budget of `budget`
/// ([`LlmPlannerConfig::max_nodes`]).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("expansion proposed {proposed} nodes, exceeding the configured budget of {budget}")]
pub struct ExpansionTooLarge {
    /// Total nodes the model's plan would have inserted.
    pub proposed: usize,
    /// The configured ceiling (`max_nodes`).
    pub budget: usize,
}

impl LlmPlanner {
    /// Decomposes `clarified_prompt` into the tree — the implementation
    /// behind [`crate::planner::Planner::expand`].
    ///
    /// Categories become roots under `parent` (`None` = tree roots),
    /// subcategories children of categories, leaves children of
    /// subcategories. Returns ONLY leaf ids, depth-first. See the
    /// [module docs](self) for the full contract.
    pub async fn expand_tree(
        &self,
        tree: &mut TaskTree,
        parent: Option<TaskId>,
        clarified_prompt: &str,
    ) -> Result<Vec<TaskId>, AiError> {
        let attempts = self.config().parse_retries.saturating_add(1);
        let mut parsed = None;
        for _ in 0..attempts {
            let text = self
                .generate_json(
                    &expand_instruction(clarified_prompt),
                    "task_expansion",
                    expansion_schema(),
                    self.config().decomposition_temperature,
                )
                .await?;
            if let Some(plan) = parse_plan(&text) {
                parsed = Some(plan);
                break;
            }
            // Parse failure only; transport errors above propagate as-is.
        }

        // Budget guard BEFORE any insertion: a rejected plan leaves the
        // tree untouched.
        if let Some(plan) = &parsed {
            let total = count_nodes(plan);
            if total > self.config().max_nodes {
                return Err(AiError::Validation(ValidationError {
                    message: format!(
                        "expansion proposed {total} nodes, exceeding the configured \
                         budget of {}",
                        self.config().max_nodes
                    ),
                    source: Some(Box::new(ExpansionTooLarge {
                        proposed: total,
                        budget: self.config().max_nodes,
                    })),
                }));
            }
        }

        match parsed {
            Some(plan) => {
                // Fail fast on a bogus parent before mutating anything.
                if let Some(p) = parent {
                    if tree.get(p).is_none() {
                        return Err(AiError::Validation(ValidationError {
                            message: format!("cannot expand under unknown parent {p}"),
                            source: None,
                        }));
                    }
                }
                Ok(insert_plan(tree, &plan, parent))
            }
            // Fail-safe: always leave the orchestrator something executable.
            None => {
                let id = attach_leaf(tree, parent, "Objective", clarified_prompt);
                Ok(vec![id])
            }
        }
    }
}

// -- expand: instruction + schema ---------------------------------------------

/// Builds the user message driving the decomposition call. The JSON
/// contract is spelled out in prose because not every gateway enforces
/// `response_format`.
fn expand_instruction(prompt: &str) -> String {
    format!(
        "You are the decomposition engine of an orchestrator. Break the \
         objective below into a hierarchy of tasks.\n\
         \n\
         Respond with EXACTLY ONE JSON object and nothing else:\n\
         {{\"categories\": [{{\"title\": \"<category>\", \"brief\": \"<what this \
         area covers>\", \"children\": [{{\"title\": \"<subcategory>\", \"brief\": \
         \"...\", \"children\": [{{\"title\": \"<leaf task>\", \"brief\": \"...\"}}]}}]}}]}}\n\
         \n\
         Rules:\n\
         - At most THREE levels: categories -> subcategories -> leaves.\n\
         - EVERY node needs \"title\" (short label) and \"brief\".\n\
         - LEAF briefs must be SELF-CONTAINED: a fresh agent with no other \
         context can execute the task from the brief alone — include goal, \
         constraints and acceptance checks; never refer to 'the objective' or \
         'above'.\n\
         - Do NOT output dependencies; ordering is handled elsewhere.\n\
         - Keep the plan small: at most 32 nodes in total.\n\
         - Never wrap the JSON in markdown fences or commentary.\n\
         \n\
         OBJECTIVE:\n{prompt}"
    )
}

/// JSON Schema advertised via `ResponseFormat::JsonSchema` (best-effort
/// enforcement on gateways that support it), three levels deep.
fn expansion_schema() -> Value {
    fn node_schema(depth: usize) -> Value {
        let mut schema = serde_json::json!({
            "type": "object",
            "properties": {
                "title": {"type": "string"},
                "brief": {"type": "string"}
            },
            "required": ["title", "brief"]
        });
        if depth < 2 {
            schema["properties"]["children"] = serde_json::json!({
                "type": "array",
                "items": node_schema(depth + 1)
            });
        }
        schema
    }
    let mut schema = serde_json::json!({
        "type": "object",
        "properties": {
            "categories": {"type": "array", "items": node_schema(0)}
        },
        "required": ["categories"]
    });
    // Strict-mode compatibility (same contract as the clarify verdict).
    crate::clarifier::make_strict(&mut schema);
    schema
}

// -- defensive parsing ---------------------------------------------------------

/// One node of the model's plan, coerced into owned strings.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RawNode {
    title: String,
    brief: String,
    children: Vec<RawNode>,
}

/// Parses one expansion response into a list of root nodes, or `None` when
/// nothing usable could be recovered (triggering retry → fallback).
///
/// `None` cases: no JSON object found, unparseable JSON, `categories`
/// missing or not an array, or an EMPTY plan — all equally unusable for the
/// orchestrator, which needs at least one delegable leaf.
fn parse_plan(text: &str) -> Option<Vec<RawNode>> {
    let raw: Value = serde_json::from_str(&extract_json_object(text)?).ok()?;
    let nodes = parse_nodes(raw.get("categories")?)?;
    if nodes.is_empty() {
        // An empty hierarchy delegates nothing; treat it like a failed
        // parse so retry/fallback can produce a workable leaf.
        None
    } else {
        Some(nodes)
    }
}

fn parse_nodes(value: &Value) -> Option<Vec<RawNode>> {
    let items = value.as_array()?;
    let mut nodes = Vec::with_capacity(items.len());
    for item in items {
        nodes.push(RawNode {
            title: item
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("(untitled)")
                .to_owned(),
            brief: item
                .get("brief")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            children: item
                .get("children")
                .and_then(parse_nodes)
                .unwrap_or_default(),
        });
    }
    Some(nodes)
}

/// Total node count of a parsed plan (categories + subcategories + leaves).
fn count_nodes(nodes: &[RawNode]) -> usize {
    nodes
        .iter()
        .map(|node| 1 + count_nodes(&node.children))
        .sum()
}

// -- insertion -----------------------------------------------------------------

/// Inserts the plan under `parent` (`None` = roots) and returns the ids of
/// all CHILDLESS nodes in depth-first order.
fn insert_plan(tree: &mut TaskTree, nodes: &[RawNode], parent: Option<TaskId>) -> Vec<TaskId> {
    let mut leaves = Vec::new();
    for node in nodes {
        collect_leaves(tree, node, parent, &mut leaves);
    }
    leaves
}

fn collect_leaves(
    tree: &mut TaskTree,
    node: &RawNode,
    parent: Option<TaskId>,
    leaves: &mut Vec<TaskId>,
) {
    let id = attach_leaf(tree, parent, &node.title, &node.brief);
    if node.children.is_empty() {
        leaves.push(id);
        return;
    }
    for child in &node.children {
        collect_leaves(tree, child, Some(id), leaves);
    }
}

/// Inserts one node under `parent` (or as a root). Dependencies are
/// deliberately empty: ordering flows from the returned DFS leaf sequence.
fn attach_leaf(tree: &mut TaskTree, parent: Option<TaskId>, title: &str, brief: &str) -> TaskId {
    match parent {
        Some(p) => tree
            .add_child(p, title.to_owned(), brief.to_owned(), vec![])
            .expect("parent validated before insertion"),
        None => tree
            .add_root(title.to_owned(), brief.to_owned(), vec![])
            .expect("fresh root cannot fail on empty dependencies"),
    }
}

// -- tests ----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clarifier::test_support::{ScriptedModel, completion};
    use std::sync::Arc;

    use crate::clarifier::LlmPlannerConfig;
    use ai_core::Model;

    fn planner(script: &[&str]) -> (LlmPlanner, Arc<ScriptedModel>) {
        let completions = script.iter().map(|s| completion(s)).collect();
        let model = Arc::new(ScriptedModel::new(completions));
        (LlmPlanner::new(Arc::clone(&model) as Arc<dyn Model>), model)
    }

    const HIERARCHY_JSON: &str = r#"{
        "categories": [
            {
                "title": "Backend",
                "brief": "Server-side work",
                "children": [
                    {
                        "title": "API",
                        "brief": "HTTP surface",
                        "children": [
                            {"title": "GET /health", "brief": "Implement GET /health returning {\"status\":\"ok\"} with 200 and a unit test."},
                            {"title": "POST /users", "brief": "Implement POST /users validating the payload and persisting to sqlite."}
                        ]
                    },
                    {
                        "title": "Jobs",
                        "brief": "Background work",
                        "children": [
                            {"title": "Nightly cleanup", "brief": "Write a cron job deleting rows older than 30 days from the events table."}
                        ]
                    }
                ]
            },
            {
                "title": "Docs",
                "brief": "Documentation pass",
                "children": [
                    {"title": "README", "brief": "Write a README covering install, run and test commands."}
                ]
            }
        ]
    }"#;

    #[tokio::test]
    async fn canned_hierarchy_maps_to_exact_expected_tree_shape() {
        let (p, _model) = planner(&[HIERARCHY_JSON]);
        let mut tree = TaskTree::new();

        let leaves = p
            .expand_tree(&mut tree, None, "build the service")
            .await
            .unwrap();

        // Shape: 2 root categories; Backend has API+Jobs subcategories;
        // every title/brief/parent link exactly as authored.
        assert_eq!(tree.roots().len(), 2);
        let backend = tree.roots()[0];
        assert_eq!(tree.get(backend).unwrap().title, "Backend");
        assert_eq!(tree.get(backend).unwrap().brief, "Server-side work");

        let backend_children = tree.get(backend).unwrap().children.clone();
        assert_eq!(backend_children.len(), 2);
        assert_eq!(tree.get(backend_children[0]).unwrap().title, "API");
        assert_eq!(tree.get(backend_children[0]).unwrap().brief, "HTTP surface");
        assert_eq!(tree.get(backend_children[1]).unwrap().title, "Jobs");
        assert_eq!(tree.get(backend_children[0]).unwrap().parent, Some(backend));

        let api_leaves = tree.get(backend_children[0]).unwrap().children.clone();
        assert_eq!(api_leaves.len(), 2);
        assert_eq!(tree.get(api_leaves[0]).unwrap().title, "GET /health");
        assert_eq!(
            tree.get(api_leaves[0]).unwrap().parent,
            Some(backend_children[0])
        );
        assert_eq!(
            tree.get(api_leaves[0]).unwrap().brief,
            "Implement GET /health returning {\"status\":\"ok\"} with 200 and a unit test."
        );
        assert_eq!(tree.get(api_leaves[1]).unwrap().title, "POST /users");

        let docs = tree.roots()[1];
        let docs_leaves = tree.get(docs).unwrap().children.clone();
        assert_eq!(docs_leaves.len(), 1);
        assert_eq!(tree.get(docs_leaves[0]).unwrap().title, "README");
        assert_eq!(
            tree.get(docs_leaves[0]).unwrap().brief,
            "Write a README covering install, run and test commands."
        );

        // Exactly 4 delegable leaves (2 API + 1 Jobs + 1 README), all
        // Pending; structural invariants hold.
        assert_eq!(leaves.len(), 4);
        for id in &leaves {
            assert_eq!(
                tree.get(*id).unwrap().status,
                crate::tree::NodeStatus::Pending
            );
        }
        assert!(tree.check_invariants().is_ok());
    }

    #[tokio::test]
    async fn returns_only_leaves_in_depth_first_order() {
        let (p, _model) = planner(&[HIERARCHY_JSON]);
        let mut tree = TaskTree::new();

        let leaves = p.expand_tree(&mut tree, None, "x").await.unwrap();

        // DFS pre-order over the JSON: health, users (API), nightly (Jobs),
        // readme (Docs).
        let titles: Vec<_> = leaves
            .iter()
            .map(|id| tree.get(*id).unwrap().title.clone())
            .collect();
        assert_eq!(
            titles,
            vec!["GET /health", "POST /users", "Nightly cleanup", "README"]
        );

        // Internal scaffolding exists in the tree but is never returned:
        // 8 nodes total = 4 internal + 4 leaves.
        assert_eq!(tree.len(), 8);
        let leaf_set: std::collections::BTreeSet<_> = leaves.iter().copied().collect();
        for root in tree.roots() {
            assert!(!leaf_set.contains(&root), "roots are not leaves");
        }
        for root in tree.roots() {
            for child in tree.get(root).unwrap().children.clone() {
                let child_node = tree.get(child).unwrap();
                if !child_node.children.is_empty() {
                    assert!(!leaf_set.contains(&child));
                }
            }
        }
    }

    #[tokio::test]
    async fn parent_option_attaches_categories_under_existing_node() {
        let (p, _model) = planner(&[r#"{"categories": [
            {"title": "C1", "brief": "b", "children": [
                {"title": "S1", "brief": "b", "children": [
                    {"title": "L1", "brief": "leaf brief"}
                ]}
            ]}
        ]}"#]);

        let mut tree = TaskTree::new();
        let anchor = tree
            .add_root("anchor", "pre-existing work", vec![])
            .unwrap();

        let leaves = p.expand_tree(&mut tree, Some(anchor), "x").await.unwrap();

        assert_eq!(leaves, vec![TaskId(3)], "anchor=0, C1=1, S1=2, L1=3");
        let c1 = tree.get(anchor).unwrap().children[0];
        assert_eq!(tree.get(c1).unwrap().title, "C1");
        assert_eq!(tree.get(c1).unwrap().parent, Some(anchor));
        assert_eq!(tree.roots(), vec![anchor], "no new roots created");
        assert!(tree.check_invariants().is_ok());
    }

    #[tokio::test]
    async fn ragged_depth_childless_category_is_itself_a_leaf() {
        let (p, _model) = planner(&[r#"{"categories": [
            {"title": "Flat", "brief": "no children at all"},
            {"title": "Deep", "brief": "skips a level", "children": [
                {"title": "Direct leaf", "brief": "two-level leaf", "children": []}
            ]}
        ]}"#]);
        let mut tree = TaskTree::new();

        let leaves = p.expand_tree(&mut tree, None, "x").await.unwrap();

        let titles: Vec<_> = leaves
            .iter()
            .map(|id| tree.get(*id).unwrap().title.clone())
            .collect();
        assert_eq!(titles, vec!["Flat", "Direct leaf"]);
        assert_eq!(tree.len(), 3);
    }

    #[tokio::test]
    async fn oversized_plan_is_rejected_with_typed_count_and_untouched_tree() {
        // 1 category + 1 subcategory + 3 leaves = 5 nodes > budget 4.
        let (p, _model) = planner(&[r#"{"categories": [
            {"title": "Cat", "brief": "b", "children": [
                {"title": "Sub", "brief": "b", "children": [
                    {"title": "L1", "brief": "b"},
                    {"title": "L2", "brief": "b"},
                    {"title": "L3", "brief": "b"}
                ]}
            ]}
        ]}"#]);
        let p = LlmPlanner::with_config(
            p.model().clone(),
            LlmPlannerConfig {
                max_nodes: 4,
                ..LlmPlannerConfig::default()
            },
        );
        let mut tree = TaskTree::new();

        let err = p.expand_tree(&mut tree, None, "big").await.unwrap_err();

        match err {
            AiError::Validation(v) => {
                assert!(v.message.contains("5"), "count in message: {}", v.message);
                assert!(v.message.contains("4"), "budget in message: {}", v.message);
                let cause = v
                    .source
                    .as_ref()
                    .and_then(|s| s.downcast_ref::<ExpansionTooLarge>())
                    .expect("typed ExpansionTooLarge source");
                assert_eq!(cause.proposed, 5);
                assert_eq!(cause.budget, 4);
            }
            other => panic!("expected AiError::Validation, got {other:?}"),
        }
        // Rejection happens before any insertion.
        assert!(tree.is_empty());
    }

    #[tokio::test]
    async fn unknown_parent_is_rejected_before_insertion() {
        let (p, _model) = planner(&[r#"{"categories": [{"title": "C", "brief": "b"}]}"#]);
        let mut tree = TaskTree::new();

        let err = p
            .expand_tree(&mut tree, Some(TaskId(99)), "x")
            .await
            .unwrap_err();

        assert!(matches!(err, AiError::Validation(_)));
        assert!(tree.is_empty());
    }

    #[tokio::test]
    async fn malformed_then_valid_retry_succeeds_on_second_call() {
        let (p, model) = planner(&[
            "Let me think about that instead of answering.",
            HIERARCHY_JSON,
        ]);
        let mut tree = TaskTree::new();

        let leaves = p.expand_tree(&mut tree, None, "retry me").await.unwrap();

        assert_eq!(leaves.len(), 4);
        assert_eq!(model.calls(), 2, "exactly one retry after the bad parse");
    }

    #[tokio::test]
    async fn permanently_malformed_output_falls_back_to_single_whole_prompt_leaf() {
        let (p, model) = planner(&[
            "garbage",
            "{\"categories\": \"not an array\"",
            "[[], [], []]",
        ]);
        let mut tree = TaskTree::new();
        const PROMPT: &str = "Ship the feature flag system end to end";

        let leaves = p.expand_tree(&mut tree, None, PROMPT).await.unwrap();

        assert_eq!(leaves.len(), 1);
        let node = tree.get(leaves[0]).unwrap();
        assert_eq!(
            node.brief, PROMPT,
            "fallback brief = whole clarified prompt"
        );
        assert_eq!(node.parent, None);
        assert_eq!(model.calls(), 3, "1 attempt + 2 retries before fallback");
        assert!(tree.check_invariants().is_ok());
    }

    #[tokio::test]
    async fn empty_categories_is_unusable_and_falls_back_after_retries() {
        let (p, model) = planner(&[r#"{"categories": []}"#, "{}"]);
        let mut tree = TaskTree::new();

        let leaves = p
            .expand_tree(&mut tree, None, "do the empty-plan dance")
            .await
            .unwrap();

        assert_eq!(leaves.len(), 1);
        assert_eq!(
            tree.get(leaves[0]).unwrap().brief,
            "do the empty-plan dance"
        );
        // 1 initial attempt + both retries see unusable plans before the
        // fallback fires.
        assert_eq!(model.calls(), 3);
    }

    #[tokio::test]
    async fn request_carries_contract_temperature_and_schema_name() {
        let (p, model) = planner(&[HIERARCHY_JSON]);
        let mut tree = TaskTree::new();
        p.expand_tree(&mut tree, None, "the objective")
            .await
            .unwrap();

        let requests = model.requests();
        assert_eq!(
            requests[0].temperature,
            Some(p.config().decomposition_temperature)
        );
        assert!(matches!(
            requests[0].response_format,
            ai_core::ResponseFormat::JsonSchema { ref name, .. } if name == "task_expansion"
        ));
        let prompt_text = requests[0].messages[0].text_content();
        assert!(prompt_text.contains("SELF-CONTAINED"));
        assert!(prompt_text.contains("THREE levels"));
        assert!(prompt_text.contains("the objective"));
    }

    #[tokio::test]
    async fn fenced_and_embedded_plan_json_is_extracted_defensively() {
        let (p, model) = planner(&[
            "Here you go:\n```json\n{\"categories\": [{\"title\": \"Only\", \
             \"brief\": \"whole job\", \"children\": [{\"title\": \"Leaf\", \
             \"brief\": \"do everything described\"}]}]}\n```",
        ]);
        let mut tree = TaskTree::new();

        let leaves = p.expand_tree(&mut tree, None, "x").await.unwrap();

        assert_eq!(leaves.len(), 1);
        assert_eq!(model.calls(), 1);
        assert!(tree.get(leaves[0]).unwrap().parent.is_some());
    }
}
