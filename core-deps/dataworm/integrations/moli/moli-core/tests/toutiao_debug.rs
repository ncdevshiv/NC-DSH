use anyhow::Result;
use moli_core::{
    page::SubresourceNetworkOutcome,
    runtime::{Browser, BrowserConfig as AppConfig, RenderedDomWaitUntil},
};
use std::time::Duration;

fn evaluated_string(value: serde_json::Value) -> Option<String> {
    value
        .get("value")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

#[tokio::test]
#[ignore = "diagnostic smoke for real-world Toutiao article flow"]
async fn toutiao_article_shell_diagnostics() -> Result<()> {
    let browser = Browser::new(AppConfig::default())?;
    let mut page = browser
        .fetch_with_wait_until(
            "https://www.toutiao.com/article/7628119043257451050/",
            RenderedDomWaitUntil::NetworkIdle,
            Duration::from_secs(20),
        )
        .await?;

    println!("final_url={}", page.final_url());
    println!("status={}", page.status());
    println!(
        "title={:?}",
        evaluated_string(
            page.evaluate_runtime_expression_async("document.title")
                .await?
        )
    );
    println!(
        "runtime_state={:?}",
        evaluated_string(
            page.evaluate_runtime_expression_async(
                "JSON.stringify({ \
                href: location.href, \
                search: location.search, \
                readyState: document.readyState, \
                ttWidCtor: typeof TTWidInstance, \
                ttwidInstance: typeof ttwidInstance, \
                slardar: typeof Slardar, \
                cookie: document.cookie, \
                acReferer: sessionStorage.getItem('__ac_referer') \
            })"
            )
            .await?,
        )
    );

    println!(
        "console_messages={:?}",
        page.script_execution().console_messages()
    );
    println!(
        "lifecycle_errors={:?}",
        page.script_execution().lifecycle_errors()
    );
    println!("script_runs={}", page.script_execution().runs().len());
    for run in page.script_execution().runs() {
        println!(
            "script_run url={} source={:?} kind={:?} outcome={:?}",
            run.url(),
            run.source_kind(),
            run.kind(),
            run.outcome()
        );
    }

    let cookie_owner = page.document_cookie_owner_snapshot_async().await?;
    println!("document_cookie_owner={cookie_owner:#?}");

    let records = page.subresource_network_records();
    println!("subresource_records={}", records.len());
    for record in records {
        match record.outcome() {
            SubresourceNetworkOutcome::Success {
                final_url, status, ..
            } => {
                println!(
                    "subresource {:?} {} {} -> success status={} final_url={} cookie_sets={}",
                    record.resource_type(),
                    record.method(),
                    record.url(),
                    status,
                    final_url,
                    record.cookie_set_reports().len()
                );
            }
            SubresourceNetworkOutcome::Failure { error_text } => {
                println!(
                    "subresource {:?} {} {} -> failure error={} cookie_sets={}",
                    record.resource_type(),
                    record.method(),
                    record.url(),
                    error_text,
                    record.cookie_set_reports().len()
                );
            }
        }
    }

    Ok(())
}
