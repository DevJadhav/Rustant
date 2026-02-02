//! # DM Pairing Protocol
//!
//! Implements a challenge-response device pairing protocol for authenticated
//! direct-message communication between the agent and trusted devices.
//!
//! The flow:
//! 1. Device requests pairing via [`PairingManager::create_challenge`].
//! 2. Agent presents a challenge (nonce) to the user.
//! 3. Device computes an HMAC response over the nonce using the shared secret.
//! 4. Agent verifies the response via [`PairingManager::verify_response`].
//! 5. On success the device is added to the paired-devices list.

use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Identity of a paired device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceIdentity {
    pub device_id: Uuid,
    pub device_name: String,
    pub public_key: String,
    pub created_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

/// A challenge issued to a device during the pairing flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingChallenge {
    pub challenge_id: Uuid,
    pub nonce: String,
    pub expires_at: DateTime<Utc>,
}

/// A device's response to a pairing challenge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingResponse {
    pub challenge_id: Uuid,
    pub device_id: Uuid,
    pub device_name: String,
    pub public_key: String,
    pub response_hmac: String,
}

/// Outcome of a pairing attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PairingResult {
    Accepted,
    Rejected,
    Expired,
}

/// TOTP-style code generator and verifier (time-based one-time password).
#[derive(Debug, Clone)]
pub struct TotpVerifier {
    secret: Vec<u8>,
    /// Period in seconds for each TOTP code.
    period: u64,
    /// Number of digits in the TOTP code.
    digits: u32,
}

impl TotpVerifier {
    /// Create a new TOTP verifier with the given secret.
    pub fn new(secret: &[u8], period: u64, digits: u32) -> Self {
        Self {
            secret: secret.to_vec(),
            period,
            digits,
        }
    }

    /// Generate the current TOTP code.
    pub fn generate(&self) -> String {
        self.generate_at(Utc::now().timestamp() as u64)
    }

    /// Generate a TOTP code for a specific Unix timestamp.
    pub fn generate_at(&self, timestamp: u64) -> String {
        let counter = timestamp / self.period;
        let counter_bytes = counter.to_be_bytes();

        let mut mac =
            HmacSha256::new_from_slice(&self.secret).expect("HMAC can take key of any size");
        mac.update(&counter_bytes);
        let result = mac.finalize().into_bytes();

        // Dynamic truncation (simplified)
        let offset = (result[result.len() - 1] & 0x0f) as usize;
        let truncated = u32::from_be_bytes([
            result[offset] & 0x7f,
            result[offset + 1],
            result[offset + 2],
            result[offset + 3],
        ]);

        let code = truncated % 10u32.pow(self.digits);
        format!("{:0>width$}", code, width = self.digits as usize)
    }

    /// Verify a TOTP code, allowing for +-1 period drift.
    pub fn verify(&self, code: &str) -> bool {
        let now = Utc::now().timestamp() as u64;
        // Check current, previous, and next period
        for offset in [0i64, -1, 1] {
            let ts = (now as i64 + offset * self.period as i64) as u64;
            if self.generate_at(ts) == code {
                return true;
            }
        }
        false
    }

    /// Verify a TOTP code at a specific timestamp.
    pub fn verify_at(&self, code: &str, timestamp: u64) -> bool {
        for offset in [0i64, -1, 1] {
            let ts = (timestamp as i64 + offset * self.period as i64) as u64;
            if self.generate_at(ts) == code {
                return true;
            }
        }
        false
    }
}

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

/// Manages the device pairing lifecycle: challenge creation, verification,
/// device listing, and revocation.
#[derive(Debug, Clone, Default)]
pub struct PairingManager {
    shared_secret: Vec<u8>,
    paired_devices: Vec<DeviceIdentity>,
    pending_challenges: Vec<PairingChallenge>,
    /// Challenge validity duration in seconds.
    challenge_ttl_secs: i64,
}

impl PairingManager {
    /// Create a new pairing manager with the given shared secret.
    pub fn new(shared_secret: &[u8]) -> Self {
        Self {
            shared_secret: shared_secret.to_vec(),
            paired_devices: Vec::new(),
            pending_challenges: Vec::new(),
            challenge_ttl_secs: 300, // 5 minutes
        }
    }

    /// Create a new pairing challenge.
    pub fn create_challenge(&mut self) -> PairingChallenge {
        let mut rng = rand::thread_rng();
        let nonce_bytes: [u8; 32] = rng.gen();
        let nonce = hex::encode(nonce_bytes);

        let challenge = PairingChallenge {
            challenge_id: Uuid::new_v4(),
            nonce,
            expires_at: Utc::now() + Duration::seconds(self.challenge_ttl_secs),
        };

        self.pending_challenges.push(challenge.clone());
        challenge
    }

    /// Verify a pairing response against a pending challenge.
    pub fn verify_response(&mut self, response: &PairingResponse) -> PairingResult {
        // Find and remove the matching challenge
        let challenge_idx = self
            .pending_challenges
            .iter()
            .position(|c| c.challenge_id == response.challenge_id);

        let Some(idx) = challenge_idx else {
            return PairingResult::Rejected;
        };

        let challenge = self.pending_challenges.remove(idx);

        // Check expiry
        if Utc::now() > challenge.expires_at {
            return PairingResult::Expired;
        }

        // Verify HMAC: HMAC-SHA256(shared_secret, nonce)
        let expected = compute_hmac(&self.shared_secret, challenge.nonce.as_bytes());
        if expected != response.response_hmac {
            return PairingResult::Rejected;
        }

        // Add device
        let now = Utc::now();
        self.paired_devices.push(DeviceIdentity {
            device_id: response.device_id,
            device_name: response.device_name.clone(),
            public_key: response.public_key.clone(),
            created_at: now,
            last_seen: now,
        });

        PairingResult::Accepted
    }

    /// List all currently paired devices.
    pub fn paired_devices(&self) -> &[DeviceIdentity] {
        &self.paired_devices
    }

    /// Revoke a paired device by its ID.
    pub fn revoke_device(&mut self, device_id: &Uuid) -> bool {
        let before = self.paired_devices.len();
        self.paired_devices.retain(|d| d.device_id != *device_id);
        self.paired_devices.len() < before
    }

    /// Remove expired challenges.
    pub fn cleanup_expired(&mut self) {
        let now = Utc::now();
        self.pending_challenges.retain(|c| c.expires_at > now);
    }

    /// Number of pending (unexpired) challenges.
    pub fn pending_count(&self) -> usize {
        self.pending_challenges.len()
    }
}

/// Compute HMAC-SHA256 and return the hex-encoded result.
fn compute_hmac(key: &[u8], data: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(data);
    let result = mac.finalize().into_bytes();
    hex::encode(result)
}

// Tiny hex encoding helper (no external dep needed).
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes
            .as_ref()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_secret() -> Vec<u8> {
        b"test-shared-secret-key-32bytes!!".to_vec()
    }

    // -- Challenge generation -----------------------------------------------

    #[test]
    fn test_create_challenge() {
        let mut mgr = PairingManager::new(&test_secret());
        let challenge = mgr.create_challenge();

        assert!(!challenge.challenge_id.is_nil());
        assert!(!challenge.nonce.is_empty());
        assert!(challenge.expires_at > Utc::now());
        assert_eq!(mgr.pending_count(), 1);
    }

    #[test]
    fn test_create_multiple_challenges() {
        let mut mgr = PairingManager::new(&test_secret());
        mgr.create_challenge();
        mgr.create_challenge();
        mgr.create_challenge();
        assert_eq!(mgr.pending_count(), 3);
    }

    // -- Response verification ----------------------------------------------

    #[test]
    fn test_valid_pairing_flow() {
        let secret = test_secret();
        let mut mgr = PairingManager::new(&secret);
        let challenge = mgr.create_challenge();

        // Simulate device computing HMAC
        let response_hmac = compute_hmac(&secret, challenge.nonce.as_bytes());
        let response = PairingResponse {
            challenge_id: challenge.challenge_id,
            device_id: Uuid::new_v4(),
            device_name: "My Phone".into(),
            public_key: "pk-abc123".into(),
            response_hmac,
        };

        let result = mgr.verify_response(&response);
        assert_eq!(result, PairingResult::Accepted);
        assert_eq!(mgr.paired_devices().len(), 1);
        assert_eq!(mgr.paired_devices()[0].device_name, "My Phone");
    }

    #[test]
    fn test_wrong_hmac_rejected() {
        let mut mgr = PairingManager::new(&test_secret());
        let challenge = mgr.create_challenge();

        let response = PairingResponse {
            challenge_id: challenge.challenge_id,
            device_id: Uuid::new_v4(),
            device_name: "Evil Device".into(),
            public_key: "pk-evil".into(),
            response_hmac: "wrong-hmac".into(),
        };

        let result = mgr.verify_response(&response);
        assert_eq!(result, PairingResult::Rejected);
        assert!(mgr.paired_devices().is_empty());
    }

    #[test]
    fn test_unknown_challenge_rejected() {
        let mut mgr = PairingManager::new(&test_secret());

        let response = PairingResponse {
            challenge_id: Uuid::new_v4(), // no matching challenge
            device_id: Uuid::new_v4(),
            device_name: "Unknown".into(),
            public_key: "pk".into(),
            response_hmac: "whatever".into(),
        };

        let result = mgr.verify_response(&response);
        assert_eq!(result, PairingResult::Rejected);
    }

    #[test]
    fn test_expired_challenge() {
        let secret = test_secret();
        let mut mgr = PairingManager::new(&secret);
        mgr.challenge_ttl_secs = -1; // already expired

        let challenge = mgr.create_challenge();
        let response_hmac = compute_hmac(&secret, challenge.nonce.as_bytes());

        let response = PairingResponse {
            challenge_id: challenge.challenge_id,
            device_id: Uuid::new_v4(),
            device_name: "Late".into(),
            public_key: "pk".into(),
            response_hmac,
        };

        let result = mgr.verify_response(&response);
        assert_eq!(result, PairingResult::Expired);
    }

    // -- Device lifecycle ---------------------------------------------------

    #[test]
    fn test_revoke_device() {
        let secret = test_secret();
        let mut mgr = PairingManager::new(&secret);
        let challenge = mgr.create_challenge();

        let device_id = Uuid::new_v4();
        let response_hmac = compute_hmac(&secret, challenge.nonce.as_bytes());
        let response = PairingResponse {
            challenge_id: challenge.challenge_id,
            device_id,
            device_name: "Phone".into(),
            public_key: "pk".into(),
            response_hmac,
        };

        mgr.verify_response(&response);
        assert_eq!(mgr.paired_devices().len(), 1);

        assert!(mgr.revoke_device(&device_id));
        assert!(mgr.paired_devices().is_empty());
    }

    #[test]
    fn test_revoke_nonexistent_device() {
        let mut mgr = PairingManager::new(&test_secret());
        assert!(!mgr.revoke_device(&Uuid::new_v4()));
    }

    // -- TOTP ---------------------------------------------------------------

    #[test]
    fn test_totp_generate_deterministic() {
        let verifier = TotpVerifier::new(b"secret", 30, 6);
        let code1 = verifier.generate_at(1000000);
        let code2 = verifier.generate_at(1000000);
        assert_eq!(code1, code2);
        assert_eq!(code1.len(), 6);
    }

    #[test]
    fn test_totp_different_timestamps_different_codes() {
        let verifier = TotpVerifier::new(b"secret", 30, 6);
        let code1 = verifier.generate_at(0);
        let code2 = verifier.generate_at(30);
        // Different periods should (almost certainly) produce different codes
        // There's a tiny collision probability, but with 6 digits it's 1/1M
        assert_ne!(code1, code2);
    }

    #[test]
    fn test_totp_verify_at_exact() {
        let verifier = TotpVerifier::new(b"my-key", 30, 6);
        let ts = 1700000000u64;
        let code = verifier.generate_at(ts);
        assert!(verifier.verify_at(&code, ts));
    }

    #[test]
    fn test_totp_verify_at_with_drift() {
        let verifier = TotpVerifier::new(b"my-key", 30, 6);
        let ts = 1700000000u64;
        // Generate code for the previous period
        let code = verifier.generate_at(ts - 30);
        // Should still verify within +-1 period
        assert!(verifier.verify_at(&code, ts));
    }

    #[test]
    fn test_totp_reject_wrong_code() {
        let verifier = TotpVerifier::new(b"my-key", 30, 6);
        assert!(!verifier.verify_at("000000", 1700000000));
    }

    // -- Cleanup ------------------------------------------------------------

    #[test]
    fn test_cleanup_expired_challenges() {
        let mut mgr = PairingManager::new(&test_secret());
        mgr.challenge_ttl_secs = -1; // create already-expired
        mgr.create_challenge();
        mgr.create_challenge();
        assert_eq!(mgr.pending_count(), 2);

        mgr.cleanup_expired();
        assert_eq!(mgr.pending_count(), 0);
    }

    // -- Serialization ------------------------------------------------------

    #[test]
    fn test_device_identity_serialization() {
        let now = Utc::now();
        let device = DeviceIdentity {
            device_id: Uuid::new_v4(),
            device_name: "Test".into(),
            public_key: "pk-123".into(),
            created_at: now,
            last_seen: now,
        };

        let json = serde_json::to_string(&device).unwrap();
        let restored: DeviceIdentity = serde_json::from_str(&json).unwrap();
        assert_eq!(device.device_id, restored.device_id);
        assert_eq!(device.device_name, restored.device_name);
    }

    #[test]
    fn test_pairing_result_serialization() {
        let json = serde_json::to_string(&PairingResult::Accepted).unwrap();
        let restored: PairingResult = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, PairingResult::Accepted);
    }
}
