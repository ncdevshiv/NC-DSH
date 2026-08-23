use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PendingInspectorIssue {
    QuirksMode {
        document: DomHandle,
        is_limited_quirks_mode: bool,
        url: String,
    },
    ContentSecurityPolicy {
        target: Option<DomHandle>,
        violation: crate::content_security_policy::ContentSecurityPolicyUrlViolation,
    },
}

impl DocumentRuntime {
    pub(crate) fn record_quirks_mode_inspector_issue_at_dom_content_loaded(&mut self) {
        if self.quirks_mode_issue_reported {
            return;
        }
        self.quirks_mode_issue_reported = true;

        let document = self.document_handle();
        let Some(quirks_mode) = self.dom_host().document_quirks_mode_for_handle(document) else {
            return;
        };
        let is_limited_quirks_mode = match quirks_mode {
            selectors::matching::QuirksMode::NoQuirks => return,
            selectors::matching::QuirksMode::LimitedQuirks => true,
            selectors::matching::QuirksMode::Quirks => false,
        };
        let url = self.host_document().url();
        if url.as_str().is_empty() || url.as_str() == "about:blank" {
            return;
        }
        self.pending_inspector_issues
            .push(PendingInspectorIssue::QuirksMode {
                document,
                is_limited_quirks_mode,
                url: url.as_str().to_owned(),
            });
    }

    pub(crate) fn record_content_security_policy_inspector_issue(
        &mut self,
        target: Option<DomHandle>,
        violation: &crate::content_security_policy::ContentSecurityPolicyUrlViolation,
    ) {
        self.pending_inspector_issues
            .push(PendingInspectorIssue::ContentSecurityPolicy {
                target,
                violation: violation.clone(),
            });
    }

    pub(crate) fn take_pending_inspector_issues(&mut self) -> Vec<PendingInspectorIssue> {
        std::mem::take(&mut self.pending_inspector_issues)
    }
}
