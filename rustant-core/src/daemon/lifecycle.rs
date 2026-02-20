//! Daemon lifecycle management — install/uninstall, state tracking.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Daemon process state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DaemonState {
    Starting,
    Running,
    ShuttingDown,
    Stopped,
}

impl std::fmt::Display for DaemonState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DaemonState::Starting => write!(f, "starting"),
            DaemonState::Running => write!(f, "running"),
            DaemonState::ShuttingDown => write!(f, "shutting_down"),
            DaemonState::Stopped => write!(f, "stopped"),
        }
    }
}

/// Check if the daemon is currently running by reading the PID file.
pub fn check_daemon_running(base_dir: &std::path::Path) -> Option<u32> {
    let pid_file = base_dir.join("daemon.pid");
    if !pid_file.exists() {
        return None;
    }
    let pid_str = std::fs::read_to_string(&pid_file).ok()?;
    let pid = pid_str.trim().parse::<u32>().ok()?;

    #[cfg(unix)]
    {
        // Use raw syscall via std to avoid libc dependency.
        // Safety: signal 0 does not actually send a signal — it only checks
        // whether the process exists and we have permission to signal it.
        let alive = unsafe {
            unsafe extern "C" {
                fn kill(pid: i32, sig: i32) -> i32;
            }
            kill(pid as i32, 0) == 0
        };
        if alive {
            Some(pid)
        } else {
            // Stale PID file — remove it
            let _ = std::fs::remove_file(&pid_file);
            None
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        None
    }
}

/// Install a launchd plist for auto-start on macOS.
#[cfg(target_os = "macos")]
pub fn install_launchd_plist(rustant_bin: &std::path::Path) -> Result<PathBuf, std::io::Error> {
    let plist_dir = directories::BaseDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "No home directory"))?
        .join("Library/LaunchAgents");
    std::fs::create_dir_all(&plist_dir)?;

    let plist_path = plist_dir.join("com.rustant.daemon.plist");
    let bin_path = rustant_bin.display();

    let plist_content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.rustant.daemon</string>
    <key>ProgramArguments</key>
    <array>
        <string>{bin_path}</string>
        <string>daemon</string>
        <string>start</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
    <key>StandardOutPath</key>
    <string>/tmp/rustant-daemon.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/rustant-daemon.err</string>
</dict>
</plist>"#
    );

    std::fs::write(&plist_path, plist_content)?;
    Ok(plist_path)
}

/// Uninstall the launchd plist on macOS.
#[cfg(target_os = "macos")]
pub fn uninstall_launchd_plist() -> Result<(), std::io::Error> {
    let plist_path = directories::BaseDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "No home directory"))?
        .join("Library/LaunchAgents/com.rustant.daemon.plist");

    if plist_path.exists() {
        // Unload first
        let _ = std::process::Command::new("launchctl")
            .args(["unload", &plist_path.display().to_string()])
            .status();
        std::fs::remove_file(&plist_path)?;
    }
    Ok(())
}

/// Install a systemd user service on Linux.
#[cfg(target_os = "linux")]
pub fn install_systemd_service(rustant_bin: &std::path::Path) -> Result<PathBuf, std::io::Error> {
    let service_dir = directories::BaseDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "No home directory"))?
        .join(".config/systemd/user");
    std::fs::create_dir_all(&service_dir)?;

    let service_path = service_dir.join("rustant.service");
    let bin_path = rustant_bin.display();

    let service_content = format!(
        r#"[Unit]
Description=Rustant Daemon
After=default.target

[Service]
ExecStart={bin_path} daemon start
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
"#
    );

    std::fs::write(&service_path, service_content)?;
    Ok(service_path)
}

/// Uninstall the systemd user service on Linux.
#[cfg(target_os = "linux")]
pub fn uninstall_systemd_service() -> Result<(), std::io::Error> {
    let service_path = directories::BaseDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "No home directory"))?
        .join(".config/systemd/user/rustant.service");

    if service_path.exists() {
        // Stop and disable the service first
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "stop", "rustant.service"])
            .status();
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "disable", "rustant.service"])
            .status();
        std::fs::remove_file(&service_path)?;
        // Reload systemd to pick up the removal
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_state_display() {
        assert_eq!(DaemonState::Running.to_string(), "running");
        assert_eq!(DaemonState::Stopped.to_string(), "stopped");
    }

    #[test]
    fn test_check_daemon_not_running() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(check_daemon_running(tmp.path()).is_none());
    }
}
