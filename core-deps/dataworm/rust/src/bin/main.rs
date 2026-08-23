//! Standalone DataWorm core runner.
//!
//! Reads a single JSON request from stdin, dispatches it, writes a single JSON
//! response to stdout. This is the out-of-process executor that mirrors the
//! in-process PyO3 `dispatch` exactly — same JSON contract, same code path.
//!
//! Request shape:  {"method": "<name>", "params": { ... }}
//! Response shape: {"result": <value>}  on success
//!                 {"error": "<msg>"}  on failure
//!
//! Exit codes: 0 = success, 1 = bad input / unknown method / panic.

use std::io::{self, Read, Write};

use dataworm_rust::dispatch;
use serde_json::{json, Value};

fn main() {
    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() {
        emit_error("could not read stdin");
        std::process::exit(1);
    }
    let req: Value = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(e) => {
            emit_error(&format!("invalid json request: {}", e));
            std::process::exit(1);
        }
    };
    let method = match req.get("method").and_then(|v| v.as_str()) {
        Some(m) => m.to_string(),
        None => {
            emit_error("missing 'method' field");
            std::process::exit(1);
        }
    };
    let params = req.get("params").cloned().unwrap_or(Value::Null);

    let result = dispatch(&method, params);
    let response = json!({ "result": result });
    let mut out = io::stdout();
    let _ = writeln!(out, "{}", response);
}

fn emit_error(msg: &str) {
    let response = json!({ "error": msg });
    let _ = writeln!(io::stdout(), "{}", response);
}
