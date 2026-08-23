//! Unified configuration for the AI SDK.
//!
//! Supports three sources, merged with programmatic highest precedence:
//!
//! 1. Environment variables (`OPENAI_API_KEY`, `AI_SDK_CONFIG`, …)
//! 2. TOML configuration files (`$AI_SDK_CONFIG` or `./.ai-sdk.toml`)
//! 3. Programmatic construction / overrides
//!
//! Secrets are never printed by this crate; [`Config::redacted_summary`]
//! masks API keys.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use ai_errors::{AiError, ConfigurationError};

/// The standard config file path candidates, in order of precedence.
pub const CONFIG_FILE_CANDIDATES: &[&str] = &[".ai-sdk.toml", "ai-sdk.toml"];

/// Well-known environment variable names.
pub mod env_keys {
    pub const OPENAI_API_KEY: &str = "OPENAI_API_KEY";
    pub const ANTHROPIC_API_KEY: &str = "ANTHROPIC_API_KEY";
    pub const GOOGLE_API_KEY: &str = "GOOGLE_API_KEY";
    pub const OPENROUTER_API_KEY: &str = "OPENROUTER_API_KEY";
    pub const OLLAMA_BASE_URL: &str = "OLLAMA_BASE_URL";
    pub const GATEWAY_BASE_URL: &str = "AI_SDK_GATEWAY_BASE_URL";
    pub const GATEWAY_API_KEY: &str = "AI_SDK_GATEWAY_API_KEY";
    /// Which configured provider to use when none is specified
    /// (e.g. `opencode`, `openai`, `anthropic`).
    pub const DEFAULT_PROVIDER: &str = "AI_SDK_PROVIDER";
    /// Primary chat model id (also stored as the gateway provider's
    /// `default_model` when the gateway is configured).
    pub const PRIMARY_MODEL: &str = "AI_SDK_PRIMARY_MODEL";
    /// Vision-capable model id (used by live tests/examples).
    pub const VISION_MODEL: &str = "AI_SDK_VISION_MODEL";
    /// Context window (tokens) of the primary model — surfaced so
    /// applications and agents can budget prompts without hardcoding.
    pub const PRIMARY_MODEL_CONTEXT_LENGTH: &str = "AI_SDK_PRIMARY_MODEL_CONTEXT_LENGTH";
    pub const CONFIG_FILE: &str = "AI_SDK_CONFIG";
    pub const DEFAULT_TIMEOUT: &str = "AI_SDK_DEFAULT_TIMEOUT";
    pub const DEFAULT_MAX_RETRIES: &str = "AI_SDK_MAX_RETRIES";
}

/// Default timeout for provider calls (30 s).
pub const DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
/// Default retry count for transient failures.
pub const DEFAULT_MAX_RETRIES: u32 = 2;

/// Per-provider configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderConfig {
    /// API key; usually sourced from the environment. Serialized as
    /// `api_key` but redacted on display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Base URL override (e.g. a gateway or self-hosted endpoint).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Default model to use for this provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
}

impl ProviderConfig {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: Some(api_key.into()),
            base_url: None,
            default_model: None,
        }
    }

    /// Requires an API key, returning a typed configuration error otherwise.
    pub fn require_api_key(&self, provider: &str) -> Result<&str, AiError> {
        self.api_key
            .as_deref()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| {
                AiError::Configuration(ConfigurationError::new(
                    provider.to_uppercase().replace('-', "_") + "_API_KEY",
                    format!(
                        "missing API key for provider `{provider}`; set the environment \
                         variable or provide it in the config file"
                    ),
                ))
            })
    }
}

/// Global defaults for calls.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CallDefaults {
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

fn default_timeout_secs() -> u64 {
    DEFAULT_TIMEOUT.as_secs()
}

fn default_max_retries() -> u32 {
    DEFAULT_MAX_RETRIES
}

impl Default for CallDefaults {
    fn default() -> Self {
        Self {
            timeout_secs: default_timeout_secs(),
            max_retries: default_max_retries(),
        }
    }
}

/// File-based configuration schema (`ai-sdk.toml`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FileConfig {
    #[serde(default)]
    pub defaults: Option<CallDefaults>,
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
}

/// The unified configuration for the SDK.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Config {
    pub providers: HashMap<String, ProviderConfig>,
    pub defaults: CallDefaults,
    /// Provider used when a model reference has no provider prefix
    /// (`AI_SDK_PROVIDER`).
    pub default_provider: Option<String>,
    /// Primary chat model id (`AI_SDK_PRIMARY_MODEL`).
    pub primary_model: Option<String>,
    /// Vision-capable model id (`AI_SDK_VISION_MODEL`).
    pub vision_model: Option<String>,
    /// Context window in tokens for the primary model
    /// (`AI_SDK_PRIMARY_MODEL_CONTEXT_LENGTH`).
    pub primary_model_context_length: Option<u64>,
}

impl Config {
    /// Loads configuration from the environment, then merges any TOML file
    /// found at `$AI_SDK_CONFIG` or the standard candidates.
    ///
    /// Environment variables always win over file values (file is loaded
    /// first, then env overrides).
    pub fn load() -> Result<Self, AiError> {
        let mut config = Self::default();

        // 1. File (lowest precedence).
        let file_path = std::env::var(env_keys::CONFIG_FILE).ok();
        let mut file_config = None;
        if let Some(path) = &file_path {
            file_config = Some(Self::load_file(Path::new(path))?);
        } else if let Some(found) = CONFIG_FILE_CANDIDATES
            .iter()
            .find(|candidate| Path::new(candidate).exists())
        {
            file_config = Some(Self::load_file(Path::new(found))?);
        }
        if let Some(fc) = file_config {
            config.merge_file(fc);
        }

        // 2. Environment (highest precedence).
        config.merge_env();

        Ok(config)
    }

    /// Loads a TOML config file.
    pub fn load_file(path: &Path) -> Result<FileConfig, AiError> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            AiError::Configuration(ConfigurationError::with_source(
                "AI_SDK_CONFIG",
                format!("failed to read config file `{}`", path.display()),
                e,
            ))
        })?;
        let fc: FileConfig = toml::from_str(&content).map_err(|e| {
            AiError::Configuration(ConfigurationError::with_source(
                "AI_SDK_CONFIG",
                format!("failed to parse config file `{}`", path.display()),
                e,
            ))
        })?;
        Ok(fc)
    }

    /// Merges file values into this config (does not override existing keys).
    pub fn merge_file(&mut self, file: FileConfig) {
        if let Some(defaults) = file.defaults {
            self.defaults = defaults;
        }
        for (name, pc) in file.providers {
            self.providers.entry(name).or_insert(pc);
        }
    }

    /// Merges environment variables into this config (overrides everything).
    pub fn merge_env(&mut self) {
        set_env(&mut self.providers, "openai", env_keys::OPENAI_API_KEY);
        set_env(
            &mut self.providers,
            "anthropic",
            env_keys::ANTHROPIC_API_KEY,
        );
        set_env(&mut self.providers, "google", env_keys::GOOGLE_API_KEY);
        set_env(
            &mut self.providers,
            "openrouter",
            env_keys::OPENROUTER_API_KEY,
        );
        if let Ok(base) = std::env::var(env_keys::OLLAMA_BASE_URL) {
            let entry = self.providers.entry("ollama".to_string()).or_default();
            entry.base_url = Some(base);
        }
        // OpenAI-compatible gateway (project default, e.g.
        // `opencode.ai/zen/go/v1`): registered under the `opencode` id.
        let gateway_base = std::env::var(env_keys::GATEWAY_BASE_URL).ok();
        let gateway_key = std::env::var(env_keys::GATEWAY_API_KEY).ok();
        if gateway_base.is_some() || gateway_key.is_some() {
            let entry = self.providers.entry("opencode".to_string()).or_default();
            if let Some(base) = gateway_base {
                entry.base_url = Some(base);
            }
            if let Some(key) = gateway_key {
                if !key.trim().is_empty() {
                    entry.api_key = Some(key);
                }
            }
        }
        if let Some(secs) = std::env::var(env_keys::DEFAULT_TIMEOUT)
            .ok()
            .and_then(|v| v.parse().ok())
        {
            self.defaults.timeout_secs = secs;
        }
        if let Some(retries) = std::env::var(env_keys::DEFAULT_MAX_RETRIES)
            .ok()
            .and_then(|v| v.parse().ok())
        {
            self.defaults.max_retries = retries;
        }
        // Model/provider selection.
        if let Some(p) = non_empty_env(env_keys::DEFAULT_PROVIDER) {
            self.default_provider = Some(p);
        }
        if let Some(m) = non_empty_env(env_keys::PRIMARY_MODEL) {
            // Also record it as the gateway provider's default model so
            // gateway users get model selection from one variable.
            if let Some(entry) = self.providers.get_mut("opencode") {
                entry.default_model = Some(m.clone());
            }
            self.primary_model = Some(m);
        }
        if let Some(v) = non_empty_env(env_keys::VISION_MODEL) {
            self.vision_model = Some(v);
        }
        if let Some(len) = std::env::var(env_keys::PRIMARY_MODEL_CONTEXT_LENGTH)
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
        {
            self.primary_model_context_length = Some(len);
        }
    }

    /// Returns the config for a provider, or a typed error if absent.
    pub fn provider(&self, name: &str) -> Result<&ProviderConfig, AiError> {
        self.providers.get(name).ok_or_else(|| {
            AiError::Configuration(ConfigurationError::new(
                "providers",
                format!("provider `{name}` is not configured"),
            ))
        })
    }

    /// Validates that all configured providers have non-empty keys where
    /// required. Providers without keys are *not* an error — they simply
    /// cannot be used until configured (callers get a clear error at use).
    pub fn validate(&self) -> Result<(), AiError> {
        for (name, pc) in &self.providers {
            if let Some(key) = &pc.api_key {
                if key.trim().is_empty() {
                    return Err(AiError::Configuration(ConfigurationError::new(
                        format!("{name}_API_KEY"),
                        format!("api key for provider `{name}` is empty"),
                    )));
                }
            }
        }
        Ok(())
    }

    /// A display-safe summary with API keys masked.
    pub fn redacted_summary(&self) -> String {
        let providers = self
            .providers
            .iter()
            .map(|(name, pc)| {
                let key = pc
                    .api_key
                    .as_deref()
                    .map(mask_key)
                    .unwrap_or_else(|| "unset".to_string());
                format!("{name}={key}")
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "providers: [{providers}]; timeout: {}s; max_retries: {}",
            self.defaults.timeout_secs, self.defaults.max_retries
        )
    }
}

fn set_env(map: &mut HashMap<String, ProviderConfig>, provider: &str, key: &str) {
    if let Ok(value) = std::env::var(key) {
        if !value.trim().is_empty() {
            let entry = map.entry(provider.to_string()).or_default();
            entry.api_key = Some(value);
        }
    }
}

/// Reads an env var, treating empty/whitespace-only as unset.
fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Masks an API key, showing only the first 4 and last 4 characters.
pub fn mask_key(key: &str) -> String {
    let len = key.len();
    if len <= 8 {
        return "*".repeat(len);
    }
    format!("{}…{}", &key[..4], &key[len - 4..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_key_obfuscates_middle() {
        assert_eq!(mask_key("sk-abcdefghijklmnop"), "sk-a…mnop");
        assert_eq!(mask_key("short"), "*****");
    }

    #[test]
    fn require_api_key_errors_when_missing() {
        let pc = ProviderConfig::default();
        let err = pc.require_api_key("openai").unwrap_err();
        assert!(matches!(err, AiError::Configuration(_)));
        assert!(err.to_string().contains("OPENAI_API_KEY"), "{err}");
    }

    #[test]
    fn require_api_key_succeeds_when_present() {
        let pc = ProviderConfig::new("sk-test");
        assert_eq!(pc.require_api_key("openai").unwrap(), "sk-test");
    }

    #[test]
    fn file_config_parses_toml() {
        let dir = std::env::temp_dir();
        let path = dir.join("ai-sdk-test-config.toml");
        std::fs::write(
            &path,
            r#"
[defaults]
timeout_secs = 60
max_retries = 3

[providers.openai]
api_key = "sk-file-key"
default_model = "gpt-4o"
"#,
        )
        .unwrap();
        let fc = Config::load_file(&path).unwrap();
        assert_eq!(fc.defaults.as_ref().unwrap().timeout_secs, 60);
        assert_eq!(
            fc.providers.get("openai").unwrap().api_key.as_deref(),
            Some("sk-file-key")
        );
        assert_eq!(
            fc.providers.get("openai").unwrap().default_model.as_deref(),
            Some("gpt-4o")
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn redacted_summary_hides_keys() {
        let mut cfg = Config::default();
        cfg.providers
            .insert("openai".into(), ProviderConfig::new("sk-super-secret-key"));
        let summary = cfg.redacted_summary();
        assert!(!summary.contains("sk-super-secret-key"), "{summary}");
        assert!(summary.contains("sk-s…-key"), "{summary}");
    }

    #[test]
    fn validate_rejects_empty_key() {
        let mut cfg = Config::default();
        cfg.providers.insert(
            "openai".into(),
            ProviderConfig {
                api_key: Some("   ".into()),
                ..Default::default()
            },
        );
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn merge_env_reads_provider_model_and_context_length() {
        // SAFETY (test-only): single-threaded mutation of these specific
        // variables; removed afterwards so other tests are unaffected.
        // (set_var/remove_var are unsafe as of edition 2024.)
        for (key, value) in [
            (env_keys::DEFAULT_PROVIDER, "opencode"),
            (env_keys::GATEWAY_BASE_URL, "https://gw.example/v1"),
            (env_keys::GATEWAY_API_KEY, "sk-env-test-key"),
            (env_keys::PRIMARY_MODEL, "deepseek-v4-flash"),
            (env_keys::VISION_MODEL, "mimo-v2.5"),
            (env_keys::PRIMARY_MODEL_CONTEXT_LENGTH, "131072"),
        ] {
            unsafe { std::env::set_var(key, value) };
        }

        let mut cfg = Config::default();
        cfg.merge_env();

        assert_eq!(cfg.default_provider.as_deref(), Some("opencode"));
        assert_eq!(cfg.primary_model.as_deref(), Some("deepseek-v4-flash"));
        assert_eq!(cfg.vision_model.as_deref(), Some("mimo-v2.5"));
        assert_eq!(cfg.primary_model_context_length, Some(131_072));
        let gw = cfg.providers.get("opencode").expect("gateway configured");
        assert_eq!(gw.base_url.as_deref(), Some("https://gw.example/v1"));
        assert_eq!(gw.default_model.as_deref(), Some("deepseek-v4-flash"));

        for key in [
            env_keys::DEFAULT_PROVIDER,
            env_keys::GATEWAY_BASE_URL,
            env_keys::GATEWAY_API_KEY,
            env_keys::PRIMARY_MODEL,
            env_keys::VISION_MODEL,
            env_keys::PRIMARY_MODEL_CONTEXT_LENGTH,
        ] {
            // SAFETY: restoring test-clean environment.
            unsafe { std::env::remove_var(key) };
        }
    }
}
