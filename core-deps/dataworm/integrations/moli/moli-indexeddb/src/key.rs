use std::cmp::Ordering as CmpOrdering;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Key {
    String(String),
    Integer(i64),
    Array(Vec<Key>),
}

impl Ord for Key {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        match (self, other) {
            (Self::Integer(left), Self::Integer(right)) => left.cmp(right),
            (Self::String(left), Self::String(right)) => left.cmp(right),
            (Self::Array(left), Self::Array(right)) => left.cmp(right),
            (Self::Integer(_), Self::String(_)) => CmpOrdering::Less,
            (Self::Integer(_), Self::Array(_)) => CmpOrdering::Less,
            (Self::String(_), Self::Integer(_)) => CmpOrdering::Greater,
            (Self::String(_), Self::Array(_)) => CmpOrdering::Less,
            (Self::Array(_), Self::Integer(_) | Self::String(_)) => CmpOrdering::Greater,
        }
    }
}

impl PartialOrd for Key {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

impl From<&str> for Key {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<String> for Key {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<i64> for Key {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum KeyPath {
    String(String),
    Sequence(Vec<String>),
}

impl KeyPath {
    pub fn is_sequence(&self) -> bool {
        matches!(self, Self::Sequence(_))
    }

    pub fn is_empty_string(&self) -> bool {
        matches!(self, Self::String(value) if value.is_empty())
    }
}

impl From<String> for KeyPath {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&str> for KeyPath {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}
