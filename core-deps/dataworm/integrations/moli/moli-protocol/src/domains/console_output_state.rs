#[cfg(test)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ConsoleLogEmissionSnapshot {
    console_start: usize,
    lifecycle_start: usize,
    console_messages: Vec<String>,
    lifecycle_errors: Vec<String>,
}

#[cfg(test)]
impl ConsoleLogEmissionSnapshot {
    pub(crate) fn console_messages(&self) -> &[String] {
        &self.console_messages
    }

    pub(crate) fn lifecycle_errors(&self) -> &[String] {
        &self.lifecycle_errors
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.console_messages.is_empty() && self.lifecycle_errors.is_empty()
    }

    pub(crate) fn console_end(&self) -> usize {
        self.console_start + self.console_messages.len()
    }

    pub(crate) fn lifecycle_end(&self) -> usize {
        self.lifecycle_start + self.lifecycle_errors.len()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TargetConsoleOutputState {
    console_domain_entries: usize,
    console_domain_exception_entries: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TargetConsoleOutputCursor {
    console_start: usize,
    lifecycle_start: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TargetConsoleOutputDomain {
    Console,
}

impl TargetConsoleOutputCursor {
    pub(crate) fn console_start(self) -> usize {
        self.console_start
    }

    pub(crate) fn lifecycle_start(self) -> usize {
        self.lifecycle_start
    }
}

impl TargetConsoleOutputState {
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn has_unemitted_console_domain(
        self,
        console_message_count: usize,
        lifecycle_error_count: usize,
    ) -> bool {
        console_message_count > self.console_domain_entries
            || lifecycle_error_count > self.console_domain_exception_entries
    }

    pub(crate) fn has_unemitted(
        self,
        domain: TargetConsoleOutputDomain,
        console_message_count: usize,
        lifecycle_error_count: usize,
    ) -> bool {
        match domain {
            TargetConsoleOutputDomain::Console => {
                self.has_unemitted_console_domain(console_message_count, lifecycle_error_count)
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn console_domain_cursor(self) -> (usize, usize) {
        (
            self.console_domain_entries,
            self.console_domain_exception_entries,
        )
    }

    pub(crate) fn pending_cursor(
        self,
        domain: TargetConsoleOutputDomain,
        console_message_count: usize,
        lifecycle_error_count: usize,
    ) -> Option<TargetConsoleOutputCursor> {
        self.has_unemitted(domain, console_message_count, lifecycle_error_count)
            .then_some(self.cursor(domain))
    }

    fn cursor(self, domain: TargetConsoleOutputDomain) -> TargetConsoleOutputCursor {
        match domain {
            TargetConsoleOutputDomain::Console => TargetConsoleOutputCursor {
                console_start: self.console_domain_entries,
                lifecycle_start: self.console_domain_exception_entries,
            },
        }
    }

    #[cfg(test)]
    pub(crate) fn console_domain_emission_snapshot(
        self,
        console_messages: &[String],
        lifecycle_errors: &[String],
    ) -> ConsoleLogEmissionSnapshot {
        self.emission_snapshot(
            self.console_domain_entries,
            self.console_domain_exception_entries,
            console_messages,
            lifecycle_errors,
        )
    }

    #[cfg(test)]
    fn emission_snapshot(
        self,
        console_start: usize,
        lifecycle_start: usize,
        console_messages: &[String],
        lifecycle_errors: &[String],
    ) -> ConsoleLogEmissionSnapshot {
        ConsoleLogEmissionSnapshot {
            console_start,
            lifecycle_start,
            console_messages: console_messages
                .iter()
                .skip(console_start)
                .cloned()
                .collect(),
            lifecycle_errors: lifecycle_errors
                .iter()
                .skip(lifecycle_start)
                .cloned()
                .collect(),
        }
    }

    pub(crate) fn advance_console_domain_to_current(
        &mut self,
        console_entries: usize,
        exception_entries: usize,
    ) {
        self.advance_to_current(
            TargetConsoleOutputDomain::Console,
            console_entries,
            exception_entries,
        );
    }

    pub(crate) fn advance_to_current(
        &mut self,
        domain: TargetConsoleOutputDomain,
        console_entries: usize,
        lifecycle_entries: usize,
    ) {
        match domain {
            TargetConsoleOutputDomain::Console => {
                self.console_domain_entries = console_entries;
                self.console_domain_exception_entries = lifecycle_entries;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TargetConsoleOutputDomain, TargetConsoleOutputState};

    #[test]
    fn console_output_state_tracks_console_domain_snapshot() {
        let mut state = TargetConsoleOutputState::default();
        let console_messages = vec![
            "old log".to_owned(),
            "new log".to_owned(),
            "new warn".to_owned(),
        ];
        let lifecycle_errors = vec!["old error".to_owned(), "new error".to_owned()];

        assert!(state.has_unemitted(TargetConsoleOutputDomain::Console, 3, 2));
        let console_cursor = state
            .pending_cursor(TargetConsoleOutputDomain::Console, 3, 2)
            .expect("console domain should still have pending output");
        assert_eq!(console_cursor.console_start(), 0);
        assert_eq!(console_cursor.lifecycle_start(), 0);

        let console_snapshot =
            state.console_domain_emission_snapshot(&console_messages, &lifecycle_errors);
        assert_eq!(
            console_snapshot.console_messages(),
            ["old log", "new log", "new warn"]
        );
        assert_eq!(
            console_snapshot.lifecycle_errors(),
            ["old error", "new error"]
        );

        state.advance_to_current(
            TargetConsoleOutputDomain::Console,
            console_snapshot.console_end(),
            console_snapshot.lifecycle_end(),
        );
        assert!(!state.has_unemitted(TargetConsoleOutputDomain::Console, 3, 2));
    }
}
