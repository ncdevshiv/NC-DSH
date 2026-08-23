//! Clarification plumbing: park a question, get an answer back without
//! polling.
//!
//! The orchestrator's clarify step needs to surface questions to a human and
//! continue exactly where it left off when the answer arrives. A
//! [`QuestionMailbox`] does that with one `oneshot` channel per question:
//! `ask` parks the caller on the returned receiver, the human-facing side
//! lists [`pending_questions`] and resolves one via [`answer`], and
//! [`retract`] withdraws a question (closing its waiter's channel).
//!
//! Ids are caller-supplied so wave-B can correlate a question with its task
//! node; duplicates among PENDING questions are rejected. Once answered (or
//! retracted) an id is free again.
//!
//! [`pending_questions`]: QuestionMailbox::pending_questions
//! [`answer`]: QuestionMailbox::answer
//! [`retract`]: QuestionMailbox::retract

use std::collections::HashMap;

use parking_lot::Mutex;
use tokio::sync::oneshot;

/// A clarification question surfaced to the human operator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    /// Caller-chosen correlation id (unique among currently pending
    /// questions).
    pub id: u64,
    /// What is being asked; self-contained, no other context required.
    pub text: String,
    /// Suggested answers; empty means free-form only.
    pub options: Vec<String>,
}

impl Question {
    /// Convenience constructor.
    #[must_use]
    pub fn new(id: u64, text: impl Into<String>, options: Vec<String>) -> Self {
        Self {
            id,
            text: text.into(),
            options,
        }
    }
}

/// The human's response to one pending [`Question`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    /// Which question this answers.
    pub question_id: u64,
    /// The chosen option, when the question offered any.
    pub choice: Option<String>,
    /// Free-form elaboration.
    pub free_text: Option<String>,
}

impl Answer {
    /// Convenience constructor for a bare choice.
    #[must_use]
    pub fn choice(question_id: u64, choice: impl Into<String>) -> Self {
        Self {
            question_id,
            choice: Some(choice.into()),
            free_text: None,
        }
    }

    /// Convenience constructor for a bare free-text reply.
    #[must_use]
    pub fn free_text(question_id: u64, text: impl Into<String>) -> Self {
        Self {
            question_id,
            choice: None,
            free_text: Some(text.into()),
        }
    }
}

/// Mailbox operation failures.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MailboxError {
    /// No pending question carries this id.
    #[error("unknown question id {0}")]
    UnknownQuestion(u64),
    /// A pending question already uses this id.
    #[error("question id {0} is already pending")]
    DuplicateQuestion(u64),
}

type Waiter = oneshot::Sender<Answer>;

#[derive(Debug, Default)]
struct MailboxInner {
    /// Pending questions by id.
    pending: HashMap<u64, Question>,
    /// One resolver per pending question, keyed identically.
    waiters: HashMap<u64, Waiter>,
}

/// Async-safe mailbox connecting askers to the human/operator side.
///
/// Locking is synchronous (`parking_lot`, held only for map operations);
/// the only async boundary is the per-question `oneshot` receiver.
#[derive(Debug, Default)]
pub struct QuestionMailbox {
    inner: Mutex<MailboxInner>,
}

impl QuestionMailbox {
    /// Creates an empty mailbox.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `question` and returns the receiver that resolves with the
    /// eventual [`Answer`].
    ///
    /// Errors if another pending question already uses the same id.
    pub fn ask(&self, question: Question) -> Result<oneshot::Receiver<Answer>, MailboxError> {
        let mut inner = self.inner.lock();
        if inner.pending.contains_key(&question.id) {
            return Err(MailboxError::DuplicateQuestion(question.id));
        }
        let (tx, rx) = oneshot::channel();
        inner.waiters.insert(question.id, tx);
        inner.pending.insert(question.id, question);
        Ok(rx)
    }

    /// Delivers `answer` to whoever parked on that question's receiver.
    ///
    /// The question stops being pending immediately. Errors with
    /// [`MailboxError::UnknownQuestion`] when nothing is pending under that
    /// id (never asked, already answered, or retracted).
    pub fn answer(&self, answer: Answer) -> Result<(), MailboxError> {
        let mut inner = self.inner.lock();
        match inner.waiters.remove(&answer.question_id) {
            Some(waiter) => {
                inner.pending.remove(&answer.question_id);
                // The asker may have gone away (receiver dropped); that is a
                // fine outcome for the answering side.
                let _ = waiter.send(answer);
                Ok(())
            }
            None => Err(MailboxError::UnknownQuestion(answer.question_id)),
        }
    }

    /// Withdraws a pending question without answering it.
    ///
    /// The waiting receiver observes a closed channel (`RecvError`) rather
    /// than an [`Answer`] — callers treat that as "retracted".
    pub fn retract(&self, id: u64) -> Result<(), MailboxError> {
        let mut inner = self.inner.lock();
        match inner.pending.remove(&id) {
            Some(_) => {
                inner.waiters.remove(&id); // dropping closes the channel
                Ok(())
            }
            None => Err(MailboxError::UnknownQuestion(id)),
        }
    }

    /// Snapshot of all pending questions, ascending id order.
    #[must_use]
    pub fn pending_questions(&self) -> Vec<Question> {
        let inner = self.inner.lock();
        let mut out: Vec<Question> = inner.pending.values().cloned().collect();
        out.sort_by_key(|q| q.id);
        out
    }

    /// How many questions await an answer.
    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.inner.lock().pending.len()
    }

    /// Whether `id` currently has a pending question.
    #[must_use]
    pub fn is_pending(&self, id: u64) -> bool {
        self.inner.lock().pending.contains_key(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn ask_answer_roundtrip_resumes_parked_task() {
        let mailbox = QuestionMailbox::new();

        let (result, ()) = tokio::join!(
            async {
                let rx = mailbox
                    .ask(Question::new(
                        1,
                        "Which DB?",
                        vec!["sqlite".into(), "postgres".into()],
                    ))
                    .unwrap();
                rx.await.expect("mailbox answers before shutdown")
            },
            async {
                // Operator sees the question, then answers it.
                assert_eq!(mailbox.pending_count(), 1);
                let pending = mailbox.pending_questions();
                assert_eq!(pending[0].text, "Which DB?");
                mailbox.answer(Answer::choice(1, "postgres")).unwrap();
            }
        );

        assert_eq!(
            result,
            Answer {
                question_id: 1,
                choice: Some("postgres".into()),
                free_text: None,
            }
        );
        assert_eq!(mailbox.pending_count(), 0);
    }

    #[tokio::test]
    async fn unknown_ids_are_typed_errors() {
        let mailbox = QuestionMailbox::new();

        // Answering / retracting something never asked fails cleanly.
        assert_eq!(
            mailbox.answer(Answer::choice(99, "x")).unwrap_err(),
            MailboxError::UnknownQuestion(99)
        );
        assert_eq!(
            mailbox.retract(99).unwrap_err(),
            MailboxError::UnknownQuestion(99)
        );

        // After being answered once, the id is spent.
        let _rx = mailbox.ask(Question::new(5, "?", vec![])).unwrap();
        mailbox.answer(Answer::free_text(5, "because")).unwrap();
        assert_eq!(
            mailbox.answer(Answer::free_text(5, "again")).unwrap_err(),
            MailboxError::UnknownQuestion(5)
        );

        // ...and therefore reusable for a NEW question.
        let _rx2 = mailbox.ask(Question::new(5, "again?", vec![])).unwrap();
        assert!(mailbox.is_pending(5));

        // Asking while still pending is rejected as duplicate.
        assert_eq!(
            mailbox.ask(Question::new(5, "dup", vec![])).unwrap_err(),
            MailboxError::DuplicateQuestion(5)
        );
    }

    #[tokio::test]
    async fn multiple_outstanding_questions_are_independent() {
        let mailbox = std::sync::Arc::new(QuestionMailbox::new());

        let m1 = Arc::clone(&mailbox);
        let waiter_a = tokio::spawn(async move {
            m1.ask(Question::new(10, "first?", vec![]))
                .unwrap()
                .await
                .unwrap()
        });
        let m2 = Arc::clone(&mailbox);
        let waiter_b = tokio::spawn(async move {
            m2.ask(Question::new(20, "second?", vec!["a".into(), "b".into()]))
                .unwrap()
                .await
                .unwrap()
        });

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert_eq!(mailbox.pending_count(), 2);
        // Ascending-id snapshot across both outstanding questions.
        let ids: Vec<u64> = mailbox.pending_questions().iter().map(|q| q.id).collect();
        assert_eq!(ids, vec![10, 20]);

        // Answer in REVERSE order; each parked task gets exactly its own.
        mailbox.answer(Answer::choice(20, "b")).unwrap();
        let b = waiter_b.await.unwrap();
        assert_eq!(b.question_id, 20);
        assert_eq!(b.choice.as_deref(), Some("b"));

        mailbox.answer(Answer::free_text(10, "n/a")).unwrap();
        let a = waiter_a.await.unwrap();
        assert_eq!(a.question_id, 10);
        assert_eq!(a.free_text.as_deref(), Some("n/a"));

        assert_eq!(mailbox.pending_count(), 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn retract_closes_the_waiter_channel() {
        let mailbox = QuestionMailbox::new();
        let rx = mailbox
            .ask(Question::new(7, "still needed?", vec![]))
            .unwrap();
        mailbox.retract(7).unwrap();

        // Retraction surfaces as channel closure, not an Answer.
        assert!(rx.await.is_err());
        assert!(!mailbox.is_pending(7));
    }

    #[test]
    fn dropped_asker_does_not_break_the_mailbox() {
        let mailbox = QuestionMailbox::new();
        {
            let _rx = mailbox.ask(Question::new(3, "?", vec![])).unwrap();
            // rx dropped here — the asker went away.
        }
        // Answering into the void succeeds from the operator side...
        assert!(mailbox.answer(Answer::choice(3, "x")).is_ok());
        assert_eq!(mailbox.pending_count(), 0);

        // ...and retracting after an answered question is UnknownQuestion.
        assert_eq!(
            mailbox.retract(3).unwrap_err(),
            MailboxError::UnknownQuestion(3)
        );
    }
}
