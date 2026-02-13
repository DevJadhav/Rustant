//! Scheduler state persistence — save and load cron/job state across sessions.

use crate::error::SchedulerError;
use crate::scheduler::{CronScheduler, JobManager};
use std::path::Path;
use tracing::{info, warn};

/// Save scheduler state to the given directory.
pub fn save_state(
    scheduler: &CronScheduler,
    job_manager: &JobManager,
    state_dir: &Path,
) -> Result<(), SchedulerError> {
    std::fs::create_dir_all(state_dir).map_err(|e| SchedulerError::PersistenceError {
        message: format!("Failed to create state directory: {}", e),
    })?;

    // Save cron state
    let cron_json = scheduler.to_json()?;
    let cron_path = state_dir.join("cron_state.json");
    let cron_tmp = state_dir.join("cron_state.json.tmp");
    std::fs::write(&cron_tmp, &cron_json).map_err(|e| SchedulerError::PersistenceError {
        message: format!("Failed to write cron state: {}", e),
    })?;
    std::fs::rename(&cron_tmp, &cron_path).map_err(|e| SchedulerError::PersistenceError {
        message: format!("Failed to rename cron state file: {}", e),
    })?;

    // Save job state
    let job_json = job_manager.to_json()?;
    let job_path = state_dir.join("jobs_state.json");
    let job_tmp = state_dir.join("jobs_state.json.tmp");
    std::fs::write(&job_tmp, &job_json).map_err(|e| SchedulerError::PersistenceError {
        message: format!("Failed to write job state: {}", e),
    })?;
    std::fs::rename(&job_tmp, &job_path).map_err(|e| SchedulerError::PersistenceError {
        message: format!("Failed to rename job state file: {}", e),
    })?;

    info!("Scheduler state saved to {:?}", state_dir);
    Ok(())
}

/// Load scheduler state from the given directory.
pub fn load_state(state_dir: &Path) -> (CronScheduler, JobManager) {
    let cron_path = state_dir.join("cron_state.json");
    let scheduler = if cron_path.exists() {
        match std::fs::read_to_string(&cron_path) {
            Ok(json) => match CronScheduler::from_json(&json) {
                Ok(s) => {
                    info!("Loaded {} cron jobs from state", s.len());
                    s
                }
                Err(e) => {
                    warn!("Failed to parse cron state: {}, starting fresh", e);
                    CronScheduler::new()
                }
            },
            Err(e) => {
                warn!("Failed to read cron state: {}, starting fresh", e);
                CronScheduler::new()
            }
        }
    } else {
        CronScheduler::new()
    };

    let job_path = state_dir.join("jobs_state.json");
    let job_manager = if job_path.exists() {
        match std::fs::read_to_string(&job_path) {
            Ok(json) => match JobManager::from_json(&json) {
                Ok(jm) => {
                    info!("Loaded {} jobs from state", jm.len());
                    jm
                }
                Err(e) => {
                    warn!("Failed to parse job state: {}, starting fresh", e);
                    JobManager::new(10)
                }
            },
            Err(e) => {
                warn!("Failed to read job state: {}, starting fresh", e);
                JobManager::new(10)
            }
        }
    } else {
        JobManager::new(10)
    };

    (scheduler, job_manager)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::CronJobConfig;
    use tempfile::TempDir;

    #[test]
    fn test_save_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let state_dir = dir.path();

        let mut scheduler = CronScheduler::new();
        scheduler
            .add_job(CronJobConfig::new("test", "0 0 9 * * * *", "morning task"))
            .unwrap();
        let mut job_manager = JobManager::new(10);
        let _ = job_manager.spawn("bg-job");

        save_state(&scheduler, &job_manager, state_dir).unwrap();

        let (loaded_scheduler, loaded_jm) = load_state(state_dir);
        assert_eq!(loaded_scheduler.len(), 1);
        assert!(loaded_scheduler.get_job("test").is_some());
        assert_eq!(loaded_jm.len(), 1);
    }

    #[test]
    fn test_load_missing_directory_returns_defaults() {
        let (scheduler, jm) = load_state(Path::new("/nonexistent/scheduler/state"));
        assert_eq!(scheduler.len(), 0);
        assert_eq!(jm.len(), 0);
    }

    #[test]
    fn test_save_creates_directory() {
        let dir = TempDir::new().unwrap();
        let state_dir = dir.path().join("nested").join("scheduler");

        let scheduler = CronScheduler::new();
        let job_manager = JobManager::new(5);
        save_state(&scheduler, &job_manager, &state_dir).unwrap();

        assert!(state_dir.join("cron_state.json").exists());
        assert!(state_dir.join("jobs_state.json").exists());
    }
}
