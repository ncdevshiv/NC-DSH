pub use crate::dom::native::{NativeDom, Node, NodeData};
pub use crate::page::{Page, TestingOutcome};
pub use crate::parser::HtmlParser;
pub use crate::protocol_types::{
    JsValueSnapshot, ScriptExecutionReport, ScriptRunOutcome, ScriptSkipReason,
};
use anyhow::Result;

pub trait PageScriptExt {
    fn script_execution(&self) -> &ScriptExecutionReport;
}

impl PageScriptExt for Page {
    fn script_execution(&self) -> &ScriptExecutionReport {
        Page::script_execution(self)
    }
}

pub trait PageTestingExt {
    fn collect_testing_outcome_async(
        &mut self,
    ) -> impl std::future::Future<Output = Result<TestingOutcome>>;
    fn testing_failures_async(&mut self) -> impl std::future::Future<Output = Result<Vec<String>>>;
    fn assert_testing_ok_async(&mut self) -> impl std::future::Future<Output = Result<()>>;
}

impl PageTestingExt for Page {
    async fn collect_testing_outcome_async(&mut self) -> Result<TestingOutcome> {
        Page::collect_testing_outcome_async(self).await
    }

    async fn testing_failures_async(&mut self) -> Result<Vec<String>> {
        Page::testing_failures_async(self).await
    }

    async fn assert_testing_ok_async(&mut self) -> Result<()> {
        Page::assert_testing_ok_async(self).await
    }
}
