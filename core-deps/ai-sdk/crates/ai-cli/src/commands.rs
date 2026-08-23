//! Command implementations for the `ai-sdk` CLI. Every command does real
//! work: environment checks, real provider API calls, trace inspection.

use std::sync::Arc;

use ai_config::Config;
use ai_core::AiClient;
use ai_devtools::{Inspector, diff, load_trace_file as devtools_load_trace_file, verify};
use ai_errors::AiError;
use ai_observability::EventCollector;
use ai_providers::client_from_config;

/// Builds the client from the environment (`.env` + env vars).
pub fn build_client() -> Result<AiClient, AiError> {
    let config = Config::load()?;
    client_from_config(&config)
}

/// `ai-sdk doctor`: environment and provider health checks.
pub async fn doctor() -> Result<(), AiError> {
    let config = Config::load()?;
    println!("== ai-sdk doctor ==");
    println!("config: {}", config.redacted_summary());

    let mut ok = true;
    for (name, provider_config) in &config.providers {
        let key_status = if provider_config.api_key.is_some() {
            "key: set"
        } else {
            ok = false;
            "key: MISSING"
        };
        println!("provider {name}: {key_status}");
    }

    // Probe the gateway with a real model list call.
    match build_client() {
        Ok(client) => {
            for provider_id in client.provider_ids() {
                match client.provider(&provider_id) {
                    Ok(provider) => match provider.list_models().await {
                        Ok(models) => {
                            println!(
                                "provider {provider_id}: reachable ({} models)",
                                models.len()
                            );
                        }
                        Err(e) => {
                            ok = false;
                            println!("provider {provider_id}: UNREACHABLE: {e}");
                        }
                    },
                    Err(e) => {
                        ok = false;
                        println!("provider {provider_id}: error: {e}");
                    }
                }
            }
        }
        Err(e) => {
            ok = false;
            println!("client build failed: {e}");
        }
    }

    if ok {
        println!("doctor: OK");
    } else {
        println!("doctor: ISSUES FOUND (see above)");
    }
    Ok(())
}

/// `ai-sdk providers`: lists configured providers.
pub async fn providers() -> Result<(), AiError> {
    let config = Config::load()?;
    if config.providers.is_empty() {
        println!("no providers configured");
        return Ok(());
    }
    for (name, provider_config) in &config.providers {
        let model = provider_config
            .default_model
            .clone()
            .unwrap_or_else(|| "-".to_string());
        println!(
            "{name}\tmodel={model}\tbase_url={}",
            provider_config.base_url.as_deref().unwrap_or("-")
        );
    }
    Ok(())
}

/// `ai-sdk models`: real `GET /models` against a provider.
pub async fn models(provider_name: &str) -> Result<(), AiError> {
    let client = build_client()?;
    let provider = client.provider(provider_name)?;
    let models = provider.list_models().await?;
    println!("{} models from {provider_name}:", models.len());
    for model in models {
        println!("  {}", model.id);
    }
    Ok(())
}

/// `ai-sdk config`: validates and prints a redacted summary.
pub fn config() -> Result<(), AiError> {
    let config = Config::load()?;
    config.validate()?;
    println!("{}", config.redacted_summary());
    Ok(())
}

/// `ai-sdk run`: real generation against a model.
pub async fn run(model: &str, prompt: &str, stream: bool) -> Result<(), AiError> {
    let client = build_client()?;
    if stream {
        let events = client
            .stream(
                model,
                vec![ai_types::Message::text(ai_types::Role::User, prompt)],
            )
            .await?;
        let text = ai_stream::collect_text(events).await?;
        println!("{text}");
    } else {
        let completion = client
            .generate(
                model,
                vec![ai_types::Message::text(ai_types::Role::User, prompt)],
            )
            .await?;
        println!("{}", completion.text);
        if let Some(reasoning) = &completion.reasoning {
            println!("\n[reasoning: {} chars]", reasoning.len());
        }
        println!(
            "[tokens: in={} out={}]",
            completion.usage.input_tokens, completion.usage.output_tokens
        );
    }
    Ok(())
}

/// `ai-sdk inspect`: summarizes traces from a JSONL export.
pub fn inspect(file: &str) -> Result<(), AiError> {
    let collector = load_trace_file(file)?;
    let inspector = Inspector::new(collector);
    let traces = inspector.traces();
    println!("{} traces in {file}", traces.len());
    for trace in traces {
        println!(
            "  trace {}: {} events, {:?} ({:?})",
            trace.trace_id,
            trace.events.len(),
            std::time::Duration::from_millis(trace.duration_ms),
            trace.status
        );
    }
    Ok(())
}

/// `ai-sdk trace`: chronological report of one trace (redacted), or the
/// interactive time-travel TUI.
///
/// The TUI opens by default on an interactive terminal (`--tui` requests it
/// explicitly); a non-TTY session or `--no-tui` always uses the plain
/// rendering.
pub fn trace(
    file: Option<&str>,
    trace_id: Option<String>,
    tui: bool,
    no_tui: bool,
) -> Result<(), AiError> {
    let file = file.ok_or_else(|| {
        AiError::Internal(ai_errors::InternalError::new(
            "trace requires a <file> argument (or `diff`/`verify`)",
        ))
    })?;
    let collector = load_trace_file(file)?;
    let inspector = Inspector::new(collector);
    let traces = inspector.traces();

    if wants_tui(tui, no_tui) {
        let app = crate::tui::build_app(&inspector, &traces)?;
        return crate::tui::run(app);
    }

    let trace_id = match trace_id {
        Some(id) => id,
        None => traces
            .first()
            .map(|t| t.trace_id.clone())
            .ok_or_else(|| AiError::Internal(ai_errors::InternalError::new("no traces in file")))?,
    };
    println!("== trace {trace_id} ==");
    print!("{}", inspector.report(&trace_id));
    Ok(())
}

/// The TUI needs a real terminal on both ends. `--tui` requests it, but a
/// non-TTY session (CI, pipes) ALWAYS falls back to plain rendering — an
/// interactive loop would hang forever otherwise. `--no-tui` opts out even
/// on an interactive terminal.
fn wants_tui(tui: bool, no_tui: bool) -> bool {
    if no_tui {
        return false;
    }
    use std::io::IsTerminal as _;
    let interactive = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    // On an interactive terminal the TUI is the default and `--tui` opts in
    // explicitly (same outcome); anywhere else plain rendering wins so CI
    // and pipes never hang on an input loop.
    let _ = tui;
    interactive
}

/// `ai-sdk trace diff <a> <b>`: structural diff of two recordings.
pub fn trace_diff(a_file: &str, b_file: &str, json: bool) -> Result<(), AiError> {
    let baseline = load_trace_file(a_file)?;
    let compared = load_trace_file(b_file)?;
    let report = diff::diff_recordings(&baseline.events(), &compared.events());

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| {
                AiError::Internal(ai_errors::InternalError::new(format!(
                    "cannot serialize diff: {e}"
                )))
            })?
        );
    } else {
        print!("{}", diff::format_diff_table(&report));
    }
    // A differing recording is not a command failure; exit stays 0 and the
    // caller reads the summary.
    Ok(())
}

/// `ai-sdk trace verify <file>`: validates recording invariants; exits 1
/// listing every violation found.
pub fn trace_verify(file: &str) -> Result<(), AiError> {
    let collector = load_trace_file(file)?;
    let events = collector.events();
    let violations = verify::verify_events(&events);

    if violations.is_empty() {
        let mut traces = std::collections::BTreeSet::new();
        for event in &events {
            traces.insert(event.trace_id.as_str());
        }
        println!(
            "OK: {} events across {} {} satisfy all invariants",
            events.len(),
            traces.len(),
            if traces.len() == 1 { "trace" } else { "traces" }
        );
        return Ok(());
    }

    println!(
        "FAILED: {} violation{} in {file}:",
        violations.len(),
        if violations.len() == 1 { "" } else { "s" }
    );
    print!("{}", verify::format_violations(&violations));
    Err(AiError::Internal(ai_errors::InternalError::new(format!(
        "{} invariant violation(s); see listed details above",
        violations.len()
    ))))
}

/// `ai-sdk benchmark`: real parallel generation latency/throughput.
pub async fn benchmark(model: &str, requests: usize, concurrency: usize) -> Result<(), AiError> {
    let client = Arc::new(build_client()?);
    let tasks: Vec<ai_runtime::Task<ai_types::Completion>> = (0..requests)
        .map(|index| {
            let client = client.clone();
            let model = model.to_string();
            ai_runtime::Task::new(format!("bench-{index}"), async move {
                client
                    .generate(
                        &model,
                        vec![ai_types::Message::text(
                            ai_types::Role::User,
                            "Reply with exactly: OK",
                        )],
                    )
                    .await
            })
        })
        .collect();

    let started = std::time::Instant::now();
    let results = ai_runtime::Parallel::new()
        .with_limit(concurrency)
        .execute(tasks)
        .await;
    let elapsed = started.elapsed();

    let succeeded = results.iter().filter(|r| r.succeeded()).count();
    let total_tokens: u64 = results
        .iter()
        .filter_map(|r| r.outcome.as_ref().ok())
        .map(|c| c.usage.total())
        .sum();

    println!(
        "benchmark: {succeeded}/{} succeeded in {:.2}s (concurrency {concurrency})",
        results.len(),
        elapsed.as_secs_f64()
    );
    println!(
        "  throughput: {:.1} req/s, latency p50 estimate {:.0} ms",
        succeeded as f64 / elapsed.as_secs_f64().max(0.001),
        elapsed.as_millis() as f64 / succeeded.max(1) as f64
    );
    println!("  total tokens: {total_tokens}");
    Ok(())
}

/// `ai-sdk batch`: manages OpenAI batch jobs.
pub async fn batch(action: &crate::BatchAction) -> Result<(), AiError> {
    let config = Config::load()?;
    let key = config
        .providers
        .get("openai")
        .and_then(|p| p.api_key.as_deref())
        .unwrap_or("");

    if key.is_empty() {
        return Err(AiError::Configuration(ai_errors::ConfigurationError::new(
            "api_key",
            "OPENAI_API_KEY is required for batch commands",
        )));
    }

    let base_url = config
        .providers
        .get("openai")
        .and_then(|p| p.base_url.as_deref())
        .unwrap_or("https://api.openai.com/v1");

    let client = ai_providers::batch::OpenAiBatchClient::new(key, base_url)?;

    match action {
        crate::BatchAction::Create {
            input_file_id,
            endpoint,
            window,
        } => {
            let b = client.create_batch(input_file_id, endpoint, window).await?;
            println!("batch created: id={} status={}", b.id, b.status);
        }
        crate::BatchAction::Status { id } => {
            let b = client.retrieve_batch(id).await?;
            println!(
                "batch id={} status={} input_file_id={} output_file_id={:?}",
                b.id, b.status, b.input_file_id, b.output_file_id
            );
        }
        crate::BatchAction::Cancel { id } => {
            let b = client.cancel_batch(id).await?;
            println!("batch cancelling: id={} status={}", b.id, b.status);
        }
        crate::BatchAction::List { limit } => {
            let res = client.list_batches(*limit, None).await?;
            println!(
                "batches: {} total (has_more={})",
                res.data.len(),
                res.has_more
            );
            for b in res.data {
                println!("  {} [{}] status={}", b.id, b.endpoint, b.status);
            }
        }
    }
    Ok(())
}

/// `ai-sdk finetune`: manages OpenAI fine-tuning jobs.
pub async fn finetune(action: &crate::FinetuneAction) -> Result<(), AiError> {
    let config = Config::load()?;
    let key = config
        .providers
        .get("openai")
        .and_then(|p| p.api_key.as_deref())
        .unwrap_or("");

    if key.is_empty() {
        return Err(AiError::Configuration(ai_errors::ConfigurationError::new(
            "api_key",
            "OPENAI_API_KEY is required for finetune commands",
        )));
    }

    let base_url = config
        .providers
        .get("openai")
        .and_then(|p| p.base_url.as_deref())
        .unwrap_or("https://api.openai.com/v1");

    let client = ai_providers::finetune::FineTuningClient::new(key, base_url)?;

    match action {
        crate::FinetuneAction::Create {
            training_file_id,
            model,
        } => {
            let job = client
                .create_job(model, training_file_id, Default::default())
                .await?;
            println!(
                "fine-tuning job created: id={} status={}",
                job.id, job.status
            );
        }
        crate::FinetuneAction::Status { id } => {
            let job = client.get_job(id).await?;
            println!(
                "fine-tuning job id={} status={} model={} trained_tokens={:?}",
                job.id, job.status, job.model, job.trained_tokens
            );
        }
        crate::FinetuneAction::Cancel { id } => {
            let job = client.cancel_job(id).await?;
            println!(
                "fine-tuning job cancelling: id={} status={}",
                job.id, job.status
            );
        }
        crate::FinetuneAction::List { limit } => {
            let jobs = client.list_jobs(limit.map(|l| l as u64)).await?;
            println!("fine-tuning jobs: {} total", jobs.len());
            for j in jobs {
                println!("  {} [{}] status={}", j.id, j.model, j.status);
            }
        }
    }
    Ok(())
}

/// `ai-sdk assistant`: manages OpenAI Assistants v2.
pub async fn assistant(action: &crate::AssistantAction) -> Result<(), AiError> {
    let config = Config::load()?;
    let key = config
        .providers
        .get("openai")
        .and_then(|p| p.api_key.as_deref())
        .unwrap_or("");

    if key.is_empty() {
        return Err(AiError::Configuration(ai_errors::ConfigurationError::new(
            "api_key",
            "OPENAI_API_KEY is required for assistant commands",
        )));
    }

    let base_url = config
        .providers
        .get("openai")
        .and_then(|p| p.base_url.as_deref())
        .unwrap_or("https://api.openai.com/v1");

    let client = ai_providers::assistants::AssistantsClient::new(key, base_url)?;

    match action {
        crate::AssistantAction::Create {
            model,
            name,
            instructions,
        } => {
            let asst = client
                .create_assistant(model, name.as_deref(), instructions.as_deref())
                .await?;
            println!("assistant created: id={} model={}", asst.id, asst.model);
        }
        crate::AssistantAction::List { limit } => {
            let assistants = client.list_assistants(*limit).await?;
            println!("assistants: {} total", assistants.len());
            for a in assistants {
                println!("  {} [{}] name={:?}", a.id, a.model, a.name);
            }
        }
        crate::AssistantAction::Run {
            assistant_id,
            prompt,
        } => {
            let thread = client.create_thread().await?;
            client.create_message(&thread.id, "user", prompt).await?;
            let run = client.create_run(&thread.id, assistant_id).await?;
            println!(
                "run started: id={} thread_id={} status={}",
                run.id, run.thread_id, run.status
            );
        }
    }
    Ok(())
}

/// `ai-sdk image`: generates images using DALL-E.
pub async fn image(prompt: &str, model: &str) -> Result<(), AiError> {
    let config = Config::load()?;
    let key = config
        .providers
        .get("openai")
        .and_then(|p| p.api_key.as_deref())
        .unwrap_or("");
    if key.is_empty() {
        return Err(AiError::Configuration(ai_errors::ConfigurationError::new(
            "api_key",
            "OPENAI_API_KEY is required for image commands",
        )));
    }
    let base_url = config
        .providers
        .get("openai")
        .and_then(|p| p.base_url.as_deref())
        .unwrap_or("https://api.openai.com/v1");
    let client = ai_providers::images::OpenAiImageClient::new(key, base_url)?;
    let res = client
        .generate_image(prompt, Some(model), Some(1), None, None)
        .await?;
    println!("image generated ({} items):", res.data.len());
    for item in res.data {
        if let Some(u) = item.url {
            println!("  url: {u}");
        }
    }
    Ok(())
}

/// `ai-sdk moderate`: moderates text against content policies.
pub async fn moderate(input: &str) -> Result<(), AiError> {
    let config = Config::load()?;
    let key = config
        .providers
        .get("openai")
        .and_then(|p| p.api_key.as_deref())
        .unwrap_or("");
    if key.is_empty() {
        return Err(AiError::Configuration(ai_errors::ConfigurationError::new(
            "api_key",
            "OPENAI_API_KEY is required for moderate commands",
        )));
    }
    let base_url = config
        .providers
        .get("openai")
        .and_then(|p| p.base_url.as_deref())
        .unwrap_or("https://api.openai.com/v1");
    let client = ai_providers::moderation::OpenAiModerationClient::new(key, base_url)?;
    let res = client.create_moderation(input, None).await?;
    for (i, r) in res.results.iter().enumerate() {
        println!("result #{i}: flagged={}", r.flagged);
    }
    Ok(())
}

/// `ai-sdk files`: lists uploaded files.
pub async fn files(purpose: Option<&str>) -> Result<(), AiError> {
    let config = Config::load()?;
    let key = config
        .providers
        .get("openai")
        .and_then(|p| p.api_key.as_deref())
        .unwrap_or("");
    if key.is_empty() {
        return Err(AiError::Configuration(ai_errors::ConfigurationError::new(
            "api_key",
            "OPENAI_API_KEY is required for files commands",
        )));
    }
    let base_url = config
        .providers
        .get("openai")
        .and_then(|p| p.base_url.as_deref())
        .unwrap_or("https://api.openai.com/v1");
    let client = ai_providers::files::OpenAiFilesClient::new(key, base_url)?;
    let files = client.list_files(purpose).await?;
    println!("files: {} total", files.len());
    for f in files {
        println!(
            "  {} [{}] {} ({} bytes)",
            f.id, f.purpose, f.filename, f.bytes
        );
    }
    Ok(())
}

/// `ai-sdk vector-store`: manages cloud vector stores.
pub async fn vector_store(action: &crate::VectorStoreAction) -> Result<(), AiError> {
    let config = Config::load()?;
    let key = config
        .providers
        .get("openai")
        .and_then(|p| p.api_key.as_deref())
        .unwrap_or("");
    if key.is_empty() {
        return Err(AiError::Configuration(ai_errors::ConfigurationError::new(
            "api_key",
            "OPENAI_API_KEY is required for vector-store commands",
        )));
    }
    let base_url = config
        .providers
        .get("openai")
        .and_then(|p| p.base_url.as_deref())
        .unwrap_or("https://api.openai.com/v1");
    let client = ai_providers::vector_stores::OpenAiVectorStoresClient::new(key, base_url)?;
    match action {
        crate::VectorStoreAction::Create { name } => {
            let vs = client.create_vector_store(name.as_deref(), vec![]).await?;
            println!(
                "vector store created: id={} name={:?} status={}",
                vs.id, vs.name, vs.status
            );
        }
        crate::VectorStoreAction::Delete { id } => {
            let deleted = client.delete_vector_store(id).await?;
            println!("vector store deleted: id={id} deleted={deleted}");
        }
    }
    Ok(())
}

/// Loads a JSONL event export into a collector **losslessly** via
/// `ai-devtools`: `wall_time`, `offset_ms`, ids, and order survive exactly
/// as persisted (no re-based fabricated chronology).
fn load_trace_file(file: &str) -> Result<EventCollector, AiError> {
    devtools_load_trace_file(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_trace_file_parses_jsonl() {
        let dir = std::env::temp_dir();
        let path = dir.join("ai-sdk-cli-test.jsonl");
        std::fs::write(
            &path,
            "{\"wall_time\":\"x\",\"offset_ms\":0,\"trace_id\":\"t1\",\"span_id\":\"s1\",\"kind\":\"model_call\",\"operation\":\"op\",\"status\":\"succeeded\"}\n",
        )
        .unwrap();
        let collector = load_trace_file(path.to_str().unwrap()).unwrap();
        assert_eq!(collector.events().len(), 1);
        std::fs::remove_file(&path).ok();
    }

    // ---- fixtures ------------------------------------------------------

    /// Two traces in one recording: a multi-span tree plus one standalone
    /// metric span. Offsets/wall clocks are deliberately irregular.
    fn two_trace_fixture() -> String {
        [
            r#"{"wall_time":"2025-03-04T05:06:07.123456789Z","offset_ms":0,"trace_id":"tr-alpha","span_id":"root","kind":"request_started","operation":"request","status":"started"}"#,
            r#"{"wall_time":"2025-03-04T05:06:07.5Z","offset_ms":37,"trace_id":"tr-alpha","span_id":"child","parent_span_id":"root","kind":"agent_step","operation":"think","status":"succeeded","duration_ms":30}"#,
            r#"{"wall_time":"2025-03-04T05:06:08Z","offset_ms":70,"trace_id":"tr-alpha","span_id":"root","kind":"completed","operation":"request","status":"succeeded","duration_ms":70}"#,
            r#"{"wall_time":"1999-12-31T23:59:59Z","offset_ms":4242,"trace_id":"tr-beta","span_id":"m","kind":"metric","operation":"tokens","status":"succeeded"}"#,
        ]
        .join("\n")
    }

    fn write_fixture(name: &str, content: &str) -> String {
        let path = std::env::temp_dir().join(format!("{name}-{}.jsonl", std::process::id()));
        std::fs::write(&path, content).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn loader_preserves_wall_time_and_offsets_losslessly() {
        let file = write_fixture("ai-sdk-cli-lossless", &two_trace_fixture());
        let collector = load_trace_file(&file).unwrap();
        std::fs::remove_file(&file).ok();

        let events = collector.events();
        assert_eq!(events.len(), 4);
        // The old re-recording path fabricated fresh wall clocks; the
        // lossless path must keep every persisted value verbatim.
        assert_eq!(events[0].wall_time, "2025-03-04T05:06:07.123456789Z");
        assert_eq!(events[3].wall_time, "1999-12-31T23:59:59Z");
        let offsets: Vec<u64> = events.iter().map(|e| e.offset_ms).collect();
        assert_eq!(offsets, vec![0, 37, 70, 4242]);
        assert_eq!(
            events[1].parent_span_id.as_deref(),
            Some("root"),
            "span tree survives"
        );

        // Inspector still sees ONE multi-event trace per id.
        let inspector = Inspector::new(collector);
        let traces = inspector.traces();
        assert_eq!(traces.len(), 2);
        assert_eq!(traces[0].events.len(), 3);
        assert_eq!(traces[0].duration_ms, 70);
    }

    #[test]
    fn trace_diff_reports_structural_changes() {
        let a = write_fixture("ai-sdk-cli-diff-a", &two_trace_fixture());
        let mut b_lines = two_trace_fixture();
        // Same trace ids, but the child span's duration jumps 30 -> 60 ms.
        b_lines = b_lines.replace("\"duration_ms\":30", "\"duration_ms\":60");
        let b = write_fixture("ai-sdk-cli-diff-b", &b_lines);

        // Pure diff over loaded events (same entry point the CLI uses).
        let baseline = load_trace_file(&a).unwrap().events();
        let compared = load_trace_file(&b).unwrap().events();
        std::fs::remove_file(&a).ok();
        std::fs::remove_file(&b).ok();

        let report = ai_devtools::diff::diff_recordings(&baseline, &compared);
        assert_eq!(report.traces_compared, 2);
        assert_eq!(report.duration_deltas.len(), 1);
        assert_eq!(report.duration_deltas[0].baseline_ms, 30);
        assert_eq!(report.duration_deltas[0].compared_ms, 60);
        assert!((report.duration_deltas[0].percent - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn trace_verify_passes_the_valid_fixture() {
        let file = write_fixture("ai-sdk-cli-verify-ok", &two_trace_fixture());
        let events = load_trace_file(&file).unwrap().events();
        std::fs::remove_file(&file).ok();
        let violations = ai_devtools::verify::verify_events(&events);
        assert!(violations.is_empty(), "{violations:?}");
    }
}
