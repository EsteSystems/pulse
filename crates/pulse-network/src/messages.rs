use serde::{Deserialize, Serialize};

/// Messages exchanged over GossipSub and request-response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PulseMessage {
    /// A new stub has been created and should be propagated.
    NewStub {
        /// Serialized stub bytes.
        stub_bytes: Vec<u8>,
    },
    /// A social graph edge has been created or updated.
    EdgeUpdate {
        /// JSON-serialized edge.
        edge_json: String,
    },
    /// Request a stub by content hash.
    StubRequest {
        content_hash: [u8; 32],
    },
    /// Response with a requested stub.
    StubResponse {
        /// Serialized stub bytes, or empty if not found.
        stub_bytes: Vec<u8>,
    },
    /// Profile metadata update (display name, avatar).
    ProfileUpdate {
        /// JSON-serialized ProfileMeta.
        profile_json: String,
    },
}

impl PulseMessage {
    pub fn serialize(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("message serialization should not fail")
    }

    pub fn deserialize(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}
