//! Browser Automation Module for Rustant.
//!
//! Provides a trait-based CDP (Chrome DevTools Protocol) abstraction for
//! browser automation with security guard, snapshot modes, and session management.

pub mod cdp;
pub mod persistence;
pub mod security;
pub mod session;
pub mod snapshot;

#[cfg(feature = "browser")]
pub mod chromium;

pub use cdp::{CdpClient, MockCdpClient, TabInfo};
pub use persistence::{BrowserConnectionInfo, BrowserSessionStore};
pub use security::BrowserSecurityGuard;
pub use session::BrowserSession;
pub use snapshot::{PageSnapshot, SnapshotMode};

#[cfg(feature = "browser")]
pub use chromium::ChromiumCdpClient;
