//! The clarifier: an LLM-backed ambiguity gate in front of expansion.
//!
//! Before a prompt is decomposed into a task tree, [`LlmPlanner::assess`]
//! asks the model — in exactly ONE call — whether the prompt is executable
//! as stated. The model must answer with a single JSON object:
//!
//! ```json
//! {"clear": false,
//!  "rationale": "target database unspecified",
//!  "questions": [{"text": "Which database?", "options": ["sqlite", "postgres"]}]}
//! ```
//!
//! # JSON contract
//!
//! | field        | type              | required | notes                                  |
//! |--------------|-------------------|----------|----------------------------------------|
//! | `clear`      | bool              | yes      | `true` = expand as stated              |
//! | `rationale`  | string            | yes      | short human-readable justification     |
//! | `questions`  | array of objects  | no       | each `{text, options}`, options free   |
//!
//! # Defensive parsing and retry policy
//!
//! Gateways vary, so responses are parsed defensively, never trusted:
//! code fences are stripped and the first balanced `{...}` block is
//! extracted; `clear` accepts booleans and `"true"`/`"false"` strings;
//! non-string options are dropped; more questions than
//! [`LlmPlannerConfig::max_questions`] are truncated.
//!
//! A response that yields no usable verdict is a *parse failure*: the call
//! is retried up to [`LlmPlannerConfig::parse_retries`] extra times (fresh
//! model calls, not re-parses). If every attempt fails to parse, the
//! fail-safe verdict `{ clear: true, rationale: "assessment unavailable;
//! proceeding", questions: [] }` is returned so orchestration can always
//! proceed — this is deliberately loud-by-documentation rather than silent:
//! this crate carries no logging dependency, so callers should treat the
//! sentinel rationale as the warning signal (wire `tracing` at the
//! orchestrator layer). Transport errors (`Err` from the model) are NOT
//! masked: they propagate unchanged.
//!
//! Two further anti-deadlock rules, both documented behaviour:
//!
//! - a verdict claiming `clear == false` but surviving with zero usable
//!   questions would strand wave-B2's loop waiting on nothing, so it is
//!   coerced to `clear == true`;
//! - [`verdict_into_mailbox`] allocates ids from a caller-owned counter,
//!   skipping any id currently pending in the mailbox.

use std::sync::Arc;

use ai_core::{ChatRequest, Model, ResponseFormat};
use ai_errors::AiError;
use async_trait::async_trait;
use serde_json::Value;

use crate::mailbox::QuestionMailbox;
use crate::planner::{ClarifyVerdict, PendingQuestion, Planner};

/// Rationale of the parse-failure fallback verdict. Callers can match on
/// this exact string to detect "the assessment could not be obtained".
pub const ASSESS_UNAVAILABLE_RATIONALE: &str = "assessment unavailable; proceeding";

/// Knobs for the LLM-backed planner.
#[derive(Debug, Clone, PartialEq)]
pub struct LlmPlannerConfig {
    /// Sampling temperature for the ambiguity-assessment call (low: we want
    /// judgement, not creativity).
    pub clarity_temperature: f32,
    /// Sampling temperature for the decomposition call (see
    /// [`crate::expander`]).
    pub decomposition_temperature: f32,
    /// Hard cap on clarifying questions surfaced per assessment. Model
    /// output beyond the cap is truncated.
    pub max_questions: usize,
    /// Hard cap on total nodes one expansion may propose (see
    /// [`crate::expander`]); larger plans are rejected.
    pub max_nodes: usize,
    /// Extra model calls after the first when a response does not parse.
    pub parse_retries: usize,
}

impl Default for LlmPlannerConfig {
    fn default() -> Self {
        Self {
            clarity_temperature: 0.2,
            decomposition_temperature: 0.2,
            max_questions: 4,
            max_nodes: 32,
            parse_retries: 2,
        }
    }
}

/// The single planner implementation of wave B1: one LLM call per phase
/// ([`assess`](Planner::assess) here, [`expand`](Planner::expand) in
/// [`crate::expander`]), defensive parsing throughout, and typed fallbacks
/// so the orchestration loop can always make progress.
pub struct LlmPlanner {
    model: Arc<dyn Model>,
    config: LlmPlannerConfig,
}

impl LlmPlanner {
    /// Creates a planner backed by `model` with default configuration.
    #[must_use]
    pub fn new(model: Arc<dyn Model>) -> Self {
        Self::with_config(model, LlmPlannerConfig::default())
    }

    /// Creates a planner backed by `model` with explicit configuration.
    #[must_use]
    pub fn with_config(model: Arc<dyn Model>, config: LlmPlannerConfig) -> Self {
        Self { model, config }
    }

    /// The backing model handle.
    #[must_use]
    pub fn model(&self) -> &Arc<dyn Model> {
        &self.model
    }

    /// The active configuration.
    #[must_use]
    pub const fn config(&self) -> &LlmPlannerConfig {
        &self.config
    }

    /// Runs one generate call with the given instruction prompt and
    /// structured-output request, returning the completion text.
    ///
    /// Shared by both phases (assess here, expand in `expander`) so the
    /// request shape — temperature knob + JSON-schema response format — is
    /// identical everywhere.
    pub(crate) async fn generate_json(
        &self,
        instruction: &str,
        schema_name: &str,
        schema: Value,
        temperature: f32,
    ) -> Result<String, AiError> {
        let request = ChatRequest::new(vec![ai_types::Message::text(
            ai_types::Role::User,
            instruction,
        )])
        .with_temperature(temperature)
        .with_response_format(ResponseFormat::JsonSchema {
            schema,
            name: schema_name.to_owned(),
        });
        let completion = self.model.generate(request).await?;
        Ok(completion.text)
    }
}

#[async_trait]
impl Planner for LlmPlanner {
    /// Assesses `prompt` for executability in a single LLM call.
    ///
    /// See the [module docs](self) for the JSON contract, retry policy and
    /// fail-safe behaviour.
    async fn assess(&self, prompt: &str) -> Result<ClarifyVerdict, AiError> {
        let attempts = self.config.parse_retries.saturating_add(1);
        for _ in 0..attempts {
            let text = self
                .generate_json(
                    &assess_instruction(prompt),
                    "clarify_verdict",
                    verdict_schema(),
                    self.config.clarity_temperature,
                )
                .await?;
            if let Some(verdict) = parse_verdict(&text, self.config.max_questions) {
                return Ok(verdict);
            }
            // Parse failure only; transport errors above propagate as-is.
        }
        // Fail-safe: never deadlock the orchestrator on a babbling gateway.
        Ok(ClarifyVerdict {
            clear: true,
            rationale: ASSESS_UNAVAILABLE_RATIONALE.to_owned(),
            questions: Vec::new(),
        })
    }

    async fn expand(
        &self,
        tree: &mut crate::tree::TaskTree,
        parent: Option<crate::tree::TaskId>,
        clarified_prompt: &str,
    ) -> Result<Vec<crate::tree::TaskId>, AiError> {
        // Delegation to the inherent method keeps expander.rs the single
        // owner of the decomposition contract.
        self.expand_tree(tree, parent, clarified_prompt).await
    }
}

// -- assess: instruction + schema ------------------------------------------

/// Builds the user message driving the assessment call. The JSON contract
/// is spelled out in prose because not every gateway enforces
/// `response_format`.
fn assess_instruction(prompt: &str) -> String {
    format!(
        "You are the clarification gate of an orchestrator. Decide whether the \
         objective below is specific enough to decompose into tasks and execute \
         WITHOUT asking anyone else.\n\
         \n\
         Respond with EXACTLY ONE JSON object and nothing else:\n\
         {{\"clear\": <bool>, \"rationale\": \"<short reason>\", \"questions\": \
         [{{\"text\": \"<question>\", \"options\": [\"<choice>\", ...]}}, ...]}}\n\
         \n\
         Rules:\n\
         - \"clear\": true only if you would start work immediately.\n\
         - When not clear, list AT MOST 4 questions; each question must be \
         answerable in one short reply; prefer 2-4 concrete \"options\" per \
         question; use [] when free-form is the only sensible answer.\n\
         - Never wrap the JSON in markdown fences or commentary.\n\
         \n\
         OBJECTIVE:\n{prompt}"
    )
}

/// JSON Schema advertised via `ResponseFormat::JsonSchema` (best-effort
/// enforcement on gateways that support it).
fn verdict_schema() -> Value {
    let mut schema = serde_json::json!({
        "type": "object",
        "properties": {
            "clear": {"type": "boolean"},
            "rationale": {"type": "string"},
            "questions": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "text": {"type": "string"},
                        "options": {"type": "array", "items": {"type": "string"}}
                    },
                    "required": ["text"]
                }
            }
        },
        "required": ["clear", "rationale"]
    });
    make_strict(&mut schema);
    schema
}

/// Rewrites a JSON Schema into OpenAI *strict*-compatible form in place:
/// every object gets `"additionalProperties": false` and `required` listing
/// ALL of its properties (strict rejects optional properties).
///
/// Gateways that enforce `response_format` `strict: true` return HTTP 400
/// otherwise; gateways that ignore the flag are unaffected.
pub(crate) fn make_strict(schema: &mut Value) {
    let Some(obj) = schema.as_object_mut() else {
        return;
    };
    let is_object_type = obj.get("type").and_then(|t| t.as_str()) == Some("object");
    if is_object_type {
        obj.insert("additionalProperties".into(), Value::Bool(false));
        let mut new_props = serde_json::Map::new();
        if let Some(Value::Object(props)) = obj.remove("properties") {
            for (name, mut child) in props {
                make_strict(&mut child);
                new_props.insert(name, child);
            }
        }
        let names: Vec<String> = new_props.keys().cloned().collect();
        obj.insert(
            "required".into(),
            Value::Array(names.into_iter().map(Value::String).collect()),
        );
        obj.insert("properties".into(), Value::Object(new_props));
    }
    if let Some(items) = obj.get_mut("items") {
        make_strict(items);
    }
}

// -- defensive extraction (shared with expander.rs) -------------------------

/// Extracts the first balanced top-level JSON object from arbitrary model
/// text: code fences, prose prefixes and trailing chatter are tolerated.
///
/// Brace scanning is escape-aware (braces inside string literals are
/// ignored), which is where naive `find('{')..find('}')` slicing breaks.
pub(crate) fn extract_json_object(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut start = None;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'"' if !escaped => in_string = !in_string,
            b'\\' if in_string => escaped = !escaped,
            b'{' if !in_string => {
                start.get_or_insert(i);
                depth += 1;
            }
            b'}' if !in_string && depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    let from = start.unwrap_or(i);
                    return Some(text[from..=i].to_owned());
                }
            }
            _ => {}
        }
        if b != b'\\' {
            escaped = false;
        }
    }
    None
}

/// Tolerant bool reader: accepts JSON booleans and `"true"`/`"false"`
/// strings (both cases), which some gateways emit despite the schema.
pub(crate) fn coerce_bool(value: Option<&Value>) -> Option<bool> {
    match value {
        Some(Value::Bool(b)) => Some(*b),
        Some(Value::String(s)) => match s.trim().to_ascii_lowercase().as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// Tolerant string-array reader: keeps only string elements.
fn coerce_strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Parses one assessment response into a [`ClarifyVerdict`], or `None` when
/// nothing usable could be recovered (triggering the caller's retry).
///
/// Post-parse repairs applied here, all part of the contract:
/// - questions beyond `max_questions` are TRUNCATED (never dropped wholesale);
/// - questions without a usable `text` are skipped individually;
/// - `clear == false` with zero surviving questions is coerced to
///   `clear == true` (annotated rationale) — see module docs.
fn parse_verdict(text: &str, max_questions: usize) -> Option<ClarifyVerdict> {
    let raw: Value = serde_json::from_str(&extract_json_object(text)?).ok()?;
    let clear = coerce_bool(raw.get("clear"))?;

    let rationale = raw
        .get("rationale")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();

    let mut questions = Vec::new();
    if let Some(items) = raw.get("questions").and_then(Value::as_array) {
        for item in items {
            if questions.len() >= max_questions {
                break;
            }
            let Some(qtext) = item.get("text").and_then(Value::as_str) else {
                continue;
            };
            questions.push(PendingQuestion {
                text: qtext.to_owned(),
                options: coerce_strings(item.get("options")),
            });
        }
    }

    if !clear && questions.is_empty() {
        // Anti-deadlock coercion: an unclear verdict nobody can act on.
        return Some(ClarifyVerdict {
            clear: true,
            rationale: format!(
                "{rationale} (flagged unclear but produced no usable questions; proceeding)"
            ),
            questions: Vec::new(),
        });
    }

    Some(ClarifyVerdict {
        clear,
        rationale,
        questions,
    })
}

// -- mailbox convenience -----------------------------------------------------

/// Maps a verdict's [`PendingQuestion`]s into the [`QuestionMailbox`],
/// returning the ids they were registered under, in order.
///
/// Id allocation: `QuestionMailbox` has no id allocator of its own (ids are
/// caller-supplied), so this function draws sequential ids from
/// `id_counter`, starting at `*id_counter`. An id that is currently pending
/// in the mailbox is skipped (counter advanced past it) instead of failing
/// with [`crate::mailbox::MailboxError::DuplicateQuestion`]; the counter is
/// left pointing just past the last allocated id, so repeated calls chain
/// naturally.
///
/// Returns the ids actually registered. Under the collision-skip rule above
/// this equals `verdict.questions.len()` unless another thread races the
/// same counter against the same mailbox — a caller bug that surfaces as a
/// short return value rather than a panic.
pub fn verdict_into_mailbox(
    verdict: ClarifyVerdict,
    mailbox: &QuestionMailbox,
    id_counter: &mut u64,
) -> Vec<u64> {
    let mut registered = Vec::with_capacity(verdict.questions.len());
    for question in verdict.questions {
        let id = loop {
            let candidate = *id_counter;
            *id_counter += 1;
            if !mailbox.is_pending(candidate) {
                break candidate;
            }
        };
        // is_pending was just checked; a failure here means a concurrent
        // racer reused the id — stop rather than lose track silently.
        if mailbox.ask(question.into_question(id)).is_err() {
            break;
        }
        registered.push(id);
    }
    registered
}

// -- shared offline scripted model -------------------------------------------
//
// Per ADR-007 unit tests mock the LLM. Lives in its own cfg(test) module so
// BOTH planning files' tests drive the exact same fake gateway.

#[cfg(test)]
pub(crate) mod test_support {
    use ai_core::{ChatRequest, EventStream, Model};
    use ai_errors::AiError;
    use ai_models::{ModelCapabilities, ModelInfo};
    use ai_types::{Completion, ModelId, ProviderId};
    use serde_json::Value;

    pub fn model_info() -> &'static ModelInfo {
        static INFO: std::sync::OnceLock<ModelInfo> = std::sync::OnceLock::new();
        INFO.get_or_init(|| {
            ModelInfo::new(
                ProviderId::new("test"),
                ModelId::new("scripted"),
                128_000,
                8_192,
            )
            .with_capabilities(ModelCapabilities::default())
        })
    }

    pub fn completion(text: &str) -> Completion {
        Completion {
            provider: ProviderId::new("test"),
            model: ModelId::new("scripted"),
            text: text.to_owned(),
            tool_calls: Vec::new(),
            usage: ai_types::Usage::new(10, 5),
            reasoning: None,
            raw: Value::Null,
            finish_reason: Some("stop".to_owned()),
        }
    }

    /// Replays canned completions in order and records every request so
    /// tests can assert on prompts and response formats.
    pub struct ScriptedModel {
        script: Vec<Completion>,
        index: parking_lot::Mutex<usize>,
        requests: parking_lot::Mutex<Vec<ChatRequest>>,
    }

    impl ScriptedModel {
        pub fn new(script: Vec<Completion>) -> Self {
            Self {
                script,
                index: parking_lot::Mutex::new(0),
                requests: parking_lot::Mutex::new(Vec::new()),
            }
        }

        /// How many completions were served so far.
        pub fn calls(&self) -> usize {
            *self.index.lock()
        }

        /// Snapshot of every request seen, in arrival order.
        pub fn requests(&self) -> Vec<ChatRequest> {
            self.requests.lock().clone()
        }
    }

    #[async_trait::async_trait]
    impl Model for ScriptedModel {
        fn info(&self) -> &ModelInfo {
            model_info()
        }

        async fn generate(&self, request: ChatRequest) -> Result<Completion, AiError> {
            self.requests.lock().push(request);
            let mut index = self.index.lock();
            let completion = self
                .script
                .get(*index)
                .cloned()
                .unwrap_or_else(|| completion(""));
            *index += 1;
            Ok(completion)
        }

        async fn stream(&self, _request: ChatRequest) -> Result<EventStream, AiError> {
            unreachable!("clarifier tests never stream")
        }
    }
}

// -- tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clarifier::test_support::{ScriptedModel, completion};
    use ai_types::Completion;
    use std::sync::Arc;

    fn planner(script: Vec<Completion>) -> (LlmPlanner, Arc<ScriptedModel>) {
        let model = Arc::new(ScriptedModel::new(script));
        (LlmPlanner::new(Arc::clone(&model) as Arc<dyn Model>), model)
    }

    // -- assess -------------------------------------------------------------

    #[tokio::test]
    async fn clear_prompt_yields_clear_verdict_without_questions() {
        let (p, model) = planner(vec![completion(
            r#"{"clear": true, "rationale": "fully specified", "questions": []}"#,
        )]);

        let verdict = p.assess("Write fizzbuzz in Python").await.unwrap();

        assert!(verdict.clear);
        assert_eq!(verdict.rationale, "fully specified");
        assert!(verdict.questions.is_empty());
        assert_eq!(model.calls(), 1);

        // The single call used the clarity temperature and the JSON-schema
        // response format.
        let requests = model.requests();
        assert_eq!(
            requests[0].temperature,
            Some(p.config().clarity_temperature)
        );
        assert!(matches!(
            requests[0].response_format,
            ResponseFormat::JsonSchema { ref name, .. } if name == "clarify_verdict"
        ));
        // And the objective reached the model.
        let prompt_text = requests[0].messages[0].text_content();
        assert!(prompt_text.contains("fizzbuzz"));
    }

    #[tokio::test]
    async fn ambiguous_verdict_maps_questions_one_to_one_in_order() {
        let (p, _model) = planner(vec![completion(
            r#"{"clear": false, "rationale": "db and scope unknown", "questions": [
                {"text": "Which DB?", "options": ["sqlite", "postgres"]},
                {"text": "CLI or web?", "options": []}
            ]}"#,
        )]);

        let verdict = p.assess("Build me a data app").await.unwrap();

        assert!(!verdict.clear);
        assert_eq!(verdict.rationale, "db and scope unknown");
        assert_eq!(verdict.questions.len(), 2);
        assert_eq!(verdict.questions[0].text, "Which DB?");
        assert_eq!(verdict.questions[0].options, vec!["sqlite", "postgres"]);
        assert_eq!(verdict.questions[1].text, "CLI or web?");
        assert!(verdict.questions[1].options.is_empty());

        // Ready for the mailbox: PendingQuestion -> Question round-trip.
        let q = verdict.questions[0].clone().into_question(42);
        assert_eq!(q.id, 42);
        assert_eq!(q.text, "Which DB?");
    }

    #[tokio::test]
    async fn fenced_and_embedded_json_is_extracted_defensively() {
        let (p, _model) = planner(vec![completion(
            "Sure! Here is my assessment:\n```json\n\
             {\"clear\": false, \"rationale\": \"needs target\", \"questions\": \
             [{\"text\": \"Target?\", \"options\": [\"x86\", {\"nonsense\": 1}]}]}\n\
             ```\nHope that helps!",
        )]);

        let verdict = p.assess("compile it").await.unwrap();

        assert!(!verdict.clear);
        assert_eq!(verdict.questions.len(), 1);
        // Non-string option dropped, string options kept.
        assert_eq!(verdict.questions[0].options, vec!["x86"]);
    }

    #[tokio::test]
    async fn string_coerced_clear_field_is_accepted() {
        let (p, _model) = planner(vec![completion(r#"{"clear": "true", "rationale": "ok"}"#)]);
        let verdict = p.assess("do it").await.unwrap();
        assert!(verdict.clear);
        assert!(verdict.questions.is_empty());
    }

    #[tokio::test]
    async fn questions_are_truncated_to_max_questions() {
        let model = Arc::new(ScriptedModel::new(vec![completion(
            r#"{"clear": false, "rationale": "many unknowns", "questions": [
                {"text": "q1", "options": []},
                {"text": "q2", "options": []},
                {"text": "q3", "options": []},
                {"text": "q4", "options": []},
                {"text": "q5", "options": []},
                {"text": "q6", "options": []}
            ]}"#,
        )]));
        let config = LlmPlannerConfig {
            max_questions: 4,
            ..LlmPlannerConfig::default()
        };
        let p = LlmPlanner::with_config(model, config);

        let verdict = p.assess("vague").await.unwrap();

        assert_eq!(verdict.questions.len(), 4);
        assert_eq!(verdict.questions[3].text, "q4");
    }

    #[tokio::test]
    async fn malformed_then_valid_retry_succeeds_on_second_call() {
        let (p, model) = planner(vec![
            completion("I cannot answer that in JSON, sorry."),
            completion(r#"{"clear": true, "rationale": "recovered", "questions": []}"#),
        ]);

        let verdict = p.assess("retry me").await.unwrap();

        assert!(verdict.clear);
        assert_eq!(verdict.rationale, "recovered");
        assert_eq!(model.calls(), 2, "exactly one retry after the bad parse");
    }

    #[tokio::test]
    async fn permanently_malformed_output_falls_back_to_proceed_with_warning() {
        let (p, model) = planner(vec![
            completion("nope"),
            completion("{broken"),
            completion("[1,2,3]"),
        ]);

        let verdict = p.assess("hopeless").await.unwrap();

        assert!(verdict.clear, "fail-safe must proceed");
        assert!(verdict.questions.is_empty());
        assert_eq!(verdict.rationale, ASSESS_UNAVAILABLE_RATIONALE);
        assert_eq!(verdict.rationale, "assessment unavailable; proceeding");
        // 1 initial attempt + 2 configured retries.
        assert_eq!(model.calls(), 3);
    }

    #[tokio::test]
    async fn unclear_without_usable_questions_is_coerced_to_proceeding() {
        // Anti-deadlock rule: "not clear" with zero parsable questions would
        // leave the orchestrator parked forever.
        let (p, _model) = planner(vec![
            completion(r#"{"clear": false, "rationale": "it's complicated"}"#),
            completion(""),
        ]);

        let verdict = p.assess("complicated").await.unwrap();

        assert!(verdict.clear);
        assert!(verdict.questions.is_empty());
        assert!(
            verdict.rationale.contains("proceeding"),
            "coerced rationale explains itself: {}",
            verdict.rationale
        );
    }

    #[tokio::test]
    async fn questions_without_text_are_skipped_but_valid_ones_survive() {
        let (p, _model) = planner(vec![completion(
            r#"{"clear": false, "rationale": "?", "questions": [
                {"options": ["a"]},
                {"text": "real question", "options": ["b"]}
            ]}"#,
        )]);

        let verdict = p.assess("x").await.unwrap();

        assert_eq!(verdict.questions.len(), 1);
        assert_eq!(verdict.questions[0].text, "real question");
    }

    // -- extract_json_object -------------------------------------------------

    #[test]
    fn extraction_handles_fences_prose_and_braces_in_strings() {
        assert_eq!(
            extract_json_object("noise {\"a\": 1} tail").as_deref(),
            Some("{\"a\": 1}")
        );
        // Braces inside strings must not unbalance the scan.
        assert_eq!(
            extract_json_object(r#"{"a": "curly } brace {", "b": 2}"#).as_deref(),
            Some(r#"{"a": "curly } brace {", "b": 2}"#)
        );
        // Nested objects survive intact.
        assert_eq!(
            extract_json_object(r#"prefix {"a": {"b": 3}} suffix"#).as_deref(),
            Some(r#"{"a": {"b": 3}}"#)
        );
        // Escaped quote inside a string stays in-string.
        assert_eq!(
            extract_json_object(r#"{"a": "quote \" here", "b": 1}"#).as_deref(),
            Some(r#"{"a": "quote \" here", "b": 1}"#)
        );
        // Unbalanced garbage yields nothing.
        assert_eq!(extract_json_object("no json here"), None);
        assert_eq!(extract_json_object("{ never closed"), None);
    }

    // -- verdict_into_mailbox ------------------------------------------------

    #[test]
    fn verdict_maps_into_mailbox_with_sequential_counter_ids() {
        let mailbox = QuestionMailbox::new();
        let verdict = ClarifyVerdict {
            clear: false,
            rationale: "r".into(),
            questions: vec![
                PendingQuestion {
                    text: "first?".into(),
                    options: vec!["a".into()],
                },
                PendingQuestion {
                    text: "second?".into(),
                    options: vec![],
                },
            ],
        };

        let mut counter: u64 = 7;
        let ids = verdict_into_mailbox(verdict, &mailbox, &mut counter);

        assert_eq!(ids, vec![7, 8]);
        assert_eq!(counter, 9, "counter advanced past allocated ids");
        assert_eq!(mailbox.pending_count(), 2);
        let pending = mailbox.pending_questions();
        assert_eq!(pending[0].id, 7);
        assert_eq!(pending[0].text, "first?");
        assert_eq!(pending[0].options, vec!["a".to_owned()]);
        assert_eq!(pending[1].id, 8);

        // A second mapping continues the sequence.
        let verdict2 = ClarifyVerdict {
            clear: false,
            rationale: "r".into(),
            questions: vec![PendingQuestion {
                text: "third?".into(),
                options: vec![],
            }],
        };
        let ids2 = verdict_into_mailbox(verdict2, &mailbox, &mut counter);
        assert_eq!(ids2, vec![9]);
    }

    #[test]
    fn ids_currently_pending_are_skipped_without_error() {
        let mailbox = QuestionMailbox::new();
        // Pre-existing pending question squatting on id 5.
        let _rx = mailbox
            .ask(crate::mailbox::Question::new(5, "occupied", vec![]))
            .unwrap();

        let verdict = ClarifyVerdict {
            clear: false,
            rationale: "r".into(),
            questions: vec![PendingQuestion {
                text: "mine?".into(),
                options: vec![],
            }],
        };
        let mut counter: u64 = 5;
        let ids = verdict_into_mailbox(verdict, &mailbox, &mut counter);

        assert_eq!(ids, vec![6], "id 5 skipped, next free id taken");
        assert!(mailbox.is_pending(5));
        assert!(mailbox.is_pending(6));
    }

    #[test]
    fn clear_verdict_registers_nothing() {
        let mailbox = QuestionMailbox::new();
        let mut counter: u64 = 0;
        let ids = verdict_into_mailbox(ClarifyVerdict::default(), &mailbox, &mut counter);
        assert!(ids.is_empty());
        assert_eq!(counter, 0);
        assert_eq!(mailbox.pending_count(), 0);
    }
}
