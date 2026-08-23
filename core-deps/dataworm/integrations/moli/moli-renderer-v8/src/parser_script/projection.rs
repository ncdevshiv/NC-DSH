use crate::parser_script::item::ParserClassicScriptRunnerItem;
use crate::parser_script::owner::ParserScriptExecutionBlocker;

#[derive(Debug, Clone)]
pub(crate) enum ParserClassicScriptExecutionGateProjection<C> {
    Ready,
    Blocked(ParserClassicScriptBlockedOnExecution<C>),
    NoCurrent,
}

#[derive(Debug, Clone)]
pub(crate) enum ParserClassicScriptNextActionWithBlockedScript<A, C> {
    Action(A),
    Blocked(ParserClassicScriptBlockedOnExecution<C>),
    NotReady,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ParserClassicScriptSourceResultApplication<A> {
    Applied(Option<A>),
    Waiting,
    NoSourceLoad,
}

#[derive(Debug, Clone)]
pub(crate) struct ParserClassicScriptBlockedOnSourceLoad<C> {
    script: ParserClassicScriptRunnerItem<C>,
}

impl<C> ParserClassicScriptBlockedOnSourceLoad<C> {
    pub(crate) fn new(script: ParserClassicScriptRunnerItem<C>) -> Self {
        Self { script }
    }

    #[cfg(test)]
    pub(crate) fn script(&self) -> &ParserClassicScriptRunnerItem<C> {
        &self.script
    }

    pub(crate) fn into_script(self) -> ParserClassicScriptRunnerItem<C> {
        self.script
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ParserClassicScriptBlockedOnExecution<C> {
    blocker: ParserScriptExecutionBlocker,
    script: ParserClassicScriptRunnerItem<C>,
}

impl<C> ParserClassicScriptBlockedOnExecution<C> {
    pub(crate) fn new(
        blocker: ParserScriptExecutionBlocker,
        script: ParserClassicScriptRunnerItem<C>,
    ) -> Self {
        Self { blocker, script }
    }

    pub(crate) fn blocker(&self) -> ParserScriptExecutionBlocker {
        self.blocker
    }

    #[cfg(test)]
    pub(crate) fn script(&self) -> &ParserClassicScriptRunnerItem<C> {
        &self.script
    }

    #[cfg(test)]
    pub(crate) fn script_mut(&mut self) -> &mut ParserClassicScriptRunnerItem<C> {
        &mut self.script
    }

    pub(crate) fn into_script(self) -> ParserClassicScriptRunnerItem<C> {
        self.script
    }
}
