use url::Url;

use crate::{
    MoliSite, OpaqueOriginNonce, StoragePartitionRelation, site_for_url,
    storage_key_for_origin_and_top_level_site,
};

/// Storage and messaging isolation key used by Moli browser APIs.
///
/// `origin` is the serialized origin observable to JavaScript. It remains
/// `"null"` for opaque origins. `opaque_nonce` is the hidden component that
/// keeps distinct opaque-origin realms from colliding during internal routing.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct MoliStorageKey {
    /// JavaScript-facing serialized origin.
    origin: String,
    /// Partition component derived from the top-level browsing context.
    top_level_site: MoliSite,
    /// Per-realm discriminator for opaque origins that serialize as `"null"`.
    opaque_nonce: Option<OpaqueOriginNonce>,
    /// Known current-site relation for partition diagnostics and policy.
    partition_relation: StoragePartitionRelation,
    /// Whether the current document and its ancestors are not all same-site
    /// with the top-level site.
    cross_site_ancestor: bool,
}

impl MoliStorageKey {
    /// Build a key from already computed components.
    pub fn new(
        origin: String,
        top_level_site: String,
        opaque_nonce: Option<OpaqueOriginNonce>,
        partition_relation: StoragePartitionRelation,
    ) -> Self {
        let cross_site_ancestor = partition_relation.is_third_party();
        Self {
            origin,
            top_level_site: MoliSite::new(top_level_site),
            opaque_nonce,
            partition_relation,
            cross_site_ancestor,
        }
    }

    /// Build a first-party key for a URL.
    ///
    /// The URL's own site becomes the top-level site. Pass an opaque nonce when
    /// `url_needs_opaque_nonce(url)` is true.
    pub fn first_party_from_url(url: &Url, opaque_nonce: Option<OpaqueOriginNonce>) -> Self {
        let top_level_site = site_for_url(url);
        Self::from_url_and_top_level_site(url, top_level_site, opaque_nonce)
    }

    /// Build a key for a URL inside an explicit top-level-site partition.
    ///
    /// This is the path used by workers, which inherit the top-level site from
    /// their creator instead of deriving it solely from the worker script URL.
    pub fn from_url_and_top_level_site(
        url: &Url,
        top_level_site: String,
        opaque_nonce: Option<OpaqueOriginNonce>,
    ) -> Self {
        let origin = moli_url::origin_ascii_serialization(url);
        let current_site = site_for_url(url);
        let partition_relation =
            StoragePartitionRelation::from_sites(&current_site, &top_level_site);
        Self::new(origin, top_level_site, opaque_nonce, partition_relation)
    }

    /// Return the serialized origin exposed to JavaScript-facing APIs.
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// Return the serialized top-level-site partition component.
    pub fn top_level_site(&self) -> &str {
        self.top_level_site.as_str()
    }

    /// Return the hidden opaque-origin discriminator, when one is needed.
    pub fn opaque_nonce(&self) -> Option<OpaqueOriginNonce> {
        self.opaque_nonce
    }

    /// Return the known relation between the current site and top-level site.
    pub fn partition_relation(&self) -> StoragePartitionRelation {
        self.partition_relation
    }

    /// Return whether the current URL site is known to differ from top-level.
    pub fn is_third_party_partitioned(&self) -> bool {
        self.partition_relation.is_third_party()
    }

    /// Return whether the current frame/ancestor chain contains a site that
    /// differs from the top-level site.
    pub fn has_cross_site_ancestor(&self) -> bool {
        self.cross_site_ancestor
    }

    /// Mark this key as embedded below a cross-site ancestor.
    ///
    /// This is observable in serialization only when origin and top-level site
    /// are otherwise first-party. Third-party origins are already separated by
    /// those two components.
    pub fn with_cross_site_ancestor(mut self) -> Self {
        self.cross_site_ancestor = true;
        self
    }

    /// Return the internal key used by browser storage backends.
    ///
    /// All persisted storage keys use Moli's explicit serialized
    /// storage-key shape. The public JavaScript origin remains available
    /// through `origin()`, but backend owners must not collapse first-party
    /// storage back to the bare origin string.
    pub fn serialized_storage_key(&self) -> String {
        let mut serialized =
            storage_key_for_origin_and_top_level_site(&self.origin, self.top_level_site());
        if self.cross_site_ancestor && !self.partition_relation.is_third_party() {
            serialized.push_str(";cross-site-ancestor=1");
        }
        if let Some(nonce) = self.opaque_nonce {
            serialized.push_str(&format!(";opaque-nonce={}", nonce.get()));
        }
        serialized
    }
}
