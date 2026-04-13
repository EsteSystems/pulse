use ed25519_dalek::{Signature, VerifyingKey};
use thiserror::Error;

use crate::compression;
use crate::format::{hash_content, BranchVector, ReplyEligibility, Stub, StubFlags, MAX_INLINE_CHARS};
use pulse_identity::keypair::Identity;

#[derive(Debug, Error)]
pub enum SigningError {
    #[error("content exceeds {MAX_INLINE_CHARS} character limit (got {0})")]
    ContentTooLong(usize),
    #[error("signature verification failed: {0}")]
    VerifyFailed(#[from] ed25519_dalek::SignatureError),
}

/// Build and sign a new inline stub.
pub fn create_inline_stub(
    identity: &Identity,
    text: &str,
    branch_vector: BranchVector,
    reply_eligibility: ReplyEligibility,
) -> Result<Stub, SigningError> {
    let char_count = text.chars().count();
    if char_count > MAX_INLINE_CHARS {
        return Err(SigningError::ContentTooLong(char_count));
    }

    let content_hash = hash_content(text);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    let flags = StubFlags {
        content_is_inline: true,
        expiry_set: false,
        reply_eligibility,
    };

    let compressed = compression::compress(text.as_bytes());

    let mut stub = Stub {
        content_hash,
        author_pubkey: identity.public_key_bytes(),
        signature: [0u8; 64],
        timestamp,
        flags,
        branch_vector,
        content_ref: [0u8; 32],
        inline_content: compressed,
    };

    // Sign: content_hash + timestamp + branch_vector_bytes
    let signed_bytes = stub.signed_bytes();
    let sig = identity.sign(&signed_bytes);
    stub.signature = sig.to_bytes();

    Ok(stub)
}

/// Verify a stub's signature against its author's public key.
pub fn verify_stub_signature(stub: &Stub) -> Result<(), SigningError> {
    let pubkey = VerifyingKey::from_bytes(&stub.author_pubkey)
        .map_err(|e| SigningError::VerifyFailed(e))?;
    let signature = Signature::from_bytes(&stub.signature);
    let signed_bytes = stub.signed_bytes();
    pubkey.verify_strict(&signed_bytes, &signature)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_verify_inline_stub() {
        let id = Identity::generate();
        let stub = create_inline_stub(
            &id,
            "hello pulse",
            BranchVector::root(),
            ReplyEligibility::Open,
        )
        .unwrap();

        assert!(verify_stub_signature(&stub).is_ok());
        assert!(stub.flags.content_is_inline);
        assert_eq!(stub.inline_text().unwrap().unwrap(), "hello pulse");
    }

    #[test]
    fn reject_too_long_content() {
        let id = Identity::generate();
        let long = "x".repeat(301);
        let result = create_inline_stub(
            &id,
            &long,
            BranchVector::root(),
            ReplyEligibility::Open,
        );
        assert!(result.is_err());
    }

    #[test]
    fn exactly_300_chars_ok() {
        let id = Identity::generate();
        let text = "x".repeat(300);
        let stub = create_inline_stub(
            &id,
            &text,
            BranchVector::root(),
            ReplyEligibility::Open,
        )
        .unwrap();
        assert_eq!(stub.inline_text().unwrap().unwrap(), text);
    }

    #[test]
    fn tampered_content_fails_verify() {
        let id = Identity::generate();
        let mut stub = create_inline_stub(
            &id,
            "hello",
            BranchVector::root(),
            ReplyEligibility::Open,
        )
        .unwrap();

        // Tamper with content hash
        stub.content_hash[0] ^= 0xff;
        assert!(verify_stub_signature(&stub).is_err());
    }

    #[test]
    fn wrong_author_fails_verify() {
        let id1 = Identity::generate();
        let id2 = Identity::generate();
        let mut stub = create_inline_stub(
            &id1,
            "hello",
            BranchVector::root(),
            ReplyEligibility::Open,
        )
        .unwrap();

        stub.author_pubkey = id2.public_key_bytes();
        assert!(verify_stub_signature(&stub).is_err());
    }

    #[test]
    fn reply_stub_has_parent() {
        let id = Identity::generate();
        let parent_hash = [42u8; 32];
        let stub = create_inline_stub(
            &id,
            "this is a reply",
            BranchVector::reply(parent_hash),
            ReplyEligibility::Open,
        )
        .unwrap();

        assert!(!stub.branch_vector.is_root());
        assert_eq!(stub.branch_vector.parent_hash, parent_hash);
    }

    #[test]
    fn unicode_content_works() {
        let id = Identity::generate();
        let text = "Pulse: 信頼のトポロジー 🌍";
        let stub = create_inline_stub(
            &id,
            text,
            BranchVector::root(),
            ReplyEligibility::Open,
        )
        .unwrap();
        assert_eq!(stub.inline_text().unwrap().unwrap(), text);
        assert!(verify_stub_signature(&stub).is_ok());
    }
}
