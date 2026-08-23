//! The `ai-sdk` command-line interface (spec §29): every command performs
//! real work against the configured providers.

pub mod commands;
pub mod tui;

use clap::{Parser, Subcommand};

/// The AI SDK CLI.
#[derive(Parser)]
#[command(name = "ai-sdk", version, about = "AI SDK command-line interface")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Checks the environment and provider configuration.
    Doctor,
    /// Lists configured providers.
    Providers,
    /// Lists models available from a provider (real API call).
    Models {
        /// Provider id (defaults to the gateway provider).
        #[arg(long, default_value = "opencode")]
        provider: String,
    },
    /// Shows and validates the configuration (redacted).
    Config,
    /// Runs a prompt against a model and prints the completion.
    Run {
        /// Model reference, e.g. `opencode:deepseek-v4-flash`.
        #[arg(long, default_value = "opencode:deepseek-v4-flash")]
        model: String,
        /// The user prompt.
        prompt: String,
        /// Stream the response.
        #[arg(long)]
        stream: bool,
    },
    /// Inspects execution traces from a JSONL export.
    Inspect {
        /// Path to a `.jsonl` trace export.
        file: String,
    },
    /// Prints a chronological report of a trace, opens the time-travel
    /// TUI (`--tui`), or diffs/validates recordings
    /// (`diff <a> <b>` / `verify <file>`).
    Trace {
        /// Path to a `.jsonl` trace export.
        file: Option<String>,
        /// Trace id to report (defaults to the first trace).
        #[arg(long)]
        trace_id: Option<String>,
        /// Launch the interactive time-travel TUI.
        #[arg(long)]
        tui: bool,
        /// Force plain rendering, even on an interactive terminal.
        #[arg(long)]
        no_tui: bool,
        /// Optional subcommand (`diff`, `verify`) instead of reporting.
        #[command(subcommand)]
        action: Option<TraceAction>,
    },
    /// Benchmarks generation latency/throughput against a model.
    Benchmark {
        /// Model reference.
        #[arg(long, default_value = "opencode:deepseek-v4-flash")]
        model: String,
        /// Number of parallel requests.
        #[arg(long, default_value = "8")]
        requests: usize,
        /// Concurrency limit.
        #[arg(long, default_value = "4")]
        concurrency: usize,
    },
    /// Manages OpenAI batch jobs.
    Batch {
        #[command(subcommand)]
        action: BatchAction,
    },
    /// Manages OpenAI fine-tuning jobs.
    Finetune {
        #[command(subcommand)]
        action: FinetuneAction,
    },
    /// Manages OpenAI Assistants v2.
    Assistant {
        #[command(subcommand)]
        action: AssistantAction,
    },
    /// Generates images using DALL-E.
    Image {
        /// Image generation prompt.
        prompt: String,
        /// Model reference (default: dall-e-3).
        #[arg(long, default_value = "dall-e-3")]
        model: String,
    },
    /// Moderates text against content policies.
    Moderate {
        /// Text input to moderate.
        input: String,
    },
    /// Lists uploaded files.
    Files {
        #[arg(long)]
        purpose: Option<String>,
    },
    /// Manages cloud vector stores.
    VectorStore {
        #[command(subcommand)]
        action: VectorStoreAction,
    },
}

/// Subcommands of `ai-sdk trace`.
#[derive(Subcommand)]
pub enum TraceAction {
    /// Structurally diffs two recordings (`ai trace diff <a> <b>`).
    Diff {
        /// Baseline recording (JSONL).
        a: String,
        /// Compared recording (JSONL).
        b: String,
        /// Emit machine-readable JSON instead of a human table.
        #[arg(long)]
        json: bool,
    },
    /// Validates recording invariants (`ai trace verify <file>`); exits 1
    /// listing violations when any invariant is broken.
    Verify {
        /// Recording to validate (JSONL).
        file: String,
    },
}

#[derive(Subcommand)]
pub enum VectorStoreAction {
    /// Creates a vector store.
    Create {
        /// Store name.
        #[arg(long)]
        name: Option<String>,
    },
    /// Deletes a vector store by ID.
    Delete {
        /// Store ID.
        id: String,
    },
}

#[derive(Subcommand)]
pub enum AssistantAction {
    /// Creates an assistant.
    Create {
        /// Base model.
        #[arg(long, default_value = "gpt-4o")]
        model: String,
        /// Assistant name.
        #[arg(long)]
        name: Option<String>,
        /// System instructions.
        #[arg(long)]
        instructions: Option<String>,
    },
    /// Lists assistants.
    List {
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Runs an assistant on a new thread with a user prompt.
    Run {
        /// Assistant ID.
        #[arg(long)]
        assistant_id: String,
        /// User prompt message.
        prompt: String,
    },
}

#[derive(Subcommand)]
pub enum BatchAction {
    /// Creates a batch job.
    Create {
        /// Input JSONL file ID.
        #[arg(long)]
        input_file_id: String,
        /// Endpoint (default: /v1/chat/completions).
        #[arg(long, default_value = "/v1/chat/completions")]
        endpoint: String,
        /// Completion window (default: 24h).
        #[arg(long, default_value = "24h")]
        window: String,
    },
    /// Retrieves a batch job by ID.
    Status {
        /// Batch job ID.
        id: String,
    },
    /// Cancels a batch job by ID.
    Cancel {
        /// Batch job ID.
        id: String,
    },
    /// Lists batch jobs.
    List {
        #[arg(long)]
        limit: Option<u32>,
    },
}

#[derive(Subcommand)]
pub enum FinetuneAction {
    /// Creates a fine-tuning job.
    Create {
        /// Training file ID.
        #[arg(long)]
        training_file_id: String,
        /// Base model.
        #[arg(long, default_value = "gpt-4o-mini")]
        model: String,
    },
    /// Retrieves a fine-tuning job by ID.
    Status {
        /// Fine-tuning job ID.
        id: String,
    },
    /// Cancels a fine-tuning job by ID.
    Cancel {
        /// Fine-tuning job ID.
        id: String,
    },
    /// Lists fine-tuning jobs.
    List {
        #[arg(long)]
        limit: Option<u32>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_parses_subcommands() {
        let cli = Cli::try_parse_from(["ai-sdk", "doctor"]).unwrap();
        assert!(matches!(cli.command, Commands::Doctor));

        let cli = Cli::try_parse_from(["ai-sdk", "run", "--model", "x:y", "hello"]).unwrap();
        match cli.command {
            Commands::Run {
                model,
                prompt,
                stream,
            } => {
                assert_eq!(model, "x:y");
                assert_eq!(prompt, "hello");
                assert!(!stream);
            }
            _ => panic!("expected run"),
        }
    }

    #[test]
    fn trace_parses_plain_file_and_flags() {
        let cli = Cli::try_parse_from(["ai-sdk", "trace", "run.jsonl"]).unwrap();
        match cli.command {
            Commands::Trace {
                file,
                trace_id,
                tui,
                no_tui,
                action,
            } => {
                assert_eq!(file.as_deref(), Some("run.jsonl"));
                assert_eq!(trace_id, None);
                assert!(!tui);
                assert!(!no_tui);
                assert!(action.is_none());
            }
            _ => panic!("expected trace"),
        }

        let cli = Cli::try_parse_from([
            "ai-sdk",
            "trace",
            "run.jsonl",
            "--trace-id",
            "t9",
            "--no-tui",
        ])
        .unwrap();
        match cli.command {
            Commands::Trace {
                file,
                trace_id,
                no_tui,
                ..
            } => {
                assert_eq!(file.as_deref(), Some("run.jsonl"));
                assert_eq!(trace_id.as_deref(), Some("t9"));
                assert!(no_tui);
            }
            _ => panic!("expected trace"),
        }
    }

    #[test]
    fn trace_diff_and_verify_subcommands_parse() {
        let cli = Cli::try_parse_from(["ai-sdk", "trace", "diff", "a.jsonl", "b.jsonl", "--json"])
            .unwrap();
        match cli.command {
            Commands::Trace {
                action: Some(TraceAction::Diff { a, b, json }),
                ..
            } => {
                assert_eq!(a, "a.jsonl");
                assert_eq!(b, "b.jsonl");
                assert!(json);
            }
            _ => panic!("expected trace diff"),
        }

        let cli = Cli::try_parse_from(["ai-sdk", "trace", "verify", "rec.jsonl"]).unwrap();
        match cli.command {
            Commands::Trace {
                action: Some(TraceAction::Verify { file }),
                ..
            } => assert_eq!(file, "rec.jsonl"),
            _ => panic!("expected trace verify"),
        }
    }

    #[test]
    fn trace_tui_flag_parses() {
        let cli = Cli::try_parse_from(["ai-sdk", "trace", "--tui", "run.jsonl"]).unwrap();
        match cli.command {
            Commands::Trace {
                file, tui, action, ..
            } => {
                assert_eq!(file.as_deref(), Some("run.jsonl"));
                assert!(tui);
                assert!(action.is_none());
            }
            _ => panic!("expected trace --tui"),
        }
    }

    #[test]
    fn trace_without_file_or_action_parses_and_is_rejected_at_runtime() {
        // Bare `ai trace` parses (everything is optional) but the command
        // handler turns it into a friendly argument error.
        let cli = Cli::try_parse_from(["ai-sdk", "trace"]).unwrap();
        match cli.command {
            Commands::Trace { file, action, .. } => {
                assert!(file.is_none());
                assert!(action.is_none());
            }
            _ => panic!("expected trace"),
        }
    }
}
