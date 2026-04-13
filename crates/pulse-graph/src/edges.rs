use pulse_identity::keypair::Identity;
use pulse_store::graph_store::{Edge, EdgeType, GraphStore};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EdgeError {
    #[error("store error: {0}")]
    Store(#[from] pulse_store::graph_store::GraphStoreError),
    #[error("weight must be between 0.0 and 1.0, got {0}")]
    InvalidWeight(f32),
    #[error("cannot create edge to self")]
    SelfEdge,
}

/// Create a signed trust edge.
pub fn create_trust_edge(
    identity: &Identity,
    target: &[u8; 32],
    topic_scope: [u8; 32],
    weight: f32,
) -> Result<Edge, EdgeError> {
    if weight < 0.0 || weight > 1.0 {
        return Err(EdgeError::InvalidWeight(weight));
    }
    let source = identity.public_key_bytes();
    if source == *target {
        return Err(EdgeError::SelfEdge);
    }
    Ok(sign_edge(identity, source, *target, EdgeType::Trust, topic_scope, weight))
}

/// Create a signed watch edge.
pub fn create_watch_edge(
    identity: &Identity,
    target: &[u8; 32],
) -> Result<Edge, EdgeError> {
    let source = identity.public_key_bytes();
    if source == *target {
        return Err(EdgeError::SelfEdge);
    }
    Ok(sign_edge(identity, source, *target, EdgeType::Watch, [0u8; 32], 0.0))
}

/// Create a signed silence edge.
pub fn create_silence_edge(
    identity: &Identity,
    target: &[u8; 32],
) -> Result<Edge, EdgeError> {
    let source = identity.public_key_bytes();
    if source == *target {
        return Err(EdgeError::SelfEdge);
    }
    Ok(sign_edge(identity, source, *target, EdgeType::Silence, [0u8; 32], 0.0))
}

/// Store an edge locally.
pub fn store_edge(store: &GraphStore, edge: &Edge) -> Result<(), EdgeError> {
    store.put_edge(edge)?;
    Ok(())
}

/// Revoke an edge (remove from local store).
pub fn revoke_edge(
    store: &GraphStore,
    source: &[u8; 32],
    target: &[u8; 32],
    edge_type: EdgeType,
    topic_scope: &[u8; 32],
) -> Result<bool, EdgeError> {
    Ok(store.remove_edge(source, target, edge_type, topic_scope)?)
}

/// Serialize an edge to JSON for network propagation.
pub fn edge_to_json(edge: &Edge) -> String {
    serde_json::to_string(edge).expect("edge serialization should not fail")
}

/// Deserialize an edge from JSON.
pub fn edge_from_json(json: &str) -> Result<Edge, serde_json::Error> {
    serde_json::from_str(json)
}

fn sign_edge(
    identity: &Identity,
    source: [u8; 32],
    target: [u8; 32],
    edge_type: EdgeType,
    topic_scope: [u8; 32],
    weight: f32,
) -> Edge {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    // Build signed payload: source + target + edge_type + topic_scope + weight + timestamp
    let mut payload = Vec::with_capacity(101);
    payload.extend_from_slice(&source);
    payload.extend_from_slice(&target);
    payload.push(edge_type as u8);
    payload.extend_from_slice(&topic_scope);
    payload.extend_from_slice(&weight.to_be_bytes());
    payload.extend_from_slice(&timestamp.to_be_bytes());

    let sig = identity.sign(&payload);

    Edge {
        source_pubkey: source,
        target_pubkey: target,
        edge_type,
        topic_scope,
        weight,
        timestamp,
        signature: sig.to_bytes(),
    }
}

/// Verify an edge's signature.
pub fn verify_edge_signature(edge: &Edge) -> bool {
    let pubkey = match pulse_identity::keypair::PublicIdentity::from_bytes(&edge.source_pubkey) {
        Ok(p) => p,
        Err(_) => return false,
    };

    let mut payload = Vec::with_capacity(101);
    payload.extend_from_slice(&edge.source_pubkey);
    payload.extend_from_slice(&edge.target_pubkey);
    payload.push(edge.edge_type as u8);
    payload.extend_from_slice(&edge.topic_scope);
    payload.extend_from_slice(&edge.weight.to_be_bytes());
    payload.extend_from_slice(&edge.timestamp.to_be_bytes());

    let sig = ed25519_dalek::Signature::from_bytes(&edge.signature);
    pubkey.verify(&payload, &sig).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_verify_trust_edge() {
        let id = Identity::generate();
        let target = [42u8; 32];
        let edge = create_trust_edge(&id, &target, [0u8; 32], 0.8).unwrap();

        assert_eq!(edge.edge_type, EdgeType::Trust);
        assert_eq!(edge.source_pubkey, id.public_key_bytes());
        assert_eq!(edge.target_pubkey, target);
        assert!((edge.weight - 0.8).abs() < f32::EPSILON);
        assert!(verify_edge_signature(&edge));
    }

    #[test]
    fn create_and_verify_watch_edge() {
        let id = Identity::generate();
        let target = [43u8; 32];
        let edge = create_watch_edge(&id, &target).unwrap();

        assert_eq!(edge.edge_type, EdgeType::Watch);
        assert!(verify_edge_signature(&edge));
    }

    #[test]
    fn create_and_verify_silence_edge() {
        let id = Identity::generate();
        let target = [44u8; 32];
        let edge = create_silence_edge(&id, &target).unwrap();

        assert_eq!(edge.edge_type, EdgeType::Silence);
        assert!(verify_edge_signature(&edge));
    }

    #[test]
    fn tampered_edge_fails_verify() {
        let id = Identity::generate();
        let target = [42u8; 32];
        let mut edge = create_trust_edge(&id, &target, [0u8; 32], 0.8).unwrap();
        edge.weight = 1.0; // tamper
        assert!(!verify_edge_signature(&edge));
    }

    #[test]
    fn self_edge_rejected() {
        let id = Identity::generate();
        let pubkey = id.public_key_bytes();
        assert!(create_trust_edge(&id, &pubkey, [0u8; 32], 0.5).is_err());
    }

    #[test]
    fn invalid_weight_rejected() {
        let id = Identity::generate();
        let target = [42u8; 32];
        assert!(create_trust_edge(&id, &target, [0u8; 32], 1.5).is_err());
        assert!(create_trust_edge(&id, &target, [0u8; 32], -0.1).is_err());
    }

    #[test]
    fn edge_json_roundtrip() {
        let id = Identity::generate();
        let target = [42u8; 32];
        let edge = create_trust_edge(&id, &target, [0u8; 32], 0.8).unwrap();
        let json = edge_to_json(&edge);
        let recovered = edge_from_json(&json).unwrap();
        assert_eq!(recovered.source_pubkey, edge.source_pubkey);
        assert_eq!(recovered.edge_type, edge.edge_type);
        assert!(verify_edge_signature(&recovered));
    }

    #[test]
    fn store_and_query_edges() {
        let store = GraphStore::open_in_memory().unwrap();
        let id = Identity::generate();
        let bob = [2u8; 32];
        let carol = [3u8; 32];
        let dave = [4u8; 32];

        let trust = create_trust_edge(&id, &bob, [0u8; 32], 0.9).unwrap();
        let watch = create_watch_edge(&id, &carol).unwrap();
        let silence = create_silence_edge(&id, &dave).unwrap();

        store_edge(&store, &trust).unwrap();
        store_edge(&store, &watch).unwrap();
        store_edge(&store, &silence).unwrap();

        let source = id.public_key_bytes();
        assert_eq!(store.get_outbound_by_type(&source, EdgeType::Trust).unwrap().len(), 1);
        assert_eq!(store.get_outbound_by_type(&source, EdgeType::Watch).unwrap().len(), 1);
        assert_eq!(store.get_outbound_by_type(&source, EdgeType::Silence).unwrap().len(), 1);
        assert!(store.is_silenced(&source, &dave).unwrap());
        assert!(!store.is_silenced(&source, &bob).unwrap());
    }
}
