/// Document-owned script execution lanes shared by owner adapters.
///
/// This is intentionally narrower than `PageTask`: it names script execution
/// semantics without carrying main-frame queue variants, ordering keys, or
/// lifecycle task labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocumentScriptExecutionLane {
    ParserBlocking,
    ParseTimeAsync,
    ClassicDefer,
    ModuleDefer,
    AsyncPhase,
}

impl DocumentScriptExecutionLane {
    pub(crate) fn phase_label(self) -> &'static str {
        match self {
            Self::ParserBlocking => "parser-blocking task",
            Self::ParseTimeAsync => "parse-time async task",
            Self::ClassicDefer => "classic defer task",
            Self::ModuleDefer => "module defer task",
            Self::AsyncPhase => "async phase task",
        }
    }

    pub(crate) fn sets_document_ready_state_loading(self) -> bool {
        matches!(self, Self::ParserBlocking)
    }
}

/// Document-owned source failure lanes for executable script work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocumentScriptSourceFailureLane {
    ParseTimeAsync,
    AsyncPhase,
}

impl DocumentScriptSourceFailureLane {
    pub(crate) fn phase_label(self) -> &'static str {
        match self {
            Self::ParseTimeAsync => "parse-time async failure task",
            Self::AsyncPhase => "async phase failure task",
        }
    }
}
