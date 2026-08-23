//! Live integration tests against the real project gateway.
//!
//! These tests perform **real HTTP calls** — no mocks, no fake providers.
//! Like `hercules_bench` / `orchestra_live`, every test is `#[ignore]`d AND
//! credential-gated: plain `cargo test` never touches the network; run them
//! explicitly with `-- --ignored` and the environment below. When the
//! variables are absent or blank, each test skips with a message instead of
//! calling anything.
//!
//! Required env (see `.env.example`):
//! - `AI_SDK_GATEWAY_BASE_URL`
//! - `AI_SDK_GATEWAY_API_KEY`
//! - `AI_SDK_PRIMARY_MODEL`    (deepseek-v4-flash)
//! - `AI_SDK_VISION_MODEL`     (mimo-v2.5)

use std::sync::Arc;

use ai_core::{AiClient, ChatRequest, Provider, ResponseFormat, ToolDefinition};
use ai_errors::AiError;
use ai_models::ModelRegistry;

use ai_runtime::Parallel;
use ai_stream::collect_text;
use ai_types::{ContentPart, Message, Role, StreamEvent};
use futures::StreamExt;

/// A 1×1 solid-red PNG (generated programmatically, embedded as base64) used
/// to exercise vision input without external image hosts.
const RED_PIXEL_PNG_BASE64: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";

struct Gateway {
    provider_id: String,
    base_url: String,
    api_key: String,
    primary_model: String,
    vision_model: String,
}

fn gateway_from_env() -> Option<Gateway> {
    // Blank env values count as unset (same rule as `ai-config`): CI
    // workflows commonly export `ENV: ${{ secrets.ENV }}`, which creates
    // EMPTY variables when the secret does not exist. Treating empty as
    // configured would fire real calls at an empty base URL.
    let base_url = std::env::var("AI_SDK_GATEWAY_BASE_URL")
        .ok()
        .filter(|v| !v.trim().is_empty())?;
    let api_key = std::env::var("AI_SDK_GATEWAY_API_KEY")
        .ok()
        .filter(|v| !v.trim().is_empty())?;
    let primary_model =
        std::env::var("AI_SDK_PRIMARY_MODEL").unwrap_or_else(|_| "deepseek-v4-flash".to_string());
    let vision_model =
        std::env::var("AI_SDK_VISION_MODEL").unwrap_or_else(|_| "mimo-v2.5".to_string());
    Some(Gateway {
        provider_id: "opencode".to_string(),
        base_url,
        api_key,
        primary_model,
        vision_model,
    })
}

fn build_provider(gateway: &Gateway) -> Arc<ai_providers::openai_compat::OpenAiCompatProvider> {
    Arc::new(
        ai_providers::openai_compat::OpenAiCompatProvider::new(
            ai_providers::openai_compat::OpenAiCompatConfig::new(
                gateway.provider_id.clone(),
                gateway.api_key.clone(),
                gateway.base_url.clone(),
            ),
        )
        .expect("provider builds"),
    )
}

fn build_client(gateway: &Gateway) -> AiClient {
    AiClient::builder()
        .provider(build_provider(gateway))
        .registry(ModelRegistry::new())
        .build()
        .expect("client builds")
}

fn gateway_available(gateway: &Option<Gateway>) -> bool {
    gateway.is_some()
}

// ---------------------------------------------------------------------------
// Model discovery
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "live gateway suite: real HTTP calls; run with -- --ignored and AI_SDK_GATEWAY_* env set"]
async fn list_models_contains_primary_and_vision() {
    let gateway = gateway_from_env();
    if !gateway_available(&gateway) {
        eprintln!("SKIP: gateway env not set");
        return;
    }
    let gateway = gateway.unwrap();
    let provider = build_provider(&gateway);

    let models = provider.list_models().await.expect("list_models succeeds");
    let ids: Vec<String> = models.iter().map(|m| m.id.to_string()).collect();

    assert!(
        ids.iter().any(|id| id == &gateway.primary_model),
        "primary model {} not in gateway model list: {:?}",
        gateway.primary_model,
        ids
    );
    assert!(
        ids.iter().any(|id| id == &gateway.vision_model),
        "vision model {} not in gateway model list: {:?}",
        gateway.vision_model,
        ids
    );
    eprintln!("PASS: {} models listed, primary+vision present", ids.len());
}

// ---------------------------------------------------------------------------
// Non-streaming generation
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "live gateway suite: real HTTP calls; run with -- --ignored and AI_SDK_GATEWAY_* env set"]
async fn generate_non_streaming_primary_exact_reply() {
    let gateway = gateway_from_env();
    if !gateway_available(&gateway) {
        eprintln!("SKIP: gateway env not set");
        return;
    }
    let gateway = gateway.unwrap();
    let client = build_client(&gateway);

    let completion = client
        .generate(
            &format!("{}:{}", gateway.provider_id, gateway.primary_model),
            vec![Message::text(Role::User, "Reply with exactly: PONG")],
        )
        .await
        .expect("generate succeeds");

    assert_eq!(
        completion.text.trim(),
        "PONG",
        "model must reply PONG exactly"
    );
    assert_eq!(completion.provider.as_str(), "opencode");
    assert_eq!(completion.model.as_str(), gateway.primary_model);
    eprintln!(
        "PASS: non-streaming generate returned exact reply ({} tokens)",
        completion.usage.total()
    );
}

#[tokio::test]
#[ignore = "live gateway suite: real HTTP calls; run with -- --ignored and AI_SDK_GATEWAY_* env set"]
async fn generate_exposes_reasoning_content() {
    let gateway = gateway_from_env();
    if !gateway_available(&gateway) {
        eprintln!("SKIP: gateway env not set");
        return;
    }
    let gateway = gateway.unwrap();
    let client = build_client(&gateway);

    let completion = client
        .generate(
            &format!("{}:{}", gateway.provider_id, gateway.primary_model),
            vec![Message::text(
                Role::User,
                "What is 2+2? Reply with only the number.",
            )],
        )
        .await
        .expect("generate succeeds");

    // deepseek-v4-flash exposes reasoning_content (observed in the contract
    // probe); the adapter must surface it on Completion.
    assert!(
        completion.reasoning.is_some(),
        "expected reasoning_content to be surfaced; raw={}",
        completion.raw
    );
    assert!(!completion.reasoning.as_deref().unwrap_or("").is_empty());
    eprintln!(
        "PASS: reasoning surfaced ({} chars)",
        completion.reasoning.as_deref().unwrap_or("").len()
    );
}

#[tokio::test]
#[ignore = "live gateway suite: real HTTP calls; run with -- --ignored and AI_SDK_GATEWAY_* env set"]
async fn generate_reports_usage_and_finish_reason() {
    let gateway = gateway_from_env();
    if !gateway_available(&gateway) {
        eprintln!("SKIP: gateway env not set");
        return;
    }
    let gateway = gateway.unwrap();
    let client = build_client(&gateway);

    let completion = client
        .generate(
            &format!("{}:{}", gateway.provider_id, gateway.primary_model),
            vec![Message::text(Role::User, "Say hello.")],
        )
        .await
        .expect("generate succeeds");

    assert!(completion.usage.total() > 0, "usage must be reported");
    assert!(
        matches!(
            completion.finish_reason.as_deref(),
            Some("stop") | Some("length")
        ),
        "unexpected finish_reason: {:?}",
        completion.finish_reason
    );
    eprintln!(
        "PASS: usage in={} out={} total={} finish={:?}",
        completion.usage.input_tokens,
        completion.usage.output_tokens,
        completion.usage.total(),
        completion.finish_reason
    );
}

// ---------------------------------------------------------------------------
// Streaming
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "live gateway suite: real HTTP calls; run with -- --ignored and AI_SDK_GATEWAY_* env set"]
async fn stream_primary_collects_expected_text() {
    let gateway = gateway_from_env();
    if !gateway_available(&gateway) {
        eprintln!("SKIP: gateway env not set");
        return;
    }
    let gateway = gateway.unwrap();
    let client = build_client(&gateway);

    let stream = client
        .stream(
            &format!("{}:{}", gateway.provider_id, gateway.primary_model),
            vec![Message::text(
                Role::User,
                "Reply with exactly: STREAMING-OK",
            )],
        )
        .await
        .expect("stream opens");
    let text = collect_text(stream).await.expect("stream text collected");

    assert!(
        text.contains("STREAMING-OK"),
        "streamed text must contain the expected marker; got: {text:?}"
    );
    eprintln!("PASS: streamed text = {text:?}");
}

#[tokio::test]
#[ignore = "live gateway suite: real HTTP calls; run with -- --ignored and AI_SDK_GATEWAY_* env set"]
async fn stream_emits_unified_events() {
    let gateway = gateway_from_env();
    if !gateway_available(&gateway) {
        eprintln!("SKIP: gateway env not set");
        return;
    }
    let gateway = gateway.unwrap();
    let client = build_client(&gateway);

    let mut stream = client
        .stream(
            &format!("{}:{}", gateway.provider_id, gateway.primary_model),
            vec![Message::text(Role::User, "Say the word alpha.")],
        )
        .await
        .expect("stream opens");

    let mut saw_text = false;
    let mut saw_completed = false;
    let mut saw_usage = false;
    let mut count = 0usize;
    while let Some(event) = stream.next().await {
        let event = event.expect("stream events are Ok");
        count += 1;
        match event {
            StreamEvent::TextDelta { delta } => {
                if !delta.is_empty() {
                    saw_text = true;
                }
            }
            StreamEvent::Completed { .. } => saw_completed = true,
            StreamEvent::UsageUpdate { usage } if usage.total() > 0 => {
                saw_usage = true;
            }
            _ => {}
        }
    }

    assert!(count > 0, "stream must emit at least one event");
    assert!(saw_text, "stream must emit text deltas");
    assert!(saw_completed, "stream must emit a Completed event");
    assert!(saw_usage, "stream must emit usage");
    eprintln!(
        "PASS: {} unified events, text+completed+usage all present",
        count
    );
}

// ---------------------------------------------------------------------------
// Tool calling (full loop through the real API)
// ---------------------------------------------------------------------------

fn calculator_tool() -> ToolDefinition {
    ToolDefinition::new(
        "calculator",
        "Evaluates a simple arithmetic expression and returns the numeric result",
        serde_json::json!({
            "type": "object",
            "properties": {
                "expression": {
                    "type": "string",
                    "description": "Arithmetic expression, e.g. '6 * 7'"
                }
            },
            "required": ["expression"]
        }),
    )
}

#[tokio::test]
#[ignore = "live gateway suite: real HTTP calls; run with -- --ignored and AI_SDK_GATEWAY_* env set"]
async fn tool_calling_full_loop_returns_42() {
    let gateway = gateway_from_env();
    if !gateway_available(&gateway) {
        eprintln!("SKIP: gateway env not set");
        return;
    }
    let gateway = gateway.unwrap();
    let client = build_client(&gateway);
    let reference = format!("{}:{}", gateway.provider_id, gateway.primary_model);

    // Round 1: the model should decide to call the calculator tool.
    let request = ChatRequest::new(vec![Message::text(
        Role::User,
        "Use the calculator tool to compute 6 * 7, then reply with the result as a single number.",
    )])
    .with_tools(vec![calculator_tool()])
    .with_max_tokens(300);
    let completion = client
        .generate_request(&reference, request)
        .await
        .expect("tool round 1 succeeds");

    assert!(
        !completion.tool_calls.is_empty(),
        "model must call the calculator tool; text={:?}",
        completion.text
    );
    let call = &completion.tool_calls[0];
    assert_eq!(call.name, "calculator");
    eprintln!(
        "PASS: round 1 — model called `{}` with {:?}",
        call.name, call.arguments
    );

    // Execute the tool for real (local arithmetic — this is OUR tool, not
    // a fake provider; the LLM call itself was real).
    let args: serde_json::Value = serde_json::from_str(&call.arguments).expect("args are JSON");
    let expression = args["expression"].as_str().expect("expression arg");
    // Minimal safe evaluator for basic arithmetic (no eval()).
    let result = evaluate_basic_arithmetic(expression).expect("expression evaluates");
    eprintln!("PASS: calculator evaluated {expression} = {result}");

    // Round 2: feed the tool result back and let the model answer.
    let messages = vec![
        Message::text(
            Role::User,
            "Use the calculator tool to compute 6 * 7, then reply with the result as a single number.",
        ),
        Message::new(
            Role::Assistant,
            vec![ContentPart::ToolCall { call: call.clone() }],
        ),
        Message::new(
            Role::Tool,
            vec![ContentPart::tool_result(
                &call.id,
                &call.name,
                format!(r#"{{"result":{result}}}"#),
                false,
            )],
        ),
    ];
    let final_completion = client
        .generate(&reference, messages)
        .await
        .expect("tool round 2 succeeds");
    assert!(
        final_completion.text.contains("42"),
        "final answer must contain 42; got: {:?}",
        final_completion.text
    );
    eprintln!(
        "PASS: round 2 — final answer = {:?}",
        final_completion.text.trim()
    );
}

#[tokio::test]
#[ignore = "live gateway suite: real HTTP calls; run with -- --ignored and AI_SDK_GATEWAY_* env set"]
async fn streamed_tool_call_is_finalized() {
    let gateway = gateway_from_env();
    if !gateway_available(&gateway) {
        eprintln!("SKIP: gateway env not set");
        return;
    }
    let gateway = gateway.unwrap();
    let client = build_client(&gateway);
    let reference = format!("{}:{}", gateway.provider_id, gateway.primary_model);

    let request = ChatRequest::new(vec![Message::text(
        Role::User,
        "Use the calculator tool to compute 5 + 3, then reply with the result as a single number.",
    )])
    .with_tools(vec![calculator_tool()])
    .with_max_tokens(300);

    let mut stream = client
        .stream_request(&reference, request)
        .await
        .expect("stream opens");
    let mut completed_calls = 0usize;
    while let Some(event) = stream.next().await {
        if let StreamEvent::ToolCallCompleted { call } = event.expect("event ok") {
            assert_eq!(call.name, "calculator");
            completed_calls += 1;
        }
    }
    assert!(completed_calls >= 1, "streamed tool call must be finalized");
    eprintln!("PASS: streamed tool call finalized ({completed_calls} call(s))");
}

// ---------------------------------------------------------------------------
// Structured output
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "live gateway suite: real HTTP calls; run with -- --ignored and AI_SDK_GATEWAY_* env set"]
async fn structured_json_object_output() {
    let gateway = gateway_from_env();
    if !gateway_available(&gateway) {
        eprintln!("SKIP: gateway env not set");
        return;
    }
    let gateway = gateway.unwrap();
    let client = build_client(&gateway);
    let reference = format!("{}:{}", gateway.provider_id, gateway.primary_model);

    let request = ChatRequest::new(vec![Message::text(
        Role::User,
        "Return a JSON object with exactly two keys: \"ok\" (boolean, always true) and \"answer\" (string, the word 'yes').",
    )])
    .with_response_format(ResponseFormat::JsonObject)
    .with_max_tokens(200);

    let completion = client
        .generate_request(&reference, request)
        .await
        .expect("structured generate succeeds");

    let parsed: serde_json::Value = serde_json::from_str(&completion.text)
        .unwrap_or_else(|e| panic!("output must be valid JSON; got {:?} ({e})", completion.text));
    assert_eq!(parsed["ok"], serde_json::json!(true), "ok must be true");
    assert_eq!(
        parsed["answer"],
        serde_json::json!("yes"),
        "answer must be 'yes'"
    );
    eprintln!("PASS: structured JSON output = {}", parsed);
}

// ---------------------------------------------------------------------------
// Vision (secondary model)
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "live gateway suite: real HTTP calls; run with -- --ignored and AI_SDK_GATEWAY_* env set"]
async fn vision_model_identifies_red_pixel() {
    let gateway = gateway_from_env();
    if !gateway_available(&gateway) {
        eprintln!("SKIP: gateway env not set");
        return;
    }
    let gateway = gateway.unwrap();
    let client = build_client(&gateway);
    let reference = format!("{}:{}", gateway.provider_id, gateway.vision_model);

    let messages = vec![Message::new(
        Role::User,
        vec![
            ContentPart::text("What is the color of this image? Reply with a single word."),
            ContentPart::Image {
                image: ai_types::ImageSource::Base64 {
                    media_type: "image/png".to_string(),
                    data: RED_PIXEL_PNG_BASE64.to_string(),
                },
            },
        ],
    )];

    let completion = client
        .generate(&reference, messages)
        .await
        .expect("vision generate succeeds");

    let answer = completion.text.to_lowercase();
    assert!(
        answer.contains("red"),
        "vision model must identify red; got: {:?}",
        completion.text
    );
    eprintln!(
        "PASS: vision model identified color: {:?}",
        completion.text.trim()
    );
}

// ---------------------------------------------------------------------------
// Error paths (real negative tests)
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "live gateway suite: real HTTP calls; run with -- --ignored and AI_SDK_GATEWAY_* env set"]
async fn invalid_api_key_returns_authentication_error() {
    let gateway = gateway_from_env();
    if !gateway_available(&gateway) {
        eprintln!("SKIP: gateway env not set");
        return;
    }
    let gateway = gateway.unwrap();
    let provider = Arc::new(
        ai_providers::openai_compat::OpenAiCompatProvider::new(
            ai_providers::openai_compat::OpenAiCompatConfig::new(
                "opencode",
                "sk-invalid-key-for-testing",
                gateway.base_url.clone(),
            ),
        )
        .expect("provider builds"),
    );

    let model = provider
        .model(&gateway.primary_model)
        .expect("model resolves");
    let err = model
        .generate(ChatRequest::new(vec![Message::text(Role::User, "hi")]))
        .await
        .expect_err("invalid key must fail");
    assert!(
        matches!(err, AiError::Authentication(_)),
        "expected AuthenticationError, got: {err}"
    );
    eprintln!("PASS: invalid key → AuthenticationError: {err}");
}

/// Gateway contract fact (verified 2026-08-09): the gateway answers
/// **HTTP 401** for unknown models ("Model <id> is not supported"), the
/// same status it uses for invalid API keys. Per the standard OpenAI
/// contract, our adapter maps 401 → AuthenticationError. This test asserts
/// the observed contract and documents the quirk; if the gateway later
/// distinguishes the status codes, this test must be updated.
#[tokio::test]
#[ignore = "live gateway suite: real HTTP calls; run with -- --ignored and AI_SDK_GATEWAY_* env set"]
async fn unknown_model_returns_typed_error() {
    let gateway = gateway_from_env();
    if !gateway_available(&gateway) {
        eprintln!("SKIP: gateway env not set");
        return;
    }
    let gateway = gateway.unwrap();
    let client = build_client(&gateway);

    let err = client
        .generate(
            &format!("{}:definitely-not-a-real-model-xyz", gateway.provider_id),
            vec![Message::text(Role::User, "hi")],
        )
        .await
        .expect_err("unknown model must fail");
    assert!(
        matches!(err, AiError::Authentication(_) | AiError::Provider(_)),
        "expected a typed auth/provider error, got: {err}"
    );
    assert!(
        err.to_string().contains("not supported")
            || err.to_string().contains("not exist")
            || err.to_string().contains("model"),
        "error must reference the unknown model; got: {err}"
    );
    eprintln!("PASS: unknown model → typed error: {err}");
}

// ---------------------------------------------------------------------------
// Parallel execution across both models (real, concurrent)
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "live gateway suite: real HTTP calls; run with -- --ignored and AI_SDK_GATEWAY_* env set"]
async fn parallel_calls_both_models_concurrently() {
    let gateway = gateway_from_env();
    if !gateway_available(&gateway) {
        eprintln!("SKIP: gateway env not set");
        return;
    }
    let gateway = gateway.unwrap();
    let client = Arc::new(build_client(&gateway));

    let primary_ref = format!("{}:{}", gateway.provider_id, gateway.primary_model);
    let vision_ref = format!("{}:{}", gateway.provider_id, gateway.vision_model);

    let tasks = vec![
        ai_runtime::Task::new("primary", {
            let client = client.clone();
            let reference = primary_ref.clone();
            async move {
                client
                    .generate(
                        &reference,
                        vec![Message::text(Role::User, "Reply with exactly: PRIMARY")],
                    )
                    .await
            }
        }),
        ai_runtime::Task::new("vision", {
            let client = client.clone();
            let reference = vision_ref.clone();
            async move {
                client
                    .generate(
                        &reference,
                        vec![Message::text(Role::User, "Reply with exactly: VISION")],
                    )
                    .await
            }
        }),
    ];

    let results = Parallel::new().with_limit(2).execute(tasks).await;
    assert_eq!(results.len(), 2);
    for result in &results {
        assert!(
            result.succeeded(),
            "task `{}` must succeed: {:?}",
            result.name,
            result.outcome
        );
    }
    let primary = results[0].outcome.as_ref().unwrap();
    assert_eq!(primary.text.trim(), "PRIMARY");
    let vision = results[1].outcome.as_ref().unwrap();
    assert_eq!(vision.text.trim(), "VISION");
    eprintln!("PASS: parallel calls to both models succeeded concurrently");
}

// ---------------------------------------------------------------------------
// Helper: basic arithmetic evaluator for the calculator tool
// ---------------------------------------------------------------------------

/// Evaluates simple arithmetic expressions (`+ - * /` and parentheses) used
/// by the calculator tool. No eval() — a tiny recursive-descent parser.
fn evaluate_basic_arithmetic(input: &str) -> Result<f64, String> {
    let mut parser = ExprParser {
        chars: input.chars().filter(|c| !c.is_whitespace()).collect(),
        pos: 0,
    };
    let value = parser.parse_expr()?;
    if parser.pos != parser.chars.len() {
        return Err(format!("trailing characters in `{input}`"));
    }
    Ok(value)
}

struct ExprParser {
    chars: Vec<char>,
    pos: usize,
}

impl ExprParser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn parse_expr(&mut self) -> Result<f64, String> {
        let mut value = self.parse_term()?;
        loop {
            match self.peek() {
                Some('+') => {
                    self.next();
                    value += self.parse_term()?;
                }
                Some('-') => {
                    self.next();
                    value -= self.parse_term()?;
                }
                _ => return Ok(value),
            }
        }
    }

    fn parse_term(&mut self) -> Result<f64, String> {
        let mut value = self.parse_factor()?;
        loop {
            match self.peek() {
                Some('*') => {
                    self.next();
                    value *= self.parse_factor()?;
                }
                Some('/') => {
                    self.next();
                    let divisor = self.parse_factor()?;
                    if divisor == 0.0 {
                        return Err("division by zero".to_string());
                    }
                    value /= divisor;
                }
                _ => return Ok(value),
            }
        }
    }

    fn parse_factor(&mut self) -> Result<f64, String> {
        if self.peek() == Some('(') {
            self.next();
            let value = self.parse_expr()?;
            if self.next() != Some(')') {
                return Err("missing closing parenthesis".to_string());
            }
            return Ok(value);
        }
        let mut digits = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == '.' || c == '-' && digits.is_empty() {
                digits.push(c);
                self.next();
            } else {
                break;
            }
        }
        if digits.is_empty() {
            return Err(format!("expected a number at position {}", self.pos));
        }
        digits
            .parse::<f64>()
            .map_err(|e| format!("invalid number `{digits}`: {e}"))
    }
}

// ---------------------------------------------------------------------------
// Agent runtime (live): full tool loop through the agent with the primary
// model only.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "live gateway suite: real HTTP calls; run with -- --ignored and AI_SDK_GATEWAY_* env set"]
async fn agent_tool_loop_live_primary_model() {
    let gateway = gateway_from_env();
    if !gateway_available(&gateway) {
        eprintln!("SKIP: gateway env not set");
        return;
    }
    let gateway = gateway.unwrap();
    let client = Arc::new(build_client(&gateway));
    let reference = format!("{}:{}", gateway.provider_id, gateway.primary_model);

    let (_provider_name, model) = client.resolve_model(&reference).expect("model resolves");

    let mut tools = ai_tools::ToolRegistry::new();
    tools.register(Arc::new(ai_tools::FunctionTool::new(
        "calculator",
        "Evaluates a simple arithmetic expression like '6 * 7' and returns the numeric result",
        serde_json::json!({
            "type": "object",
            "properties": {"expression": {"type": "string"}},
            "required": ["expression"]
        }),
        |args| {
            let expression = args["expression"].as_str().unwrap_or("");
            let result = evaluate_basic_arithmetic(expression)
                .map_err(|e| AiError::Tool(ai_errors::ToolError::new("calculator", e)))?;
            Ok(ai_tools::ToolOutput::ok(result.to_string()))
        },
    )));

    let agent = ai_agents::AgentBuilder::new(
        "live-agent",
        "You are a helpful assistant. Use the calculator tool for arithmetic.",
        model,
    )
    .with_tools(tools)
    .with_max_iterations(5)
    .build();

    let result = agent
        .run("Use the calculator tool to compute 6 * 7, then reply with the result as a single number.")
        .await
        .expect("agent run succeeds");

    assert!(
        result.text.contains("42"),
        "agent answer must contain 42: {:?}",
        result.text
    );
    assert!(result.tool_calls_used >= 1, "agent must have used the tool");
    eprintln!(
        "PASS: agent tool loop ({} tool call(s), {} iterations) -> {:?}",
        result.tool_calls_used,
        result.iterations,
        result.text.trim()
    );
}

// ---------------------------------------------------------------------------
// Self-hosted RAG + semantic memory (live, primary model only): embeddings
// are computed locally (StatisticalEmbeddings) — no external service.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "live gateway suite: real HTTP calls; run with -- --ignored and AI_SDK_GATEWAY_* env set"]
async fn rag_self_hosted_live_answer_from_retrieved_context() {
    let gateway = gateway_from_env();
    if !gateway_available(&gateway) {
        eprintln!("SKIP: gateway env not set");
        return;
    }
    let gateway = gateway.unwrap();
    let client = Arc::new(build_client(&gateway));
    let reference = format!("{}:{}", gateway.provider_id, gateway.primary_model);

    // Self-hosted pipeline: local statistical embeddings + in-memory store.
    let store: Arc<dyn ai_storage::VectorStore> =
        Arc::new(ai_storage::InMemoryVectorStore::new(1000));
    let embeddings: Arc<dyn ai_memory::EmbeddingsProvider> =
        Arc::new(ai_memory::StatisticalEmbeddings::defaults());
    let pipeline = ai_rag::RagPipeline::new(
        store,
        embeddings,
        ai_rag::RagConfig {
            chunking: ai_rag::ChunkingStrategy::Fixed {
                size: 400,
                overlap: 40,
            },
            min_similarity: 0.2,
            ..Default::default()
        },
    );

    let document = "The AI SDK gateway routes requests to deepseek-v4-flash, which is a fast \
                    reasoning model. The gateway base URL is opencode.ai/zen/go/v1 and it speaks \
                    the OpenAI-compatible protocol with DeepSeek-style reasoning content. \
                    Mimo-v2.5 is the vision model used for image inputs.";
    pipeline.ingest("gateway-doc", document).await.unwrap();

    let retrieved = pipeline
        .retrieve("which model handles vision inputs?", 2)
        .await
        .unwrap();
    assert!(!retrieved.is_empty(), "retrieval must return chunks");
    assert!(
        retrieved
            .iter()
            .any(|c| c.text.contains("mimo") || c.text.contains("vision")),
        "retrieved chunk must mention the vision model: {:?}",
        retrieved.iter().map(|c| &c.text).collect::<Vec<_>>()
    );

    // Ground the answer in the retrieved context using the primary model.
    let context = ai_rag::ContextAssembler::default().assemble(&retrieved);
    let prompt = format!(
        "Answer from the context only.\n\n{context}\n\nQuestion: which model handles vision inputs?"
    );
    let completion = client
        .generate(&reference, vec![Message::text(Role::User, &prompt)])
        .await
        .expect("generation succeeds");
    assert!(
        completion.text.to_lowercase().contains("mimo"),
        "answer must reference mimo: {:?}",
        completion.text
    );
    eprintln!(
        "PASS: self-hosted RAG ({} chunks, top score {:.3}) → {:?}",
        retrieved.len(),
        retrieved[0].score,
        completion.text.trim()
    );
}

#[tokio::test]
#[ignore = "live gateway suite: real HTTP calls; run with -- --ignored and AI_SDK_GATEWAY_* env set"]
async fn semantic_memory_self_hosted_live() {
    let gateway = gateway_from_env();
    if !gateway_available(&gateway) {
        eprintln!("SKIP: gateway env not set");
        return;
    }
    let gateway = gateway.unwrap();
    let _ = gateway; // semantic memory uses local embeddings only

    let embeddings: Arc<dyn ai_memory::EmbeddingsProvider> =
        Arc::new(ai_memory::StatisticalEmbeddings::defaults());
    let memory = ai_memory::SemanticMemory::new(
        embeddings,
        ai_memory::SemanticMemoryConfig {
            min_similarity: 0.3,
            ..Default::default()
        },
    );

    memory
        .store(ai_memory::SemanticFact {
            id: "fact-1".into(),
            text: "the primary model is deepseek-v4-flash".into(),
            metadata: serde_json::json!({"type": "model"}),
        })
        .await
        .unwrap();
    memory
        .store(ai_memory::SemanticFact {
            id: "fact-2".into(),
            text: "the vision model is mimo-v2.5".into(),
            metadata: serde_json::json!({"type": "model"}),
        })
        .await
        .unwrap();

    let results = memory
        .retrieve("which model is used for vision?", 2)
        .await
        .unwrap();
    assert!(!results.is_empty(), "retrieval must return facts");
    assert_eq!(
        results[0].0.id,
        "fact-2",
        "vision fact ranks first: {:?}",
        results
            .iter()
            .map(|(f, s)| (f.id.as_str(), s))
            .collect::<Vec<_>>()
    );
    eprintln!(
        "PASS: self-hosted semantic memory — top fact `{}` (score {:.3})",
        results[0].0.id, results[0].1
    );
}
