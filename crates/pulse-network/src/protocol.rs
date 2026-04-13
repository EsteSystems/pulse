use std::time::Duration;

use libp2p::StreamProtocol;

/// The GossipSub topic for all Pulse stub propagation.
pub const STUBS_TOPIC: &str = "pulse/stubs/v1";

/// The GossipSub topic for social graph edge updates.
pub const EDGES_TOPIC: &str = "pulse/edges/v1";

/// Protocol ID for direct stub request-response.
pub const STUB_REQUEST_PROTOCOL: StreamProtocol =
    StreamProtocol::new("/pulse/stub-request/1.0.0");

/// DHT protocol prefix.
pub const DHT_PROTOCOL: &str = "/pulse/kad/1.0.0";

/// How often to publish our address to DHT.
pub const DHT_PUBLISH_INTERVAL: Duration = Duration::from_secs(300);

/// Gossipsub heartbeat interval.
pub const GOSSIPSUB_HEARTBEAT: Duration = Duration::from_secs(1);

/// Maximum gossipsub message size (512KB — generous for MVP).
pub const MAX_MESSAGE_SIZE: usize = 512 * 1024;
