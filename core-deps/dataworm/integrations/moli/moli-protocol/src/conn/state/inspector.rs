#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InspectorCommandDispatch {
    protocol_method: &'static str,
    inspector_json: String,
}

impl InspectorCommandDispatch {
    pub(super) fn new(protocol_method: &'static str, inspector_json: String) -> Self {
        Self {
            protocol_method,
            inspector_json,
        }
    }

    pub(crate) fn protocol_method(&self) -> &'static str {
        self.protocol_method
    }

    pub(crate) fn into_inspector_json(self) -> String {
        self.inspector_json
    }
}
