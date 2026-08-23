//! Example: chat completion and streaming through the AI SDK facade.
//!
//! Run: `cargo run -p ai-sdk --example chat` (requires `.env`).

use ai_sdk::prelude::*;

#[tokio::main]
async fn main() -> ai_sdk::Result<()> {
    let config = ai_sdk::Config::load()?;
    let client = ai_sdk::AiClient::builder()
        .provider(ai_sdk::create_provider(
            "opencode",
            config.provider("opencode")?,
        )?)
        .build()?;

    let completion = client
        .generate(
            "opencode:deepseek-v4-flash",
            vec![Message::text(Role::User, "Reply with exactly: HELLO")],
        )
        .await?;
    println!("non-streaming: {}", completion.text);

    let events = client
        .stream(
            "opencode:deepseek-v4-flash",
            vec![Message::text(Role::User, "Reply with exactly: STREAM")],
        )
        .await?;
    let text = ai_sdk::collect_text(events).await?;
    println!("streaming: {text}");

    Ok(())
}
