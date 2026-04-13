use pulse_store::graph_store::{EdgeType, GraphStore};
use pulse_store::stub_store::StubStore;
use pulse_stub::format::Stub;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FeedError {
    #[error("graph store error: {0}")]
    Graph(#[from] pulse_store::graph_store::GraphStoreError),
    #[error("stub store error: {0}")]
    Stub(#[from] pulse_store::stub_store::StubStoreError),
}

/// A feed item — what the user sees in their feed.
#[derive(Debug, Clone)]
pub enum FeedItem {
    /// Your own stub.
    OwnStub {
        stub: Stub,
    },
    /// Full stub from a trusted account — content visible.
    TrustStub {
        stub: Stub,
        trust_weight: f32,
    },
    /// Header from a watched account — content on demand.
    WatchHeader {
        content_hash: [u8; 32],
        author_pubkey: [u8; 32],
        timestamp: u64,
        branch_id: [u8; 32],
    },
}

impl FeedItem {
    pub fn timestamp(&self) -> u64 {
        match self {
            FeedItem::OwnStub { stub } => stub.timestamp,
            FeedItem::TrustStub { stub, .. } => stub.timestamp,
            FeedItem::WatchHeader { timestamp, .. } => *timestamp,
        }
    }

    pub fn author(&self) -> &[u8; 32] {
        match self {
            FeedItem::OwnStub { stub } => &stub.author_pubkey,
            FeedItem::TrustStub { stub, .. } => &stub.author_pubkey,
            FeedItem::WatchHeader { author_pubkey, .. } => author_pubkey,
        }
    }

    pub fn is_own(&self) -> bool {
        matches!(self, FeedItem::OwnStub { .. })
    }

    pub fn is_trust(&self) -> bool {
        matches!(self, FeedItem::TrustStub { .. })
    }

    pub fn is_watch(&self) -> bool {
        matches!(self, FeedItem::WatchHeader { .. })
    }
}

/// Build the feed for a given identity.
///
/// Feed construction per MVP spec:
/// 1. All new stubs from direct trust edges — full content, highest priority
/// 2. Watch edge stub headers — awareness without depth
/// 3. Silence edges — excluded entirely
/// 4. Nothing else — the feed earns its presence or goes quiet
pub fn build_feed(
    my_pubkey: &[u8; 32],
    graph_store: &GraphStore,
    stub_store: &StubStore,
    limit: usize,
) -> Result<Vec<FeedItem>, FeedError> {
    let mut items: Vec<FeedItem> = Vec::new();

    // Get all outbound edges from this identity
    let edges = graph_store.get_outbound(my_pubkey)?;

    // Build sets for quick lookup
    let mut trusted: Vec<([u8; 32], f32)> = Vec::new();
    let mut watched: Vec<[u8; 32]> = Vec::new();
    let mut silenced: Vec<[u8; 32]> = Vec::new();

    for edge in &edges {
        match edge.edge_type {
            EdgeType::Trust => trusted.push((edge.target_pubkey, edge.weight)),
            EdgeType::Watch => watched.push(edge.target_pubkey),
            EdgeType::Silence => silenced.push(edge.target_pubkey),
        }
    }

    // Collect all stubs from the store, filter by trust graph
    // For MVP at small scale, iterate all stubs. At scale this would use indices.
    for result in stub_store.iter_all() {
        let (_hash, stub) = match result {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Own stubs shown as a distinct type
        if stub.author_pubkey == *my_pubkey {
            items.push(FeedItem::OwnStub { stub: stub.clone() });
            continue;
        }

        // Silenced authors excluded entirely
        if silenced.contains(&stub.author_pubkey) {
            continue;
        }

        // Trusted authors: full stub with weight
        if let Some((_target, weight)) = trusted.iter().find(|(t, _)| *t == stub.author_pubkey) {
            items.push(FeedItem::TrustStub {
                stub: stub.clone(),
                trust_weight: *weight,
            });
            continue;
        }

        // Watched authors: header only
        if watched.contains(&stub.author_pubkey) {
            items.push(FeedItem::WatchHeader {
                content_hash: stub.content_hash,
                author_pubkey: stub.author_pubkey,
                timestamp: stub.timestamp,
                branch_id: stub.branch_id(),
            });
        }

        // Everyone else: not in feed (the feed goes quiet)
    }

    // Sort: own + trust stubs first (by timestamp desc), then watch headers (by timestamp desc)
    items.sort_by(|a, b| {
        let a_priority = a.is_own() || a.is_trust();
        let b_priority = b.is_own() || b.is_trust();
        match (a_priority, b_priority) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => b.timestamp().cmp(&a.timestamp()),
        }
    });

    items.truncate(limit);
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edges::*;
    use pulse_identity::keypair::Identity;
    use pulse_stub::format::{BranchVector, ReplyEligibility};
    use pulse_stub::signing::create_inline_stub;

    fn setup() -> (Identity, Identity, Identity, Identity, StubStore, GraphStore) {
        let alice = Identity::generate();
        let bob = Identity::generate(); // will be trusted
        let carol = Identity::generate(); // will be watched
        let dave = Identity::generate(); // will be silenced

        let stub_dir = tempfile::tempdir().unwrap();
        let stub_store = StubStore::open(stub_dir.path()).unwrap();
        let graph_store = GraphStore::open_in_memory().unwrap();

        // Leak the tempdir so it doesn't get cleaned up during test
        std::mem::forget(stub_dir);

        (alice, bob, carol, dave, stub_store, graph_store)
    }

    #[test]
    fn trust_stubs_appear_in_feed() {
        let (alice, bob, _carol, _dave, stub_store, graph_store) = setup();

        // Alice trusts Bob
        let edge = create_trust_edge(&alice, &bob.public_key_bytes(), [0u8; 32], 0.9).unwrap();
        store_edge(&graph_store, &edge).unwrap();

        // Bob posts a stub
        let stub = create_inline_stub(&bob, "trusted content", BranchVector::root(), ReplyEligibility::Open).unwrap();
        stub_store.put(&stub).unwrap();

        let feed = build_feed(&alice.public_key_bytes(), &graph_store, &stub_store, 50).unwrap();
        assert_eq!(feed.len(), 1);
        assert!(feed[0].is_trust());
        if let FeedItem::TrustStub { stub: s, trust_weight } = &feed[0] {
            assert_eq!(s.inline_text().unwrap().unwrap(), "trusted content");
            assert!((*trust_weight - 0.9).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn watch_headers_appear_without_content() {
        let (alice, _bob, carol, _dave, stub_store, graph_store) = setup();

        // Alice watches Carol
        let edge = create_watch_edge(&alice, &carol.public_key_bytes()).unwrap();
        store_edge(&graph_store, &edge).unwrap();

        // Carol posts
        let stub = create_inline_stub(&carol, "watched content", BranchVector::root(), ReplyEligibility::Open).unwrap();
        stub_store.put(&stub).unwrap();

        let feed = build_feed(&alice.public_key_bytes(), &graph_store, &stub_store, 50).unwrap();
        assert_eq!(feed.len(), 1);
        assert!(feed[0].is_watch());
        if let FeedItem::WatchHeader { author_pubkey, .. } = &feed[0] {
            assert_eq!(author_pubkey, &carol.public_key_bytes());
        }
    }

    #[test]
    fn silenced_authors_excluded() {
        let (alice, _bob, _carol, dave, stub_store, graph_store) = setup();

        // Alice silences Dave
        let edge = create_silence_edge(&alice, &dave.public_key_bytes()).unwrap();
        store_edge(&graph_store, &edge).unwrap();

        // Dave posts
        let stub = create_inline_stub(&dave, "silenced content", BranchVector::root(), ReplyEligibility::Open).unwrap();
        stub_store.put(&stub).unwrap();

        let feed = build_feed(&alice.public_key_bytes(), &graph_store, &stub_store, 50).unwrap();
        assert_eq!(feed.len(), 0); // Dave's content is excluded
    }

    #[test]
    fn unknown_authors_not_in_feed() {
        let (alice, _bob, _carol, _dave, stub_store, graph_store) = setup();

        // Random stranger posts
        let stranger = Identity::generate();
        let stub = create_inline_stub(&stranger, "stranger content", BranchVector::root(), ReplyEligibility::Open).unwrap();
        stub_store.put(&stub).unwrap();

        let feed = build_feed(&alice.public_key_bytes(), &graph_store, &stub_store, 50).unwrap();
        assert_eq!(feed.len(), 0); // Stranger not in feed
    }

    #[test]
    fn full_feed_with_all_edge_types() {
        let (alice, bob, carol, dave, stub_store, graph_store) = setup();

        // Alice trusts Bob, watches Carol, silences Dave
        store_edge(&graph_store, &create_trust_edge(&alice, &bob.public_key_bytes(), [0u8; 32], 0.9).unwrap()).unwrap();
        store_edge(&graph_store, &create_watch_edge(&alice, &carol.public_key_bytes()).unwrap()).unwrap();
        store_edge(&graph_store, &create_silence_edge(&alice, &dave.public_key_bytes()).unwrap()).unwrap();

        // Everyone posts
        let stranger = Identity::generate();
        stub_store.put(&create_inline_stub(&bob, "from bob", BranchVector::root(), ReplyEligibility::Open).unwrap()).unwrap();
        stub_store.put(&create_inline_stub(&carol, "from carol", BranchVector::root(), ReplyEligibility::Open).unwrap()).unwrap();
        stub_store.put(&create_inline_stub(&dave, "from dave", BranchVector::root(), ReplyEligibility::Open).unwrap()).unwrap();
        stub_store.put(&create_inline_stub(&stranger, "from stranger", BranchVector::root(), ReplyEligibility::Open).unwrap()).unwrap();

        let feed = build_feed(&alice.public_key_bytes(), &graph_store, &stub_store, 50).unwrap();

        // Bob (trusted): full stub. Carol (watched): header. Dave (silenced): excluded. Stranger: excluded.
        assert_eq!(feed.len(), 2);
        assert!(feed[0].is_trust()); // Trust items first
        assert!(feed[1].is_watch());
    }

    #[test]
    fn own_stubs_appear_as_own_type() {
        let (alice, _bob, _carol, _dave, stub_store, graph_store) = setup();

        let stub = create_inline_stub(&alice, "my own post", BranchVector::root(), ReplyEligibility::Open).unwrap();
        stub_store.put(&stub).unwrap();

        let feed = build_feed(&alice.public_key_bytes(), &graph_store, &stub_store, 50).unwrap();
        assert_eq!(feed.len(), 1);
        assert!(feed[0].is_own());
    }

    #[test]
    fn feed_ordered_trust_first_then_by_time() {
        let (alice, bob, carol, _dave, stub_store, graph_store) = setup();

        store_edge(&graph_store, &create_trust_edge(&alice, &bob.public_key_bytes(), [0u8; 32], 0.9).unwrap()).unwrap();
        store_edge(&graph_store, &create_watch_edge(&alice, &carol.public_key_bytes()).unwrap()).unwrap();

        // Carol posts first (watched), then Bob (trusted)
        let carol_stub = create_inline_stub(&carol, "carol first", BranchVector::root(), ReplyEligibility::Open).unwrap();
        stub_store.put(&carol_stub).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));

        let bob_stub = create_inline_stub(&bob, "bob second", BranchVector::root(), ReplyEligibility::Open).unwrap();
        stub_store.put(&bob_stub).unwrap();

        let feed = build_feed(&alice.public_key_bytes(), &graph_store, &stub_store, 50).unwrap();
        assert_eq!(feed.len(), 2);
        // Trust items come first regardless of timestamp
        assert!(feed[0].is_trust());
        assert!(feed[1].is_watch());
    }
}
