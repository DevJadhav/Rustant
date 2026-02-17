//! Session encryption — AES-256-GCM for encrypting session data at rest.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::RngCore;
use rand::rngs::OsRng;

/// Errors that can occur during encryption/decryption.
#[derive(Debug, thiserror::Error)]
pub enum EncryptionError {
    #[error("Encryption failed: {0}")]
    EncryptFailed(String),
    #[error("Decryption failed: {0}")]
    DecryptFailed(String),
    #[error("Invalid key length: expected 32 bytes, got {0}")]
    InvalidKeyLength(usize),
    #[error("Data too short to contain nonce")]
    DataTooShort,
    #[error("Keyring error: {0}")]
    KeyringError(String),
}

/// Encrypts and decrypts session data using AES-256-GCM.
pub struct SessionEncryptor {
    cipher: Aes256Gcm,
}

impl SessionEncryptor {
    /// Create an encryptor from a raw 32-byte key.
    pub fn from_key(key: &[u8; 32]) -> Self {
        let cipher = Aes256Gcm::new_from_slice(key).expect("32-byte key is always valid");
        Self { cipher }
    }

    /// Create an encryptor from the system keyring.
    /// If no key exists, generates and stores a new one.
    pub fn from_keyring() -> Result<Self, EncryptionError> {
        let store = crate::credentials::KeyringCredentialStore::new();
        let service = "rustant_session_encryption";

        match crate::credentials::CredentialStore::get_key(&store, service) {
            Ok(key_b64) => {
                let key_bytes = base64_decode(&key_b64)?;
                if key_bytes.len() != 32 {
                    return Err(EncryptionError::InvalidKeyLength(key_bytes.len()));
                }
                let mut key = [0u8; 32];
                key.copy_from_slice(&key_bytes);
                Ok(Self::from_key(&key))
            }
            Err(_) => {
                // Generate a new key
                let mut key = [0u8; 32];
                OsRng.fill_bytes(&mut key);
                let key_b64 = base64_encode(&key);
                crate::credentials::CredentialStore::store_key(&store, service, &key_b64).map_err(
                    |e| EncryptionError::KeyringError(format!("Failed to store key: {}", e)),
                )?;
                Ok(Self::from_key(&key))
            }
        }
    }

    /// Encrypt plaintext data.
    /// Returns nonce (12 bytes) prepended to ciphertext.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| EncryptionError::EncryptFailed(e.to_string()))?;

        // Prepend nonce to ciphertext
        let mut result = Vec::with_capacity(12 + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }

    /// Decrypt data that was encrypted with `encrypt()`.
    /// Expects nonce (12 bytes) prepended to ciphertext.
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        if data.len() < 12 {
            return Err(EncryptionError::DataTooShort);
        }

        let (nonce_bytes, ciphertext) = data.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        self.cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| EncryptionError::DecryptFailed(e.to_string()))
    }
}

fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn base64_decode(s: &str) -> Result<Vec<u8>, EncryptionError> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|e| EncryptionError::DecryptFailed(format!("Base64 decode error: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> [u8; 32] {
        let mut key = [0u8; 32];
        for (i, byte) in key.iter_mut().enumerate() {
            *byte = i as u8;
        }
        key
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let encryptor = SessionEncryptor::from_key(&test_key());
        let plaintext = b"Hello, Rustant session data!";

        let encrypted = encryptor.encrypt(plaintext).unwrap();
        assert_ne!(&encrypted, plaintext);
        assert!(encrypted.len() > plaintext.len()); // nonce + tag overhead

        let decrypted = encryptor.decrypt(&encrypted).unwrap();
        assert_eq!(&decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_empty_data() {
        let encryptor = SessionEncryptor::from_key(&test_key());
        let encrypted = encryptor.encrypt(b"").unwrap();
        let decrypted = encryptor.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, b"");
    }

    #[test]
    fn test_decrypt_wrong_key_fails() {
        let encryptor1 = SessionEncryptor::from_key(&test_key());
        let mut wrong_key = test_key();
        wrong_key[0] = 255;
        let encryptor2 = SessionEncryptor::from_key(&wrong_key);

        let encrypted = encryptor1.encrypt(b"secret data").unwrap();
        let result = encryptor2.decrypt(&encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_too_short_data() {
        let encryptor = SessionEncryptor::from_key(&test_key());
        let result = encryptor.decrypt(&[1, 2, 3]); // Less than 12 bytes
        assert!(result.is_err());
        match result.unwrap_err() {
            EncryptionError::DataTooShort => {}
            e => panic!("Expected DataTooShort, got: {:?}", e),
        }
    }

    #[test]
    fn test_decrypt_tampered_data_fails() {
        let encryptor = SessionEncryptor::from_key(&test_key());
        let mut encrypted = encryptor.encrypt(b"important data").unwrap();
        // Tamper with ciphertext
        if let Some(last) = encrypted.last_mut() {
            *last ^= 0xFF;
        }
        let result = encryptor.decrypt(&encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_different_encryptions_produce_different_output() {
        let encryptor = SessionEncryptor::from_key(&test_key());
        let plaintext = b"same data";
        let enc1 = encryptor.encrypt(plaintext).unwrap();
        let enc2 = encryptor.encrypt(plaintext).unwrap();
        // Different nonces should produce different ciphertext
        assert_ne!(enc1, enc2);
        // But both should decrypt to the same plaintext
        assert_eq!(encryptor.decrypt(&enc1).unwrap(), plaintext);
        assert_eq!(encryptor.decrypt(&enc2).unwrap(), plaintext);
    }

    #[test]
    fn test_large_data() {
        let encryptor = SessionEncryptor::from_key(&test_key());
        let large_data: Vec<u8> = (0..100_000).map(|i| (i % 256) as u8).collect();
        let encrypted = encryptor.encrypt(&large_data).unwrap();
        let decrypted = encryptor.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, large_data);
    }
}
