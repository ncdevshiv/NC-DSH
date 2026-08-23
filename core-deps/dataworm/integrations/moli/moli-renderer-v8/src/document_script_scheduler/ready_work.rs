use std::convert::Infallible;

use crate::parser_script::action::ParserClassicScriptNextOwnerAction;

use super::{
    DocumentModuleScriptReadyWork, FrameDocumentModuleGraphReadyTarget, ModuleScriptGraphReadyWork,
};

/// Ready action paired with the document owner that produced it.
///
/// Store-level users should prefer this envelope when dispatching ready work to
/// an owner task lane. The inner action remains owner-adapter specific, but the
/// scheduler store does not lose the document owner while popping work.
#[derive(Debug)]
pub(crate) struct DocumentOwnedScriptReadyAction<Owner, Action> {
    owner: Owner,
    action: Action,
}

/// Ready work that has passed the common queued-owner vs payload-owner check.
#[derive(Debug)]
pub(crate) struct DocumentScriptReadyDispatch<Owner, Action, Route> {
    queued_owner: Owner,
    action: Action,
    route: Route,
}

/// Ready work whose queue owner no longer matches the payload owner.
#[derive(Debug)]
pub(crate) struct DocumentScriptReadyDispatchOwnerMismatch<Owner, Route> {
    queued_owner: Owner,
    payload_owner: Owner,
    route: Route,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocumentScriptExecutionOutcome {
    NoProgress,
    Progressed,
    TriggeredNavigation,
    BlockedOnDocumentWriteExternalLoad,
}

impl DocumentScriptExecutionOutcome {
    pub(crate) fn made_progress(self) -> bool {
        !matches!(self, Self::NoProgress)
    }
}

pub(crate) trait ParserClassicDocumentScriptReadyOwner<Ready, SourceFailure> {
    type Output<'owner>
    where
        Self: 'owner;

    fn run_parser_classic_ready<'owner>(&'owner mut self, ready: Ready) -> Self::Output<'owner>;

    fn run_parser_classic_source_failed<'owner>(
        &'owner mut self,
        failure: SourceFailure,
    ) -> Self::Output<'owner>;
}

pub(crate) trait DocumentScriptReadyWorkOwner<
    Target,
    ParserModuleEvaluation,
    ParserModuleGraphFailure,
    ParserClassicReady,
    ParserClassicSourceFailure,
>
{
    type Output<'owner>
    where
        Self: 'owner;

    fn run_module_script_ready_work<'owner>(
        &'owner mut self,
        work: DocumentModuleScriptReadyWork<
            ModuleScriptGraphReadyWork<Target>,
            ParserModuleGraphFailure,
            ParserModuleEvaluation,
        >,
    ) -> Self::Output<'owner>;

    fn run_parser_classic_ready_work<'owner>(
        &'owner mut self,
        work: ParserClassicScriptNextOwnerAction<ParserClassicReady, ParserClassicSourceFailure>,
    ) -> Self::Output<'owner>;
}

impl<Ready, SourceFailure> ParserClassicScriptNextOwnerAction<Ready, SourceFailure> {
    pub(crate) fn run_with<'owner, Owner>(self, owner: &'owner mut Owner) -> Owner::Output<'owner>
    where
        Owner: ParserClassicDocumentScriptReadyOwner<Ready, SourceFailure>,
    {
        match self {
            Self::Ready(ready) => owner.run_parser_classic_ready(ready),
            Self::SourceFailed(failure) => owner.run_parser_classic_source_failed(failure),
        }
    }
}

/// Ready actions that can expose the document owner carried by their payload.
///
/// The scheduler envelope owns the queued owner. Some owner-adapter actions also
/// carry a concrete document/task owner in their payload. Implementing this
/// trait lets the common envelope perform the first owner consistency check
/// before owner-specific dispatch continues.
pub(crate) trait DocumentScriptReadyActionRoute<Owner> {
    fn payload_document_owner(&self) -> Owner;
}

impl<Owner, T> DocumentScriptReadyActionRoute<Owner> for Box<T>
where
    T: DocumentScriptReadyActionRoute<Owner>,
{
    fn payload_document_owner(&self) -> Owner {
        (**self).payload_document_owner()
    }
}

/// Ready actions that can expose the owner-specific dispatch route carried by
/// their payload.
///
/// This is deliberately separate from `DocumentScriptReadyActionRoute`: the
/// owner envelope only needs a document-owner equality check, while concrete
/// frame dispatch also needs realm and script-handle details for currentness
/// checks and diagnostics.
pub(crate) trait DocumentScriptReadyActionDispatchRoute<Route> {
    fn dispatch_route(&self) -> Route;
}

impl<Route, T> DocumentScriptReadyActionDispatchRoute<Route> for Box<T>
where
    T: DocumentScriptReadyActionDispatchRoute<Route>,
{
    fn dispatch_route(&self) -> Route {
        (**self).dispatch_route()
    }
}

impl<Owner> DocumentScriptReadyActionRoute<Owner> for Infallible {
    fn payload_document_owner(&self) -> Owner {
        match *self {}
    }
}

impl<Route> DocumentScriptReadyActionDispatchRoute<Route> for Infallible {
    fn dispatch_route(&self) -> Route {
        match *self {}
    }
}

impl<Owner, Ready, SourceFailure> DocumentScriptReadyActionRoute<Owner>
    for ParserClassicScriptNextOwnerAction<Ready, SourceFailure>
where
    Ready: DocumentScriptReadyActionRoute<Owner>,
    SourceFailure: DocumentScriptReadyActionRoute<Owner>,
{
    fn payload_document_owner(&self) -> Owner {
        match self {
            Self::Ready(ready) => ready.payload_document_owner(),
            Self::SourceFailed(failure) => failure.payload_document_owner(),
        }
    }
}

impl<Route, Ready, SourceFailure> DocumentScriptReadyActionDispatchRoute<Route>
    for ParserClassicScriptNextOwnerAction<Ready, SourceFailure>
where
    Ready: DocumentScriptReadyActionDispatchRoute<Route>,
    SourceFailure: DocumentScriptReadyActionDispatchRoute<Route>,
{
    fn dispatch_route(&self) -> Route {
        match self {
            Self::Ready(ready) => ready.dispatch_route(),
            Self::SourceFailed(failure) => failure.dispatch_route(),
        }
    }
}

impl<Owner, Action> DocumentOwnedScriptReadyAction<Owner, Action> {
    pub(crate) fn new(owner: Owner, action: Action) -> Self {
        Self { owner, action }
    }

    pub(crate) fn owner(&self) -> &Owner {
        &self.owner
    }

    pub(crate) fn action(&self) -> &Action {
        &self.action
    }

    pub(crate) fn into_action(self) -> Action {
        self.action
    }

    pub(crate) fn into_dispatch<Route>(
        self,
    ) -> Result<
        DocumentScriptReadyDispatch<Owner, Action, Route>,
        DocumentScriptReadyDispatchOwnerMismatch<Owner, Route>,
    >
    where
        Owner: Copy + PartialEq,
        Action:
            DocumentScriptReadyActionDispatchRoute<Route> + DocumentScriptReadyActionRoute<Owner>,
    {
        let queued_owner = self.owner;
        let payload_owner = self.action.payload_document_owner();
        let route = self.action.dispatch_route();
        if queued_owner == payload_owner {
            Ok(DocumentScriptReadyDispatch {
                queued_owner,
                action: self.action,
                route,
            })
        } else {
            Err(DocumentScriptReadyDispatchOwnerMismatch {
                queued_owner,
                payload_owner,
                route,
            })
        }
    }
}

impl<Owner, Action, Route> DocumentScriptReadyDispatch<Owner, Action, Route> {
    pub(crate) fn queued_owner(&self) -> &Owner {
        &self.queued_owner
    }

    pub(crate) fn route(&self) -> &Route {
        &self.route
    }

    pub(crate) fn into_action_and_route(self) -> (Action, Route) {
        (self.action, self.route)
    }
}

impl<Owner, Route> DocumentScriptReadyDispatchOwnerMismatch<Owner, Route> {
    pub(crate) fn queued_owner(&self) -> &Owner {
        &self.queued_owner
    }

    pub(crate) fn payload_owner(&self) -> &Owner {
        &self.payload_owner
    }

    pub(crate) fn route(&self) -> &Route {
        &self.route
    }
}

/// Ready work emitted by the document script runner.
///
/// Keep this type free of main-frame `PageTask` and child-frame task payloads.
/// Owner adapters convert these items into the concrete execution path for the
/// owning document.
#[derive(Debug, Clone)]
pub(crate) enum DocumentScriptReadyWork<
    Target = FrameDocumentModuleGraphReadyTarget,
    ParserModuleEvaluation = Infallible,
    ParserModuleGraphFailure = Infallible,
    ParserClassicReady = Infallible,
    ParserClassicSourceFailure = Infallible,
> {
    ModuleScriptGraphReady(Box<ModuleScriptGraphReadyWork<Target>>),
    ModuleScriptGraphFailed(Box<ParserModuleGraphFailure>),
    ModuleScriptEvaluationCompleted(Box<ParserModuleEvaluation>),
    ParserClassicReady(Box<ParserClassicReady>),
    ParserClassicSourceFailed(Box<ParserClassicSourceFailure>),
}

impl<
    Target,
    ParserModuleEvaluation,
    ParserModuleGraphFailure,
    ParserClassicReady,
    ParserClassicSourceFailure,
>
    DocumentScriptReadyWork<
        Target,
        ParserModuleEvaluation,
        ParserModuleGraphFailure,
        ParserClassicReady,
        ParserClassicSourceFailure,
    >
{
    pub(crate) fn module_script_graph_ready(work: ModuleScriptGraphReadyWork<Target>) -> Self {
        Self::ModuleScriptGraphReady(Box::new(work))
    }

    pub(crate) fn module_script_graph_failed(failure: ParserModuleGraphFailure) -> Self {
        Self::ModuleScriptGraphFailed(Box::new(failure))
    }

    pub(crate) fn module_script_evaluation_completed(evaluation: ParserModuleEvaluation) -> Self {
        Self::ModuleScriptEvaluationCompleted(Box::new(evaluation))
    }

    pub(crate) fn parser_classic_ready(ready: ParserClassicReady) -> Self {
        Self::ParserClassicReady(Box::new(ready))
    }

    pub(crate) fn parser_classic_source_failed(failure: ParserClassicSourceFailure) -> Self {
        Self::ParserClassicSourceFailed(Box::new(failure))
    }

    pub(crate) fn run_with_ready_owner<'owner, Owner>(
        self,
        owner: &'owner mut Owner,
    ) -> Owner::Output<'owner>
    where
        Owner: DocumentScriptReadyWorkOwner<
                Target,
                ParserModuleEvaluation,
                ParserModuleGraphFailure,
                ParserClassicReady,
                ParserClassicSourceFailure,
            >,
    {
        match self {
            Self::ModuleScriptGraphReady(work) => {
                owner.run_module_script_ready_work(DocumentModuleScriptReadyWork::GraphReady(*work))
            }
            Self::ModuleScriptGraphFailed(failure) => owner
                .run_module_script_ready_work(DocumentModuleScriptReadyWork::GraphFailed(*failure)),
            Self::ModuleScriptEvaluationCompleted(evaluation) => owner
                .run_module_script_ready_work(DocumentModuleScriptReadyWork::EvaluationCompleted(
                    *evaluation,
                )),
            Self::ParserClassicReady(ready) => owner
                .run_parser_classic_ready_work(ParserClassicScriptNextOwnerAction::Ready(*ready)),
            Self::ParserClassicSourceFailed(failure) => owner.run_parser_classic_ready_work(
                ParserClassicScriptNextOwnerAction::SourceFailed(*failure),
            ),
        }
    }

    #[cfg(test)]
    pub(crate) fn into_module_script_graph_ready(self) -> ModuleScriptGraphReadyWork<Target> {
        match self {
            Self::ModuleScriptGraphReady(work) => *work,
            Self::ModuleScriptEvaluationCompleted(_) => {
                panic!("expected parser module graph-ready document script work")
            }
            Self::ParserClassicReady(_) => {
                panic!("expected parser module graph-ready document script work")
            }
            Self::ParserClassicSourceFailed(_) => {
                panic!("expected parser module graph-ready document script work")
            }
            Self::ModuleScriptGraphFailed(_) => {
                panic!("expected parser module graph-ready document script work")
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn into_module_script_graph_failed(self) -> ParserModuleGraphFailure {
        match self {
            Self::ModuleScriptGraphFailed(failure) => *failure,
            Self::ModuleScriptGraphReady(_)
            | Self::ModuleScriptEvaluationCompleted(_)
            | Self::ParserClassicReady(_)
            | Self::ParserClassicSourceFailed(_) => {
                panic!("expected parser module graph-failed document script work")
            }
        }
    }
}

impl<
    Owner,
    Target,
    ParserModuleEvaluation,
    ParserModuleGraphFailure,
    ParserClassicReady,
    ParserClassicSourceFailure,
> DocumentScriptReadyActionRoute<Owner>
    for DocumentScriptReadyWork<
        Target,
        ParserModuleEvaluation,
        ParserModuleGraphFailure,
        ParserClassicReady,
        ParserClassicSourceFailure,
    >
where
    ModuleScriptGraphReadyWork<Target>: DocumentScriptReadyActionRoute<Owner>,
    ParserModuleEvaluation: DocumentScriptReadyActionRoute<Owner>,
    ParserModuleGraphFailure: DocumentScriptReadyActionRoute<Owner>,
    ParserClassicReady: DocumentScriptReadyActionRoute<Owner>,
    ParserClassicSourceFailure: DocumentScriptReadyActionRoute<Owner>,
{
    fn payload_document_owner(&self) -> Owner {
        match self {
            Self::ModuleScriptGraphReady(work) => work.payload_document_owner(),
            Self::ModuleScriptGraphFailed(failure) => failure.payload_document_owner(),
            Self::ModuleScriptEvaluationCompleted(evaluation) => {
                evaluation.payload_document_owner()
            }
            Self::ParserClassicReady(ready) => ready.payload_document_owner(),
            Self::ParserClassicSourceFailed(failure) => failure.payload_document_owner(),
        }
    }
}

impl<
    Route,
    Target,
    ParserModuleEvaluation,
    ParserModuleGraphFailure,
    ParserClassicReady,
    ParserClassicSourceFailure,
> DocumentScriptReadyActionDispatchRoute<Route>
    for DocumentScriptReadyWork<
        Target,
        ParserModuleEvaluation,
        ParserModuleGraphFailure,
        ParserClassicReady,
        ParserClassicSourceFailure,
    >
where
    ModuleScriptGraphReadyWork<Target>: DocumentScriptReadyActionDispatchRoute<Route>,
    ParserModuleEvaluation: DocumentScriptReadyActionDispatchRoute<Route>,
    ParserModuleGraphFailure: DocumentScriptReadyActionDispatchRoute<Route>,
    ParserClassicReady: DocumentScriptReadyActionDispatchRoute<Route>,
    ParserClassicSourceFailure: DocumentScriptReadyActionDispatchRoute<Route>,
{
    fn dispatch_route(&self) -> Route {
        match self {
            Self::ModuleScriptGraphReady(work) => work.dispatch_route(),
            Self::ModuleScriptGraphFailed(failure) => failure.dispatch_route(),
            Self::ModuleScriptEvaluationCompleted(evaluation) => evaluation.dispatch_route(),
            Self::ParserClassicReady(ready) => ready.dispatch_route(),
            Self::ParserClassicSourceFailed(failure) => failure.dispatch_route(),
        }
    }
}

impl<Owner, Action> DocumentScriptReadyActionRoute<Owner>
    for DocumentOwnedScriptReadyAction<Owner, Action>
where
    Owner: Clone,
{
    fn payload_document_owner(&self) -> Owner {
        self.owner.clone()
    }
}

impl<Route, Owner, Action> DocumentScriptReadyActionDispatchRoute<Route>
    for DocumentOwnedScriptReadyAction<Owner, Action>
where
    Action: DocumentScriptReadyActionDispatchRoute<Route>,
{
    fn dispatch_route(&self) -> Route {
        self.action.dispatch_route()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DocumentOwnedScriptReadyAction, DocumentScriptReadyActionDispatchRoute,
        DocumentScriptReadyActionRoute, DocumentScriptReadyWork, DocumentScriptReadyWorkOwner,
        ParserClassicDocumentScriptReadyOwner,
    };
    use crate::document_script_scheduler::{
        DocumentModuleScriptReadyWork, ModuleScriptGraphReadyWork,
    };
    use crate::parser_script::action::ParserClassicScriptNextOwnerAction;

    #[derive(Debug)]
    struct RoutedReadyAction {
        payload_owner: u64,
    }

    impl DocumentScriptReadyActionRoute<u64> for RoutedReadyAction {
        fn payload_document_owner(&self) -> u64 {
            self.payload_owner
        }
    }

    impl DocumentScriptReadyActionDispatchRoute<u64> for RoutedReadyAction {
        fn dispatch_route(&self) -> u64 {
            self.payload_owner
        }
    }

    #[derive(Default)]
    struct FakeParserClassicOwner {
        seen: Vec<u64>,
    }

    impl ParserClassicDocumentScriptReadyOwner<RoutedReadyAction, RoutedReadyAction>
        for FakeParserClassicOwner
    {
        type Output<'owner>
            = &'static str
        where
            Self: 'owner;

        fn run_parser_classic_ready<'owner>(
            &'owner mut self,
            ready: RoutedReadyAction,
        ) -> Self::Output<'owner> {
            self.seen.push(ready.payload_owner);
            "classic-ready"
        }

        fn run_parser_classic_source_failed<'owner>(
            &'owner mut self,
            failure: RoutedReadyAction,
        ) -> Self::Output<'owner> {
            self.seen.push(failure.payload_owner);
            "classic-source-failed"
        }
    }

    #[derive(Default)]
    struct FakeDocumentScriptReadyOwner {
        seen: Vec<&'static str>,
    }

    impl
        DocumentScriptReadyWorkOwner<
            (),
            &'static str,
            &'static str,
            RoutedReadyAction,
            RoutedReadyAction,
        > for FakeDocumentScriptReadyOwner
    {
        type Output<'owner>
            = &'static str
        where
            Self: 'owner;

        fn run_module_script_ready_work<'owner>(
            &'owner mut self,
            work: DocumentModuleScriptReadyWork<
                ModuleScriptGraphReadyWork<()>,
                &'static str,
                &'static str,
            >,
        ) -> Self::Output<'owner> {
            match work {
                DocumentModuleScriptReadyWork::GraphReady(_) => {
                    self.seen.push("module-ready");
                    "module-ready"
                }
                DocumentModuleScriptReadyWork::GraphFailed(error) => {
                    self.seen.push(error);
                    "module-failed"
                }
                DocumentModuleScriptReadyWork::EvaluationCompleted(evaluation) => {
                    self.seen.push(evaluation);
                    "module-evaluation"
                }
            }
        }

        fn run_parser_classic_ready_work<'owner>(
            &'owner mut self,
            work: ParserClassicScriptNextOwnerAction<RoutedReadyAction, RoutedReadyAction>,
        ) -> Self::Output<'owner> {
            match work {
                ParserClassicScriptNextOwnerAction::Ready(ready) => {
                    self.seen.push("classic-ready");
                    assert_eq!(ready.payload_owner, 17);
                    "classic-ready"
                }
                ParserClassicScriptNextOwnerAction::SourceFailed(failure) => {
                    self.seen.push("classic-source-failed");
                    assert_eq!(failure.payload_owner, 19);
                    "classic-source-failed"
                }
            }
        }
    }

    #[test]
    fn owned_ready_action_exposes_queue_owner_and_payload() {
        let matching =
            DocumentOwnedScriptReadyAction::new(7, RoutedReadyAction { payload_owner: 7 });
        assert_eq!(*matching.owner(), 7);
        let dispatch = matching
            .into_dispatch::<u64>()
            .expect("matching payload owner should produce a dispatch item");
        let (action, _) = dispatch.into_action_and_route();
        assert_eq!(action.payload_document_owner(), 7);
    }

    #[test]
    fn owned_ready_action_dispatch_carries_matching_owner_action_and_route() {
        let matching =
            DocumentOwnedScriptReadyAction::new(7, RoutedReadyAction { payload_owner: 7 });
        let dispatch = matching
            .into_dispatch::<u64>()
            .expect("matching payload owner should produce a dispatch item");
        assert_eq!(*dispatch.queued_owner(), 7);
        assert_eq!(*dispatch.route(), 7);
        let (action, route) = dispatch.into_action_and_route();
        assert_eq!(action.payload_owner, 7);
        assert_eq!(route, 7);

        let mismatched =
            DocumentOwnedScriptReadyAction::new(7, RoutedReadyAction { payload_owner: 8 });
        let mismatch = mismatched
            .into_dispatch::<u64>()
            .expect_err("mismatched payload owner should preserve dispatch diagnostics");
        assert_eq!(*mismatch.queued_owner(), 7);
        assert_eq!(*mismatch.payload_owner(), 8);
        assert_eq!(*mismatch.route(), 8);
    }

    #[test]
    fn parser_classic_next_owner_action_forwards_owner_and_route() {
        let ready =
            ParserClassicScriptNextOwnerAction::<_, RoutedReadyAction>::Ready(RoutedReadyAction {
                payload_owner: 7,
            });
        assert_eq!(ready.payload_document_owner(), 7);
        assert_eq!(ready.dispatch_route(), 7);

        let source_failed =
            ParserClassicScriptNextOwnerAction::<RoutedReadyAction, _>::SourceFailed(
                RoutedReadyAction { payload_owner: 9 },
            );
        assert_eq!(source_failed.payload_document_owner(), 9);
        assert_eq!(source_failed.dispatch_route(), 9);
    }

    #[test]
    fn parser_classic_next_owner_action_runs_through_owner_contract() {
        let mut owner = FakeParserClassicOwner::default();

        assert_eq!(
            ParserClassicScriptNextOwnerAction::<_, RoutedReadyAction>::Ready(RoutedReadyAction {
                payload_owner: 7,
            })
            .run_with(&mut owner),
            "classic-ready"
        );
        assert_eq!(
            ParserClassicScriptNextOwnerAction::<RoutedReadyAction, _>::SourceFailed(
                RoutedReadyAction { payload_owner: 9 },
            )
            .run_with(&mut owner),
            "classic-source-failed"
        );
        assert_eq!(owner.seen, [7, 9]);
    }

    #[test]
    fn ready_work_runs_through_shared_ready_owner_contract() {
        let mut owner = FakeDocumentScriptReadyOwner::default();

        assert_eq!(
            DocumentScriptReadyWork::<
                (),
                &'static str,
                &'static str,
                RoutedReadyAction,
                RoutedReadyAction,
            >::module_script_evaluation_completed("settled")
            .run_with_ready_owner(&mut owner),
            "module-evaluation"
        );
        assert_eq!(
            DocumentScriptReadyWork::<
                (),
                &'static str,
                &'static str,
                RoutedReadyAction,
                RoutedReadyAction,
            >::parser_classic_ready(RoutedReadyAction { payload_owner: 17 })
            .run_with_ready_owner(&mut owner),
            "classic-ready"
        );
        assert_eq!(
            DocumentScriptReadyWork::<
                (),
                &'static str,
                &'static str,
                RoutedReadyAction,
                RoutedReadyAction,
            >::parser_classic_source_failed(RoutedReadyAction { payload_owner: 19 })
            .run_with_ready_owner(&mut owner),
            "classic-source-failed"
        );

        assert_eq!(
            owner.seen,
            ["settled", "classic-ready", "classic-source-failed"]
        );
    }
}
