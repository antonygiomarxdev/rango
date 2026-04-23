use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use rand::RngCore;
use rango_types::RangoError;

/// Transparent encryption engine for file-based persistence.
/// Format: `[nonce 12b][ciphertext+tag]` (caller manages salt/key derivation).
pub struct CryptoEngine {
    cipher: Aes256Gcm,
}

impl std::fmt::Debug for CryptoEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CryptoEngine")
            .field("algorithm", &"AES-256-GCM")
            .finish()
    }
}

impl CryptoEngine {
    /// Derive a 256-bit key from a passphrase and salt using PBKDF2-HMAC-SHA256.
    pub fn from_passphrase(passphrase: &str, salt: &[u8]) -> Self {
        let mut key = [0u8; 32];
        pbkdf2::pbkdf2_hmac::<sha2::Sha256>(passphrase.as_bytes(), salt, 600_000, &mut key);
        let cipher = Aes256Gcm::new_from_slice(&key).expect("valid 256-bit key");
        Self { cipher }
    }

    /// Encrypt plaintext, prepending a random nonce.
    pub fn encrypt(&self, plaintext: &[u8]) -> Vec<u8> {
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext)
            .expect("encryption should not fail with valid nonce");
        let mut result = Vec::with_capacity(12 + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);
        result
    }

    /// Decrypt ciphertext with embedded nonce.
    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, RangoError> {
        if ciphertext.len() < 12 {
            return Err(RangoError::Storage(
                "ciphertext too short (missing nonce)".to_string(),
            ));
        }
        let nonce = Nonce::from_slice(&ciphertext[..12]);
        let plaintext = self
            .cipher
            .decrypt(nonce, &ciphertext[12..])
            .map_err(|e| RangoError::Storage(format!("decryption failed: {}", e)))?;
        Ok(plaintext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_trip() {
        let engine = CryptoEngine::from_passphrase("secret", b"salt1234");
        let plaintext = b"hello world";
        let ciphertext = engine.encrypt(plaintext);
        assert_ne!(&ciphertext[12..], plaintext.as_slice());
        let decrypted = engine.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_decrypt_too_short() {
        let engine = CryptoEngine::from_passphrase("secret", b"salt1234");
        assert!(engine.decrypt(b"short").is_err());
    }
}
