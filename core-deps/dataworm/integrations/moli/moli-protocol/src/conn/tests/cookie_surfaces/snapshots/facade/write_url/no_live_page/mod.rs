use super::*;

mod capability_surface;
mod freshness;
mod structured_write;

async fn configured_connection_without_live_page() -> CdpConnection {
    let mut conn = CdpConnection::new();
    conn.browser_context = Some(BrowserContext::new("BID-cookie-facade".into()));
    conn.browser_context
        .as_mut()
        .unwrap()
        .apply_cookie_manager_policy_overrides_async(
            &BrowserCookieFacadeOverrides::default()
                .with_cookies_enabled(false)
                .with_storage_access_status(
                    moli_cookie_jar::BrowserCookieStorageAccessStatus::Granted,
                ),
        )
        .await;
    conn
}
