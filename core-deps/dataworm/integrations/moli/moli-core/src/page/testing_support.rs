use anyhow::{Result, anyhow};
use serde::Deserialize;

use super::Page;
use super::renderer_command_support::TestingOutcome;
use crate::renderer::ScriptRunOutcome;

// ---------------------------------------------------------------------------
// Types & constants
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawTestingOutcome {
    observations: usize,
    #[serde(default)]
    failures: Vec<String>,
    #[serde(default)]
    pending_async: usize,
}

const TESTING_UNAVAILABLE_SENTINEL: &str = "__moli_testing_unavailable__";
const COLLECT_TESTING_OUTCOME_EXPRESSION: &str = r#"
(() => {
  const testing =
    globalThis.testing ||
    (globalThis.top && globalThis.top !== globalThis ? globalThis.top.testing : undefined);
  if (!testing || typeof testing.assertOk !== "function") {
    return "__moli_testing_unavailable__";
  }
  if (typeof testing.collect === "function") {
    return JSON.stringify(testing.collect());
  }
  try {
    const result = testing.assertOk();
    const observations =
      result && typeof result.observations === "number" ? result.observations : 1;
    const pending_async =
      result && typeof result.pending_async === "number" ? result.pending_async : 0;
    return JSON.stringify({
      observations,
      failures: [],
      pending_async,
    });
  } catch (err) {
    return JSON.stringify({
      observations: 1,
      failures: [
        err instanceof Error && typeof err.message === "string" && err.message.length > 0
          ? err.message
          : String(err),
      ],
      pending_async: 0,
    });
  }
})()
"#;

// ---------------------------------------------------------------------------
// Page testing methods
// ---------------------------------------------------------------------------

impl Page {
    pub async fn collect_testing_outcome_async(&mut self) -> Result<TestingOutcome> {
        let payload = self
            .evaluate_runtime_expression_async(COLLECT_TESTING_OUTCOME_EXPRESSION)
            .await?;
        self.decode_testing_outcome_payload(payload)
    }

    fn decode_testing_outcome_payload(&self, payload: serde_json::Value) -> Result<TestingOutcome> {
        let raw = payload
            .get("value")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                anyhow!("testing.collect did not return a serialized string payload: {payload}")
            })?;
        if raw == TESTING_UNAVAILABLE_SENTINEL {
            return Err(anyhow!("testing.assertOk is unavailable"));
        }

        let harness = serde_json::from_str::<RawTestingOutcome>(raw)
            .map_err(|error| anyhow!("failed to decode testing.collect payload: {error}"))?;

        Ok(TestingOutcome {
            observations: harness.observations,
            harness_failures: harness.failures,
            pending_async: harness.pending_async,
            script_failures: self
                .script_execution()
                .runs()
                .iter()
                .filter_map(|run| match run.outcome() {
                    ScriptRunOutcome::Failed(message) => Some(message.clone()),
                    _ => None,
                })
                .collect(),
            lifecycle_errors: self.page_state.lifecycle_errors().to_vec(),
        })
    }

    pub async fn testing_failures_async(&mut self) -> Result<Vec<String>> {
        let outcome = self.collect_testing_outcome_async().await?;
        Self::testing_failures_from_outcome(outcome)
    }

    fn testing_failures_from_outcome(outcome: TestingOutcome) -> Result<Vec<String>> {
        let mut failures = Vec::new();

        if outcome.observations == 0 {
            failures.push("no test observations were recorded".to_owned());
        }
        if outcome.pending_async > 0 {
            failures.push(format!(
                "{} async test task(s) still pending",
                outcome.pending_async
            ));
        }
        failures.extend(
            outcome
                .harness_failures
                .into_iter()
                .map(|failure| format!("harness: {failure}")),
        );
        failures.extend(
            outcome
                .script_failures
                .into_iter()
                .map(|failure| format!("script run failed: {failure}")),
        );
        failures.extend(
            outcome
                .lifecycle_errors
                .into_iter()
                .map(|failure| format!("lifecycle: {failure}")),
        );

        Ok(failures)
    }

    pub async fn assert_testing_ok_async(&mut self) -> Result<()> {
        let failures = self.testing_failures_async().await?;
        Self::assert_testing_failures_empty(failures)
    }

    fn assert_testing_failures_empty(failures: Vec<String>) -> Result<()> {
        if !failures.is_empty() {
            return Err(anyhow!(failures.join("\n")));
        }

        Ok(())
    }
}
