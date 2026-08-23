use std::sync::Arc;

use crate::planning::{PreparedScript, SharedScriptSourceLoad};

type DocumentScriptSourceLoadStarter =
    dyn Fn(PreparedScript, Option<String>) -> SharedScriptSourceLoad + Send + Sync + 'static;

#[derive(Clone)]
pub(super) struct DocumentScriptSourceLoadPort {
    starter: Arc<DocumentScriptSourceLoadStarter>,
}

impl DocumentScriptSourceLoadPort {
    pub(super) fn new(
        starter: impl Fn(PreparedScript, Option<String>) -> SharedScriptSourceLoad
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            starter: Arc::new(starter),
        }
    }

    pub(super) fn start_with_document_character_set(
        &self,
        script: PreparedScript,
        document_character_set: Option<&str>,
    ) -> SharedScriptSourceLoad {
        (self.starter)(script, document_character_set.map(str::to_owned))
    }
}
