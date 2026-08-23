use crate::{
    frame_owner_model::{
        FrameDocumentTaskOwner, MainDocumentScriptLoadDelayKind, MainDocumentScriptLoadDelayLease,
    },
    planning::PreparedScript,
    types::{ScriptKind, ScriptMode},
};

/// One parser-inserted async module transferring into the shared
/// main-Document `PendingScript` owner.
///
/// The admission is deliberately a one-shot value rather than a queue entry.
/// Its exact load-delay lease is acquired at parser discovery and travels with
/// the script until the selected main-runtime task either installs the shared
/// `PendingScript` or is discarded with its retired Document.
#[derive(Debug)]
pub(crate) struct MainParserAsyncModuleAdmission {
    script: Box<PreparedScript>,
    load_delay_binding: MainDocumentScriptLoadDelayLease,
}

impl MainParserAsyncModuleAdmission {
    pub(crate) fn new(
        script: PreparedScript,
        load_delay_binding: MainDocumentScriptLoadDelayLease,
    ) -> Self {
        assert_eq!(
            (script.kind, script.mode),
            (ScriptKind::Module, ScriptMode::Async),
            "main parser async-module admission requires an async module"
        );
        assert_eq!(
            load_delay_binding.kind(),
            MainDocumentScriptLoadDelayKind::Module,
            "main parser async-module admission requires a module load-delay lease"
        );
        Self {
            script: Box::new(script),
            load_delay_binding,
        }
    }

    pub(crate) fn owner(&self) -> FrameDocumentTaskOwner {
        self.load_delay_binding.owner()
    }

    pub(crate) fn into_parts(self) -> (PreparedScript, MainDocumentScriptLoadDelayLease) {
        (*self.script, self.load_delay_binding)
    }
}
