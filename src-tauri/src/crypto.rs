use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use aes_gcm::aead::Aead;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rand::rngs::OsRng;
use rand::RngCore;
use std::fs;
use std::path::PathBuf;

const NONCE_LEN: usize = 12; // 96-bit nonce for AES-GCM
const KEY_LEN: usize = 32; // 256 bits
const KEY_FILE_NAME: &str = ".whatchanged.key";
const ENCRYPTED_PREFIX: &str = "wcenc:"; // Definitive marker for encrypted values

/// Encryption manager for AES-256-GCM operations.
/// Handles key lifecycle and encrypt/decrypt operations for webhook secrets.
pub struct CryptoManager {
    cipher: Aes256Gcm,
}

impl CryptoManager {
    /// Initialize with the app data directory. Loads or generates the master key.
    pub fn new(app_data_dir: &PathBuf) -> Result<Self, String> {
        let key = Self::load_or_generate_key(app_data_dir)?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| format!("Failed to create cipher: {}", e))?;
        Ok(CryptoManager { cipher })
    }

    /// Encrypt plaintext, return prefixed base64-encoded (nonce + ciphertext + GCM tag).
    pub fn encrypt(&self, plaintext: &str) -> Result<String, String> {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| format!("Encryption failed: {}", e))?;
        // Prepend nonce to ciphertext
        let mut combined = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        combined.extend_from_slice(&nonce_bytes);
        combined.extend_from_slice(&ciphertext);
        Ok(format!("{}{}", ENCRYPTED_PREFIX, B64.encode(&combined)))
    }

    /// Decrypt prefixed base64-encoded (nonce + ciphertext + GCM tag) back to plaintext.
    pub fn decrypt(&self, encoded: &str) -> Result<String, String> {
        let b64_part = encoded.strip_prefix(ENCRYPTED_PREFIX)
            .ok_or("Data is not encrypted (missing wcenc: prefix)")?;
        let combined = B64
            .decode(b64_part)
            .map_err(|e| format!("Invalid base64: {}", e))?;
        if combined.len() < NONCE_LEN + 16 {
            // 16 bytes minimum for GCM tag
            return Err("Ciphertext too short".to_string());
        }
        let (nonce_bytes, ciphertext) = combined.split_at(NONCE_LEN);
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = self
            .cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| format!("Decryption failed (wrong key or corrupted data): {}", e))?;
        String::from_utf8(plaintext)
            .map_err(|e| format!("Decrypted data is not valid UTF-8: {}", e))
    }

    /// Definitively check if a string is encrypted by our system using the prefix marker.
    pub fn is_encrypted(encoded: &str) -> bool {
        encoded.starts_with(ENCRYPTED_PREFIX)
    }

    fn load_or_generate_key(app_data_dir: &PathBuf) -> Result<[u8; KEY_LEN], String> {
        let key_path = app_data_dir.join(KEY_FILE_NAME);
        if key_path.exists() {
            let key_bytes = fs::read(&key_path)
                .map_err(|e| format!("Cannot read encryption key: {}", e))?;
            if key_bytes.len() != KEY_LEN {
                return Err(format!(
                    "Encryption key has wrong length ({} bytes, expected {})",
                    key_bytes.len(),
                    KEY_LEN
                ));
            }
            let mut key = [0u8; KEY_LEN];
            key.copy_from_slice(&key_bytes);
            Ok(key)
        } else {
            // Generate new key
            let mut key = [0u8; KEY_LEN];
            OsRng.fill_bytes(&mut key);
            fs::write(&key_path, key)
                .map_err(|e| format!("Cannot write encryption key: {}", e))?;
            Self::set_key_permissions(&key_path)?;
            log::info!("Generated new encryption key");
            Ok(key)
        }
    }

    /// Set restrictive file permissions on the key file.
    fn set_key_permissions(key_path: &PathBuf) -> Result<(), String> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o600);
            fs::set_permissions(key_path, perms)
                .map_err(|e| format!("Cannot set key file permissions: {}", e))?;
        }
        #[cfg(target_os = "windows")]
        {
            // Windows: mark as hidden (best effort — full ACL requires winapi crate)
            if let Some(path_str) = key_path.to_str() {
                let _ = std::process::Command::new("attrib")
                    .args(["+H", path_str])
                    .output();
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_encrypt_decrypt_round_trip() {
        let dir = tempdir().unwrap();
        let crypto = CryptoManager::new(&dir.path().to_path_buf()).unwrap();
        let plaintext = "my-super-secret-webhook-key-12345";
        let encrypted = crypto.encrypt(plaintext).unwrap();
        assert_ne!(encrypted, plaintext);
        let decrypted = crypto.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_is_encrypted() {
        let dir = tempdir().unwrap();
        let crypto = CryptoManager::new(&dir.path().to_path_buf()).unwrap();
        let encrypted = crypto.encrypt("test").unwrap();
        assert!(encrypted.starts_with("wcenc:"));
        assert!(CryptoManager::is_encrypted(&encrypted));
        assert!(!CryptoManager::is_encrypted("plaintext-secret"));
        assert!(!CryptoManager::is_encrypted("short"));
        assert!(!CryptoManager::is_encrypted("SGVsbG8=")); // base64 without prefix
    }
}
