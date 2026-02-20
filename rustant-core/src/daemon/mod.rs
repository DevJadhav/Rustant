//! Rustant background daemon.
//!
//! Runs Rustant as a persistent background process with IPC, job queue,
//! warm MoE cache, and active session. Supports auto-start via launchd (macOS)
//! or systemd (Linux).

pub mod ipc;
pub mod lifecycle;
pub mod process;

pub use ipc::{IpcMessage, IpcServer};
pub use lifecycle::DaemonState;
pub use process::RustantDaemon;
