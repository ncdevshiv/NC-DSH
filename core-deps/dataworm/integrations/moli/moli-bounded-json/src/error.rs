use std::{error::Error, fmt};

#[derive(Debug)]
pub enum BoundedJsonError {
    LimitExceeded { limit: usize },
    Serialization(serde_json::Error),
}

impl fmt::Display for BoundedJsonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LimitExceeded { limit } => {
                write!(formatter, "serialized JSON exceeds {limit} bytes")
            }
            Self::Serialization(error) => error.fmt(formatter),
        }
    }
}

impl Error for BoundedJsonError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::LimitExceeded { .. } => None,
            Self::Serialization(error) => Some(error),
        }
    }
}
