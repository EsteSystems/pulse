use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;

use libp2p::gossipsub::{self, MessageAuthenticity};
use libp2p::identity::Keypair;
use libp2p::kad;
use libp2p::kad::store::MemoryStore;
use libp2p::swarm::NetworkBehaviour;

use crate::protocol;

/// Composed libp2p behaviour for a Pulse node.
#[derive(NetworkBehaviour)]
pub struct PulseBehaviour {
    /// Kademlia DHT for peer discovery and content routing.
    pub kademlia: kad::Behaviour<MemoryStore>,
    /// GossipSub for topic-based message propagation.
    pub gossipsub: gossipsub::Behaviour,
    /// Identify protocol for exchanging peer info.
    pub identify: libp2p::identify::Behaviour,
}

impl PulseBehaviour {
    pub fn new(keypair: &Keypair, peer_id: libp2p::PeerId) -> Self {
        // Kademlia
        let store = MemoryStore::new(peer_id);
        let mut kad_config = kad::Config::new(
            libp2p::StreamProtocol::try_from_owned(protocol::DHT_PROTOCOL.to_string())
                .expect("valid protocol"),
        );
        kad_config.set_query_timeout(Duration::from_secs(30));
        let kademlia = kad::Behaviour::with_config(peer_id, store, kad_config);

        // GossipSub
        let message_id_fn = |message: &gossipsub::Message| {
            let mut hasher = DefaultHasher::new();
            message.data.hash(&mut hasher);
            message.source.hash(&mut hasher);
            gossipsub::MessageId::from(hasher.finish().to_be_bytes().to_vec())
        };

        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(protocol::GOSSIPSUB_HEARTBEAT)
            .max_transmit_size(protocol::MAX_MESSAGE_SIZE)
            .validation_mode(gossipsub::ValidationMode::Strict)
            .message_id_fn(message_id_fn)
            .build()
            .expect("valid gossipsub config");

        let gossipsub = gossipsub::Behaviour::new(
            MessageAuthenticity::Signed(keypair.clone()),
            gossipsub_config,
        )
        .expect("valid gossipsub behaviour");

        // Identify
        let identify = libp2p::identify::Behaviour::new(libp2p::identify::Config::new(
            "/pulse/id/1.0.0".to_string(),
            keypair.public(),
        ));

        Self {
            kademlia,
            gossipsub,
            identify,
        }
    }
}
