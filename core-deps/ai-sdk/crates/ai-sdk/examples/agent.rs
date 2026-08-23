//! Example: an agent with tools and memory through the SDK facade.
//!
//! Run: `cargo run -p ai-sdk --example agent` (requires `.env`).

use std::sync::Arc;

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
    let (_provider, model) = client.resolve_model("opencode:deepseek-v4-flash")?;

    let mut tools = ai_sdk::default_tools();
    tools.register(Arc::new(ai_tools::FunctionTool::new(
        "greet",
        "Greets a person by name",
        serde_json::json!({
            "type": "object",
            "properties": {"name": {"type": "string"}},
            "required": ["name"]
        }),
        |args| {
            let name = args["name"].as_str().unwrap_or("world");
            Ok(ai_sdk::ToolOutput::ok(format!("Hello, {name}!")))
        },
    )));

    let agent = AgentBuilder::new(
        "example-agent",
        "You are helpful. Use tools when asked.",
        model,
    )
    .with_tools(tools)
    .with_max_iterations(4)
    .build();

    let result = agent
        .run("Use the greet tool for Ada, then say the greeting back.")
        .await?;
    println!("agent: {:?}", result.text.trim());

    Ok(())
}
