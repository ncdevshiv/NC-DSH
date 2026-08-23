//! Test-only access to a rendering-update task body.
//!
//! These helpers exist only for domain authorization/result unit tests. Tests
//! covering a complete HTML task use `page_selected_task_test_harness`.

use crate::page_task_queue::RendererPageRenderingUpdateTask;

use super::PageVm;

impl PageVm {
    pub(crate) fn take_rendering_update_body_task_for_test(
        &mut self,
    ) -> Option<RendererPageRenderingUpdateTask> {
        self.page_task_executor_sources_for_test()
            .take_rendering_update_for_executor_test()
    }
}
