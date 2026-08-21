use super::*;

#[derive(Clone)]

pub struct WorkerResultSender {
    sender: mpsc::Sender<WorkerResultBox>,
    repaint_context: Option<egui::Context>,
}

impl WorkerResultSender {
    pub(crate) fn new(
        sender: mpsc::Sender<WorkerResultBox>,
        repaint_context: Option<egui::Context>,
    ) -> Self {
        Self {
            sender,
            repaint_context,
        }
    }

    /// Await queue capacity instead of blocking a Tokio worker thread. The await is cancellation
    /// safe, so tearing down a watcher always releases it even while the UI is busy.
    pub async fn send<R: WorkerResult + 'static>(
        &self,
        result: R,
    ) -> Result<(), mpsc::error::SendError<WorkerResultBox>> {
        self.send_box(Box::new(result)).await
    }

    pub async fn send_box(
        &self,
        result: WorkerResultBox,
    ) -> Result<(), mpsc::error::SendError<WorkerResultBox>> {
        self.sender.send(result).await?;
        if let Some(context) = &self.repaint_context {
            context.request_repaint();
        }
        Ok(())
    }
}

/// Kubernetes API status information retained for YAML editor feedback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceApiError {
    pub message: String,
    pub causes: Vec<ResourceApiErrorCause>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceApiErrorCause {
    pub field: String,
    pub message: String,
    pub reason: String,
}
