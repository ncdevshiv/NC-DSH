use std::{fmt, str::FromStr};

pub const DEFAULT_PROFILE_PARTITION_ID: &str = "default";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProfilePartitionId(String);

impl ProfilePartitionId {
    pub fn new(id: impl Into<String>) -> Result<Self, ProfilePartitionIdError> {
        let id = id.into();
        validate_profile_partition_id(&id)?;
        Ok(Self(id))
    }

    pub fn default_partition() -> Self {
        Self(DEFAULT_PROFILE_PARTITION_ID.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_default(&self) -> bool {
        self.0 == DEFAULT_PROFILE_PARTITION_ID
    }
}

impl Default for ProfilePartitionId {
    fn default() -> Self {
        Self::default_partition()
    }
}

impl AsRef<str> for ProfilePartitionId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ProfilePartitionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ProfilePartitionId {
    type Err = ProfilePartitionIdError;

    fn from_str(id: &str) -> Result<Self, Self::Err> {
        Self::new(id)
    }
}

impl TryFrom<&str> for ProfilePartitionId {
    type Error = ProfilePartitionIdError;

    fn try_from(id: &str) -> Result<Self, Self::Error> {
        Self::new(id)
    }
}

impl TryFrom<String> for ProfilePartitionId {
    type Error = ProfilePartitionIdError;

    fn try_from(id: String) -> Result<Self, Self::Error> {
        Self::new(id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfilePartitionIdError {
    Empty,
    DotSegment,
    InvalidCharacter,
}

impl fmt::Display for ProfilePartitionIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("profile partition id is empty"),
            Self::DotSegment => f.write_str("profile partition id cannot be `.` or `..`"),
            Self::InvalidCharacter => {
                f.write_str("profile partition id contains a path-unsafe character")
            }
        }
    }
}

impl std::error::Error for ProfilePartitionIdError {}

fn validate_profile_partition_id(id: &str) -> Result<(), ProfilePartitionIdError> {
    if id.is_empty() {
        return Err(ProfilePartitionIdError::Empty);
    }
    if id == "." || id == ".." {
        return Err(ProfilePartitionIdError::DotSegment);
    }
    if id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        Ok(())
    } else {
        Err(ProfilePartitionIdError::InvalidCharacter)
    }
}

#[cfg(test)]
mod tests {
    use super::{ProfilePartitionId, ProfilePartitionIdError};

    #[test]
    fn profile_partition_id_accepts_path_safe_ascii_ids() {
        for id in ["default", "tenant-a", "tenant_a", "tenant.a", "Tenant9"] {
            let parsed = ProfilePartitionId::new(id).expect("id should be valid");
            assert_eq!(parsed.as_str(), id);
        }
    }

    #[test]
    fn profile_partition_id_rejects_path_unsafe_ids() {
        for (id, expected) in [
            ("", ProfilePartitionIdError::Empty),
            (".", ProfilePartitionIdError::DotSegment),
            ("..", ProfilePartitionIdError::DotSegment),
            ("tenant/a", ProfilePartitionIdError::InvalidCharacter),
            ("tenant\\a", ProfilePartitionIdError::InvalidCharacter),
            ("C:", ProfilePartitionIdError::InvalidCharacter),
            ("tenant a", ProfilePartitionIdError::InvalidCharacter),
            ("租户", ProfilePartitionIdError::InvalidCharacter),
        ] {
            assert_eq!(
                ProfilePartitionId::new(id).expect_err("id should be invalid"),
                expected,
                "{id:?}"
            );
        }
    }
}
