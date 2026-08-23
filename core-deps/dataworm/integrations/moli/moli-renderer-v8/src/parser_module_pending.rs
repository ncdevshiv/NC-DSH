#[derive(Debug)]
pub(crate) struct ParserPendingModuleScriptState<T> {
    terminal: Option<T>,
    watching_for_load: bool,
}

impl<T> ParserPendingModuleScriptState<T> {
    pub(crate) fn new() -> Self {
        Self {
            terminal: None,
            watching_for_load: false,
        }
    }

    pub(crate) fn mark_watching_for_load(&mut self) {
        self.watching_for_load = true;
    }

    #[cfg(test)]
    pub(crate) fn is_watching_for_load(&self) -> bool {
        self.watching_for_load
    }

    pub(crate) fn notify_module_tree_load_finished(&mut self, terminal: T) {
        self.terminal = Some(terminal);
    }

    pub(crate) fn has_terminal(&self) -> bool {
        self.terminal.is_some()
    }

    pub(crate) fn has_ready_terminal(&self) -> bool {
        self.watching_for_load && self.has_terminal()
    }

    pub(crate) fn take_terminal(&mut self) -> Option<T> {
        self.terminal.take()
    }

    pub(crate) fn take_ready_terminal(&mut self) -> Option<T> {
        if !self.watching_for_load {
            return None;
        }
        self.take_terminal()
    }
}
