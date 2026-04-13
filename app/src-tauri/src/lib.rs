use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::{Mutex, RwLock};

use pulse_graph::edges;
use pulse_graph::feed::{self, FeedItem};
use pulse_graph::profile;
use pulse_identity::keypair::{Identity, PublicIdentity};
use pulse_identity::keystore::EncryptedKeystore;
use pulse_network::node::{NodeConfig, NodeEvent, PulseNode};
use pulse_store::branch_store::BranchStore;
use pulse_store::graph_store::{EdgeType, GraphStore};
use pulse_store::profile_store::{ProfileMeta, ProfileStore};
use pulse_store::stub_store::StubStore;
use pulse_stub::format::{BranchVector, ReplyEligibility};
use pulse_stub::signing::create_inline_stub;

/// Application state shared across Tauri commands.
pub struct AppState {
    identity: RwLock<Option<Identity>>,
    stub_store: Arc<StubStore>,
    graph_store: Arc<GraphStore>,
    branch_store: Arc<BranchStore>,
    profile_store: Arc<ProfileStore>,
    node: Mutex<Option<PulseNode>>,
    listen_addrs: Arc<RwLock<Vec<String>>>,
    data_dir: PathBuf,
}

// ---- DTOs for the frontend ----

#[derive(Debug, Serialize, Deserialize)]
pub struct StubDto {
    pub content_hash: String,
    pub author_pubkey: String,
    pub author_short_id: String,
    pub author_display_name: Option<String>,
    pub timestamp: u64,
    pub text: Option<String>,
    pub is_root: bool,
    pub parent_hash: Option<String>,
    pub branch_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FeedItemDto {
    pub item_type: String, // "own", "trust", or "watch"
    pub stub: Option<StubDto>,
    pub content_hash: Option<String>,
    pub author_pubkey: Option<String>,
    pub author_short_id: Option<String>,
    pub author_display_name: Option<String>,
    pub timestamp: u64,
    pub trust_weight: Option<f32>,
    pub branch_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProfileDto {
    pub pubkey: String,
    pub short_id: String,
    pub display_name: Option<String>,
    pub trust_density: usize,
    pub watch_count: usize,
    pub silence_count: usize,
    pub built_upon_count: usize,
    pub stub_count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EdgeDto {
    pub source_pubkey: String,
    pub target_pubkey: String,
    pub edge_type: String,
    pub weight: f32,
    pub timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkStatusDto {
    pub peer_count: usize,
    pub is_running: bool,
    pub local_peer_id: Option<String>,
    pub listen_addrs: Vec<String>,
}

// ---- Helpers ----

fn resolve_name(profile_store: &ProfileStore, pubkey: &[u8; 32]) -> Option<String> {
    profile_store.get_display_name(pubkey).ok().flatten()
}

fn stub_to_dto(stub: &pulse_stub::format::Stub, profile_store: &ProfileStore) -> StubDto {
    StubDto {
        content_hash: hex::encode(stub.content_hash),
        author_pubkey: hex::encode(stub.author_pubkey),
        author_short_id: hex::encode(&stub.author_pubkey[..8]),
        author_display_name: resolve_name(profile_store, &stub.author_pubkey),
        timestamp: stub.timestamp,
        text: stub.inline_text().ok().flatten(),
        is_root: stub.branch_vector.is_root(),
        parent_hash: if stub.branch_vector.is_root() {
            None
        } else {
            Some(hex::encode(stub.branch_vector.parent_hash))
        },
        branch_id: hex::encode(stub.branch_id()),
    }
}

fn feed_item_to_dto(item: &FeedItem, profile_store: &ProfileStore) -> FeedItemDto {
    match item {
        FeedItem::OwnStub { stub } => FeedItemDto {
            item_type: "own".into(),
            stub: Some(stub_to_dto(stub, profile_store)),
            content_hash: Some(hex::encode(stub.content_hash)),
            author_pubkey: Some(hex::encode(stub.author_pubkey)),
            author_short_id: Some(hex::encode(&stub.author_pubkey[..8])),
            author_display_name: resolve_name(profile_store, &stub.author_pubkey),
            timestamp: stub.timestamp,
            trust_weight: None,
            branch_id: Some(hex::encode(stub.branch_id())),
        },
        FeedItem::TrustStub { stub, trust_weight } => FeedItemDto {
            item_type: "trust".into(),
            stub: Some(stub_to_dto(stub, profile_store)),
            content_hash: Some(hex::encode(stub.content_hash)),
            author_pubkey: Some(hex::encode(stub.author_pubkey)),
            author_short_id: Some(hex::encode(&stub.author_pubkey[..8])),
            author_display_name: resolve_name(profile_store, &stub.author_pubkey),
            timestamp: stub.timestamp,
            trust_weight: Some(*trust_weight),
            branch_id: Some(hex::encode(stub.branch_id())),
        },
        FeedItem::WatchHeader {
            content_hash,
            author_pubkey,
            timestamp,
            branch_id,
        } => FeedItemDto {
            item_type: "watch".into(),
            stub: None,
            content_hash: Some(hex::encode(content_hash)),
            author_pubkey: Some(hex::encode(author_pubkey)),
            author_short_id: Some(hex::encode(&author_pubkey[..8])),
            author_display_name: resolve_name(profile_store, author_pubkey),
            timestamp: *timestamp,
            trust_weight: None,
            branch_id: Some(hex::encode(branch_id)),
        },
    }
}

fn parse_pubkey(hex_str: &str) -> Result<[u8; 32], String> {
    let bytes = hex::decode(hex_str).map_err(|e| format!("Invalid hex: {e}"))?;
    if bytes.len() != 32 {
        return Err(format!("Expected 32 bytes, got {}", bytes.len()));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

// ---- Tauri Commands ----

#[tauri::command]
async fn check_identity(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let id = state.identity.read().await;
    Ok(id.as_ref().map(|i| hex::encode(i.public_key_bytes())))
}

#[tauri::command]
async fn create_identity(state: State<'_, AppState>, passphrase: String) -> Result<String, String> {
    let identity = Identity::generate();
    let pubkey_hex = hex::encode(identity.public_key_bytes());

    let ks = EncryptedKeystore::encrypt(&identity, &passphrase)
        .map_err(|e| e.to_string())?;
    ks.save_to_file(&state.data_dir.join("identity.json"))
        .map_err(|e| e.to_string())?;

    *state.identity.write().await = Some(identity);
    Ok(pubkey_hex)
}

#[tauri::command]
async fn unlock_identity(state: State<'_, AppState>, passphrase: String) -> Result<String, String> {
    let ks_path = state.data_dir.join("identity.json");
    let ks = EncryptedKeystore::load_from_file(&ks_path)
        .map_err(|e| e.to_string())?;
    let identity = ks.decrypt(&passphrase).map_err(|e| e.to_string())?;
    let pubkey_hex = hex::encode(identity.public_key_bytes());
    *state.identity.write().await = Some(identity);

    // Auto-start network if previously configured
    let config_path = state.data_dir.join("network.json");
    if config_path.exists() {
        if let Ok(json) = std::fs::read_to_string(&config_path) {
            if let Ok(config) = serde_json::from_str::<serde_json::Value>(&json) {
                let peers: Vec<String> = config["bootstrap_peers"]
                    .as_array()
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();

                let addrs: Vec<libp2p::Multiaddr> = peers.iter().filter_map(|p| p.parse().ok()).collect();
                let mut node_lock = state.node.lock().await;
                if node_lock.is_none() {
                    match PulseNode::start(NodeConfig {
                        listen_port: 0,
                        bootstrap_peers: addrs,
                        data_dir: state.data_dir.clone(),
                        stub_store: Some(state.stub_store.clone()),
                        graph_store: Some(state.graph_store.clone()),
                        branch_store: Some(state.branch_store.clone()),
                        profile_store: Some(state.profile_store.clone()),
                    }).await {
                        Ok(mut node) => {
                            let listen_addrs = state.listen_addrs.clone();
                            let mut event_rx = std::mem::replace(&mut node.event_rx, tokio::sync::mpsc::channel(256).1);
                            let (tx, rx) = tokio::sync::mpsc::channel(256);
                            node.event_rx = rx;
                            tokio::spawn(async move {
                                while let Some(event) = event_rx.recv().await {
                                    if let NodeEvent::Listening(ref addr) = event {
                                        listen_addrs.write().await.push(addr.to_string());
                                    }
                                    let _ = tx.send(event).await;
                                }
                            });
                            *node_lock = Some(node);
                        }
                        Err(e) => {
                            tracing::warn!("Auto-start network failed: {e}");
                        }
                    }
                }
            }
        }
    }

    Ok(pubkey_hex)
}

#[tauri::command]
async fn has_keystore(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.data_dir.join("identity.json").exists())
}

#[tauri::command]
async fn start_network(
    state: State<'_, AppState>,
    bootstrap_peers: Vec<String>,
) -> Result<String, String> {
    let mut node_lock = state.node.lock().await;
    if node_lock.is_some() {
        return Err("Network already running".into());
    }

    // Save bootstrap peers for auto-reconnect
    let config_path = state.data_dir.join("network.json");
    let config = serde_json::json!({ "bootstrap_peers": &bootstrap_peers });
    let _ = std::fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap_or_default());

    let peers: Vec<libp2p::Multiaddr> = bootstrap_peers
        .iter()
        .filter_map(|p| p.parse().ok())
        .collect();

    let mut node = PulseNode::start(NodeConfig {
        listen_port: 0,
        bootstrap_peers: peers,
        data_dir: state.data_dir.clone(),
        stub_store: Some(state.stub_store.clone()),
        graph_store: Some(state.graph_store.clone()),
        branch_store: Some(state.branch_store.clone()),
        profile_store: Some(state.profile_store.clone()),
    })
    .await
    .map_err(|e| e.to_string())?;

    let peer_id = node.local_peer_id.to_string();

    // Collect listening addresses (arrive as events in the first few seconds)
    let addrs = state.listen_addrs.clone();
    let mut event_rx = std::mem::replace(&mut node.event_rx, tokio::sync::mpsc::channel(256).1);
    let returned_event_tx = {
        let (tx, rx) = tokio::sync::mpsc::channel(256);
        node.event_rx = rx;
        tx
    };

    tokio::spawn(async move {
        // Forward events and capture listen addresses
        while let Some(event) = event_rx.recv().await {
            if let NodeEvent::Listening(ref addr) = event {
                let addr_str = addr.to_string();
                addrs.write().await.push(addr_str);
            }
            let _ = returned_event_tx.send(event).await;
        }
    });

    *node_lock = Some(node);
    Ok(peer_id)
}

#[tauri::command]
async fn publish_stub(
    state: State<'_, AppState>,
    text: String,
    parent_hash: Option<String>,
    reply_eligibility: Option<String>,
) -> Result<StubDto, String> {
    let id_guard = state.identity.read().await;
    let identity = id_guard.as_ref().ok_or("No identity loaded")?;

    let bv = match parent_hash {
        Some(ref ph) => {
            let hash = parse_pubkey(ph)?;
            BranchVector::reply(hash)
        }
        None => BranchVector::root(),
    };

    let re = match reply_eligibility.as_deref() {
        Some("trust_graph") => ReplyEligibility::TrustGraph,
        Some("trusted_only") => ReplyEligibility::TrustedOnly,
        _ => ReplyEligibility::Open,
    };

    let stub = create_inline_stub(identity, &text, bv, re)
        .map_err(|e| e.to_string())?;

    let dto = stub_to_dto(&stub, &state.profile_store);

    // Publish to network if running
    let node_lock = state.node.lock().await;
    if let Some(node) = node_lock.as_ref() {
        node.publish_stub(stub).await.map_err(|e| e.to_string())?;
    } else {
        // Store locally even without network
        state.stub_store.put(&stub).map_err(|e| e.to_string())?;
        if stub.branch_vector.is_root() {
            let _ = state.branch_store.create_branch(&stub.content_hash, stub.timestamp);
        } else {
            let branch_id = state
                .branch_store
                .find_branch_for_stub(&stub.branch_vector.parent_hash)
                .ok()
                .flatten()
                .unwrap_or(stub.branch_vector.parent_hash);
            let _ = state.branch_store.attach_stub(&branch_id, &stub.content_hash, stub.timestamp);
        }
    }

    Ok(dto)
}

#[tauri::command]
async fn get_feed(state: State<'_, AppState>, limit: Option<usize>, show_all: Option<bool>) -> Result<Vec<FeedItemDto>, String> {
    let id_guard = state.identity.read().await;
    let identity = id_guard.as_ref().ok_or("No identity loaded")?;
    let pubkey = identity.public_key_bytes();

    let items = feed::build_feed(
        &pubkey,
        &state.graph_store,
        &state.stub_store,
        limit.unwrap_or(100),
        show_all.unwrap_or(false),
    )
    .map_err(|e| e.to_string())?;

    let ps = &state.profile_store;
    Ok(items.iter().map(|i| feed_item_to_dto(i, ps)).collect())
}

#[tauri::command]
async fn create_edge(
    state: State<'_, AppState>,
    target_pubkey: String,
    edge_type: String,
    weight: Option<f32>,
) -> Result<EdgeDto, String> {
    let id_guard = state.identity.read().await;
    let identity = id_guard.as_ref().ok_or("No identity loaded")?;
    let target = parse_pubkey(&target_pubkey)?;

    let edge = match edge_type.as_str() {
        "trust" => edges::create_trust_edge(identity, &target, [0u8; 32], weight.unwrap_or(0.8))
            .map_err(|e| e.to_string())?,
        "watch" => edges::create_watch_edge(identity, &target)
            .map_err(|e| e.to_string())?,
        "silence" => edges::create_silence_edge(identity, &target)
            .map_err(|e| e.to_string())?,
        _ => return Err(format!("Invalid edge type: {edge_type}")),
    };

    edges::store_edge(&state.graph_store, &edge).map_err(|e| e.to_string())?;

    // Publish to network
    let edge_json = edges::edge_to_json(&edge);
    let node_lock = state.node.lock().await;
    if let Some(node) = node_lock.as_ref() {
        node.publish_edge(edge_json).await.map_err(|e| e.to_string())?;
    }

    Ok(EdgeDto {
        source_pubkey: hex::encode(edge.source_pubkey),
        target_pubkey: hex::encode(edge.target_pubkey),
        edge_type: edge_type.clone(),
        weight: edge.weight,
        timestamp: edge.timestamp,
    })
}

#[tauri::command]
async fn remove_edge(
    state: State<'_, AppState>,
    target_pubkey: String,
    edge_type: String,
) -> Result<bool, String> {
    let id_guard = state.identity.read().await;
    let identity = id_guard.as_ref().ok_or("No identity loaded")?;
    let source = identity.public_key_bytes();
    let target = parse_pubkey(&target_pubkey)?;

    let et = match edge_type.as_str() {
        "trust" => EdgeType::Trust,
        "watch" => EdgeType::Watch,
        "silence" => EdgeType::Silence,
        _ => return Err(format!("Invalid edge type: {edge_type}")),
    };

    edges::revoke_edge(&state.graph_store, &source, &target, et, &[0u8; 32])
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_outbound_edges(state: State<'_, AppState>) -> Result<Vec<EdgeDto>, String> {
    let id_guard = state.identity.read().await;
    let identity = id_guard.as_ref().ok_or("No identity loaded")?;
    let pubkey = identity.public_key_bytes();

    let edges = state
        .graph_store
        .get_outbound(&pubkey)
        .map_err(|e| e.to_string())?;

    Ok(edges
        .iter()
        .map(|e| EdgeDto {
            source_pubkey: hex::encode(e.source_pubkey),
            target_pubkey: hex::encode(e.target_pubkey),
            edge_type: match e.edge_type {
                EdgeType::Trust => "trust".into(),
                EdgeType::Watch => "watch".into(),
                EdgeType::Silence => "silence".into(),
            },
            weight: e.weight,
            timestamp: e.timestamp,
        })
        .collect())
}

#[tauri::command]
async fn get_profile(
    state: State<'_, AppState>,
    pubkey: Option<String>,
) -> Result<ProfileDto, String> {
    let target = match pubkey {
        Some(pk) => parse_pubkey(&pk)?,
        None => {
            let id_guard = state.identity.read().await;
            let identity = id_guard.as_ref().ok_or("No identity loaded")?;
            identity.public_key_bytes()
        }
    };

    let metrics = profile::compute_profile(
        &target,
        &state.graph_store,
        &state.stub_store,
        &state.branch_store,
    )
    .map_err(|e| e.to_string())?;

    Ok(ProfileDto {
        pubkey: hex::encode(target),
        short_id: hex::encode(&target[..8]),
        display_name: resolve_name(&state.profile_store, &target),
        trust_density: metrics.trust_density,
        watch_count: metrics.watch_count,
        silence_count: metrics.silence_count,
        built_upon_count: metrics.built_upon_count,
        stub_count: metrics.stub_count,
    })
}

#[tauri::command]
async fn get_branch_stubs(
    state: State<'_, AppState>,
    branch_id: String,
) -> Result<Vec<StubDto>, String> {
    let bid = parse_pubkey(&branch_id)?;
    let hashes = state
        .branch_store
        .get_branch_stubs(&bid)
        .map_err(|e| e.to_string())?;

    let mut stubs = Vec::new();
    for hash in hashes {
        if let Ok(Some(stub)) = state.stub_store.get(&hash) {
            stubs.push(stub_to_dto(&stub, &state.profile_store));
        }
    }
    Ok(stubs)
}

#[tauri::command]
async fn get_network_status(state: State<'_, AppState>) -> Result<NetworkStatusDto, String> {
    let node_lock = state.node.lock().await;
    let addrs = state.listen_addrs.read().await.clone();
    match node_lock.as_ref() {
        Some(node) => {
            let peer_count = node.peer_count().await.unwrap_or(0);
            Ok(NetworkStatusDto {
                peer_count,
                is_running: true,
                local_peer_id: Some(node.local_peer_id.to_string()),
                listen_addrs: addrs,
            })
        }
        None => Ok(NetworkStatusDto {
            peer_count: 0,
            is_running: false,
            local_peer_id: None,
            listen_addrs: vec![],
        }),
    }
}

#[tauri::command]
async fn get_known_identities(state: State<'_, AppState>) -> Result<Vec<ProfileDto>, String> {
    // Collect all unique author pubkeys from stored stubs
    let mut seen = std::collections::HashSet::new();
    let mut identities = Vec::new();

    for result in state.stub_store.iter_all() {
        let (_hash, stub) = match result {
            Ok(s) => s,
            Err(_) => continue,
        };
        if seen.insert(stub.author_pubkey) {
            let metrics = profile::compute_profile(
                &stub.author_pubkey,
                &state.graph_store,
                &state.stub_store,
                &state.branch_store,
            )
            .map_err(|e| e.to_string())?;

            identities.push(ProfileDto {
                pubkey: hex::encode(stub.author_pubkey),
                short_id: hex::encode(&stub.author_pubkey[..8]),
                display_name: resolve_name(&state.profile_store, &stub.author_pubkey),
                trust_density: metrics.trust_density,
                watch_count: metrics.watch_count,
                silence_count: metrics.silence_count,
                built_upon_count: metrics.built_upon_count,
                stub_count: metrics.stub_count,
            });
        }
    }
    Ok(identities)
}

#[tauri::command]
async fn set_display_name(
    state: State<'_, AppState>,
    name: Option<String>,
) -> Result<(), String> {
    let id_guard = state.identity.read().await;
    let identity = id_guard.as_ref().ok_or("No identity loaded")?;
    let pubkey = identity.public_key_bytes();

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    let meta = ProfileMeta {
        pubkey,
        display_name: name,
        avatar: None,
        timestamp,
    };

    state.profile_store.set_profile(&meta).map_err(|e| e.to_string())?;

    // Publish to network
    let profile_json = serde_json::to_string(&meta).map_err(|e| e.to_string())?;
    let node_lock = state.node.lock().await;
    if let Some(node) = node_lock.as_ref() {
        node.publish_profile(profile_json).await.map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
async fn get_display_name(
    state: State<'_, AppState>,
    pubkey: String,
) -> Result<Option<String>, String> {
    let pk = parse_pubkey(&pubkey)?;
    state.profile_store.get_display_name(&pk).map_err(|e| e.to_string())
}

// ---- App Initialization ----

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let data_dir = dirs_next::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pulse");
    std::fs::create_dir_all(&data_dir).expect("Failed to create data directory");

    let stub_store_path = data_dir.join("stubs");
    std::fs::create_dir_all(&stub_store_path).expect("Failed to create stubs directory");

    let stub_store = Arc::new(
        StubStore::open(&stub_store_path).expect("Failed to open stub store"),
    );
    let graph_store = Arc::new(
        GraphStore::open(&data_dir.join("graph.db")).expect("Failed to open graph store"),
    );
    let branch_store = Arc::new(
        BranchStore::open(&data_dir.join("branches.db")).expect("Failed to open branch store"),
    );
    let profile_store = Arc::new(
        ProfileStore::open(&data_dir.join("profiles.db")).expect("Failed to open profile store"),
    );

    let app_state = AppState {
        identity: RwLock::new(None),
        stub_store,
        graph_store,
        branch_store,
        profile_store,
        node: Mutex::new(None),
        listen_addrs: Arc::new(RwLock::new(Vec::new())),
        data_dir,
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            check_identity,
            create_identity,
            unlock_identity,
            has_keystore,
            start_network,
            publish_stub,
            get_feed,
            create_edge,
            remove_edge,
            get_outbound_edges,
            get_profile,
            get_branch_stubs,
            get_network_status,
            get_known_identities,
            set_display_name,
            get_display_name,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
