use std::{collections::HashMap, sync::Arc};

use crate::stylesheet_blocking::{StylesheetFetch, StylesheetFetchIdentity};

use super::StylesheetLinkClient;

/// Pending DOM clients grouped by the exact typed resource they observe.
///
/// Authoritative owner state remains in `StylesheetOwnerRuntimeStates`. This
/// index only narrows terminal fan-out and preserves client registration order.
#[derive(Debug, Default)]
pub(in crate::document_runtime) struct StylesheetLinkClientIndex {
    // Each non-empty value retains clients, and every client retains its fetch.
    // That keeps the address-derived identity live until the entry is removed.
    by_fetch: HashMap<StylesheetFetchIdentity, Vec<Arc<StylesheetLinkClient>>>,
    client_count: usize,
}

impl StylesheetLinkClientIndex {
    pub(in crate::document_runtime) fn register(&mut self, client: Arc<StylesheetLinkClient>) {
        debug_assert!(
            client.fetch().terminal().is_none(),
            "terminal stylesheet clients must be delivered directly"
        );
        let clients = self.by_fetch.entry(client.fetch().identity()).or_default();
        if clients
            .iter()
            .any(|current| StylesheetLinkClient::ptr_eq(current, &client))
        {
            return;
        }
        clients.push(client);
        self.client_count += 1;
    }

    pub(in crate::document_runtime) fn unregister(&mut self, client: &Arc<StylesheetLinkClient>) {
        let fetch = client.fetch();
        let identity = fetch.identity();
        let remove_entry = {
            let Some(clients) = self.by_fetch.get_mut(&identity) else {
                return;
            };
            let previous_len = clients.len();
            clients.retain(|current| !StylesheetLinkClient::ptr_eq(current, client));
            self.client_count -= previous_len - clients.len();
            clients.is_empty()
        };
        if remove_entry {
            self.by_fetch.remove(&identity);
        }
    }

    pub(in crate::document_runtime) fn take_for_fetch(
        &mut self,
        fetch: &StylesheetFetch,
    ) -> Vec<Arc<StylesheetLinkClient>> {
        let clients = self.by_fetch.remove(&fetch.identity()).unwrap_or_default();
        self.client_count -= clients.len();
        clients
    }

    pub(in crate::document_runtime) fn len(&self) -> usize {
        self.client_count
    }

    #[cfg(test)]
    pub(in crate::document_runtime) fn contains(&self, client: &Arc<StylesheetLinkClient>) -> bool {
        self.by_fetch
            .get(&client.fetch().identity())
            .is_some_and(|clients| {
                clients
                    .iter()
                    .any(|current| StylesheetLinkClient::ptr_eq(current, client))
            })
    }
}
