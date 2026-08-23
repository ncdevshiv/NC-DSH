#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebCryptoError {
    Data,
    Operation,
    Syntax,
}

impl WebCryptoError {
    pub fn message(self) -> &'static str {
        match self {
            Self::Data => "DataError",
            Self::Operation => "OperationError",
            Self::Syntax => "SyntaxError",
        }
    }
}
