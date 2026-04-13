use pulse_store::graph_store::GraphStore;
use pulse_store::branch_store::BranchStore;
use pulse_store::stub_store::StubStore;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("graph store error: {0}")]
    Graph(#[from] pulse_store::graph_store::GraphStoreError),
    #[error("stub store error: {0}")]
    Stub(#[from] pulse_store::stub_store::StubStoreError),
    #[error("branch store error: {0}")]
    Branch(#[from] pulse_store::branch_store::BranchStoreError),
}

/// Profile metrics — replaces the single follower count.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileMetrics {
    /// Count of inbound trust edges.
    pub trust_density: usize,
    /// Count of inbound watch edges.
    pub watch_count: usize,
    /// Count of inbound silence edges.
    pub silence_count: usize,
    /// Total stubs that reference this author's stubs as parent.
    pub built_upon_count: usize,
    /// Total stubs authored.
    pub stub_count: usize,
}

/// Compute profile metrics for a given public key.
pub fn compute_profile(
    pubkey: &[u8; 32],
    graph_store: &GraphStore,
    stub_store: &StubStore,
    branch_store: &BranchStore,
) -> Result<ProfileMetrics, ProfileError> {
    let trust_density = graph_store.trust_density(pubkey)?;
    let watch_count = graph_store.watch_count(pubkey)?;
    let silence_count = graph_store.silence_count(pubkey)?;

    // Count authored stubs and built-upon score
    let mut stub_count = 0;
    let mut authored_hashes: Vec<[u8; 32]> = Vec::new();

    for result in stub_store.iter_all() {
        let (hash, stub) = match result {
            Ok(s) => s,
            Err(_) => continue,
        };
        if stub.author_pubkey == *pubkey {
            stub_count += 1;
            authored_hashes.push(hash);
        }
    }

    // Count how many stubs reference authored stubs as parent
    let mut built_upon_count = 0;
    for result in stub_store.iter_all() {
        let (_hash, stub) = match result {
            Ok(s) => s,
            Err(_) => continue,
        };
        if stub.author_pubkey == *pubkey {
            continue; // Don't count self-references
        }
        if authored_hashes.contains(&stub.branch_vector.parent_hash) {
            built_upon_count += 1;
        }
    }

    Ok(ProfileMetrics {
        trust_density,
        watch_count,
        silence_count,
        built_upon_count,
        stub_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edges::*;
    use pulse_identity::keypair::Identity;
    use pulse_stub::format::{BranchVector, ReplyEligibility};
    use pulse_stub::signing::create_inline_stub;

    #[test]
    fn profile_metrics_basic() {
        let alice = Identity::generate();
        let bob = Identity::generate();
        let carol = Identity::generate();

        let stub_dir = tempfile::tempdir().unwrap();
        let stub_store = StubStore::open(stub_dir.path()).unwrap();
        let graph_store = GraphStore::open_in_memory().unwrap();
        let branch_store = BranchStore::open_in_memory().unwrap();

        // Bob and Carol trust Alice
        store_edge(&graph_store, &create_trust_edge(&bob, &alice.public_key_bytes(), [0u8; 32], 0.9).unwrap()).unwrap();
        store_edge(&graph_store, &create_trust_edge(&carol, &alice.public_key_bytes(), [0u8; 32], 0.7).unwrap()).unwrap();
        // Bob watches Alice too (different edge type)
        store_edge(&graph_store, &create_watch_edge(&bob, &alice.public_key_bytes()).unwrap()).unwrap();

        // Alice posts a root stub
        let root = create_inline_stub(&alice, "alice says", BranchVector::root(), ReplyEligibility::Open).unwrap();
        let root_hash = root.content_hash;
        stub_store.put(&root).unwrap();

        // Bob replies to Alice's stub (built-upon)
        let reply = create_inline_stub(&bob, "bob replies", BranchVector::reply(root_hash), ReplyEligibility::Open).unwrap();
        stub_store.put(&reply).unwrap();

        let metrics = compute_profile(
            &alice.public_key_bytes(),
            &graph_store,
            &stub_store,
            &branch_store,
        ).unwrap();

        assert_eq!(metrics.trust_density, 2); // Bob + Carol
        assert_eq!(metrics.watch_count, 1);   // Bob
        assert_eq!(metrics.silence_count, 0);
        assert_eq!(metrics.stub_count, 1);     // Alice's root stub
        assert_eq!(metrics.built_upon_count, 1); // Bob's reply
    }
}
