use super::post_parse_owner::PostParsePageTaskPopBlocker;
use super::*;
use crate::page_task_queue::PostParseLifecycleWork;
use crate::stylesheet_blocking::StylesheetBlockingReadView;

pub(super) async fn wait_for_task_source_load(
    load: Option<crate::planning::SharedScriptSourceLoad>,
) -> bool {
    let Some(load) = load else {
        return std::future::pending().await;
    };
    let _ = load.wait_outcome().await;
    true
}

impl DocumentRuntime {
    fn parse_time_document_processing_action_for_unblocked_task(
        task: PageTask,
    ) -> DocumentProcessingAction {
        let lifecycle_work = PostParseLifecycleWork::from_parse_time_page_task(task);
        DocumentProcessingAction::PostParsePageOwnedWork(Box::new(
            PostParsePageOwnedWork::lifecycle_work(lifecycle_work),
        ))
    }

    #[cfg(test)]
    fn parse_time_primary_wake_source(
        source: DocumentProcessingWakeSource,
    ) -> Option<ParseTimeWakeSource> {
        match source {
            DocumentProcessingWakeSource::InjectedPageTask => {
                Some(ParseTimeWakeSource::InjectedPageTask)
            }
            DocumentProcessingWakeSource::TaskSourceLoadCompletion => {
                Some(ParseTimeWakeSource::TaskSourceLoadCompletion)
            }
        }
    }

    pub(crate) fn has_ready_document_processing_wake(
        &mut self,
        task_queue: &mut PageTaskQueue,
    ) -> bool {
        // Connected style/link events are independent document-owned work. A
        // parser-deferred source wait must not hide an event that is already
        // ready, because that event may release a modulepreload client needed
        // by the blocked script's graph.
        if self.has_pending_ready_connected_style_loads() {
            return true;
        }
        if task_queue
            .front()
            .is_some_and(PageTask::is_waiting_for_source_load)
        {
            return false;
        }
        if self
            .ready_main_parser_deferred_script_action(task_queue)
            .is_some()
        {
            return true;
        }
        if matches!(
            self.post_parse_page_task_pop_blocker(task_queue, false, false),
            Some(PostParsePageTaskPopBlocker::MainParserDeferredScript)
        ) {
            return false;
        }
        if let Some(work) = task_queue.post_parse_front() {
            if work.is_waiting_for_source_load() {
                return false;
            }
            return !self.post_parse_work_is_blocked_by_document_stylesheets(work);
        }
        false
    }

    pub(crate) fn has_ready_parse_time_document_processing_wake(
        &mut self,
        task_queue: &PageTaskQueue,
    ) -> bool {
        if self.has_pending_ready_connected_style_loads() {
            return true;
        }
        if task_queue.parse_time_document_script_front().is_some() {
            return true;
        }
        let Some(task) = task_queue.parse_time_front() else {
            return false;
        };
        !self.page_task_is_blocked_by_document_stylesheets(task)
    }

    #[cfg(test)]
    pub(crate) async fn wait_for_document_processing_wake_source(
        &mut self,
        task_queue: &mut PageTaskQueue,
    ) -> Option<DocumentProcessingWakeSource> {
        if task_queue.complete_ready_source_loads() {
            return Some(DocumentProcessingWakeSource::TaskSourceLoadCompletion);
        }
        let pending_task_source_load = task_queue.pending_task_source_load();
        tokio::select! {
            biased;
            arrived = task_queue.wait_for_injected_task_arrival_without_timeout() => {
                arrived.then_some(DocumentProcessingWakeSource::InjectedPageTask)
            },
            arrived = wait_for_task_source_load(pending_task_source_load) => {
                arrived.then_some(DocumentProcessingWakeSource::TaskSourceLoadCompletion)
            },
        }
    }

    pub(crate) async fn wait_for_parse_time_document_processing_wake_source(
        &mut self,
        task_queue: &mut PageTaskQueue,
    ) -> Option<DocumentProcessingWakeSource> {
        if task_queue.complete_ready_source_loads() {
            return Some(DocumentProcessingWakeSource::TaskSourceLoadCompletion);
        }
        let pending_task_source_load = task_queue.pending_task_source_load();
        tokio::select! {
            biased;
            arrived = task_queue.wait_for_parse_time_injected_task_arrival_without_timeout() => {
                arrived.then_some(DocumentProcessingWakeSource::InjectedPageTask)
            },
            arrived = wait_for_task_source_load(pending_task_source_load) => {
                arrived.then_some(DocumentProcessingWakeSource::TaskSourceLoadCompletion)
            },
        }
    }

    pub(crate) fn drain_document_processing_wakes(&mut self) {
        #[cfg(test)]
        self.apply_ready_stylesheet_networking_tasks_for_test();
    }

    pub(crate) fn poll_document_processing_action(
        &mut self,
        task_queue: &mut PageTaskQueue,
        parse_time_document: Option<&(impl StylesheetBlockingReadView + ?Sized)>,
    ) -> Option<DocumentProcessingAction> {
        self.drain_document_processing_wakes();

        if parse_time_document.is_none() {
            if let Some(action) = self.ready_main_parser_deferred_script_action(task_queue) {
                return Some(action);
            }
            return self.poll_post_parse_document_processing_action(task_queue);
        }

        // Runtime-inserted connected stylesheet/link loads should not hold back
        // DOMContentLoaded. They can still keep the final window `load` turn
        // queued behind the pending connected load so the inserted node's own
        // `load` dispatch is not overtaken by document completion.
        let document = parse_time_document
            .expect("post-parse document processing should return before this branch");
        let front_task = task_queue.parse_time_front();

        if front_task.is_some_and(PageTask::is_waiting_for_source_load) {
            return None;
        }

        if front_task.is_some_and(|task| !task.is_window_load_task()) {
            let task = task_queue.parse_time_pop_front();

            if let Some(task) = task {
                let task_is_blocked =
                    self.page_task_is_blocked_by_stylesheets_in_document(document, &task);
                if task_is_blocked {
                    task_queue.enqueue_parser_boundary(task);
                } else {
                    return Some(
                        Self::parse_time_document_processing_action_for_unblocked_task(task),
                    );
                }
            }
        }

        if let Some(ready) = self.pop_ready_connected_style_load() {
            return Some(DocumentProcessingAction::DispatchConnectedStyleLoad(ready));
        }

        let front_task = task_queue.parse_time_front();

        if front_task.is_some_and(PageTask::is_waiting_for_source_load) {
            return None;
        }

        let task = task_queue.parse_time_pop_front();

        if let Some(task) = task {
            let task_is_blocked =
                self.page_task_is_blocked_by_stylesheets_in_document(document, &task);
            if task_is_blocked {
                task_queue.enqueue_parser_boundary(task);
            } else {
                return Some(Self::parse_time_document_processing_action_for_unblocked_task(task));
            }
        }

        None
    }

    fn poll_post_parse_document_processing_action(
        &mut self,
        task_queue: &mut PageTaskQueue,
    ) -> Option<DocumentProcessingAction> {
        if matches!(
            self.post_parse_page_task_pop_blocker(task_queue, false, false),
            Some(PostParsePageTaskPopBlocker::MainParserDeferredScript)
        ) {
            return self
                .pop_ready_connected_style_load()
                .map(DocumentProcessingAction::DispatchConnectedStyleLoad);
        }
        let front_work = task_queue.post_parse_front();

        if front_work.is_some_and(PostParsePageOwnedWork::is_waiting_for_source_load) {
            return self
                .pop_ready_connected_style_load()
                .map(DocumentProcessingAction::DispatchConnectedStyleLoad);
        }
        if front_work.is_some_and(|work| !work.is_window_load_task())
            && let Some(work) = task_queue.post_parse_pop_front()
        {
            if self.post_parse_work_is_blocked_by_document_stylesheets(&work) {
                task_queue.enqueue_front_post_parse_work_preserving_order(vec![work]);
            } else {
                return Some(DocumentProcessingAction::PostParsePageOwnedWork(Box::new(
                    work,
                )));
            }
        }

        if let Some(handle) = self.pop_ready_connected_style_load() {
            return Some(DocumentProcessingAction::DispatchConnectedStyleLoad(handle));
        }

        let front_work = task_queue.post_parse_front();

        if front_work.is_some_and(PostParsePageOwnedWork::is_waiting_for_source_load) {
            return None;
        }
        if let Some(work) = task_queue.post_parse_pop_front() {
            if self.post_parse_work_is_blocked_by_document_stylesheets(&work) {
                task_queue.enqueue_front_post_parse_work_preserving_order(vec![work]);
            } else {
                return Some(DocumentProcessingAction::PostParsePageOwnedWork(Box::new(
                    work,
                )));
            }
        }

        None
    }

    pub(crate) fn has_pending_document_processing(&self, task_queue: &mut PageTaskQueue) -> bool {
        !task_queue.is_empty() || self.has_pending_style_loads()
    }

    pub(crate) fn has_pending_parse_time_document_processing(
        &self,
        task_queue: &PageTaskQueue,
    ) -> bool {
        !task_queue.parse_time_is_empty()
            || self.has_pending_document_write_stylesheet_blocked_script()
    }

    #[cfg(test)]
    pub(crate) async fn observe_document_processing_wake(
        &mut self,
        task_queue: &mut PageTaskQueue,
    ) -> DocumentProcessingWakeObservation {
        self.drain_document_processing_wakes();
        if self.has_ready_document_processing_wake(task_queue) {
            return DocumentProcessingWakeObservation::ReadyNow;
        }
        match self
            .wait_for_document_processing_wake_source(task_queue)
            .await
        {
            Some(source) => {
                self.drain_document_processing_wakes();
                DocumentProcessingWakeObservation::Arrived(source)
            }
            None => DocumentProcessingWakeObservation::NoWake,
        }
    }

    #[cfg(test)]
    pub(crate) async fn wait_for_parse_time_turn_arrival(
        &mut self,
        task_queue: &mut PageTaskQueue,
    ) -> ParseTimeWakeObservation {
        const PARSE_TIME_DOCUMENT_WAKE_WAIT_TIMEOUT: std::time::Duration =
            std::time::Duration::from_millis(3);
        self.drain_document_processing_wakes();
        if self.has_ready_document_processing_wake(task_queue) {
            return ParseTimeWakeObservation::ReadyNow;
        }

        let deadline = tokio::time::Instant::now() + PARSE_TIME_DOCUMENT_WAKE_WAIT_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                self.drain_document_processing_wakes();
                if self.has_ready_document_processing_wake(task_queue) {
                    return ParseTimeWakeObservation::ReadyNow;
                } else {
                    return ParseTimeWakeObservation::TimedOutNoReady;
                }
            }

            match tokio::time::timeout(
                remaining,
                self.wait_for_document_processing_wake_source(task_queue),
            )
            .await
            {
                Ok(Some(source)) => {
                    self.drain_document_processing_wakes();
                    if let Some(source) = Self::parse_time_primary_wake_source(source) {
                        return ParseTimeWakeObservation::Arrived(source);
                    }
                    if self.has_ready_document_processing_wake(task_queue) {
                        return ParseTimeWakeObservation::ReadyNow;
                    }
                }
                Ok(None) => return ParseTimeWakeObservation::TimedOutNoReady,
                Err(_) => {
                    self.drain_document_processing_wakes();
                    if self.has_ready_document_processing_wake(task_queue) {
                        return ParseTimeWakeObservation::ReadyNow;
                    }
                    return ParseTimeWakeObservation::TimedOutNoReady;
                }
            }
        }
    }
}
