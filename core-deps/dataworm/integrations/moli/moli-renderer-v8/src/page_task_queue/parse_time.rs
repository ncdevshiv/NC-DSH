use std::collections::VecDeque;

use super::*;

impl PageTaskQueue {
    pub(crate) fn parse_time_document_script_sender(
        &self,
    ) -> tokio::sync::mpsc::UnboundedSender<ParseTimeDocumentScriptEvent> {
        self.parse_time_document_script_source.sender()
    }

    pub(crate) fn parser_boundary_sender(&self) -> tokio::sync::mpsc::UnboundedSender<PageTask> {
        self.task_source.parser_boundary_sender()
    }

    pub(crate) fn enqueue_parser_boundary(&mut self, task: PageTask) {
        self.task_source.enqueue_parser_boundary_local(task);
    }

    pub(crate) fn enqueue_parse_time_document_script_task(
        &mut self,
        task: Option<crate::document_script_scheduler::ParseTimeDocumentScriptTask>,
    ) {
        if let Some(task) = task {
            self.parse_time_document_script_source
                .enqueue_local(ParseTimeDocumentScriptEvent::ready_task(task));
        }
    }

    pub(crate) fn accept_ready_parse_time_wakes(&mut self) {
        self.task_source.accept_ready_wakes();
        self.parse_time_document_script_source.accept_ready_wakes();
    }

    /// Admits producer payloads that are already resident and reports whether
    /// the parse-time Document owner has concrete work to consume.
    ///
    /// This is used when an open main-Document stream resumes without new
    /// parser bytes. It never waits and does not treat an outstanding
    /// reevaluation credit as runnable work.
    pub(crate) fn admit_ready_parse_time_document_work(&mut self) -> bool {
        self.accept_ready_parse_time_wakes();
        !self.parse_time_is_empty()
    }

    #[cfg(test)]
    pub(crate) fn enqueue_parse_time_document_script_event(
        &mut self,
        event: ParseTimeDocumentScriptEvent,
    ) {
        self.parse_time_document_script_source.enqueue_local(event);
    }

    pub(crate) fn parse_time_pop_front(&mut self) -> Option<PageTask> {
        self.task_source.pop_front_local_only()
    }

    pub(crate) fn parse_time_front(&self) -> Option<&PageTask> {
        self.task_source.front_local_only()
    }

    pub(crate) fn parse_time_document_script_pop_front(
        &mut self,
    ) -> Option<ParseTimeDocumentScriptEvent> {
        self.parse_time_document_script_source
            .pop_front_local_only()
    }

    pub(crate) fn parse_time_document_script_front(&self) -> Option<&ParseTimeDocumentScriptEvent> {
        self.parse_time_document_script_source.front_local_only()
    }

    pub(crate) fn parse_time_is_empty(&self) -> bool {
        self.task_source.is_empty_local_only()
            && self.parse_time_document_script_source.is_empty_local_only()
    }

    pub(crate) async fn wait_for_parse_time_injected_task_arrival_without_timeout(
        &mut self,
    ) -> bool {
        tokio::select! {
            biased;
            arrived = self.parse_time_document_script_source.wait_for_local_wake_arrival() => arrived,
            arrived = self.task_source.wait_for_local_wake_arrival() => arrived,
        }
    }

    pub(crate) fn take_parse_time_document_script_events(
        &mut self,
    ) -> Vec<ParseTimeDocumentScriptEvent> {
        self.parse_time_document_script_source.with_tasks_mut(
            |tasks: &mut VecDeque<ParseTimeDocumentScriptEvent>| tasks.drain(..).collect(),
        )
    }

    pub(crate) fn take_parse_time_lifecycle_work(&mut self) -> Vec<PostParsePageOwnedWork> {
        self.task_source.with_tasks_mut(|tasks| {
            tasks
                .drain(..)
                .map(PostParseLifecycleWork::from_parse_time_page_task)
                .map(PostParsePageOwnedWork::lifecycle_work)
                .collect()
        })
    }
}
