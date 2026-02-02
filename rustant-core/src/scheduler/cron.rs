//! Cron scheduler — parses cron expressions and manages scheduled jobs.

use chrono::{DateTime, Utc};
use cron::Schedule;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;

use crate::error::SchedulerError;

/// Configuration for a single cron job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJobConfig {
    /// Unique name for the job.
    pub name: String,
    /// Cron expression (e.g., "0 0 9 * * MON-FRI *").
    pub schedule: String,
    /// Optional timezone (e.g., "America/New_York").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    /// The task to execute (agent task string or workflow name).
    pub task: String,
    /// Whether the job is enabled.
    pub enabled: bool,
}

impl CronJobConfig {
    pub fn new(
        name: impl Into<String>,
        schedule: impl Into<String>,
        task: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            schedule: schedule.into(),
            timezone: None,
            task: task.into(),
            enabled: true,
        }
    }
}

/// A parsed and tracked cron job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    /// The job configuration.
    pub config: CronJobConfig,
    /// Last time this job was executed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run: Option<DateTime<Utc>>,
    /// Next scheduled execution time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_run: Option<DateTime<Utc>>,
    /// How many times this job has been executed.
    pub run_count: usize,
}

impl CronJob {
    /// Create a new cron job from a config, parsing the cron expression.
    pub fn new(config: CronJobConfig) -> Result<Self, SchedulerError> {
        // Validate the expression by parsing it
        let _schedule = parse_cron_expression(&config.schedule)?;

        let mut job = Self {
            config,
            last_run: None,
            next_run: None,
            run_count: 0,
        };
        job.calculate_next_run();
        Ok(job)
    }

    /// Recalculate the next run time based on the cron expression.
    pub fn calculate_next_run(&mut self) {
        if let Ok(schedule) = parse_cron_expression(&self.config.schedule) {
            let from = self.last_run.unwrap_or_else(Utc::now);
            self.next_run = schedule.after(&from).next();
        }
    }

    /// Check if this job is due to run (next_run <= now).
    pub fn is_due(&self) -> bool {
        if !self.config.enabled {
            return false;
        }
        match self.next_run {
            Some(next) => next <= Utc::now(),
            None => false,
        }
    }

    /// Mark the job as having just run.
    pub fn mark_executed(&mut self) {
        self.last_run = Some(Utc::now());
        self.run_count += 1;
        self.calculate_next_run();
    }
}

/// Parse a cron expression string into a Schedule.
fn parse_cron_expression(expr: &str) -> Result<Schedule, SchedulerError> {
    Schedule::from_str(expr).map_err(|e| SchedulerError::InvalidCronExpression {
        expression: expr.to_string(),
        message: e.to_string(),
    })
}

/// Manages a collection of cron jobs.
#[derive(Debug, Default)]
pub struct CronScheduler {
    jobs: HashMap<String, CronJob>,
}

impl CronScheduler {
    pub fn new() -> Self {
        Self {
            jobs: HashMap::new(),
        }
    }

    /// Add a job to the scheduler.
    pub fn add_job(&mut self, config: CronJobConfig) -> Result<(), SchedulerError> {
        if self.jobs.contains_key(&config.name) {
            return Err(SchedulerError::JobAlreadyExists {
                name: config.name.clone(),
            });
        }
        let job = CronJob::new(config.clone())?;
        self.jobs.insert(config.name, job);
        Ok(())
    }

    /// Remove a job by name.
    pub fn remove_job(&mut self, name: &str) -> Result<(), SchedulerError> {
        if self.jobs.remove(name).is_none() {
            return Err(SchedulerError::JobNotFound {
                name: name.to_string(),
            });
        }
        Ok(())
    }

    /// Disable a job by name.
    pub fn disable_job(&mut self, name: &str) -> Result<(), SchedulerError> {
        let job = self
            .jobs
            .get_mut(name)
            .ok_or_else(|| SchedulerError::JobNotFound {
                name: name.to_string(),
            })?;
        job.config.enabled = false;
        Ok(())
    }

    /// Enable a job by name.
    pub fn enable_job(&mut self, name: &str) -> Result<(), SchedulerError> {
        let job = self
            .jobs
            .get_mut(name)
            .ok_or_else(|| SchedulerError::JobNotFound {
                name: name.to_string(),
            })?;
        job.config.enabled = true;
        job.calculate_next_run();
        Ok(())
    }

    /// Get all jobs that are currently due to run.
    pub fn due_jobs(&self) -> Vec<&CronJob> {
        self.jobs.values().filter(|j| j.is_due()).collect()
    }

    /// Mark a job as executed.
    pub fn mark_executed(&mut self, name: &str) -> Result<(), SchedulerError> {
        let job = self
            .jobs
            .get_mut(name)
            .ok_or_else(|| SchedulerError::JobNotFound {
                name: name.to_string(),
            })?;
        job.mark_executed();
        Ok(())
    }

    /// Get a job by name.
    pub fn get_job(&self, name: &str) -> Option<&CronJob> {
        self.jobs.get(name)
    }

    /// List all job names.
    pub fn list_jobs(&self) -> Vec<&CronJob> {
        self.jobs.values().collect()
    }

    /// Number of jobs.
    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    /// Whether the scheduler has no jobs.
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    /// Serialize the scheduler state to JSON.
    pub fn to_json(&self) -> Result<String, SchedulerError> {
        serde_json::to_string_pretty(&self.jobs).map_err(|e| SchedulerError::PersistenceError {
            message: e.to_string(),
        })
    }

    /// Deserialize the scheduler state from JSON.
    pub fn from_json(json: &str) -> Result<Self, SchedulerError> {
        let jobs: HashMap<String, CronJob> =
            serde_json::from_str(json).map_err(|e| SchedulerError::PersistenceError {
                message: e.to_string(),
            })?;
        Ok(Self { jobs })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cron_job_parse_valid_expression() {
        let config = CronJobConfig::new("test", "0 0 9 * * MON-FRI *", "check updates");
        let job = CronJob::new(config).unwrap();
        assert_eq!(job.config.name, "test");
        assert!(job.next_run.is_some());
        assert_eq!(job.run_count, 0);
    }

    #[test]
    fn test_cron_job_parse_invalid_expression() {
        let config = CronJobConfig::new("bad", "not a cron", "task");
        let result = CronJob::new(config);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not a cron"));
    }

    #[test]
    fn test_cron_job_next_run_calculation() {
        let config = CronJobConfig::new("every_min", "0 * * * * * *", "ping");
        let job = CronJob::new(config).unwrap();
        // Next run should be within the next 60 seconds
        let next = job.next_run.unwrap();
        let diff = next - Utc::now();
        assert!(diff.num_seconds() <= 60);
        assert!(diff.num_seconds() >= 0);
    }

    #[test]
    fn test_cron_scheduler_add_job() {
        let mut scheduler = CronScheduler::new();
        let config = CronJobConfig::new("daily", "0 0 9 * * * *", "morning task");
        scheduler.add_job(config).unwrap();
        assert_eq!(scheduler.len(), 1);
        assert!(scheduler.get_job("daily").is_some());
    }

    #[test]
    fn test_cron_scheduler_add_duplicate_fails() {
        let mut scheduler = CronScheduler::new();
        let config = CronJobConfig::new("daily", "0 0 9 * * * *", "task");
        scheduler.add_job(config.clone()).unwrap();
        let result = scheduler.add_job(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_cron_scheduler_remove_job() {
        let mut scheduler = CronScheduler::new();
        scheduler
            .add_job(CronJobConfig::new("temp", "0 0 9 * * * *", "task"))
            .unwrap();
        assert_eq!(scheduler.len(), 1);
        scheduler.remove_job("temp").unwrap();
        assert_eq!(scheduler.len(), 0);
    }

    #[test]
    fn test_cron_scheduler_remove_nonexistent() {
        let mut scheduler = CronScheduler::new();
        assert!(scheduler.remove_job("ghost").is_err());
    }

    #[test]
    fn test_cron_scheduler_disable_job() {
        let mut scheduler = CronScheduler::new();
        scheduler
            .add_job(CronJobConfig::new("j", "0 0 9 * * * *", "task"))
            .unwrap();
        scheduler.disable_job("j").unwrap();
        let job = scheduler.get_job("j").unwrap();
        assert!(!job.config.enabled);
        assert!(!job.is_due());
    }

    #[test]
    fn test_cron_scheduler_enable_job() {
        let mut scheduler = CronScheduler::new();
        let mut config = CronJobConfig::new("j", "0 0 9 * * * *", "task");
        config.enabled = false;
        scheduler.add_job(config).unwrap();
        scheduler.enable_job("j").unwrap();
        let job = scheduler.get_job("j").unwrap();
        assert!(job.config.enabled);
    }

    #[test]
    fn test_cron_scheduler_due_jobs() {
        let mut scheduler = CronScheduler::new();
        // Job every second — should be due immediately
        scheduler
            .add_job(CronJobConfig::new("fast", "* * * * * * *", "task"))
            .unwrap();
        // Give it a moment for the next_run to be in the past
        // Since "every second" means next_run is ≤1s from now, wait briefly
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let due = scheduler.due_jobs();
        assert!(!due.is_empty());
    }

    #[test]
    fn test_cron_scheduler_state_serde() {
        let mut scheduler = CronScheduler::new();
        scheduler
            .add_job(CronJobConfig::new("a", "0 0 9 * * * *", "task a"))
            .unwrap();
        scheduler
            .add_job(CronJobConfig::new("b", "0 0 12 * * * *", "task b"))
            .unwrap();
        let json = scheduler.to_json().unwrap();
        let restored = CronScheduler::from_json(&json).unwrap();
        assert_eq!(restored.len(), 2);
        assert!(restored.get_job("a").is_some());
        assert!(restored.get_job("b").is_some());
    }

    #[test]
    fn test_cron_job_config_serde() {
        let config = CronJobConfig::new("test", "0 0 9 * * * *", "my task");
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: CronJobConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "test");
        assert_eq!(deserialized.schedule, "0 0 9 * * * *");
        assert_eq!(deserialized.task, "my task");
        assert!(deserialized.enabled);
    }
}
