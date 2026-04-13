use std::time::Duration;

use pulse_identity::keypair::Identity;
use pulse_network::node::{NodeConfig, NodeEvent, PulseNode};
use pulse_stub::format::{BranchVector, ReplyEligibility};
use pulse_stub::signing::create_inline_stub;

#[tokio::test]
async fn two_nodes_exchange_stubs() {
    tracing_subscriber::fmt()
        .with_env_filter("pulse_network=debug")
        .try_init()
        .ok();

    let dir1 = tempfile::tempdir().unwrap();
    let dir2 = tempfile::tempdir().unwrap();

    // Start node 1 on a random port
    let mut node1 = PulseNode::start(NodeConfig {
        listen_port: 0,
        bootstrap_peers: vec![],
        data_dir: dir1.path().to_path_buf(),
        stub_store: None,
        graph_store: None,
        branch_store: None,
    })
    .await
    .unwrap();

    // Wait for node1 to start listening and get its address
    let node1_addr = loop {
        match tokio::time::timeout(Duration::from_secs(5), node1.event_rx.recv()).await {
            Ok(Some(NodeEvent::Listening(addr))) => break addr,
            Ok(Some(_)) => continue,
            _ => panic!("Node 1 didn't start listening"),
        }
    };
    println!("Node 1 listening on: {node1_addr}");

    // Start node 2, bootstrapping to node 1
    let mut node2 = PulseNode::start(NodeConfig {
        listen_port: 0,
        bootstrap_peers: vec![node1_addr],
        data_dir: dir2.path().to_path_buf(),
        stub_store: None,
        graph_store: None,
        branch_store: None,
    })
    .await
    .unwrap();

    // Wait for connection on node 2
    let connected = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match node2.event_rx.recv().await {
                Some(NodeEvent::PeerConnected(_)) => return true,
                Some(_) => continue,
                None => return false,
            }
        }
    })
    .await;
    assert!(connected.unwrap_or(false), "Node 2 didn't connect to Node 1");

    // Give gossipsub mesh time to form
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Create and publish a stub from node 1
    let identity = Identity::generate();
    let stub = create_inline_stub(
        &identity,
        "hello from node 1",
        BranchVector::root(),
        ReplyEligibility::Open,
    )
    .unwrap();

    let expected_hash = stub.content_hash;
    node1.publish_stub(stub).await.unwrap();

    // Wait for node 2 to receive the stub
    let received = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match node2.event_rx.recv().await {
                Some(NodeEvent::StubReceived { content_hash, .. }) => {
                    if content_hash == expected_hash {
                        return true;
                    }
                }
                Some(_) => continue,
                None => return false,
            }
        }
    })
    .await;

    assert!(
        received.unwrap_or(false),
        "Node 2 didn't receive the stub from Node 1"
    );

    // Verify peer counts
    let count1 = node1.peer_count().await.unwrap();
    let count2 = node2.peer_count().await.unwrap();
    assert!(count1 >= 1, "Node 1 should have at least 1 peer");
    assert!(count2 >= 1, "Node 2 should have at least 1 peer");

    // Cleanup
    node1.shutdown().await.unwrap();
    node2.shutdown().await.unwrap();
}
