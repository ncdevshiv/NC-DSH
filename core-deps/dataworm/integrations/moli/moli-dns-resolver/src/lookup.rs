use std::{
    collections::HashSet,
    net::{IpAddr, ToSocketAddrs},
    sync::Arc,
};

use crate::DnsTarget;

/// Terminal result published for one DNS lookup.
pub type DnsLookupResult = Result<Arc<[IpAddr]>, Arc<str>>;

pub(crate) type DnsLookup = dyn Fn(&DnsTarget) -> DnsLookupResult + Send + Sync + 'static;

pub(crate) fn system_dns_lookup(target: &DnsTarget) -> DnsLookupResult {
    let socket_addresses = (target.host(), target.port())
        .to_socket_addrs()
        .map_err(|error| {
            Arc::from(format!(
                "failed to resolve {}:{}: {error}",
                target.host(),
                target.port()
            ))
        })?;
    let mut seen = HashSet::new();
    let addresses = socket_addresses
        .map(|address| address.ip())
        .filter(|address| seen.insert(*address))
        .collect::<Vec<_>>();
    if addresses.is_empty() {
        return Err(Arc::from(format!(
            "DNS resolution returned no addresses for {}:{}",
            target.host(),
            target.port()
        )));
    }
    Ok(Arc::from(addresses))
}
