use std::{future::Future, sync::Arc};

/// Executor selected by a concrete resource-loading authority.
///
/// Resource code must never rely on whichever Tokio runtime happens to be
/// entered when a DOM callback runs. A committed Document or live Worker
/// captures its runner once, then every asynchronous load spawned for that
/// execution context uses the captured runner.
#[derive(Clone)]
pub struct RendererResourceTaskRunner {
    handle: Arc<tokio::runtime::Handle>,
}

impl RendererResourceTaskRunner {
    /// Captures the runtime that owns a production execution context.
    pub fn from_current_tokio() -> anyhow::Result<Self> {
        let handle = tokio::runtime::Handle::try_current().map_err(|error| {
            anyhow::anyhow!("resource authority requires a Tokio runtime: {error}")
        })?;
        Ok(Self::from_tokio_handle(handle))
    }

    pub(crate) fn from_tokio_handle(handle: tokio::runtime::Handle) -> Self {
        Self {
            handle: Arc::new(handle),
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test() -> Self {
        static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
        let runtime = RUNTIME.get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("resource test runtime should initialize")
        });
        Self::from_tokio_handle(runtime.handle().clone())
    }

    pub(crate) fn spawn(&self, task: impl Future<Output = ()> + Send + 'static) {
        drop(self.spawn_abortable(task));
    }

    pub(crate) fn spawn_abortable(
        &self,
        task: impl Future<Output = ()> + Send + 'static,
    ) -> tokio::task::JoinHandle<()> {
        self.handle.spawn(task)
    }

    pub(crate) fn shares_executor_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.handle, &other.handle)
    }
}

impl std::fmt::Debug for RendererResourceTaskRunner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RendererResourceTaskRunner")
            .field("kind", &"tokio")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn captured_tokio_runner_executes_resource_work() {
        let runner = RendererResourceTaskRunner::from_current_tokio()
            .expect("Tokio test should expose its runtime");
        let (completed_tx, completed_rx) = tokio::sync::oneshot::channel();

        runner.spawn(async move {
            let _ = completed_tx.send(());
        });

        completed_rx
            .await
            .expect("captured resource runner should poll spawned work");
    }
}
