use crate::page_task_queue::{
    PageNavigationAndTraversalTurnAction, PageNavigationAndTraversalTurnOutcome,
    RendererPageNavigationAndTraversalTask,
};

use super::PageVm;

impl PageVm {
    pub(in crate::runtime) fn apply_selected_page_navigation_and_traversal_turn(
        &mut self,
        task: RendererPageNavigationAndTraversalTask,
    ) -> anyhow::Result<PageNavigationAndTraversalTurnOutcome> {
        match task {
            RendererPageNavigationAndTraversalTask::ChildNavigationCommit(task) => self
                .apply_selected_page_child_navigation_commit_turn(task)
                .map(|outcome| {
                    outcome.map_action(PageNavigationAndTraversalTurnAction::ChildNavigationCommit)
                }),
            RendererPageNavigationAndTraversalTask::HistoryTraversal(task) => self
                .apply_selected_page_history_traversal_turn(task)
                .map(|outcome| {
                    outcome.map_action(PageNavigationAndTraversalTurnAction::HistoryTraversal)
                }),
            RendererPageNavigationAndTraversalTask::NavigationApi(task) => self
                .apply_selected_page_navigation_api_task_turn(task)
                .map(|outcome| {
                    outcome.map_action(PageNavigationAndTraversalTurnAction::NavigationApi)
                }),
        }
    }
}
