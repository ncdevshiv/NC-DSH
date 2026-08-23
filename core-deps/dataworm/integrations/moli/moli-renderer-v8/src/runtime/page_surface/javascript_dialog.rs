use super::{
    super::javascript_dialog::RendererJavaScriptDialogCompletion,
    RendererDocumentLifecycleIdentity, RendererWindowDocumentSource,
};

/// Page-local identity of one JavaScript dialog request.
///
/// The sequence is allocated when the source Window invokes `alert`,
/// `confirm`, or `prompt`. It is meaningful only together with the exact
/// renderer Page residence carried by the protocol handoff; a replacement
/// Page may restart the sequence without colliding with an older dialog.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RendererJavaScriptDialogId(u64);

impl RendererJavaScriptDialogId {
    pub const fn new(sequence: u64) -> Self {
        Self(sequence)
    }

    pub fn sequence(self) -> u64 {
        self.0
    }
}

/// Dialog-domain name for the shared exact Window/Document source identity.
pub type RendererJavaScriptDialogSource = RendererWindowDocumentSource;

#[derive(Debug, Clone)]
pub struct RendererPendingJavaScriptDialog {
    id: RendererJavaScriptDialogId,
    source_document: RendererDocumentLifecycleIdentity,
    source: RendererJavaScriptDialogSource,
    source_url: String,
    dialog_type: String,
    message: String,
    default_prompt: String,
    completion: Option<RendererJavaScriptDialogCompletion>,
}

impl RendererPendingJavaScriptDialog {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: RendererJavaScriptDialogId,
        source_document: RendererDocumentLifecycleIdentity,
        source: RendererJavaScriptDialogSource,
        source_url: String,
        dialog_type: String,
        message: String,
        default_prompt: String,
        completion: Option<RendererJavaScriptDialogCompletion>,
    ) -> Self {
        Self {
            id,
            source_document,
            source,
            source_url,
            dialog_type,
            message,
            default_prompt,
            completion,
        }
    }

    pub fn id(&self) -> RendererJavaScriptDialogId {
        self.id
    }

    pub fn source_document(&self) -> RendererDocumentLifecycleIdentity {
        self.source_document
    }

    pub fn source(&self) -> &RendererJavaScriptDialogSource {
        &self.source
    }

    pub fn source_url(&self) -> &str {
        &self.source_url
    }

    pub fn dialog_type(&self) -> &str {
        &self.dialog_type
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn default_prompt(&self) -> &str {
        &self.default_prompt
    }

    pub(crate) fn install_completion(&mut self, completion: RendererJavaScriptDialogCompletion) {
        assert!(
            self.completion.replace(completion).is_none(),
            "a JavaScript dialog may install its modal completion only once"
        );
    }

    pub(crate) fn completion_matches(
        &self,
        completion: &RendererJavaScriptDialogCompletion,
    ) -> bool {
        self.completion.as_ref() == Some(completion)
    }

    #[cfg(test)]
    pub(crate) fn completion_for_test(&self) -> Option<RendererJavaScriptDialogCompletion> {
        self.completion.clone()
    }

    /// Completes the renderer-side modal request exactly once.
    ///
    /// Current non-blocking renderer calls do not install a completion, while
    /// command and integration boundaries may do so. Keeping completion
    /// private prevents protocol state from replacing the one-shot capability
    /// independently of the dialog identity.
    pub fn finish(&self, accepted: bool, user_input: String) -> bool {
        self.completion
            .as_ref()
            .is_none_or(|completion| completion.finish(accepted, user_input))
    }
}

impl PartialEq for RendererPendingJavaScriptDialog {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.source_document == other.source_document
            && self.source == other.source
            && self.source_url == other.source_url
            && self.dialog_type == other.dialog_type
            && self.message == other.message
            && self.default_prompt == other.default_prompt
    }
}

impl Eq for RendererPendingJavaScriptDialog {}
