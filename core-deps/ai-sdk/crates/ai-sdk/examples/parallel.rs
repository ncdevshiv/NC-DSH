//! Example: parallel calls across two models (primary + vision) using the
//! parallel executor.
//!
//! Run: `cargo run -p ai-sdk --example parallel` (requires `.env`).

use ai_sdk::prelude::*;

#[tokio::main]
async fn main() -> ai_sdk::Result<()> {
    let config = ai_sdk::Config::load()?;
    let client = std::sync::Arc::new(
        ai_sdk::AiClient::builder()
            .provider(ai_sdk::create_provider(
                "opencode",
                config.provider("opencode")?,
            )?)
            .build()?,
    );

    let primary = "opencode:deepseek-v4-flash".to_string();
    let vision = "opencode:mimo-v2.5".to_string();

    let tasks = vec![
        ai_sdk::Task::new("primary", {
            let client = client.clone();
            let model = primary.clone();
            async move {
                client
                    .generate(
                        &model,
                        vec![Message::text(Role::User, "Reply with exactly: P1")],
                    )
                    .await
            }
        }),
        ai_sdk::Task::new("vision", {
            let client = client.clone();
            let model = vision.clone();
            async move {
                client
                    .generate(
                        &model,
                        vec![Message::text(Role::User, "Reply with exactly: V1")],
                    )
                    .await
            }
        }),
    ];

    let results = ai_sdk::Parallel::new().with_limit(2).execute(tasks).await;
    for result in results {
        match result.outcome {
            Ok(completion) => println!("{}: {:?}", result.name, completion.text.trim()),
            Err(e) => println!("{}: ERROR {e}", result.name),
        }
    }

    Ok(())
}
