use std::sync::Arc;

use parking_lot::Mutex;
use serde::Serialize;

use super::phases::JobPhase;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct JobProgressSnapshot {
    pub job_id: String,
    pub phase: String,
    pub progress: u8,
    pub current_step: String,
}

#[derive(Clone, Default)]
pub struct JobProgressTracker {
    inner: Arc<Mutex<Option<JobProgressSnapshot>>>,
}

impl JobProgressTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, job_id: &str, phase: JobPhase, step: &str) {
        *self.inner.lock() = Some(JobProgressSnapshot {
            job_id: job_id.to_string(),
            phase: phase.status_str().to_string(),
            progress: phase.progress(),
            current_step: step.to_string(),
        });
    }

    pub fn clear(&self) {
        *self.inner.lock() = None;
    }

    pub fn snapshot(&self) -> Option<JobProgressSnapshot> {
        self.inner.lock().clone()
    }
}

#[cfg(test)]
mod tracker_tests {
    use super::*;

    #[test]
    fn tracks_and_clears_progress() {
        let tracker = JobProgressTracker::new();
        tracker.set("job-1", JobPhase::FillingFields, "Filling listing fields");
        let snap = tracker.snapshot().unwrap();
        assert_eq!(snap.phase, "fields_filling");
        assert_eq!(snap.progress, 88);
        tracker.clear();
        assert!(tracker.snapshot().is_none());
    }
}
