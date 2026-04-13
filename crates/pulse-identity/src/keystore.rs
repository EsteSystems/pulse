use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2, Params,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::Zeroize;

use crate::keypair::Identity;

#[derive(Debug, Error)]
pub enum KeystoreError {
    #[error("wrong passphrase")]
    WrongPassphrase,
    #[error("corrupted keystore file")]
    Corrupted,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("argon2 error: {0}")]
    Argon2(String),
}

/// On-disk format for an encrypted identity.
#[derive(Serialize, Deserialize)]
pub struct EncryptedKeystore {
    /// Argon2id salt (22 chars, base64)
    pub salt: String,
    /// Argon2id parameters
    pub argon2_m_cost: u32,
    pub argon2_t_cost: u32,
    pub argon2_p_cost: u32,
    /// XOR-encrypted secret key (32 bytes, hex-encoded)
    pub encrypted_secret: String,
    /// BLAKE3 hash of the derived key, to verify passphrase correctness
    pub key_check: String,
}

impl EncryptedKeystore {
    /// Encrypt an identity with a passphrase and save to disk.
    pub fn encrypt(identity: &Identity, passphrase: &str) -> Result<Self, KeystoreError> {
        let salt = SaltString::generate(&mut OsRng);

        let params = Params::new(65536, 3, 1, Some(32))
            .map_err(|e| KeystoreError::Argon2(e.to_string()))?;
        let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);

        let derived = derive_key(&argon2, passphrase, &salt)?;
        let key_check = blake3::hash(&derived).to_hex().to_string();

        let secret = identity.secret_key_bytes();
        let mut encrypted = [0u8; 32];
        for i in 0..32 {
            encrypted[i] = secret[i] ^ derived[i];
        }

        let ks = EncryptedKeystore {
            salt: salt.to_string(),
            argon2_m_cost: 65536,
            argon2_t_cost: 3,
            argon2_p_cost: 1,
            encrypted_secret: hex::encode(encrypted),
            key_check,
        };

        encrypted.zeroize();
        Ok(ks)
    }

    /// Decrypt the keystore with a passphrase, recovering the identity.
    pub fn decrypt(&self, passphrase: &str) -> Result<Identity, KeystoreError> {
        let salt = SaltString::from_b64(&self.salt)
            .map_err(|e| KeystoreError::Argon2(e.to_string()))?;

        let params = Params::new(self.argon2_m_cost, self.argon2_t_cost, self.argon2_p_cost, Some(32))
            .map_err(|e| KeystoreError::Argon2(e.to_string()))?;
        let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);

        let derived = derive_key(&argon2, passphrase, &salt)?;
        let check = blake3::hash(&derived).to_hex().to_string();

        if check != self.key_check {
            return Err(KeystoreError::WrongPassphrase);
        }

        let encrypted_bytes = hex::decode(&self.encrypted_secret)
            .map_err(|_| KeystoreError::Corrupted)?;
        if encrypted_bytes.len() != 32 {
            return Err(KeystoreError::Corrupted);
        }

        let mut secret = [0u8; 32];
        for i in 0..32 {
            secret[i] = encrypted_bytes[i] ^ derived[i];
        }

        let identity = Identity::from_secret_bytes(&secret);
        secret.zeroize();
        Ok(identity)
    }

    /// Save keystore to a file.
    pub fn save_to_file(&self, path: &std::path::Path) -> Result<(), KeystoreError> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load keystore from a file.
    pub fn load_from_file(path: &std::path::Path) -> Result<Self, KeystoreError> {
        let json = std::fs::read_to_string(path)?;
        let ks: EncryptedKeystore = serde_json::from_str(&json)?;
        Ok(ks)
    }
}

fn derive_key(argon2: &Argon2, passphrase: &str, salt: &SaltString) -> Result<[u8; 32], KeystoreError> {
    let hash = argon2
        .hash_password(passphrase.as_bytes(), salt)
        .map_err(|e| KeystoreError::Argon2(e.to_string()))?;

    let output = hash.hash.ok_or_else(|| KeystoreError::Argon2("no hash output".into()))?;
    let bytes = output.as_bytes();
    if bytes.len() < 32 {
        return Err(KeystoreError::Argon2("hash output too short".into()));
    }

    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes[..32]);
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let identity = Identity::generate();
        let original_pubkey = identity.public_key_bytes();

        let ks = EncryptedKeystore::encrypt(&identity, "test-passphrase").unwrap();
        let recovered = ks.decrypt("test-passphrase").unwrap();

        assert_eq!(recovered.public_key_bytes(), original_pubkey);
    }

    #[test]
    fn wrong_passphrase_fails() {
        let identity = Identity::generate();
        let ks = EncryptedKeystore::encrypt(&identity, "correct").unwrap();
        assert!(matches!(
            ks.decrypt("wrong"),
            Err(KeystoreError::WrongPassphrase)
        ));
    }

    #[test]
    fn file_roundtrip() {
        let identity = Identity::generate();
        let original_pubkey = identity.public_key_bytes();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keystore.json");

        let ks = EncryptedKeystore::encrypt(&identity, "pass").unwrap();
        ks.save_to_file(&path).unwrap();

        let loaded = EncryptedKeystore::load_from_file(&path).unwrap();
        let recovered = loaded.decrypt("pass").unwrap();

        assert_eq!(recovered.public_key_bytes(), original_pubkey);
    }

    #[test]
    fn different_identities_produce_different_keystores() {
        let id1 = Identity::generate();
        let id2 = Identity::generate();

        let ks1 = EncryptedKeystore::encrypt(&id1, "pass").unwrap();
        let ks2 = EncryptedKeystore::encrypt(&id2, "pass").unwrap();

        assert_ne!(ks1.encrypted_secret, ks2.encrypted_secret);
    }
}
