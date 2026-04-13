use blake3::Hash;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::compression;

mod bytes64_serde {
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8; 64], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(bytes)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 64], D::Error>
    where
        D: Deserializer<'de>,
    {
        let v: Vec<u8> = Deserialize::deserialize(deserializer)?;
        v.try_into()
            .map_err(|_| serde::de::Error::custom("expected 64 bytes for signature"))
    }
}

/// Maximum inline content length in characters before compression.
pub const MAX_INLINE_CHARS: usize = 300;

/// Maximum branch vector length in bytes.
pub const MAX_BRANCH_VECTOR_LEN: usize = 128;

#[derive(Debug, Error)]
pub enum StubError {
    #[error("content exceeds {MAX_INLINE_CHARS} character limit (got {0})")]
    ContentTooLong(usize),
    #[error("branch vector exceeds {MAX_BRANCH_VECTOR_LEN} bytes (got {0})")]
    BranchVectorTooLong(usize),
    #[error("serialization error: {0}")]
    Serialize(String),
    #[error("deserialization error: {0}")]
    Deserialize(String),
    #[error("content hash mismatch")]
    HashMismatch,
}

/// Reply eligibility — who can nest replies under this stub.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum ReplyEligibility {
    Open = 0,
    TrustGraph = 1,
    TrustedOnly = 2,
}

impl ReplyEligibility {
    pub fn from_bits(bits: u8) -> Self {
        match bits & 0b11 {
            0 => Self::Open,
            1 => Self::TrustGraph,
            2 => Self::TrustedOnly,
            _ => Self::Open,
        }
    }

    pub fn to_bits(self) -> u8 {
        self as u8
    }
}

/// Stub flags — 16-bit field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StubFlags {
    pub content_is_inline: bool,
    pub expiry_set: bool,
    pub reply_eligibility: ReplyEligibility,
}

impl StubFlags {
    pub fn to_u16(self) -> u16 {
        let mut bits: u16 = 0;
        if self.content_is_inline {
            bits |= 1;
        }
        if self.expiry_set {
            bits |= 1 << 1;
        }
        bits |= (self.reply_eligibility.to_bits() as u16) << 2;
        bits
    }

    pub fn from_u16(bits: u16) -> Self {
        Self {
            content_is_inline: bits & 1 != 0,
            expiry_set: bits & (1 << 1) != 0,
            reply_eligibility: ReplyEligibility::from_bits(((bits >> 2) & 0b11) as u8),
        }
    }
}

/// Branch vector — identifies which branch this stub belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchVector {
    /// Hash of parent stub (all zeros if root stub).
    pub parent_hash: [u8; 32],
    /// Hashes of explicitly referenced stubs.
    pub explicit_refs: Vec<[u8; 32]>,
}

impl BranchVector {
    pub fn root() -> Self {
        Self {
            parent_hash: [0u8; 32],
            explicit_refs: Vec::new(),
        }
    }

    pub fn reply(parent_hash: [u8; 32]) -> Self {
        Self {
            parent_hash,
            explicit_refs: Vec::new(),
        }
    }

    pub fn is_root(&self) -> bool {
        self.parent_hash == [0u8; 32]
    }

    /// Serialize to bytes for inclusion in stub.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&self.parent_hash);
        for r in &self.explicit_refs {
            buf.extend_from_slice(r);
        }
        buf
    }

    /// Deserialize from bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, StubError> {
        if data.len() < 32 {
            return Err(StubError::Deserialize("branch vector too short".into()));
        }
        let mut parent_hash = [0u8; 32];
        parent_hash.copy_from_slice(&data[..32]);

        let remaining = &data[32..];
        if remaining.len() % 32 != 0 {
            return Err(StubError::Deserialize(
                "branch vector explicit_refs not aligned to 32 bytes".into(),
            ));
        }

        let explicit_refs: Vec<[u8; 32]> = remaining
            .chunks_exact(32)
            .map(|chunk| {
                let mut arr = [0u8; 32];
                arr.copy_from_slice(chunk);
                arr
            })
            .collect();

        Ok(Self {
            parent_hash,
            explicit_refs,
        })
    }
}

/// A Pulse stub — the atomic unit of a speech act.
/// Immutable from creation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stub {
    /// BLAKE3 hash of canonical content.
    pub content_hash: [u8; 32],
    /// Author's Ed25519 public key.
    pub author_pubkey: [u8; 32],
    /// Ed25519 signature over (content_hash + timestamp + branch_vector_bytes).
    #[serde(with = "bytes64_serde")]
    pub signature: [u8; 64],
    /// Unix epoch, millisecond precision.
    pub timestamp: u64,
    /// Flags (inline, expiry, reply eligibility).
    pub flags: StubFlags,
    /// Branch vector (variable length, max 128 bytes).
    pub branch_vector: BranchVector,
    /// Content reference hash (all zeros if inline).
    pub content_ref: [u8; 32],
    /// Inline content (LZ4 compressed UTF-8). Empty if content_ref is used.
    pub inline_content: Vec<u8>,
}

impl Stub {
    /// Get the content hash as a BLAKE3 Hash.
    pub fn content_hash(&self) -> Hash {
        Hash::from_bytes(self.content_hash)
    }

    /// Get the branch this stub belongs to.
    /// For root stubs, the branch ID is the stub's own content_hash.
    /// For reply stubs, traverse parent_hash to find the root.
    pub fn branch_id(&self) -> [u8; 32] {
        if self.branch_vector.is_root() {
            self.content_hash
        } else {
            self.branch_vector.parent_hash
        }
    }

    /// Decompress and return the inline content as a UTF-8 string.
    /// Returns None if this stub uses a content_ref instead.
    pub fn inline_text(&self) -> Result<Option<String>, StubError> {
        if !self.flags.content_is_inline || self.inline_content.is_empty() {
            return Ok(None);
        }
        let decompressed = compression::decompress(&self.inline_content)
            .map_err(|e| StubError::Deserialize(e.to_string()))?;
        let text = String::from_utf8(decompressed)
            .map_err(|e| StubError::Deserialize(e.to_string()))?;
        Ok(Some(text))
    }

    /// Compute the bytes that are signed: content_hash + timestamp + branch_vector_bytes.
    pub fn signed_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&self.content_hash);
        buf.extend_from_slice(&self.timestamp.to_be_bytes());
        buf.extend_from_slice(&self.branch_vector.to_bytes());
        buf
    }

    /// Serialize to binary format per Appendix A.
    pub fn to_bytes(&self) -> Result<Vec<u8>, StubError> {
        let bv_bytes = self.branch_vector.to_bytes();
        if bv_bytes.len() > MAX_BRANCH_VECTOR_LEN {
            return Err(StubError::BranchVectorTooLong(bv_bytes.len()));
        }

        let mut buf = Vec::with_capacity(320);

        // Fixed header
        buf.extend_from_slice(&self.content_hash);         // 0: 32 bytes
        buf.extend_from_slice(&self.author_pubkey);         // 32: 32 bytes
        buf.extend_from_slice(&self.signature);             // 64: 64 bytes
        buf.extend_from_slice(&self.timestamp.to_be_bytes()); // 128: 8 bytes
        buf.extend_from_slice(&self.flags.to_u16().to_be_bytes()); // 136: 2 bytes

        // Variable: branch vector
        buf.extend_from_slice(&(bv_bytes.len() as u16).to_be_bytes()); // 138: 2 bytes
        buf.extend_from_slice(&bv_bytes);                              // 140: N bytes

        // Content ref
        buf.extend_from_slice(&self.content_ref);           // 140+N: 32 bytes

        // Inline content
        buf.extend_from_slice(&(self.inline_content.len() as u16).to_be_bytes());
        buf.extend_from_slice(&self.inline_content);

        Ok(buf)
    }

    /// Deserialize from binary format.
    pub fn from_bytes(data: &[u8]) -> Result<Self, StubError> {
        if data.len() < 172 {
            return Err(StubError::Deserialize("stub too short".into()));
        }

        let mut content_hash = [0u8; 32];
        content_hash.copy_from_slice(&data[0..32]);

        let mut author_pubkey = [0u8; 32];
        author_pubkey.copy_from_slice(&data[32..64]);

        let mut signature = [0u8; 64];
        signature.copy_from_slice(&data[64..128]);

        let timestamp = u64::from_be_bytes(data[128..136].try_into().unwrap());
        let flags = StubFlags::from_u16(u16::from_be_bytes(data[136..138].try_into().unwrap()));

        let bv_len = u16::from_be_bytes(data[138..140].try_into().unwrap()) as usize;
        if data.len() < 140 + bv_len + 34 {
            return Err(StubError::Deserialize("stub truncated at branch vector".into()));
        }

        let branch_vector = BranchVector::from_bytes(&data[140..140 + bv_len])?;

        let offset = 140 + bv_len;
        let mut content_ref = [0u8; 32];
        content_ref.copy_from_slice(&data[offset..offset + 32]);

        let ic_offset = offset + 32;
        if data.len() < ic_offset + 2 {
            return Err(StubError::Deserialize("stub truncated at inline content length".into()));
        }
        let ic_len = u16::from_be_bytes(data[ic_offset..ic_offset + 2].try_into().unwrap()) as usize;

        if data.len() < ic_offset + 2 + ic_len {
            return Err(StubError::Deserialize("stub truncated at inline content".into()));
        }
        let inline_content = data[ic_offset + 2..ic_offset + 2 + ic_len].to_vec();

        Ok(Self {
            content_hash,
            author_pubkey,
            signature,
            timestamp,
            flags,
            branch_vector,
            content_ref,
            inline_content,
        })
    }
}

/// Compute the BLAKE3 content hash for text content.
/// Text-only hashing: same text = same hash regardless of author or time.
/// This enables content-addressable deduplication — the network stores
/// the text once and tracks multiple signers separately.
pub fn hash_content(text: &str) -> [u8; 32] {
    *blake3::hash(text.as_bytes()).as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_roundtrip() {
        let flags = StubFlags {
            content_is_inline: true,
            expiry_set: false,
            reply_eligibility: ReplyEligibility::TrustGraph,
        };
        let bits = flags.to_u16();
        let recovered = StubFlags::from_u16(bits);
        assert_eq!(flags, recovered);
    }

    #[test]
    fn flags_all_combinations() {
        for inline in [false, true] {
            for expiry in [false, true] {
                for re in [
                    ReplyEligibility::Open,
                    ReplyEligibility::TrustGraph,
                    ReplyEligibility::TrustedOnly,
                ] {
                    let flags = StubFlags {
                        content_is_inline: inline,
                        expiry_set: expiry,
                        reply_eligibility: re,
                    };
                    assert_eq!(flags, StubFlags::from_u16(flags.to_u16()));
                }
            }
        }
    }

    #[test]
    fn branch_vector_root_roundtrip() {
        let bv = BranchVector::root();
        let bytes = bv.to_bytes();
        let recovered = BranchVector::from_bytes(&bytes).unwrap();
        assert_eq!(bv, recovered);
    }

    #[test]
    fn branch_vector_reply_roundtrip() {
        let parent = [42u8; 32];
        let bv = BranchVector::reply(parent);
        let bytes = bv.to_bytes();
        let recovered = BranchVector::from_bytes(&bytes).unwrap();
        assert_eq!(bv, recovered);
        assert!(!recovered.is_root());
    }

    #[test]
    fn branch_vector_with_refs_roundtrip() {
        let bv = BranchVector {
            parent_hash: [1u8; 32],
            explicit_refs: vec![[2u8; 32], [3u8; 32]],
        };
        let bytes = bv.to_bytes();
        let recovered = BranchVector::from_bytes(&bytes).unwrap();
        assert_eq!(bv, recovered);
    }

    #[test]
    fn hash_content_deterministic() {
        let h1 = hash_content("hello pulse");
        let h2 = hash_content("hello pulse");
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_content_different_for_different_input() {
        let h1 = hash_content("hello");
        let h2 = hash_content("world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn stub_binary_roundtrip() {
        use crate::signing::create_inline_stub;
        use pulse_identity::keypair::Identity;

        let id = Identity::generate();
        let stub = create_inline_stub(
            &id,
            "binary roundtrip test",
            BranchVector::root(),
            ReplyEligibility::TrustedOnly,
        )
        .unwrap();

        let bytes = stub.to_bytes().unwrap();
        let recovered = Stub::from_bytes(&bytes).unwrap();

        assert_eq!(stub.content_hash, recovered.content_hash);
        assert_eq!(stub.author_pubkey, recovered.author_pubkey);
        assert_eq!(stub.signature, recovered.signature);
        assert_eq!(stub.timestamp, recovered.timestamp);
        assert_eq!(stub.flags, recovered.flags);
        assert_eq!(stub.branch_vector, recovered.branch_vector);
        assert_eq!(stub.content_ref, recovered.content_ref);
        assert_eq!(stub.inline_content, recovered.inline_content);
        assert_eq!(
            recovered.inline_text().unwrap().unwrap(),
            "binary roundtrip test"
        );
    }

    #[test]
    fn stub_binary_roundtrip_with_reply_and_refs() {
        use crate::signing::create_inline_stub;
        use pulse_identity::keypair::Identity;

        let id = Identity::generate();
        let bv = BranchVector {
            parent_hash: [7u8; 32],
            explicit_refs: vec![[8u8; 32], [9u8; 32]],
        };
        let stub = create_inline_stub(
            &id,
            "reply with refs",
            bv,
            ReplyEligibility::Open,
        )
        .unwrap();

        let bytes = stub.to_bytes().unwrap();
        let recovered = Stub::from_bytes(&bytes).unwrap();

        assert_eq!(stub.branch_vector, recovered.branch_vector);
        assert_eq!(recovered.branch_vector.explicit_refs.len(), 2);
    }
}
