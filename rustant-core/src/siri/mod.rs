//! Siri integration for macOS.
//!
//! Provides a bridge between Siri Shortcuts and the Rustant daemon,
//! enabling voice-controlled agent interaction via "Hey Siri" triggers.
//!
//! Activation model:
//! 1. "Hey Siri, activate Rustant" → starts daemon + sets active flag
//! 2. All Siri shortcuts check the active flag before routing to daemon
//! 3. "Hey Siri, deactivate Rustant" → clears flag, optionally stops daemon

pub mod bridge;
pub mod responder;
pub mod shortcuts;

pub use bridge::SiriBridge;
pub use responder::SiriResponder;
pub use shortcuts::ShortcutGenerator;
