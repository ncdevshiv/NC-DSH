/// Relationship between the current site and the top-level-site partition.
///
/// This is deliberately a three-state value. Some call sites know that a
/// context is opaque but do not know the URL/site that produced it; those paths
/// must not pretend the key is first-party or third-party.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum StoragePartitionRelation {
    /// The current site matches the top-level site.
    FirstParty,
    /// The current site differs from the top-level site.
    ThirdParty,
    /// The current site is not known, so the relation cannot be computed.
    Unknown,
}

impl StoragePartitionRelation {
    /// Compute the relation from two serialized site values.
    pub fn from_sites(current_site: &str, top_level_site: &str) -> Self {
        if current_site == top_level_site {
            Self::FirstParty
        } else {
            Self::ThirdParty
        }
    }

    /// Return whether this relation is explicitly third-party.
    pub fn is_third_party(self) -> bool {
        matches!(self, Self::ThirdParty)
    }
}
