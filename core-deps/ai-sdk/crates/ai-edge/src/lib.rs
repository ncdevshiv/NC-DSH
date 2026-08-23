//! Edge and WASM build targets (PRD §4.4): runtime capability detection so
//! the SDK can adapt its behavior to the deployment environment.

use ai_errors::{AiError, InternalError};

/// The detected runtime environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    /// Full Node.js-compatible environment.
    Node,
    /// Web browser (WASM or JS bindings).
    Browser,
    /// Edge workers (Cloudflare Workers, Vercel Edge).
    Edge,
    /// Native (desktop/server binaries).
    Native,
    /// Unknown environment.
    Unknown,
}

impl Runtime {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Browser => "browser",
            Self::Edge => "edge",
            Self::Native => "native",
            Self::Unknown => "unknown",
        }
    }
}

/// Detects the runtime from the environment.
///
/// Uses compile-time targets (WASM ⇒ browser/edge) and runtime probes
/// (environment variables used by edge platforms, and feature detection
/// that is honest about uncertainty).
pub fn detect_runtime() -> Runtime {
    #[cfg(target_arch = "wasm32")]
    {
        return Runtime::Browser;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Cloudflare Workers sets `CF` variables; Vercel Edge sets
        // `VERCEL` + `EDGE_RUNTIME`.
        if std::env::var("CF_WORKER").is_ok()
            || (std::env::var("VERCEL").is_ok() && std::env::var("EDGE_RUNTIME").is_ok())
        {
            return Runtime::Edge;
        }
        if std::env::var("NODE_ENV").is_ok() || std::env::var("npm_node_execpath").is_ok() {
            return Runtime::Node;
        }
        Runtime::Native
    }
}

/// Which features are available in the detected runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    pub filesystem: bool,
    pub network: bool,
    pub persistent_storage: bool,
    pub web_crypto: bool,
}

impl Capabilities {
    /// Capabilities for the current runtime.
    pub fn detect() -> Self {
        match detect_runtime() {
            Runtime::Browser => Self {
                filesystem: false,
                network: true,
                persistent_storage: true, // IndexedDB/localStorage
                web_crypto: true,
            },
            Runtime::Edge => Self {
                filesystem: false,
                network: true,
                persistent_storage: true, // KV/durable objects
                web_crypto: true,
            },
            Runtime::Node | Runtime::Native => Self {
                filesystem: true,
                network: true,
                persistent_storage: true,
                web_crypto: true,
            },
            Runtime::Unknown => Self {
                filesystem: false,
                network: false,
                persistent_storage: false,
                web_crypto: false,
            },
        }
    }
}

/// Build-target helpers and JS/WASM interop for WASM compilation.
pub mod wasm {
    /// True when compiled for a WASM target.
    pub const IS_WASM: bool = cfg!(target_arch = "wasm32");

    /// The WASM target triple, when known.
    pub fn target() -> Option<&'static str> {
        #[cfg(target_arch = "wasm32")]
        {
            Some(if cfg!(target_vendor = "wasi") {
                if cfg!(target_env = "p1") {
                    "wasm32-wasip1"
                } else {
                    "wasm32-wasi"
                }
            } else {
                "wasm32-unknown-unknown"
            })
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            None
        }
    }

    /// Prepares a JSON payload string for WASM boundary transfer.
    pub fn serialize_payload<T: serde::Serialize>(val: &T) -> Result<String, String> {
        serde_json::to_string(val).map_err(|e| e.to_string())
    }

    /// Deserializes a JSON string payload from WASM boundary input.
    pub fn deserialize_payload<T: serde::de::DeserializeOwned>(json: &str) -> Result<T, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }
}

/// Returns a typed error for unsupported runtime features.
pub fn unsupported(feature: &str) -> AiError {
    AiError::Internal(InternalError::new(format!(
        "{feature} is not supported in the current runtime ({})",
        detect_runtime().as_str()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_detection_is_stable() {
        let runtime = detect_runtime();
        // On the test host this is native (or node when run under node env).
        assert!(
            matches!(runtime, Runtime::Native | Runtime::Node),
            "{runtime:?}"
        );
        assert_eq!(
            runtime.as_str(),
            if runtime == Runtime::Native {
                "native"
            } else {
                "node"
            }
        );
    }

    #[test]
    fn native_capabilities_are_complete() {
        let capabilities = Capabilities::detect();
        if detect_runtime() == Runtime::Native {
            assert!(capabilities.filesystem);
            assert!(capabilities.network);
        }
    }

    #[test]
    fn wasm_helpers_are_consistent() {
        assert_eq!(wasm::IS_WASM, cfg!(target_arch = "wasm32"));
        if wasm::IS_WASM {
            assert!(wasm::target().is_some());
        } else {
            assert!(wasm::target().is_none());
        }
    }

    #[test]
    fn unsupported_error_is_typed() {
        let err = unsupported("filesystem");
        assert!(matches!(err, AiError::Internal(_)));
        assert!(err.to_string().contains("filesystem"));
    }
}
