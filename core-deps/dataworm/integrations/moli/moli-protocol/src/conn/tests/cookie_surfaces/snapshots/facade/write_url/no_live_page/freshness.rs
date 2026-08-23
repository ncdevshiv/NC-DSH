use super::*;

#[tokio::test]
async fn browser_context_document_cookie_facade_snapshot_keeps_freshness_without_live_page() {
    let conn = configured_connection_without_live_page().await;
    let snapshot = conn
        .browser_context
        .as_ref()
        .unwrap()
        .document_cookie_facade_snapshot();

    assert!(snapshot.freshness.cache.is_none());
    assert_eq!(
        snapshot.freshness.cookie_get_freshness_status,
        BrowserContextCookieGetFreshnessStatus::NoLivePage
    );
    assert_eq!(
        snapshot.freshness.cookie_set_readiness_status,
        BrowserContextCookieSetReadinessStatus::NoLivePage
    );
    assert!(!snapshot.freshness.cookie_get_would_need_backend_access);
    assert!(!snapshot.freshness.cookie_get_would_need_backend_reconnect);
    assert!(!snapshot.freshness.cookie_get_would_hit_cache);
    assert!(!snapshot.freshness.cookie_get_would_revalidate_after_write);
}
