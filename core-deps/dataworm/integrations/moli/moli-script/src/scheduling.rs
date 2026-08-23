use moli_page_types::{ScriptKind, ScriptMode, ScriptSourceKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScriptElementClassificationInput<'a> {
    pub script_type: Option<&'a str>,
    pub language: Option<&'a str>,
    pub event: Option<&'a str>,
    pub for_attribute: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptElementClassification {
    pub kind: ScriptKind,
    pub legacy_event_for_mismatch: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptSchedulingInput {
    pub parser_inserted: bool,
    pub allow_parser_blocking_modes: bool,
    pub force_async: bool,
    pub async_attribute_present: bool,
    pub defer_attribute_present: bool,
    pub kind: ScriptKind,
    pub source_kind: ScriptSourceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptPreparationClassificationInput<'a> {
    pub element: ScriptElementClassificationInput<'a>,
    pub parser_inserted: bool,
    pub allow_parser_blocking_modes: bool,
    pub force_async: bool,
    pub async_attribute_present: bool,
    pub defer_attribute_present: bool,
    pub source_kind: ScriptSourceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptPreparationClassification {
    pub disposition: ScriptPreparationDisposition,
    pub legacy_event_for_mismatch: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptPreparationDisposition {
    Classic(ScriptMode),
    Module(ScriptMode),
    ImportMap,
    DataBlock,
}

impl ScriptPreparationDisposition {
    pub fn kind(self) -> ScriptKind {
        match self {
            Self::Classic(_) => ScriptKind::Classic,
            Self::Module(_) => ScriptKind::Module,
            Self::ImportMap => ScriptKind::ImportMap,
            Self::DataBlock => ScriptKind::DataBlock,
        }
    }

    pub fn executable(self) -> Option<(ScriptKind, ScriptMode)> {
        match self {
            Self::Classic(mode) => Some((ScriptKind::Classic, mode)),
            Self::Module(mode) => Some((ScriptKind::Module, mode)),
            Self::ImportMap | Self::DataBlock => None,
        }
    }
}
