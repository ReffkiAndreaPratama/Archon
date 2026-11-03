use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

use crate::error::{ArchonError, ArchonResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProgressPhase {
    Scanning,
    Compressing,
    Extracting,
    Testing,
    Finalizing,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub job_id: String,
    pub phase: ProgressPhase,
    pub current: usize,
    pub total: usize,
    pub path: Option<String>,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

pub type ProgressReporter = Arc<dyn Fn(Progress) + Send + Sync>;

/// Cancellation token and progress reporter shared by every long-running
/// operation. `job_id` lets the desktop app correlate events with a job.
#[derive(Clone)]
pub struct OperationContext {
    job_id: String,
    cancel: Arc<AtomicBool>,
    reporter: ProgressReporter,
}

impl OperationContext {
    pub fn new(job_id: impl Into<String>) -> Self {
        Self {
            job_id: job_id.into(),
            cancel: Arc::new(AtomicBool::new(false)),
            reporter: Arc::new(|_| {}),
        }
    }

    pub fn with_cancel(job_id: impl Into<String>, cancel: Arc<AtomicBool>) -> Self {
        Self {
            job_id: job_id.into(),
            cancel,
            reporter: Arc::new(|_| {}),
        }
    }

    pub fn with_reporter<F>(mut self, reporter: F) -> Self
    where
        F: Fn(Progress) + Send + Sync + 'static,
    {
        self.reporter = Arc::new(reporter);
        self
    }

    pub fn job_id(&self) -> &str {
        &self.job_id
    }

    pub fn cancel_token(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel)
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    pub fn check_cancelled(&self) -> ArchonResult<()> {
        if self.is_cancelled() {
            Err(ArchonError::Cancelled)
        } else {
            Ok(())
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn report(
        &self,
        phase: ProgressPhase,
        current: usize,
        total: usize,
        path: Option<&str>,
        bytes_done: u64,
        bytes_total: u64,
    ) {
        (self.reporter)(Progress {
            job_id: self.job_id.clone(),
            phase,
            current,
            total,
            path: path.map(str::to_owned),
            bytes_done,
            bytes_total,
        });
    }
}
