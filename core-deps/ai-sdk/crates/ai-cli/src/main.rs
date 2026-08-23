//! The `ai-sdk` CLI binary.

use ai_cli::{Cli, Commands, TraceAction};
use clap::Parser;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Doctor => ai_cli::commands::doctor().await,
        Commands::Providers => ai_cli::commands::providers().await,
        Commands::Models { provider } => ai_cli::commands::models(&provider).await,
        Commands::Config => ai_cli::commands::config(),
        Commands::Run {
            model,
            prompt,
            stream,
        } => ai_cli::commands::run(&model, &prompt, stream).await,
        Commands::Inspect { file } => ai_cli::commands::inspect(&file),
        Commands::Trace {
            file,
            trace_id,
            tui,
            no_tui,
            action,
        } => match action {
            Some(TraceAction::Diff { a, b, json }) => ai_cli::commands::trace_diff(&a, &b, json),
            Some(TraceAction::Verify { file }) => ai_cli::commands::trace_verify(&file),
            None => ai_cli::commands::trace(file.as_deref(), trace_id, tui, no_tui),
        },
        Commands::Benchmark {
            model,
            requests,
            concurrency,
        } => ai_cli::commands::benchmark(&model, requests, concurrency).await,
        Commands::Batch { action } => ai_cli::commands::batch(&action).await,
        Commands::Finetune { action } => ai_cli::commands::finetune(&action).await,
        Commands::Assistant { action } => ai_cli::commands::assistant(&action).await,
        Commands::Image { prompt, model } => ai_cli::commands::image(&prompt, &model).await,
        Commands::Moderate { input } => ai_cli::commands::moderate(&input).await,
        Commands::Files { purpose } => ai_cli::commands::files(purpose.as_deref()).await,
        Commands::VectorStore { action } => ai_cli::commands::vector_store(&action).await,
    };

    match result {
        Ok(()) => Ok(()),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
