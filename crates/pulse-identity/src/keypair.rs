use ed25519_dalek::{
    Signature, Signer, SigningKey, Verifier, VerifyingKey, SECRET_KEY_LENGTH,
};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("invalid secret key bytes")]
    InvalidSecretKey,
    #[error("invalid public key bytes")]
    InvalidPublicKey,
    #[error("signature verification failed")]
    SignatureInvalid(#[from] ed25519_dalek::SignatureError),
}

/// A Pulse identity — an Ed25519 keypair.
///
/// The public key is the identity. The private key signs speech acts.
/// Nothing in the protocol links this to a real-world identity.
#[derive(Clone)]
pub struct Identity {
    signing_key: SigningKey,
}

/// Public identity — the verifying (public) key only.
/// Safe to share, store, and transmit.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PublicIdentity {
    #[serde(with = "pubkey_serde")]
    pub key: VerifyingKey,
}

mod pubkey_serde {
    use ed25519_dalek::VerifyingKey;
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(key: &VerifyingKey, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(key.as_bytes())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<VerifyingKey, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes: Vec<u8> = Deserialize::deserialize(deserializer)?;
        VerifyingKey::from_bytes(bytes.as_slice().try_into().map_err(serde::de::Error::custom)?)
            .map_err(serde::de::Error::custom)
    }
}

impl Identity {
    /// Generate a new random identity.
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        Self { signing_key }
    }

    /// Create an identity from a known seed (deterministic — for testing only).
    pub fn from_seed(seed: &[u8; SECRET_KEY_LENGTH]) -> Self {
        let signing_key = SigningKey::from_bytes(seed);
        Self { signing_key }
    }

    /// Create an identity from existing secret key bytes.
    pub fn from_secret_bytes(bytes: &[u8; SECRET_KEY_LENGTH]) -> Self {
        let signing_key = SigningKey::from_bytes(bytes);
        Self { signing_key }
    }

    /// Get the public identity (verifying key).
    pub fn public(&self) -> PublicIdentity {
        PublicIdentity {
            key: self.signing_key.verifying_key(),
        }
    }

    /// Get the raw public key bytes (32 bytes).
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    /// Get the raw secret key bytes (32 bytes).
    /// Handle with care — this is the private key material.
    pub fn secret_key_bytes(&self) -> &[u8; SECRET_KEY_LENGTH] {
        self.signing_key.as_bytes()
    }

    /// Sign arbitrary bytes.
    pub fn sign(&self, message: &[u8]) -> Signature {
        self.signing_key.sign(message)
    }
}

impl PublicIdentity {
    /// Create from raw public key bytes.
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self, IdentityError> {
        let key = VerifyingKey::from_bytes(bytes).map_err(|_| IdentityError::InvalidPublicKey)?;
        Ok(Self { key })
    }

    /// Get the raw public key bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        self.key.as_bytes()
    }

    /// Verify a signature against this public key.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), IdentityError> {
        self.key.verify(message, signature)?;
        Ok(())
    }

    /// Display as truncated hex (first 8 bytes) for UI.
    pub fn short_id(&self) -> String {
        hex::encode(&self.key.as_bytes()[..8])
    }

    /// Display as full hex for copy/paste.
    pub fn full_id(&self) -> String {
        hex::encode(self.key.as_bytes())
    }
}

impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Identity")
            .field("public_key", &self.public().short_id())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_and_sign_verify() {
        let id = Identity::generate();
        let message = b"hello pulse";
        let sig = id.sign(message);
        assert!(id.public().verify(message, &sig).is_ok());
    }

    #[test]
    fn wrong_message_fails_verify() {
        let id = Identity::generate();
        let sig = id.sign(b"hello");
        assert!(id.public().verify(b"wrong", &sig).is_err());
    }

    #[test]
    fn wrong_key_fails_verify() {
        let id1 = Identity::generate();
        let id2 = Identity::generate();
        let sig = id1.sign(b"hello");
        assert!(id2.public().verify(b"hello", &sig).is_err());
    }

    #[test]
    fn deterministic_from_seed() {
        let seed = [42u8; 32];
        let id1 = Identity::from_seed(&seed);
        let id2 = Identity::from_seed(&seed);
        assert_eq!(id1.public_key_bytes(), id2.public_key_bytes());
    }

    #[test]
    fn public_key_roundtrip() {
        let id = Identity::generate();
        let bytes = id.public_key_bytes();
        let public = PublicIdentity::from_bytes(&bytes).unwrap();
        assert_eq!(public.as_bytes(), &bytes);
    }

    #[test]
    fn short_id_is_16_hex_chars() {
        let id = Identity::generate();
        let short = id.public().short_id();
        assert_eq!(short.len(), 16);
        assert!(short.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn full_id_is_64_hex_chars() {
        let id = Identity::generate();
        let full = id.public().full_id();
        assert_eq!(full.len(), 64);
    }

    #[test]
    fn secret_key_roundtrip() {
        let id = Identity::generate();
        let secret = *id.secret_key_bytes();
        let id2 = Identity::from_secret_bytes(&secret);
        assert_eq!(id.public_key_bytes(), id2.public_key_bytes());
    }

    #[test]
    fn serde_public_identity_roundtrip() {
        let id = Identity::generate();
        let public = id.public();
        let json = serde_json::to_string(&public).unwrap();
        let deserialized: PublicIdentity = serde_json::from_str(&json).unwrap();
        assert_eq!(public, deserialized);
    }
}
