use crate::frame_owner_model::{
    FrameDocumentClassicScriptReadyTarget, FrameDocumentClassicScriptSourceFailureAction,
    FrameDocumentOwner,
};
use crate::parser_module_evaluation::ParserModuleEvaluationContinuation;
use crate::parser_script::action::{
    ParserClassicScriptNextOwnerAction, ParserClassicScriptReadyAction,
};

use super::{
    DocumentModuleGraphFailedWork, DocumentModuleGraphReadyWork, DocumentModuleScriptReadyWork,
    DocumentScriptReadyActionDispatchRoute, DocumentScriptReadyActionRoute,
    DocumentScriptReadyWork, DocumentScriptSchedulerStore, FrameDocumentModuleGraphReadyTarget,
    FrameDocumentReadyActionRoute,
};

pub(crate) type FrameDocumentClassicReadyWork =
    ParserClassicScriptReadyAction<FrameDocumentClassicScriptReadyTarget>;

pub(crate) type FrameDocumentClassicSourceFailureWork =
    FrameDocumentClassicScriptSourceFailureAction;

pub(crate) type FrameDocumentClassicScriptSchedulerWork = ParserClassicScriptNextOwnerAction<
    FrameDocumentClassicReadyWork,
    FrameDocumentClassicSourceFailureWork,
>;

pub(crate) type FrameDocumentScriptReadyWork = DocumentScriptReadyWork<
    FrameDocumentModuleGraphReadyTarget,
    ParserModuleEvaluationContinuation<DocumentModuleGraphReadyWork>,
    DocumentModuleGraphFailedWork,
    FrameDocumentClassicReadyWork,
    FrameDocumentClassicSourceFailureWork,
>;

pub(crate) type FrameDocumentModuleScriptReadyWork = DocumentModuleScriptReadyWork<
    DocumentModuleGraphReadyWork,
    DocumentModuleGraphFailedWork,
    ParserModuleEvaluationContinuation<DocumentModuleGraphReadyWork>,
>;

pub(crate) type FrameDocumentScriptSchedulerStore = DocumentScriptSchedulerStore<
    FrameDocumentOwner,
    FrameDocumentModuleGraphReadyTarget,
    ParserModuleEvaluationContinuation<DocumentModuleGraphReadyWork>,
    DocumentModuleGraphFailedWork,
    DocumentModuleGraphReadyWork,
    FrameDocumentClassicReadyWork,
    FrameDocumentClassicSourceFailureWork,
>;

impl DocumentScriptReadyActionRoute<FrameDocumentOwner> for FrameDocumentClassicReadyWork {
    fn payload_document_owner(&self) -> FrameDocumentOwner {
        self.target().task_owner().document_owner()
    }
}

impl DocumentScriptReadyActionDispatchRoute<FrameDocumentReadyActionRoute>
    for FrameDocumentClassicReadyWork
{
    fn dispatch_route(&self) -> FrameDocumentReadyActionRoute {
        let target = self.target();
        let realm_id = target.realm_id();
        FrameDocumentReadyActionRoute::from_frame_document_parts(
            Some(target.child_handle()),
            target.task_owner(),
            realm_id,
            realm_id.is_some(),
            self.script_handle(),
        )
    }
}

impl DocumentScriptReadyActionRoute<FrameDocumentOwner> for FrameDocumentClassicSourceFailureWork {
    fn payload_document_owner(&self) -> FrameDocumentOwner {
        self.target().task_owner().document_owner()
    }
}

impl DocumentScriptReadyActionDispatchRoute<FrameDocumentReadyActionRoute>
    for FrameDocumentClassicSourceFailureWork
{
    fn dispatch_route(&self) -> FrameDocumentReadyActionRoute {
        let target = self.target();
        FrameDocumentReadyActionRoute::from_frame_document_parts(
            Some(target.child_handle()),
            target.task_owner(),
            target.realm_id(),
            false,
            self.script_handle(),
        )
    }
}
