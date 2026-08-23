//! # ai-computer — native automation plugins
//!
//! Real browser and desktop control for SDK agents, speaking to two local
//! engines over authenticated JSON-RPC:
//!
//! - **Browser** ([`omnichrome`]): the OmniChrome Chrome-extension bridge
//!   (CDP navigation, clicks, typing, screenshots, Markdown/DOM extraction,
//!   evaluate, network/console logs) on `http://localhost:8765/rpc`.
//! - **Desktop** ([`native`]): the Native Computer Use engine (Win32
//!   Bézier mouse/keyboard, GDI+ screenshots, UIA tree + OCR grounding,
//!   visual waits, window management) on `http://localhost:8888/rpc`.
//!
//! Both plugins are honest by construction: when an engine is not running,
//! calls fail with a typed, actionable error — nothing is fabricated.
//!
//! # Configuration
//!
//! | Engine | URL env | Token env | Token file fallback |
//! |---|---|---|---|
//! | OmniChrome | `OMNICHROME_BRIDGE_URL` (`http://localhost:8765/rpc`) | `OMNICHROME_TOKEN` | `<root>/server/.bridge-token` |
//! | ComputerUse | `COMPUTERUSE_SERVER_URL` (`http://localhost:8888/rpc`) | `COMPUTERUSE_TOKEN` | `%USERPROFILE%\.computeruse\auth.token` |

#![forbid(unsafe_code)]

pub mod jsonrpc_client;
pub mod native;
pub mod omnichrome;

pub use jsonrpc_client::{ComputerError, JsonRpcHttpClient, field, resolve_token};
