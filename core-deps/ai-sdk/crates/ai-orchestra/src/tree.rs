//! The task tree: decomposed objectives with legal status transitions,
//! dependency mechanics, and completion propagation.
//!
//! A [`TaskTree`] is a forest of [`TaskNode`]s stored in a
//! `BTreeMap<TaskId, TaskNode>`. Every node carries a **brief** —
//! self-contained instructions a fresh agent could execute without any other
//! context — an optional parent, explicit dependencies, and a
//! [`NodeStatus`] that may only move along the legal-transition map below
//! (anything else is a typed [`TreeError::IllegalTransition`]).
//!
//! ```text
//! Pending ──▶ InProgress ──▶ Completed   (terminal)
//!    │ ▲            │  │
//!    │ │retry       │  └─▶ Cancelled   (terminal)
//!    ▼ │            ▼
//! Blocked ◀────── Failed ──▶ Pending (retry)
//!    │
//!    └─▶ Pending (on unblock)
//! ```
//!
//! Blocking is **explicit only**: the tree never auto-blocks a node when its
//! dependencies are incomplete. Callers mark nodes `Blocked` while they wait;
//! [`TaskTree::propagate_completion`] moves them back to `Pending` once every
//! dependency has reached `Completed`.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::tree::NodeStatus::{Blocked, Cancelled, Completed, Failed, InProgress, Pending};

/// Stable identifier of one task node, displayed as `#7`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TaskId(pub u64);

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// Lifecycle state of a task node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum NodeStatus {
    /// Waiting to start; the default state of freshly added tasks.
    Pending,
    /// Work has been delegated and is running (`attempts` was bumped).
    InProgress,
    /// Finished successfully. Terminal.
    Completed,
    /// Finished unsuccessfully; may retry via `Failed → Pending`. Terminal
    /// until retried.
    Failed,
    /// Parked waiting on dependencies (or an external condition); unblocks
    /// to `Pending`.
    Blocked,
    /// Withdrawn before completion. Terminal.
    Cancelled,
}

impl NodeStatus {
    /// The set of statuses this status may legally move to.
    ///
    /// This map IS the state machine — every transition not listed here is
    /// rejected by [`TaskTree::set_status`]:
    ///
    /// - `Pending → {InProgress, Blocked, Cancelled}` — start, park, or drop.
    /// - `InProgress → {Completed, Failed, Cancelled}` — work ends one way or
    ///   another (a mid-run park must go through failure/cancellation).
    /// - `Completed → {}` — terminal.
    /// - `Failed → {Pending}` — explicit retry.
    /// - `Blocked → {Pending}` — on unblock (dependencies satisfied).
    /// - `Cancelled → {}` — terminal.
    #[must_use]
    pub fn transitions(self) -> &'static [NodeStatus] {
        match self {
            Pending => &[InProgress, Blocked, Cancelled],
            InProgress => &[Completed, Failed, Cancelled],
            Completed => &[],
            Failed => &[Pending],
            Blocked => &[Pending],
            Cancelled => &[],
        }
    }

    /// Whether moving from `self` to `to` is legal per the transition map.
    #[must_use]
    pub fn can_transition_to(self, to: NodeStatus) -> bool {
        self.transitions().contains(&to)
    }

    /// All statuses, in declaration order.
    #[must_use]
    pub fn all() -> [NodeStatus; 6] {
        [Pending, InProgress, Completed, Failed, Blocked, Cancelled]
    }
}

impl fmt::Display for NodeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Pending => "pending",
            InProgress => "in-progress",
            Completed => "completed",
            Failed => "failed",
            Blocked => "blocked",
            Cancelled => "cancelled",
        };
        f.write_str(name)
    }
}

/// Errors produced by task-tree operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TreeError {
    /// No node with this id exists.
    #[error("unknown task {0}")]
    UnknownTask(TaskId),
    /// The requested status change violates the transition map.
    #[error("illegal transition {from} -> {to} for task {id}")]
    IllegalTransition {
        /// The task whose status was to change.
        id: TaskId,
        /// Current status.
        from: NodeStatus,
        /// Requested status.
        to: NodeStatus,
    },
    /// Adding the dependency would make it depend on itself transitively.
    #[error("dependency cycle: task {task} cannot depend on task {depends_on}")]
    DependencyCycle {
        /// The task gaining the dependency.
        task: TaskId,
        /// The (transitively) downstream task that would loop back.
        depends_on: TaskId,
    },
    /// The parent id given to [`TaskTree::add_child`] does not exist.
    #[error("cannot attach child: unknown parent {0}")]
    UnknownParent(TaskId),
    /// The node already has a different parent (single-parent invariant).
    #[error("task {id} already attached to parent {parent}")]
    AlreadyAttached {
        /// The child in question.
        id: TaskId,
        /// Its existing parent.
        parent: TaskId,
    },
}

/// One task in the tree: a self-contained unit of delegable work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskNode {
    /// Unique, monotonically increasing identifier.
    pub id: TaskId,
    /// Parent task, if any. Roots have none.
    pub parent: Option<TaskId>,
    /// Short human-facing summary.
    pub title: String,
    /// Self-contained instructions a FRESH agent could execute with no other
    /// context — results, constraints, and prohibitions included.
    pub brief: String,
    /// Lifecycle state; changes only through the legal transition map.
    pub status: NodeStatus,
    /// Agent id currently assigned to execute this task, if any.
    pub assigned: Option<String>,
    /// How many times work has STARTED on this task (bumped on every entry
    /// into `InProgress`).
    pub attempts: u32,
    /// Tasks that must be `Completed` before this one may run.
    pub dependencies: Vec<TaskId>,
    /// Direct children (subtasks), in insertion order.
    pub children: Vec<TaskId>,
    /// Reason recorded by the most recent [`TaskTree::mark_failed`], if any.
    pub last_failure: Option<String>,
}

impl TaskNode {
    /// Whether every dependency of this node is `Completed`, judged against
    /// the given lookup table.
    #[must_use]
    pub fn deps_satisfied(&self, statuses: &BTreeMap<TaskId, NodeStatus>) -> bool {
        self.dependencies
            .iter()
            .all(|dep| statuses.get(dep) == Some(&Completed))
    }
}

/// A forest of task nodes with enforced status transitions, dependency
/// acyclicity, and completion propagation.
///
/// Storage is a `BTreeMap<TaskId, TaskNode>` keyed by auto-incremented ids,
/// so snapshots iterate in deterministic id order.
#[derive(Debug, Clone, Default)]
pub struct TaskTree {
    nodes: BTreeMap<TaskId, TaskNode>,
    next_id: u64,
}

impl TaskTree {
    /// Creates an empty tree.
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes: BTreeMap::new(),
            next_id: 0,
        }
    }

    // -- construction ------------------------------------------------------

    /// Adds a root task (no parent) and returns its freshly minted id.
    ///
    /// Dependencies are allowed on roots too (e.g. across-tree ordering is
    /// modelled as sibling roots depending on each other).
    pub fn add_root(
        &mut self,
        title: impl Into<String>,
        brief: impl Into<String>,
        dependencies: Vec<TaskId>,
    ) -> Result<TaskId, TreeError> {
        for dep in &dependencies {
            self.ensure_exists(*dep)?;
        }
        let id = self.mint_id();
        debug_assert!(
            !dependencies.contains(&id),
            "fresh id cannot depend on itself"
        );
        self.nodes.insert(
            id,
            TaskNode {
                id,
                parent: None,
                title: title.into(),
                brief: brief.into(),
                status: Pending,
                assigned: None,
                attempts: 0,
                dependencies,
                children: Vec::new(),
                last_failure: None,
            },
        );
        Ok(id)
    }

    /// Adds a child under `parent` and returns its freshly minted id.
    ///
    /// The child starts `Pending`; dependencies are validated for existence
    /// and acyclicity up front (a brand-new `Pending` node can never close a
    /// cycle via the *status* graph, but it CAN via the *dependency* graph if
    /// some existing node already (transitively) depends on it — impossible
    /// for a fresh id — so here only existence is checked; see
    /// [`attach_dependency`](Self::attach_dependency) for cycle logic).
    pub fn add_child(
        &mut self,
        parent: TaskId,
        title: impl Into<String>,
        brief: impl Into<String>,
        dependencies: Vec<TaskId>,
    ) -> Result<TaskId, TreeError> {
        if !self.nodes.contains_key(&parent) {
            return Err(TreeError::UnknownParent(parent));
        }
        for dep in &dependencies {
            // A fresh child cannot be depended on by anything yet, so no
            // edge set here can close a cycle; existence is all we need.
            self.ensure_exists(*dep)?;
        }
        let id = self.mint_id();
        self.nodes.insert(
            id,
            TaskNode {
                id,
                parent: Some(parent),
                title: title.into(),
                brief: brief.into(),
                status: Pending,
                assigned: None,
                attempts: 0,
                dependencies,
                children: Vec::new(),
                last_failure: None,
            },
        );
        self.nodes
            .get_mut(&parent)
            .expect("parent existence checked above")
            .children
            .push(id);
        Ok(id)
    }

    // NOTE: minting happens AFTER validation so failed calls never burn ids.

    /// Makes `task` depend on `depends_on`: the latter must reach `Completed`
    /// before the former may leave the ready set.
    ///
    /// Rejects the edge if it would close a dependency cycle (DFS over the
    /// depends-on edges: adding `task → depends_on` closes a cycle iff
    /// `depends_on` already transitively depends on `task`) or if either side
    /// does not exist.
    pub fn attach_dependency(&mut self, task: TaskId, depends_on: TaskId) -> Result<(), TreeError> {
        self.ensure_exists(task)?;
        self.ensure_exists(depends_on)?;
        if task == depends_on {
            return Err(TreeError::DependencyCycle { task, depends_on });
        }
        if self.reaches(depends_on, task) {
            return Err(TreeError::DependencyCycle { task, depends_on });
        }
        let node = self.nodes.get_mut(&task).expect("existence checked above");
        if !node.dependencies.contains(&depends_on) {
            node.dependencies.push(depends_on);
        }
        Ok(())
    }

    /// Removes `depends_on` from `task`'s dependency list, if present.
    /// Returns whether an edge was actually removed.
    pub fn detach_dependency(
        &mut self,
        task: TaskId,
        depends_on: TaskId,
    ) -> Result<bool, TreeError> {
        self.ensure_exists(task)?;
        self.ensure_exists(depends_on)?;
        let node = self.nodes.get_mut(&task).expect("existence checked above");
        let before = node.dependencies.len();
        node.dependencies.retain(|dep| *dep != depends_on);
        Ok(node.dependencies.len() != before)
    }

    // -- status machine ----------------------------------------------------

    /// Moves `id` to `to`, validating the move against the transition map.
    ///
    /// Entering `InProgress` bumps `attempts` and clears any stale
    /// `last_failure`; entering `Failed` via [`mark_failed`](Self::mark_failed)
    /// records the reason instead. Blocking is explicit only — this method
    /// never derives a status from dependencies.
    pub fn set_status(&mut self, id: TaskId, to: NodeStatus) -> Result<(), TreeError> {
        let node = self.nodes.get_mut(&id).ok_or(TreeError::UnknownTask(id))?;
        if !node.status.can_transition_to(to) {
            return Err(TreeError::IllegalTransition {
                id,
                from: node.status,
                to,
            });
        }
        if to == InProgress {
            node.attempts = node.attempts.saturating_add(1);
            node.last_failure = None;
        }
        node.status = to;
        Ok(())
    }

    /// Marks `id` as [`Failed`] with a human-readable reason (stored on the
    /// node as `last_failure`). The move must still be legal — usually from
    /// `InProgress`.
    pub fn mark_failed(&mut self, id: TaskId, reason: impl Into<String>) -> Result<(), TreeError> {
        let reason = reason.into();
        self.set_status(id, Failed)?;
        if let Some(node) = self.nodes.get_mut(&id) {
            node.last_failure = Some(reason);
        }
        Ok(())
    }

    /// Retries a failed task: `Failed → Pending`, clearing the recorded
    /// failure reason.
    pub fn retry(&mut self, id: TaskId) -> Result<(), TreeError> {
        self.set_status(id, Pending)?;
        if let Some(node) = self.nodes.get_mut(&id) {
            node.last_failure = None;
        }
        Ok(())
    }

    /// Completion propagation for a node that just reached `Completed`.
    ///
    /// Scans direct dependents of `id` that are `Blocked` and moves each one
    /// whose ENTIRE dependency list is now `Completed` back to `Pending`
    /// (legal per `Blocked → Pending`). Returns the ids that were unblocked,
    /// in ascending order. Dependents still waiting on anything stay blocked;
    /// non-blocked dependents are untouched (they were never parked).
    pub fn propagate_completion(&mut self, id: TaskId) -> Result<Vec<TaskId>, TreeError> {
        self.ensure_exists(id)?;
        let dependents: Vec<TaskId> = self
            .nodes
            .values()
            .filter(|node| node.status == Blocked && node.dependencies.contains(&id))
            .map(|node| node.id)
            .collect();
        let mut unblocked = Vec::new();
        for dep_id in dependents {
            let ready = {
                let node = &self.nodes[&dep_id];
                node.deps_satisfied(&self.statuses())
            };
            if ready && self.set_status(dep_id, Pending).is_ok() {
                unblocked.push(dep_id);
            }
        }
        unblocked.sort_unstable();
        Ok(unblocked)
    }

    /// Assigns `agent` to `id` (bookkeeping only; does not touch status).
    /// Pass `None` to clear. Returns the previous assignment.
    pub fn assign(
        &mut self,
        id: TaskId,
        agent: Option<String>,
    ) -> Result<Option<String>, TreeError> {
        let node = self.nodes.get_mut(&id).ok_or(TreeError::UnknownTask(id))?;
        Ok(std::mem::replace(&mut node.assigned, agent))
    }

    // -- queries -----------------------------------------------------------

    /// Tasks that could start RIGHT NOW: `Pending` and every dependency
    /// `Completed`, in ascending id order.
    #[must_use]
    pub fn next_ready(&self) -> Vec<TaskId> {
        let statuses = self.statuses();
        self.nodes
            .values()
            .filter(|node| node.status == Pending && node.deps_satisfied(&statuses))
            .map(|node| node.id)
            .collect()
    }

    /// `id` plus all of its descendants (via children edges), pre-order:
    /// the node first, then each child's subtree in turn.
    pub fn subtree(&self, id: TaskId) -> Result<Vec<TaskId>, TreeError> {
        self.ensure_exists(id)?;
        let mut out = Vec::new();
        let mut queue = VecDeque::from([id]);
        while let Some(current) = queue.pop_front() {
            out.push(current);
            let node = &self.nodes[&current];
            queue.extend(node.children.iter().copied());
        }
        Ok(out)
    }

    /// Snapshot iteration over every node in ascending id order.
    pub fn iter(&self) -> impl Iterator<Item = &TaskNode> {
        self.nodes.values()
    }

    /// Look up one node.
    #[must_use]
    pub fn get(&self, id: TaskId) -> Option<&TaskNode> {
        self.nodes.get(&id)
    }

    /// Number of nodes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the tree has no nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Node count grouped by status (statuses with zero nodes are absent).
    #[must_use]
    pub fn counts_by_status(&self) -> BTreeMap<NodeStatus, usize> {
        let mut counts: BTreeMap<NodeStatus, usize> = BTreeMap::new();
        for node in self.nodes.values() {
            *counts.entry(node.status).or_default() += 1;
        }
        counts
    }

    /// Ids of root tasks (no parent), in ascending order.
    #[must_use]
    pub fn roots(&self) -> Vec<TaskId> {
        self.nodes
            .values()
            .filter(|node| node.parent.is_none())
            .map(|node| node.id)
            .collect()
    }

    // -- internals ---------------------------------------------------------

    fn mint_id(&mut self) -> TaskId {
        let id = TaskId(self.next_id);
        self.next_id += 1;
        id
    }

    fn ensure_exists(&self, id: TaskId) -> Result<(), TreeError> {
        if self.nodes.contains_key(&id) {
            Ok(())
        } else {
            Err(TreeError::UnknownTask(id))
        }
    }

    /// Current status of every node (used to judge dependency satisfaction).
    #[must_use]
    pub fn statuses(&self) -> BTreeMap<TaskId, NodeStatus> {
        self.nodes
            .iter()
            .map(|(id, node)| (*id, node.status))
            .collect()
    }

    /// DFS over depends-on edges: does `from` transitively depend on `to`
    /// via at least ONE edge? (The trivial zero-edge case `from == to` is
    /// deliberately NOT a match — otherwise every node would "reach"
    /// itself.)
    fn reaches(&self, from: TaskId, to: TaskId) -> bool {
        let Some(start) = self.nodes.get(&from) else {
            return false;
        };
        let mut seen = BTreeSet::new();
        let mut stack: Vec<TaskId> = start.dependencies.clone();
        while let Some(current) = stack.pop() {
            if current == to {
                return true;
            }
            if seen.insert(current) {
                if let Some(node) = self.nodes.get(&current) {
                    stack.extend(node.dependencies.iter().copied());
                }
            }
        }
        false
    }

    // -- invariants (used by tests and available for wave-B assertions) ----

    /// Verifies structural invariants: single parent (child's `parent` points
    /// back at a node listing it under `children`; roots have none),
    /// acyclic dependencies, unique ids, and monotonic id allocation.
    /// Returns `Err` describing the first violation found.
    pub fn check_invariants(&self) -> Result<(), String> {
        // Single parent + back-pointer consistency + unique ids.
        let mut has_parent = BTreeSet::new();
        for node in self.nodes.values() {
            let Some(parent) = node.parent else {
                continue;
            };
            let Some(parent_node) = self.nodes.get(&parent) else {
                return Err(format!("{} names missing parent {}", node.id, parent));
            };
            if parent_node
                .children
                .iter()
                .filter(|c| **c == node.id)
                .count()
                != 1
            {
                return Err(format!(
                    "{} listed {} times among children of {}",
                    node.id,
                    parent_node
                        .children
                        .iter()
                        .filter(|c| **c == node.id)
                        .count(),
                    parent
                ));
            }
            if !has_parent.insert(node.id) {
                return Err(format!("{} claimed by two parents", node.id));
            }
        }
        for node in self.nodes.values() {
            if node.parent.is_none() && has_parent.contains(&node.id) {
                return Err(format!("root {} appears in some children list", node.id));
            }
        }
        // Acyclicity of the dependency graph.
        for node in self.nodes.values() {
            if self.reaches(node.id, node.id) {
                return Err(format!("{} sits on a dependency cycle", node.id));
            }
        }
        // Monotonic allocation: every id < next_id, and next_id covers max.
        if let Some(max) = self.nodes.keys().next_back() {
            if max.0 >= self.next_id {
                return Err(format!("id {} >= next_id {}", max.0, self.next_id));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn tree_with_root() -> (TaskTree, TaskId) {
        let mut t = TaskTree::new();
        let r = t.add_root("root", "do the thing", vec![]).unwrap();
        (t, r)
    }

    // -- TaskId ------------------------------------------------------------

    #[test]
    fn task_id_displays_hash_prefixed() {
        assert_eq!(TaskId(7).to_string(), "#7");
        assert_eq!(TaskId(0).to_string(), "#0");
    }

    // -- transition map ----------------------------------------------------

    #[test]
    fn transition_map_matches_spec_exactly() {
        let expected: &[(NodeStatus, &[NodeStatus])] = &[
            (Pending, &[InProgress, Blocked, Cancelled]),
            (InProgress, &[Completed, Failed, Cancelled]),
            (Completed, &[]),
            (Failed, &[Pending]),
            (Blocked, &[Pending]),
            (Cancelled, &[]),
        ];
        for (from, tos) in expected {
            assert_eq!(from.transitions(), *tos, "row for {from:?}");
            for to in NodeStatus::all() {
                assert_eq!(from.can_transition_to(to), tos.contains(&to));
            }
        }
    }

    #[test]
    fn terminal_states_reject_everything() {
        let mut t = TaskTree::new();
        let done = t.add_root("done", "b", vec![]).unwrap();
        t.set_status(done, InProgress).unwrap();
        t.set_status(done, Completed).unwrap();
        // Completed is terminal: no moves at all, not even to itself.
        for to in NodeStatus::all() {
            assert_eq!(
                t.set_status(done, to).unwrap_err(),
                TreeError::IllegalTransition {
                    id: done,
                    from: Completed,
                    to
                }
            );
        }
        // Same for Cancelled.
        let dead = t.add_root("dead", "b", vec![]).unwrap();
        t.set_status(dead, Cancelled).unwrap();
        for to in NodeStatus::all() {
            assert!(t.set_status(dead, to).is_err());
        }
    }

    #[test]
    fn rejection_paths_and_full_lifecycle() {
        let mut t = TaskTree::new();
        let a = t.add_root("a", "b", vec![]).unwrap();

        // Pending cannot jump straight to Completed/Failed.
        assert!(t.set_status(a, Completed).is_err());
        assert!(t.set_status(a, Failed).is_err());

        t.set_status(a, InProgress).unwrap();
        assert_eq!(t.get(a).unwrap().attempts, 1);

        // InProgress cannot go sideways to Blocked/Pending.
        assert!(t.set_status(a, Blocked).is_err());
        assert!(t.set_status(a, Pending).is_err());

        // Failure + retry loop bumps attempts each restart.
        t.mark_failed(a, "boom").unwrap();
        assert_eq!(t.get(a).unwrap().last_failure.as_deref(), Some("boom"));
        t.retry(a).unwrap();
        assert_eq!(t.get(a).unwrap().last_failure, None);
        t.set_status(a, InProgress).unwrap();
        assert_eq!(t.get(a).unwrap().attempts, 2);

        t.set_status(a, Completed).unwrap();
        assert!(t.mark_failed(a, "late").is_err());
    }

    #[test]
    fn unknown_task_errors_are_typed() {
        let mut t = TaskTree::new();
        let ghost = TaskId(42);
        assert_eq!(
            t.set_status(ghost, Pending).unwrap_err(),
            TreeError::UnknownTask(ghost)
        );
        assert_eq!(
            t.attach_dependency(ghost, ghost).unwrap_err(),
            TreeError::UnknownTask(ghost)
        );
        assert_eq!(
            t.propagate_completion(ghost).unwrap_err(),
            TreeError::UnknownTask(ghost)
        );
        assert_eq!(t.subtree(ghost).unwrap_err(), TreeError::UnknownTask(ghost));
        assert_eq!(
            t.add_child(ghost, "x", "y", vec![]).unwrap_err(),
            TreeError::UnknownParent(ghost)
        );
    }

    // -- construction ------------------------------------------------------

    #[test]
    fn add_root_burns_no_ids_on_failure() {
        let mut t = TaskTree::new();
        let r = t.add_root("r", "b", vec![TaskId(9)]).unwrap_err();
        assert_eq!(r, TreeError::UnknownTask(TaskId(9)));
        let real = t.add_root("r", "b", vec![]).unwrap();
        assert_eq!(real, TaskId(0), "failed call must not consume an id");
    }

    #[test]
    fn child_parent_backlink_and_duplicate_dep_is_idempotent() {
        let (mut t, r) = tree_with_root();
        let c = t.add_child(r, "c", "b", vec![]).unwrap();
        assert_eq!(t.get(c).unwrap().parent, Some(r));
        assert_eq!(t.get(r).unwrap().children, vec![c]);

        // Duplicate dependency edge is accepted but stored once.
        t.attach_dependency(c, r).unwrap();
        t.attach_dependency(c, r).unwrap();
        assert_eq!(t.get(c).unwrap().dependencies, vec![r]);
        assert!(t.detach_dependency(c, r).unwrap());
        assert!(!t.detach_dependency(c, r).unwrap());
    }

    #[test]
    fn self_dependency_is_a_cycle() {
        let (mut t, r) = tree_with_root();
        assert_eq!(
            t.attach_dependency(r, r).unwrap_err(),
            TreeError::DependencyCycle {
                task: r,
                depends_on: r
            }
        );
        // A child may legally depend on its own parent: a brand-new node
        // cannot be reached by any existing depends-on path, so no cycle can
        // close through construction-time edges.
        let c = t.add_child(r, "c", "b", vec![r]).unwrap();
        assert_eq!(t.get(c).unwrap().dependencies, vec![r]);
        assert!(t.check_invariants().is_ok());
        // ...but a later back-edge from parent into the child IS a cycle.
        assert_eq!(
            t.attach_dependency(r, c).unwrap_err(),
            TreeError::DependencyCycle {
                task: r,
                depends_on: c
            }
        );
    }

    // -- acyclicity --------------------------------------------------------

    #[test]
    fn attach_dependency_rejects_direct_and_transitive_cycles() {
        let mut t = TaskTree::new();
        let a = t.add_root("a", "b", vec![]).unwrap();
        let b = t.add_root("b", "b", vec![]).unwrap();
        let c = t.add_root("c", "b", vec![]).unwrap();

        t.attach_dependency(b, a).unwrap(); // b depends on a
        t.attach_dependency(c, b).unwrap(); // c depends on b (=> transitively a)

        // Direct back-edge: a depends on b would close a <- b <- a.
        assert_eq!(
            t.attach_dependency(a, b).unwrap_err(),
            TreeError::DependencyCycle {
                task: a,
                depends_on: b
            }
        );
        // Transitive back-edge: a depends on c closes a <- b <- c <- a.
        assert!(matches!(
            t.attach_dependency(a, c),
            Err(TreeError::DependencyCycle { .. })
        ));
        // Diamond is fine (no cycle): d depends on b AND c.
        let d = t.add_root("d", "b", vec![]).unwrap();
        t.attach_dependency(d, b).unwrap();
        t.attach_dependency(d, c).unwrap();
        assert!(
            t.check_invariants().is_ok(),
            "diamond must satisfy invariants"
        );
    }

    // -- ready set ---------------------------------------------------------

    #[test]
    fn next_ready_respects_dependencies() {
        let mut t = TaskTree::new();
        let r = t.add_root("r", "b", vec![]).unwrap();
        let x = t.add_child(r, "x", "bx", vec![r]).unwrap();
        let y = t.add_child(r, "y", "by", vec![]).unwrap();
        let _z = t.add_child(y, "z", "bz", vec![x]).unwrap();

        // r,y ready; x waits on r; z waits on x.
        assert_eq!(t.next_ready(), vec![r, y]);

        t.set_status(r, InProgress).unwrap();
        assert_eq!(t.next_ready(), vec![y], "started tasks leave the ready set");

        t.set_status(r, Completed).unwrap();
        assert_eq!(t.next_ready(), vec![x, y]);

        // Explicitly blocked nodes are NOT ready even with deps done.
        t.set_status(y, Blocked).unwrap();
        assert_eq!(t.next_ready(), vec![x]);
    }

    // -- propagation -------------------------------------------------------

    #[test]
    fn propagate_completion_unblocks_exactly_the_right_nodes() {
        let mut t = TaskTree::new();
        let r = t.add_root("r", "b", vec![]).unwrap();
        let x = t.add_child(r, "x", "bx", vec![r]).unwrap(); // unblocks with r
        let y = t.add_child(r, "y", "by", vec![r]).unwrap(); // unblocks with r
        let z = t.add_child(r, "z", "bz", vec![r, y]).unwrap(); // needs r AND y
        let w = t.add_child(r, "w", "bw", vec![z]).unwrap(); // waits on z

        for id in [x, y, z, w] {
            t.set_status(id, Blocked).unwrap();
        }
        t.set_status(r, InProgress).unwrap();

        // Propagating completion of x unblocks nothing: nobody depends on x,
        // and the source being incomplete is irrelevant — readiness is judged
        // per dependent, caller-driven.
        assert_eq!(t.propagate_completion(x).unwrap(), Vec::<TaskId>::new());

        // r completes: exactly x and y become Pending; z still waits on y.
        t.set_status(r, Completed).unwrap();
        let mut got = t.propagate_completion(r).unwrap();
        got.sort_unstable();
        assert_eq!(got, vec![x, y]);
        assert_eq!(t.get(z).unwrap().status, Blocked);

        // y completes: now z's deps are all Completed -> unblocked.
        t.set_status(y, InProgress).unwrap();
        t.set_status(y, Completed).unwrap();
        assert_eq!(t.propagate_completion(y).unwrap(), vec![z]);

        // Cascade one more level.
        t.set_status(z, InProgress).unwrap();
        t.set_status(z, Completed).unwrap();
        assert_eq!(t.propagate_completion(z).unwrap(), vec![w]);
    }

    #[test]
    fn propagate_completion_requires_completed_source() {
        let (mut t, r) = tree_with_root();
        let c = t.add_child(r, "c", "b", vec![r]).unwrap();
        t.set_status(c, Blocked).unwrap();
        // Source exists but isn't Completed: dependents stay Blocked...
        // (propagation is caller-driven; it only looks at dep satisfaction.)
        let _ = t.propagate_completion(r).unwrap();
        assert_eq!(t.get(c).unwrap().status, Blocked);
    }

    // -- structure queries -------------------------------------------------

    #[test]
    fn subtree_counts_and_roots() {
        let mut t = TaskTree::new();
        let r = t.add_root("r", "b", vec![]).unwrap();
        let a = t.add_child(r, "a", "ba", vec![]).unwrap();
        let _ = t.add_child(a, "g", "bg", vec![]).unwrap();
        let r2 = t.add_root("r2", "b2", vec![]).unwrap();

        assert_eq!(t.subtree(r).unwrap(), vec![r, a, TaskId(2)]);
        assert_eq!(t.roots(), vec![r, r2]);
        assert_eq!(t.len(), 4);

        t.set_status(r, InProgress).unwrap();
        // mark_failed must come from a running task.
        t.set_status(a, InProgress).unwrap();
        t.mark_failed(a, "nope").unwrap();
        assert_eq!(t.get(a).unwrap().last_failure.as_deref(), Some("nope"));
        let counts = t.counts_by_status();
        // g and r2 are Pending; r is InProgress; a Failed.
        assert_eq!(counts.get(&Pending), Some(&2));
        assert_eq!(counts.get(&InProgress), Some(&1));
        assert_eq!(counts.get(&Failed), Some(&1));
        let total: usize = counts.values().sum();
        assert_eq!(total, t.len());

        // Snapshot iterates in ascending id order.
        let ids: Vec<TaskId> = t.iter().map(|n| n.id).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn assign_records_agent() {
        let (mut t, r) = tree_with_root();
        assert_eq!(t.assign(r, Some("agent-1".into())).unwrap(), None);
        assert_eq!(
            t.assign(r, Some("agent-2".into())).unwrap(),
            Some("agent-1".into())
        );
        assert_eq!(t.get(r).unwrap().assigned.as_deref(), Some("agent-2"));
    }

    // -- property test -----------------------------------------------------

    #[derive(Debug, Clone)]
    enum Op {
        AddRoot,
        AddChild(u8),
        AttachDep(u8, u8),
        DetachDep(u8, u8),
        SetStatus(u8, u8),
        Propagate(u8),
        Retry(u8),
        Subtree(u8),
    }

    fn op_strategy() -> impl Strategy<Value = Op> {
        prop_oneof![
            Just(Op::AddRoot),
            any::<u8>().prop_map(Op::AddChild),
            (any::<u8>(), any::<u8>()).prop_map(|(a, b)| Op::AttachDep(a, b)),
            (any::<u8>(), any::<u8>()).prop_map(|(a, b)| Op::DetachDep(a, b)),
            (any::<u8>(), any::<u8>()).prop_map(|(a, s)| Op::SetStatus(a, s)),
            any::<u8>().prop_map(Op::Propagate),
            any::<u8>().prop_map(Op::Retry),
            any::<u8>().prop_map(Op::Subtree),
        ]
    }

    fn pick(ids: &[TaskId], idx: u8) -> TaskId {
        ids[idx as usize % ids.len()]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        #[test]
        fn random_operations_preserve_invariants(ops in prop::collection::vec(op_strategy(), 0..96)) {
            let mut t = TaskTree::new();
            let mut expected_attempts: BTreeMap<TaskId, u32> = BTreeMap::new();

            for op in ops {
                let ids: Vec<TaskId> = t.iter().map(|n| n.id).collect();
                // Index-based ops need at least one node to pick from.
                if ids.is_empty()
                    && !matches!(op, Op::AddRoot)
                {
                    continue;
                }
                match op {
                    Op::AddRoot => {
                        let id = t.add_root("root", "brief", vec![]).unwrap();
                        prop_assert_eq!(t.get(id).unwrap().status, NodeStatus::Pending);
                    }
                    Op::AddChild(idx) => {
                        let parent = pick(&ids, idx);
                        let id = t.add_child(parent, "child", "brief", vec![]).unwrap();
                        prop_assert_eq!(t.get(id).unwrap().parent, Some(parent));
                    }
                    Op::AttachDep(a, b) => {
                        let (ta, tb) = (pick(&ids, a), pick(&ids, b));
                        match t.attach_dependency(ta, tb) {
                            Ok(()) => {}
                            Err(TreeError::DependencyCycle { .. }) => {
                                // Rejected edge: the graph must STILL be acyclic.
                                prop_assert!(t.check_invariants().is_ok());
                            }
                            Err(e) => {
                                return Err(TestCaseError::fail(format!(
                                    "unexpected attach error: {e}"
                                )));
                            }
                        }
                    }
                    Op::DetachDep(a, b) => {
                        let _ = t.detach_dependency(pick(&ids, a), pick(&ids, b));
                    }
                    Op::SetStatus(idx, raw) => {
                        let target = pick(&ids, idx);
                        let to = NodeStatus::all()[raw as usize % 6];
                        let from = t.get(target).unwrap().status;
                        match t.set_status(target, to) {
                            Ok(()) => {
                                prop_assert_eq!(t.get(target).unwrap().status, to);
                                if to == NodeStatus::InProgress {
                                    *expected_attempts.entry(target).or_insert(0) += 1;
                                }
                            }
                            Err(TreeError::IllegalTransition { id, from: f, to: tt }) => {
                                prop_assert_eq!((id, f, tt), (target, from, to));
                            }
                            Err(e) => {
                                return Err(TestCaseError::fail(format!(
                                    "unexpected status error: {e}"
                                )));
                            }
                        }
                    }
                    Op::Propagate(idx) => {
                        let source = pick(&ids, idx);
                        let unblocked = t.propagate_completion(source).unwrap();
                        for id in &unblocked {
                            let node = t.get(*id).unwrap();
                            prop_assert!(node.deps_satisfied(&t.statuses()));
                            prop_assert_eq!(node.status, NodeStatus::Pending);
                        }
                    }
                    Op::Retry(idx) => {
                        let _ = t.retry(pick(&ids, idx)); // may legally fail; ignore
                    }
                    Op::Subtree(idx) => {
                        let sub = t.subtree(pick(&ids, idx)).unwrap();
                        prop_assert!(!sub.is_empty());
                    }
                }

                // Core invariants after EVERY operation.
                if let Err(violation) = t.check_invariants() {
                    return Err(TestCaseError::fail(violation));
                }
                // Attempts bookkeeping matches what we observed.
                for (id, n) in &expected_attempts {
                    prop_assert_eq!(t.get(*id).unwrap().attempts, *n);
                }
            }
        }
    }
}
