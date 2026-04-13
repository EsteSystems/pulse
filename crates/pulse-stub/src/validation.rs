use thiserror::Error;

use crate::format::{hash_content, Stub, MAX_BRANCH_VECTOR_LEN};
use crate::signing::verify_stub_signature;

/// Maximum allowed timestamp drift into the future (5 minutes).
const MAX_FUTURE_DRIFT_MS: u64 = 5 * 60 * 1000;

/// Maximum age for a stub to be accepted (1 year).
const MAX_AGE_MS: u64 = 365 * 24 * 60 * 60 * 1000;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("content hash mismatch: expected {expected}, got {got}")]
    HashMismatch { expected: String, got: String },
    #[error("signature verification failed: {0}")]
    SignatureInvalid(String),
    #[error("timestamp is {0}ms in the future (max allowed: {MAX_FUTURE_DRIFT_MS}ms)")]
    TimestampFuture(u64),
    #[error("timestamp is too old ({0}ms ago, max age: {MAX_AGE_MS}ms)")]
    TimestampTooOld(u64),
    #[error("branch vector too long: {0} bytes (max {MAX_BRANCH_VECTOR_LEN})")]
    BranchVectorTooLong(usize),
    #[error("inline content present but flag not set")]
    InlineContentFlagMismatch,
    #[error("content_ref set but inline flag also set")]
    ContentRefAndInlineConflict,
}

/// Result of validating a stub.
#[derive(Debug)]
pub enum ValidationResult {
    Valid,
    Invalid(ValidationError),
}

/// Validate an incoming stub. Returns Valid or Invalid with reason.
pub fn validate_stub(stub: &Stub) -> ValidationResult {
    // 1. Verify content hash matches inline content (if inline)
    if stub.flags.content_is_inline && !stub.inline_content.is_empty() {
        if let Ok(Some(text)) = stub.inline_text() {
            let expected = hash_content(&text);
            if expected != stub.content_hash {
                return ValidationResult::Invalid(ValidationError::HashMismatch {
                    expected: hex::encode(expected),
                    got: hex::encode(stub.content_hash),
                });
            }
        }
    }

    // 2. Verify signature
    if let Err(e) = verify_stub_signature(stub) {
        return ValidationResult::Invalid(ValidationError::SignatureInvalid(e.to_string()));
    }

    // 3. Verify timestamp sanity
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    if stub.timestamp > now + MAX_FUTURE_DRIFT_MS {
        return ValidationResult::Invalid(ValidationError::TimestampFuture(
            stub.timestamp - now,
        ));
    }

    if now > stub.timestamp && (now - stub.timestamp) > MAX_AGE_MS {
        return ValidationResult::Invalid(ValidationError::TimestampTooOld(
            now - stub.timestamp,
        ));
    }

    // 4. Branch vector size
    let bv_len = stub.branch_vector.to_bytes().len();
    if bv_len > MAX_BRANCH_VECTOR_LEN {
        return ValidationResult::Invalid(ValidationError::BranchVectorTooLong(bv_len));
    }

    // 5. Flag consistency
    if !stub.inline_content.is_empty() && !stub.flags.content_is_inline {
        return ValidationResult::Invalid(ValidationError::InlineContentFlagMismatch);
    }

    if stub.content_ref != [0u8; 32] && stub.flags.content_is_inline {
        return ValidationResult::Invalid(ValidationError::ContentRefAndInlineConflict);
    }

    ValidationResult::Valid
}

impl ValidationResult {
    pub fn is_valid(&self) -> bool {
        matches!(self, ValidationResult::Valid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{BranchVector, ReplyEligibility};
    use crate::signing::create_inline_stub;
    use pulse_identity::keypair::Identity;

    #[test]
    fn valid_stub_passes() {
        let id = Identity::generate();
        let stub = create_inline_stub(
            &id,
            "hello pulse",
            BranchVector::root(),
            ReplyEligibility::Open,
        )
        .unwrap();
        assert!(validate_stub(&stub).is_valid());
    }

    #[test]
    fn tampered_hash_rejected() {
        let id = Identity::generate();
        let mut stub = create_inline_stub(
            &id,
            "hello",
            BranchVector::root(),
            ReplyEligibility::Open,
        )
        .unwrap();
        // Tamper with signature too to bypass sig check, isolate hash check
        // Actually, hash check happens on inline content vs content_hash
        // Tamper content_hash — sig check will catch it first
        stub.content_hash[0] ^= 0xff;
        assert!(!validate_stub(&stub).is_valid());
    }

    #[test]
    fn future_timestamp_rejected() {
        let id = Identity::generate();
        let mut stub = create_inline_stub(
            &id,
            "future post",
            BranchVector::root(),
            ReplyEligibility::Open,
        )
        .unwrap();

        // Set timestamp 10 minutes in the future
        stub.timestamp += 10 * 60 * 1000;
        // Re-sign with new timestamp
        let signed_bytes = stub.signed_bytes();
        let sig = id.sign(&signed_bytes);
        stub.signature = sig.to_bytes();

        assert!(!validate_stub(&stub).is_valid());
    }

    #[test]
    fn flag_mismatch_rejected() {
        let id = Identity::generate();
        let mut stub = create_inline_stub(
            &id,
            "test",
            BranchVector::root(),
            ReplyEligibility::Open,
        )
        .unwrap();

        // Clear inline flag but keep content
        stub.flags.content_is_inline = false;
        // Re-sign
        let signed_bytes = stub.signed_bytes();
        let sig = id.sign(&signed_bytes);
        stub.signature = sig.to_bytes();

        assert!(!validate_stub(&stub).is_valid());
    }
}
