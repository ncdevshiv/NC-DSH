use std::{error::Error, fmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectorErrorKind {
    Syntax,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorError {
    kind: SelectorErrorKind,
    message: String,
}

impl SelectorError {
    pub fn syntax(message: impl Into<String>) -> Self {
        Self {
            kind: SelectorErrorKind::Syntax,
            message: message.into(),
        }
    }

    pub fn kind(&self) -> SelectorErrorKind {
        self.kind
    }

    pub fn code(&self) -> u16 {
        12
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for SelectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for SelectorError {}
