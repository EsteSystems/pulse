use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use libp2p::gossipsub::{self, IdentTopic};
use libp2p::kad;
use libp2p::multiaddr::Protocol;
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, PeerId, Swarm};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

use pulse_store::branch_store::BranchStore;
use pulse_store::graph_store::GraphStore;
use pulse_store::profile_store::ProfileStore;
use pulse_store::signer_store::{SignerRecord, SignerStore};
use pulse_store::stub_store::StubStore;
use pulse_stub::format::{BranchVector, Stub};
use pulse_stub::signing::verify_stub_signature;
use pulse_stub::validation::{validate_stub, ValidationResult};

use crate::behaviour::{PulseBehaviour, PulseBehaviourEvent};
use crate::messages::PulseMessage;
use crate::protocol;

/// Events emitted by the node to the application layer.
#[derive(Debug, Clone)]
pub enum NodeEvent {
    /// A new stub was received and stored.
    StubReceived {
        content_hash: [u8; 32],
        author_pubkey: [u8; 32],
    },
    /// A social graph edge was received.
    EdgeReceived {
        edge_json: String,
    },
    /// A peer connected.
    PeerConnected(PeerId),
    /// A peer disconnected.
    PeerDisconnected(PeerId),
    /// Node is listening on an address.
    Listening(Multiaddr),
}

/// Commands sent to the node from the application layer.
#[derive(Debug)]
pub enum NodeCommand {
    /// Publish a new stub to the network.
    PublishStub(Stub),
    /// Publish an edge update.
    PublishEdge(String),
    /// Publish a profile update.
    PublishProfile(String),
    /// Request a stub from the network by content hash.
    RequestStub([u8; 32]),
    /// Dial a peer.
    Dial(Multiaddr),
    /// Get the number of connected peers.
    PeerCount(tokio::sync::oneshot::Sender<usize>),
    /// Shutdown the node.
    Shutdown,
}

/// Configuration for a Pulse node.
pub struct NodeConfig {
    /// Port to listen on (0 = random).
    pub listen_port: u16,
    /// Bootstrap peers to connect to on startup.
    pub bootstrap_peers: Vec<Multiaddr>,
    /// Path to the data directory (used if stores not provided).
    pub data_dir: std::path::PathBuf,
    /// Shared stores (if provided, data_dir is not used for stores).
    pub stub_store: Option<Arc<StubStore>>,
    pub graph_store: Option<Arc<GraphStore>>,
    pub branch_store: Option<Arc<BranchStore>>,
    pub profile_store: Option<Arc<ProfileStore>>,
    pub signer_store: Option<Arc<SignerStore>>,
}

/// A running Pulse P2P node.
pub struct PulseNode {
    pub command_tx: mpsc::Sender<NodeCommand>,
    pub event_rx: mpsc::Receiver<NodeEvent>,
    pub local_peer_id: PeerId,
}

impl PulseNode {
    /// Start a new Pulse node. Returns the node handle and spawns the event loop.
    pub async fn start(config: NodeConfig) -> Result<Self, Box<dyn std::error::Error>> {
        // Generate libp2p keypair
        let id_keys = libp2p::identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(id_keys.public());
        info!(%local_peer_id, "Starting Pulse node");

        // Build swarm
        let mut swarm = libp2p::SwarmBuilder::with_existing_identity(id_keys.clone())
            .with_tokio()
            .with_tcp(
                libp2p::tcp::Config::default(),
                libp2p::noise::Config::new,
                libp2p::yamux::Config::default,
            )?
            .with_quic()
            .with_behaviour(|_keypair| {
                PulseBehaviour::new(&id_keys, local_peer_id)
            })?
            .with_swarm_config(|cfg| {
                cfg.with_idle_connection_timeout(Duration::from_secs(300))
            })
            .build();

        // Subscribe to gossipsub topics
        let stubs_topic = IdentTopic::new(protocol::STUBS_TOPIC);
        let edges_topic = IdentTopic::new(protocol::EDGES_TOPIC);
        swarm.behaviour_mut().gossipsub.subscribe(&stubs_topic)?;
        swarm.behaviour_mut().gossipsub.subscribe(&edges_topic)?;

        // Listen
        let listen_addr: Multiaddr = format!("/ip4/0.0.0.0/tcp/{}", config.listen_port)
            .parse()
            .unwrap();
        swarm.listen_on(listen_addr)?;

        // Also listen on QUIC
        let quic_addr: Multiaddr = format!("/ip4/0.0.0.0/udp/{}/quic-v1", config.listen_port)
            .parse()
            .unwrap();
        let _ = swarm.listen_on(quic_addr);

        // Use provided stores or open new ones
        let stub_store = match config.stub_store {
            Some(s) => s,
            None => {
                let stub_store_path = config.data_dir.join("stubs");
                std::fs::create_dir_all(&stub_store_path)?;
                Arc::new(StubStore::open(&stub_store_path)?)
            }
        };

        let graph_store = match config.graph_store {
            Some(s) => s,
            None => Arc::new(GraphStore::open(&config.data_dir.join("graph.db"))?),
        };

        let branch_store = match config.branch_store {
            Some(s) => s,
            None => Arc::new(BranchStore::open(&config.data_dir.join("branches.db"))?),
        };

        let profile_store = match config.profile_store {
            Some(s) => s,
            None => Arc::new(ProfileStore::open(&config.data_dir.join("profiles.db"))?),
        };

        let signer_store = match config.signer_store {
            Some(s) => s,
            None => Arc::new(SignerStore::open(&config.data_dir.join("signers.db"))?),
        };

        // Channels
        let (command_tx, command_rx) = mpsc::channel(256);
        let (event_tx, event_rx) = mpsc::channel(256);

        // Spawn event loop
        tokio::spawn(event_loop(
            swarm,
            command_rx,
            event_tx,
            stub_store,
            graph_store,
            branch_store,
            profile_store,
            signer_store,
            config.bootstrap_peers,
            stubs_topic,
            edges_topic,
        ));

        Ok(Self {
            command_tx,
            event_rx,
            local_peer_id,
        })
    }

    /// Publish a stub to the network.
    pub async fn publish_stub(&self, stub: Stub) -> Result<(), mpsc::error::SendError<NodeCommand>> {
        self.command_tx.send(NodeCommand::PublishStub(stub)).await
    }

    /// Publish an edge update.
    pub async fn publish_edge(&self, edge_json: String) -> Result<(), mpsc::error::SendError<NodeCommand>> {
        self.command_tx.send(NodeCommand::PublishEdge(edge_json)).await
    }

    /// Publish a profile update.
    pub async fn publish_profile(&self, profile_json: String) -> Result<(), mpsc::error::SendError<NodeCommand>> {
        self.command_tx.send(NodeCommand::PublishProfile(profile_json)).await
    }

    /// Dial a peer.
    pub async fn dial(&self, addr: Multiaddr) -> Result<(), mpsc::error::SendError<NodeCommand>> {
        self.command_tx.send(NodeCommand::Dial(addr)).await
    }

    /// Get connected peer count.
    pub async fn peer_count(&self) -> Result<usize, Box<dyn std::error::Error>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.command_tx.send(NodeCommand::PeerCount(tx)).await?;
        Ok(rx.await?)
    }

    /// Shutdown the node.
    pub async fn shutdown(&self) -> Result<(), mpsc::error::SendError<NodeCommand>> {
        self.command_tx.send(NodeCommand::Shutdown).await
    }
}

async fn event_loop(
    mut swarm: Swarm<PulseBehaviour>,
    mut command_rx: mpsc::Receiver<NodeCommand>,
    event_tx: mpsc::Sender<NodeEvent>,
    stub_store: Arc<StubStore>,
    graph_store: Arc<GraphStore>,
    branch_store: Arc<BranchStore>,
    profile_store: Arc<ProfileStore>,
    signer_store: Arc<SignerStore>,
    bootstrap_peers: Vec<Multiaddr>,
    stubs_topic: IdentTopic,
    edges_topic: IdentTopic,
) {
    // Connect to bootstrap peers
    for addr in &bootstrap_peers {
        info!("Dialing bootstrap peer: {addr}");
        if let Err(e) = swarm.dial(addr.clone()) {
            warn!("Failed to dial bootstrap peer {addr}: {e}");
        }
    }

    // Add bootstrap peers to Kademlia
    for addr in &bootstrap_peers {
        if let Some(Protocol::P2p(peer_id)) = addr.iter().last() {
            swarm
                .behaviour_mut()
                .kademlia
                .add_address(&peer_id, addr.clone());
        }
    }

    // Bootstrap Kademlia
    if !bootstrap_peers.is_empty() {
        if let Err(e) = swarm.behaviour_mut().kademlia.bootstrap() {
            debug!("Kademlia bootstrap not started (need more peers): {e}");
        }
    }

    loop {
        tokio::select! {
            event = swarm.select_next_some() => {
                handle_swarm_event(
                    event,
                    &mut swarm,
                    &event_tx,
                    &stub_store,
                    &graph_store,
                    &branch_store,
                    &profile_store,
                    &signer_store,
                    &stubs_topic,
                    &edges_topic,
                ).await;
            }
            command = command_rx.recv() => {
                match command {
                    Some(NodeCommand::PublishStub(stub)) => {
                        handle_publish_stub(
                            &mut swarm,
                            &stub_store,
                            &branch_store,
                            &signer_store,
                            &stubs_topic,
                            stub,
                        ).await;
                    }
                    Some(NodeCommand::PublishEdge(edge_json)) => {
                        let msg = PulseMessage::EdgeUpdate { edge_json };
                        if let Err(e) = swarm.behaviour_mut().gossipsub.publish(
                            edges_topic.clone(),
                            msg.serialize(),
                        ) {
                            warn!("Failed to publish edge: {e}");
                        }
                    }
                    Some(NodeCommand::PublishProfile(profile_json)) => {
                        let msg = PulseMessage::ProfileUpdate { profile_json };
                        if let Err(e) = swarm.behaviour_mut().gossipsub.publish(
                            stubs_topic.clone(),
                            msg.serialize(),
                        ) {
                            warn!("Failed to publish profile: {e}");
                        }
                    }
                    Some(NodeCommand::Dial(addr)) => {
                        if let Some(Protocol::P2p(peer_id)) = addr.iter().last() {
                            swarm.behaviour_mut().kademlia.add_address(&peer_id, addr.clone());
                        }
                        if let Err(e) = swarm.dial(addr) {
                            warn!("Failed to dial: {e}");
                        }
                    }
                    Some(NodeCommand::RequestStub(hash)) => {
                        debug!("Stub request for {}", hex::encode(hash));
                        // For MVP, stub retrieval is via gossipsub republish request
                        // Full request-response protocol is a future enhancement
                    }
                    Some(NodeCommand::PeerCount(reply)) => {
                        let count = swarm.connected_peers().count();
                        let _ = reply.send(count);
                    }
                    Some(NodeCommand::Shutdown) => {
                        info!("Node shutting down");
                        break;
                    }
                    None => break,
                }
            }
        }
    }
}

async fn handle_swarm_event(
    event: SwarmEvent<PulseBehaviourEvent>,
    swarm: &mut Swarm<PulseBehaviour>,
    event_tx: &mpsc::Sender<NodeEvent>,
    stub_store: &Arc<StubStore>,
    graph_store: &Arc<GraphStore>,
    branch_store: &Arc<BranchStore>,
    profile_store: &Arc<ProfileStore>,
    signer_store: &Arc<SignerStore>,
    stubs_topic: &IdentTopic,
    edges_topic: &IdentTopic,
) {
    match event {
        SwarmEvent::NewListenAddr { address, .. } => {
            let local_peer_id = *swarm.local_peer_id();
            let full_addr = address.clone().with(Protocol::P2p(local_peer_id));
            info!("Listening on {full_addr}");
            let _ = event_tx.send(NodeEvent::Listening(full_addr)).await;
        }
        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
            info!("Connected to {peer_id}");
            swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
            let _ = event_tx.send(NodeEvent::PeerConnected(peer_id)).await;

            // Republish all local stubs and edges so the new peer gets history
            let stub_store_clone = stub_store.clone();
            let graph_store_clone = graph_store.clone();
            let stubs_topic_clone = stubs_topic.clone();
            let edges_topic_clone = edges_topic.clone();
            let gossipsub = &mut swarm.behaviour_mut().gossipsub;

            // Republish stubs
            let mut republished = 0;
            for result in stub_store_clone.iter_all() {
                let (_hash, stub) = match result {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                if let Ok(stub_bytes) = stub.to_bytes() {
                    let msg = PulseMessage::NewStub { stub_bytes };
                    let _ = gossipsub.publish(stubs_topic_clone.clone(), msg.serialize());
                    republished += 1;
                }
            }

            // Republish edges
            if let Ok(edges) = graph_store_clone.get_all_edges() {
                for edge in &edges {
                    let edge_json = serde_json::to_string(edge).unwrap_or_default();
                    let msg = PulseMessage::EdgeUpdate { edge_json };
                    let _ = gossipsub.publish(edges_topic_clone.clone(), msg.serialize());
                }
            }

            // Republish profiles
            if let Ok(profiles) = profile_store.get_all_profiles() {
                for meta in &profiles {
                    let profile_json = serde_json::to_string(meta).unwrap_or_default();
                    let msg = PulseMessage::ProfileUpdate { profile_json };
                    let _ = gossipsub.publish(stubs_topic_clone.clone(), msg.serialize());
                }
            }

            if republished > 0 {
                info!("Republished {republished} stubs to new peer {peer_id}");
            }
        }
        SwarmEvent::ConnectionClosed { peer_id, .. } => {
            debug!("Disconnected from {peer_id}");
            let _ = event_tx.send(NodeEvent::PeerDisconnected(peer_id)).await;
        }
        SwarmEvent::Behaviour(PulseBehaviourEvent::Gossipsub(
            gossipsub::Event::Message { message, .. },
        )) => {
            handle_gossip_message(
                &message, event_tx, stub_store, graph_store, branch_store, profile_store, signer_store,
            )
            .await;
        }
        SwarmEvent::Behaviour(PulseBehaviourEvent::Kademlia(
            kad::Event::RoutingUpdated { peer, .. },
        )) => {
            debug!("Kademlia routing updated for {peer}");
        }
        SwarmEvent::Behaviour(PulseBehaviourEvent::Identify(
            libp2p::identify::Event::Received { peer_id, info, .. },
        )) => {
            debug!("Identified peer {peer_id}: {}", info.protocol_version);
            // Add observed addresses to Kademlia
            for addr in info.listen_addrs {
                swarm
                    .behaviour_mut()
                    .kademlia
                    .add_address(&peer_id, addr);
            }
        }
        _ => {}
    }
}

async fn handle_gossip_message(
    message: &gossipsub::Message,
    event_tx: &mpsc::Sender<NodeEvent>,
    stub_store: &Arc<StubStore>,
    graph_store: &Arc<GraphStore>,
    branch_store: &Arc<BranchStore>,
    profile_store: &Arc<ProfileStore>,
    signer_store: &Arc<SignerStore>,
) {
    let msg = match PulseMessage::deserialize(&message.data) {
        Ok(m) => m,
        Err(e) => {
            warn!("Failed to deserialize gossip message: {e}");
            return;
        }
    };

    match msg {
        PulseMessage::NewStub { stub_bytes } => {
            let stub = match Stub::from_bytes(&stub_bytes) {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to deserialize stub: {e}");
                    return;
                }
            };

            // Validate
            if !validate_stub(&stub).is_valid() {
                warn!(
                    "Rejected invalid stub {}",
                    hex::encode(stub.content_hash)
                );
                return;
            }

            // Always record the signer (even if content already exists)
            let _ = signer_store.add_signer(&SignerRecord {
                content_hash: stub.content_hash,
                author_pubkey: stub.author_pubkey,
                timestamp: stub.timestamp,
                signature: stub.signature,
            });

            // Store content (dedup — same text stored once)
            if stub_store.contains(&stub.content_hash).unwrap_or(false) {
                debug!(
                    "Dedup: new signer for existing stub {}",
                    hex::encode(&stub.content_hash[..8]),
                );
                // Still emit event so feed updates
                let _ = event_tx
                    .send(NodeEvent::StubReceived {
                        content_hash: stub.content_hash,
                        author_pubkey: stub.author_pubkey,
                    })
                    .await;
                return;
            }

            if let Err(e) = stub_store.put(&stub) {
                error!("Failed to store stub: {e}");
                return;
            }

            // Update branch state
            if stub.branch_vector.is_root() {
                let _ = branch_store.create_branch(&stub.content_hash, stub.timestamp);
            } else {
                // For explicit replies, branch_id is the parent's branch
                // In MVP we use parent_hash as branch_id lookup
                let branch_id = match branch_store.find_branch_for_stub(&stub.branch_vector.parent_hash) {
                    Ok(Some(id)) => id,
                    Ok(None) => stub.branch_vector.parent_hash, // parent is the root
                    Err(_) => stub.branch_vector.parent_hash,
                };
                let _ = branch_store.attach_stub(&branch_id, &stub.content_hash, stub.timestamp);
            }

            debug!(
                "Stored stub {} from {}",
                hex::encode(&stub.content_hash[..8]),
                hex::encode(&stub.author_pubkey[..8]),
            );

            let _ = event_tx
                .send(NodeEvent::StubReceived {
                    content_hash: stub.content_hash,
                    author_pubkey: stub.author_pubkey,
                })
                .await;
        }
        PulseMessage::EdgeUpdate { edge_json } => {
            // Parse and store the edge
            if let Ok(edge) = serde_json::from_str::<pulse_store::graph_store::Edge>(&edge_json) {
                if let Err(e) = graph_store.put_edge(&edge) {
                    error!("Failed to store edge: {e}");
                    return;
                }
                debug!(
                    "Stored edge {:?} from {} -> {}",
                    edge.edge_type,
                    hex::encode(&edge.source_pubkey[..8]),
                    hex::encode(&edge.target_pubkey[..8]),
                );
                let _ = event_tx.send(NodeEvent::EdgeReceived { edge_json }).await;
            }
        }
        PulseMessage::ProfileUpdate { profile_json } => {
            if let Ok(meta) = serde_json::from_str::<pulse_store::profile_store::ProfileMeta>(&profile_json) {
                if let Err(e) = profile_store.set_profile(&meta) {
                    error!("Failed to store profile: {e}");
                    return;
                }
                debug!(
                    "Stored profile for {}{}",
                    hex::encode(&meta.pubkey[..8]),
                    meta.display_name.as_ref().map(|n| format!(" ({})", n)).unwrap_or_default(),
                );
            }
        }
        _ => {}
    }
}

async fn handle_publish_stub(
    swarm: &mut Swarm<PulseBehaviour>,
    stub_store: &Arc<StubStore>,
    branch_store: &Arc<BranchStore>,
    signer_store: &Arc<SignerStore>,
    stubs_topic: &IdentTopic,
    stub: Stub,
) {
    // Record signer
    let _ = signer_store.add_signer(&SignerRecord {
        content_hash: stub.content_hash,
        author_pubkey: stub.author_pubkey,
        timestamp: stub.timestamp,
        signature: stub.signature,
    });

    // Store content locally
    if let Err(e) = stub_store.put(&stub) {
        error!("Failed to store own stub: {e}");
        return;
    }

    // Update branch state
    if stub.branch_vector.is_root() {
        let _ = branch_store.create_branch(&stub.content_hash, stub.timestamp);
    } else {
        let branch_id = match branch_store.find_branch_for_stub(&stub.branch_vector.parent_hash) {
            Ok(Some(id)) => id,
            Ok(None) => stub.branch_vector.parent_hash,
            Err(_) => stub.branch_vector.parent_hash,
        };
        let _ = branch_store.attach_stub(&branch_id, &stub.content_hash, stub.timestamp);
    }

    // Publish to network
    let stub_bytes = match stub.to_bytes() {
        Ok(b) => b,
        Err(e) => {
            error!("Failed to serialize stub for gossip: {e}");
            return;
        }
    };

    let msg = PulseMessage::NewStub { stub_bytes };
    if let Err(e) = swarm
        .behaviour_mut()
        .gossipsub
        .publish(stubs_topic.clone(), msg.serialize())
    {
        warn!("Failed to gossip stub: {e}");
    } else {
        info!(
            "Published stub {}",
            hex::encode(&stub.content_hash[..8]),
        );
    }
}
