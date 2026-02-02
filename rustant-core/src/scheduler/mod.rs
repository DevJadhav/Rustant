//! Scheduling Module for Rustant.
//!
//! Provides cron-based scheduling, heartbeat triggers with cooldowns and quiet hours,
//! webhook endpoints with HMAC verification, and background job management.

pub mod cron;
pub mod heartbeat;
pub mod jobs;
pub mod webhook;

pub use cron::{CronJob, CronJobConfig, CronScheduler};
pub use heartbeat::{HeartbeatConfig, HeartbeatManager, HeartbeatTask, QuietHours};
pub use jobs::{BackgroundJob, JobManager, JobStatus};
pub use webhook::{
    compute_hmac_signature, WebhookEndpoint, WebhookHandler, WebhookRequest, WebhookResult,
};
