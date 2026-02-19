//! AI Security pillar — adversarial detection, provenance, red teaming.

pub mod adversarial;
pub mod attacks;
pub mod exfiltration;
pub mod provenance;
pub mod red_team;

pub use attacks::{EnhancedInjectionDetector, JailbreakDetector};
pub use exfiltration::DataExfiltrationDetector;
pub use provenance::ProvenanceVerifier;
pub use red_team::RedTeamEngine;
