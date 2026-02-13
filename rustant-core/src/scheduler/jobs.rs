//! Background job manager — spawn, track, and cancel long-running tasks.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::error::SchedulerError;

/// Status of a background job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobStatus::Pending => write!(f, "pending"),
            JobStatus::Running => write!(f, "running"),
            JobStatus::Completed => write!(f, "completed"),
            JobStatus::Failed => write!(f, "failed"),
            JobStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// A background job instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundJob {
    pub id: Uuid,
    pub name: String,
    pub status: JobStatus,
    pub started_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl BackgroundJob {
    /// Create a new pending background job.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            status: JobStatus::Pending,
            started_at: Utc::now(),
            completed_at: None,
            result: None,
            error: None,
        }
    }

    /// Mark the job as running.
    pub fn start(&mut self) {
        self.status = JobStatus::Running;
        self.started_at = Utc::now();
    }

    /// Mark the job as completed with a result.
    pub fn complete(&mut self, result: impl Into<String>) {
        self.status = JobStatus::Completed;
        self.completed_at = Some(Utc::now());
        self.result = Some(result.into());
    }

    /// Mark the job as failed with an error.
    pub fn fail(&mut self, error: impl Into<String>) {
        self.status = JobStatus::Failed;
        self.completed_at = Some(Utc::now());
        self.error = Some(error.into());
    }

    /// Mark the job as cancelled.
    pub fn cancel(&mut self) {
        self.status = JobStatus::Cancelled;
        self.completed_at = Some(Utc::now());
    }

    /// Whether the job is in a terminal state.
    pub fn is_finished(&self) -> bool {
        matches!(
            self.status,
            JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled
        )
    }
}

/// Manages background jobs with a configurable maximum.
pub struct JobManager {
    jobs: HashMap<Uuid, BackgroundJob>,
    max_jobs: usize,
}

impl JobManager {
    /// Create a new job manager with the given max concurrent jobs.
    pub fn new(max_jobs: usize) -> Self {
        Self {
            jobs: HashMap::new(),
            max_jobs,
        }
    }

    /// Spawn a new background job. Returns the job ID.
    pub fn spawn(&mut self, name: impl Into<String>) -> Result<Uuid, SchedulerError> {
        let active = self.active_count();
        if active >= self.max_jobs {
            return Err(SchedulerError::MaxJobsExceeded { max: self.max_jobs });
        }
        let mut job = BackgroundJob::new(name);
        job.start();
        let id = job.id;
        self.jobs.insert(id, job);
        Ok(id)
    }

    /// Get a job by ID.
    pub fn get(&self, id: &Uuid) -> Option<&BackgroundJob> {
        self.jobs.get(id)
    }

    /// Get a mutable reference to a job by ID.
    pub fn get_mut(&mut self, id: &Uuid) -> Option<&mut BackgroundJob> {
        self.jobs.get_mut(id)
    }

    /// Complete a job with a result.
    pub fn complete_job(
        &mut self,
        id: &Uuid,
        result: impl Into<String>,
    ) -> Result<(), SchedulerError> {
        let job = self
            .jobs
            .get_mut(id)
            .ok_or(SchedulerError::BackgroundJobNotFound { id: *id })?;
        job.complete(result);
        Ok(())
    }

    /// Fail a job with an error.
    pub fn fail_job(&mut self, id: &Uuid, error: impl Into<String>) -> Result<(), SchedulerError> {
        let job = self
            .jobs
            .get_mut(id)
            .ok_or(SchedulerError::BackgroundJobNotFound { id: *id })?;
        job.fail(error);
        Ok(())
    }

    /// Cancel a job.
    pub fn cancel_job(&mut self, id: &Uuid) -> Result<(), SchedulerError> {
        let job = self
            .jobs
            .get_mut(id)
            .ok_or(SchedulerError::BackgroundJobNotFound { id: *id })?;
        job.cancel();
        Ok(())
    }

    /// List all jobs.
    pub fn list(&self) -> Vec<&BackgroundJob> {
        self.jobs.values().collect()
    }

    /// Number of active (non-finished) jobs.
    pub fn active_count(&self) -> usize {
        self.jobs.values().filter(|j| !j.is_finished()).count()
    }

    /// Total number of jobs (including finished).
    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    /// Whether the job manager has no jobs.
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    /// Serialize the job manager state to JSON.
    pub fn to_json(&self) -> Result<String, SchedulerError> {
        let jobs: Vec<&BackgroundJob> = self.jobs.values().collect();
        let state = serde_json::json!({
            "max_jobs": self.max_jobs,
            "jobs": jobs,
        });
        serde_json::to_string_pretty(&state).map_err(|e| SchedulerError::PersistenceError {
            message: e.to_string(),
        })
    }

    /// Deserialize the job manager from JSON.
    pub fn from_json(json: &str) -> Result<Self, SchedulerError> {
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|e| SchedulerError::PersistenceError {
                message: e.to_string(),
            })?;
        let max_jobs = value.get("max_jobs").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
        let jobs_arr: Vec<BackgroundJob> = value
            .get("jobs")
            .map(|v| serde_json::from_value(v.clone()).unwrap_or_default())
            .unwrap_or_default();
        let mut jobs_map = HashMap::new();
        for job in jobs_arr {
            jobs_map.insert(job.id, job);
        }
        Ok(Self {
            jobs: jobs_map,
            max_jobs,
        })
    }

    /// Clean up finished jobs.
    pub fn cleanup_finished(&mut self) {
        self.jobs.retain(|_, j| !j.is_finished());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_background_job_creation() {
        let job = BackgroundJob::new("test-job");
        assert_eq!(job.name, "test-job");
        assert_eq!(job.status, JobStatus::Pending);
        assert!(job.completed_at.is_none());
        assert!(job.result.is_none());
        assert!(job.error.is_none());
        assert!(!job.is_finished());
    }

    #[test]
    fn test_background_job_lifecycle() {
        let mut job = BackgroundJob::new("lifecycle");
        assert_eq!(job.status, JobStatus::Pending);

        job.start();
        assert_eq!(job.status, JobStatus::Running);
        assert!(!job.is_finished());

        job.complete("done!");
        assert_eq!(job.status, JobStatus::Completed);
        assert!(job.is_finished());
        assert_eq!(job.result, Some("done!".to_string()));
        assert!(job.completed_at.is_some());
    }

    #[test]
    fn test_background_job_fail() {
        let mut job = BackgroundJob::new("fail-job");
        job.start();
        job.fail("something went wrong");
        assert_eq!(job.status, JobStatus::Failed);
        assert!(job.is_finished());
        assert_eq!(job.error, Some("something went wrong".to_string()));
    }

    #[test]
    fn test_background_job_cancel() {
        let mut job = BackgroundJob::new("cancel-job");
        job.start();
        job.cancel();
        assert_eq!(job.status, JobStatus::Cancelled);
        assert!(job.is_finished());
    }

    #[test]
    fn test_job_manager_spawn_job() {
        let mut manager = JobManager::new(10);
        let id = manager.spawn("job-1").unwrap();
        assert_eq!(manager.len(), 1);
        let job = manager.get(&id).unwrap();
        assert_eq!(job.name, "job-1");
        assert_eq!(job.status, JobStatus::Running);
    }

    #[test]
    fn test_job_manager_list_jobs() {
        let mut manager = JobManager::new(10);
        manager.spawn("a").unwrap();
        manager.spawn("b").unwrap();
        manager.spawn("c").unwrap();
        let jobs = manager.list();
        assert_eq!(jobs.len(), 3);
    }

    #[test]
    fn test_job_manager_cancel_job() {
        let mut manager = JobManager::new(10);
        let id = manager.spawn("to-cancel").unwrap();
        manager.cancel_job(&id).unwrap();
        let job = manager.get(&id).unwrap();
        assert_eq!(job.status, JobStatus::Cancelled);
    }

    #[test]
    fn test_job_manager_max_jobs_enforced() {
        let mut manager = JobManager::new(2);
        manager.spawn("a").unwrap();
        manager.spawn("b").unwrap();
        let result = manager.spawn("c");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("2"));
    }

    #[test]
    fn test_job_manager_completed_job_status() {
        let mut manager = JobManager::new(10);
        let id = manager.spawn("done-job").unwrap();
        manager.complete_job(&id, "success!").unwrap();
        let job = manager.get(&id).unwrap();
        assert_eq!(job.status, JobStatus::Completed);
        assert_eq!(job.result, Some("success!".to_string()));
    }

    #[test]
    fn test_job_manager_completed_frees_slot() {
        let mut manager = JobManager::new(2);
        let id1 = manager.spawn("a").unwrap();
        manager.spawn("b").unwrap();
        // At max
        assert!(manager.spawn("c").is_err());
        // Complete one
        manager.complete_job(&id1, "done").unwrap();
        // Now should have a slot
        assert!(manager.spawn("c").is_ok());
    }

    #[test]
    fn test_job_status_serde() {
        let statuses = vec![
            JobStatus::Pending,
            JobStatus::Running,
            JobStatus::Completed,
            JobStatus::Failed,
            JobStatus::Cancelled,
        ];
        for status in &statuses {
            let json = serde_json::to_string(status).unwrap();
            let deserialized: JobStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(*status, deserialized);
        }
    }

    #[test]
    fn test_job_status_display() {
        assert_eq!(JobStatus::Pending.to_string(), "pending");
        assert_eq!(JobStatus::Running.to_string(), "running");
        assert_eq!(JobStatus::Completed.to_string(), "completed");
        assert_eq!(JobStatus::Failed.to_string(), "failed");
        assert_eq!(JobStatus::Cancelled.to_string(), "cancelled");
    }

    #[test]
    fn test_job_manager_cleanup_finished() {
        let mut manager = JobManager::new(10);
        let id1 = manager.spawn("a").unwrap();
        let _id2 = manager.spawn("b").unwrap();
        manager.complete_job(&id1, "done").unwrap();
        assert_eq!(manager.len(), 2);
        manager.cleanup_finished();
        assert_eq!(manager.len(), 1);
    }

    #[test]
    fn test_job_manager_nonexistent_job() {
        let mut manager = JobManager::new(10);
        let fake_id = Uuid::new_v4();
        assert!(manager.cancel_job(&fake_id).is_err());
        assert!(manager.complete_job(&fake_id, "done").is_err());
        assert!(manager.fail_job(&fake_id, "err").is_err());
    }
}
