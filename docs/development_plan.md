# Pulse — Development Plan

*Derived from architecture.md — April 2026*

---

## Overview

This plan breaks the Pulse architecture into four phases across 48 months. Each phase contains milestones. Each milestone contains concrete tasks with deliverables, dependencies, and acceptance criteria.

**Team scaling:** 5-8 engineers (Phase 1) → 8-10 (Phase 2) → 10-12 (Phases 3-4)

**MVP target:** End of Phase 1 — a working P2P network with stub creation, branch structure, trust graph edges, and a desktop client. Two nodes in different locations can exchange stubs and maintain coherent branch state.

---

## Phase 1 — Foundation (Months 1-12)

**Goal:** Working P2P network with stubs, branches, trust graph, and desktop client.
**Team:** 5-8 engineers
**Success metric:** Two geographically separated nodes exchange stubs and maintain coherent branch state.

---

### Milestone 1.1 — Core Identity System (Months 1-2)

The cryptographic identity layer that everything else depends on.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 1.1.1 | Ed25519 keypair generation | `pulse-identity` crate | Generate, serialize, deserialize keypairs. Deterministic from seed for testing. |
| 1.1.2 | Identity persistence | Encrypted keystore on disk | Keypair encrypted at rest with user passphrase (Argon2id KDF). Load/unlock/lock lifecycle. |
| 1.1.3 | Genesis stub creation | Identity bootstrap flow | First stub signed at identity creation. Public key is the identity. Genesis stub committed to local store. |
| 1.1.4 | Signature verification | Verify module | Verify any Ed25519 signature against pubkey. Reject malformed or invalid signatures. Batch verification for sync. |
| 1.1.5 | Recovery keypair generation | Recovery key flow | Optional offline recovery key. Public key committed into genesis stub. Warn user to store securely. |
| 1.1.6 | Key rotation stubs | Rotation stub type | Old key signs rotation stub pointing to new key. Grace period tracking (default 30 days). Trust edge migration logic. |

**Dependencies:** None — this is the foundation.

---

### Milestone 1.2 — Stub Data Model (Months 1-3)

The atomic unit of content — binary format, signing, validation.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 1.2.1 | Stub binary format | `pulse-stub` crate | Implement Appendix A format. Serialize/deserialize. Fixed header + variable content. 320 byte max for inline stubs. |
| 1.2.2 | BLAKE3 content hashing | Hash module | Content hash computed at creation. Deterministic — same content always produces same hash. |
| 1.2.3 | Stub signing | Sign-at-creation flow | Author signature covers content_hash + timestamp + branch_vector. Signature verified on receipt. |
| 1.2.4 | Stub immutability enforcement | Validation rules | Reject any stub mutation after creation. Hash mismatch = reject. Signature mismatch = reject. |
| 1.2.5 | Flags field implementation | Flag parsing/setting | Inline content bit, expiry bit, reply eligibility encoding (2-bit). Reserved bits zeroed. |
| 1.2.6 | Branch vector (basic) | Branch vector serialization | Parent hash reference. Explicit refs to other stubs. Topic signature placeholder (full inference in Phase 2). |
| 1.2.7 | Stub validation pipeline | Ingestion validator | On receipt: verify hash, verify signature, verify timestamp sanity (not future, not absurdly past), verify size constraints. Reject or accept. |
| 1.2.8 | LZ4 compression for inline content | Compression module | Compress/decompress inline UTF-8 content. Verify round-trip fidelity. |

**Dependencies:** 1.1 (keypair and signing)

---

### Milestone 1.3 — Local Storage Layer (Months 2-4)

Persistent storage for stubs, trust graph, and Merkle tree.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 1.3.1 | RocksDB stub store | `pulse-store` crate | Store/retrieve stubs by content_hash. LZ4 compression on values. Batch writes for sync. |
| 1.3.2 | SQLite trust graph store | Trust graph schema + queries | Store trust/watch/silence edges with topic scope and weight. Query edges by source, target, type, topic. |
| 1.3.3 | Merkle tree store (RocksDB) | Tree storage layer | Store tree entries (122 bytes each). Lookup by content_hash. Range queries for sync. |
| 1.3.4 | Branch state store | Branch state schema | Store branch_id, depth, stub_count, last_activity, root_hash, activation_state. Update on stub attachment. |
| 1.3.5 | Storage budget enforcement | Budget manager | Track total storage used. Alert at 80% budget. Begin eviction at 90%. Configurable budget (default 10GB). |
| 1.3.6 | Cache eviction implementation | Eviction engine | Six-tier eviction priority per Section 13.2. Own authored stubs never evicted. Eviction runs in background. |
| 1.3.7 | Author stub retention | Retention policy | Local authored stubs are exempt from all eviction. Separate tracking of authored vs. received stubs. |

**Dependencies:** 1.2 (stub format for storage schema)

---

### Milestone 1.4 — P2P Network Layer (Months 2-5)

The networking backbone — peer discovery, DHT, gossip.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 1.4.1 | libp2p node initialization | `pulse-network` crate | Initialize libp2p host with TCP + QUIC transports. Listen on configurable port. |
| 1.4.2 | Kademlia DHT integration | DHT module | Join DHT. Publish node presence. Query by key. O(log n) routing confirmed in test. |
| 1.4.3 | Protocol-identity rendezvous | Bootstrap discovery | Publish to `BLAKE3("pulse-network-v1")` DHT key. New nodes find peers via this key without seed list. |
| 1.4.4 | Seed node bootstrap | Seed node client | Connect to hardcoded seed list on first launch. Seed nodes return peer list. Graceful fallback if all seeds down. |
| 1.4.5 | Seed attestation exchange | Attestation protocol | On successful sync, exchange signed attestation records. Store attestations. Propagate via gossip. |
| 1.4.6 | GossipSub integration | Topic-based pub/sub | Subscribe to topic meshes. Publish stubs to topic. Receive stubs from subscribed topics. Forward to interested peers. |
| 1.4.7 | Topic-filtered propagation | Forwarding filter | Only forward stubs matching connected peers' declared interest vectors. No blind flooding. |
| 1.4.8 | Content retrieval protocol | Direct retrieval | Lookup stub hash → address hint → direct peer request. DHT fallback if address stale. Fan-out to trust neighborhood if author offline. |
| 1.4.9 | Address hint publishing | DHT address updates | Publish current network address to DHT keyed by public key on every address change. |
| 1.4.10 | Network diversity scoring | Diversity monitor | Track geographic regions, ASNs, and seed lineages of connected peers. Log low diversity warnings. |
| 1.4.11 | Island detection | Isolation monitor | Detect low-diversity peer sets. Trigger active peer seeking across lineages. Support intentional island mode flag. |

**Dependencies:** 1.1 (keypair for node identity), 1.2 (stub format for gossip)

---

### Milestone 1.5 — Merkle Tree and Sync (Months 3-6)

The universal map of stub existence and efficient delta synchronization.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 1.5.1 | Merkle tree construction | Tree builder | Insert stub existence entries (content_hash, author_pubkey_ref, branch_position, address_hint, timestamp). Compute root hash. |
| 1.5.2 | Incremental tree updates | Update engine | New stub insertion updates tree without full rebuild. Root hash updated incrementally. |
| 1.5.3 | Root hash comparison | Sync handshake | Two nodes exchange root hashes. Equal = nothing to sync (two round trips). Different = trigger delta sync. |
| 1.5.4 | Binary search divergence detection | Divergence finder | On hash mismatch, binary search tree to find exact divergence points. Minimize data exchanged. |
| 1.5.5 | Delta sync protocol | Sync engine | Exchange only diverging entries. A node offline for one week syncs with ~85MB at 1M users. Measure and validate. |
| 1.5.6 | Checkpoint creation | Checkpoint manager | Active branches: weekly checkpoints. Dormant branches: monthly. Checkpoint = compact Merkle root encoding full state. |
| 1.5.7 | Delta encoding between checkpoints | Delta store | Store branch state as deltas from last checkpoint. Full history reconstructable from checkpoint + deltas. |

**Dependencies:** 1.3 (storage layer), 1.4 (network for sync)

---

### Milestone 1.6 — Social Graph (Basic) (Months 4-7)

Trust, watch, and silence edges — the structural core.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 1.6.1 | Trust edge creation | Trust edge CRUD | Create signed trust edge (Appendix C schema). Topic-scoped. Weight 0.0-1.0. Store locally. Publish to network. |
| 1.6.2 | Watch edge creation | Watch edge CRUD | Create signed watch edge. Headers-only delivery behavior. Distinct from trust in storage and propagation. |
| 1.6.3 | Silence edge creation | Silence edge CRUD | Create signed silence edge. Filter content from silenced pubkey. Silenced account's graph position remains visible. |
| 1.6.4 | Edge revocation | Revocation stubs | Revoke any edge type via signed revocation stub. Propagate revocation through network. |
| 1.6.5 | Trust weight calculation | Weight engine | `effective_trust_weight = trust_edge_weight * truster_credibility_in_topic`. Basic credibility = stub count in topic (refined in Phase 2). |
| 1.6.6 | One-hop trust propagation | Propagation engine | `derived_trust_weight = direct * intermediary_credibility * hop_decay(0.7)`. User-configurable. Default off. |
| 1.6.7 | Trust neighborhood content delivery | Delivery rules | Full stub content from trust edges delivered to local storage. Watch edges deliver headers only. Silence edges deliver nothing. |
| 1.6.8 | Profile metrics (basic) | Profile data | Trust density (count, pre-weighting), watch count, built-upon score (placeholder), weighted silence score (basic). |

**Dependencies:** 1.2 (stub format for edges), 1.3 (storage), 1.4 (network for propagation)

---

### Milestone 1.7 — Branch Structure (Basic) (Months 5-8)

Basic branch creation and navigation — full inference comes in Phase 2.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 1.7.1 | Explicit branch creation | Branch creation flow | Author creates a root stub that seeds a new branch. Branch ID = root stub content hash. |
| 1.7.2 | Explicit reply attachment | Reply flow | Author sets parent_hash in branch vector to reply to existing stub. Stub attaches to parent's branch. |
| 1.7.3 | Branch state tracking | State manager | Track branch_id, depth, stub_count, last_activity, root_hash, activation_state per Section 3.3. |
| 1.7.4 | Branch Merkle root | Branch integrity | Compute branch-level Merkle root from all attached stubs. Two nodes compare to verify branch consistency. |
| 1.7.5 | Branch delta sync | Branch sync | On branch root mismatch, exchange only diverging stubs. Same mechanism as tree sync, scoped to branch. |
| 1.7.6 | Dormancy detection | Dormancy rules | Branch with no new stubs for 30 days → DORMANT state. Track last_activity. Resumable on new stub. |
| 1.7.7 | CRDT convergence verification | Convergence tests | Two nodes with same stub set converge to identical branch state regardless of operation order. Property-based tests. |

**Dependencies:** 1.2 (stub format), 1.3 (storage), 1.5 (Merkle sync)

---

### Milestone 1.8 — Desktop Client (Months 5-10)

Tauri-based cross-platform desktop application — the first user-facing interface.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 1.8.1 | Tauri app scaffold | Desktop app skeleton | Tauri + web frontend. Rust backend talks to pulse-core crates. Builds on macOS, Linux, Windows. |
| 1.8.2 | Identity creation flow | Onboarding UI | Generate keypair. Set passphrase. Show public key. Optional recovery key generation with clear warnings. |
| 1.8.3 | Stub composition | Post creation UI | Write text (≤ 160 chars inline). Preview. Sign and publish. Show confirmation with content hash. |
| 1.8.4 | Chronological branch view | Branch viewer | Display branch as chronological thread. Show stub content, author pubkey, timestamp. Expand/collapse replies. |
| 1.8.5 | Trust/watch/silence management | Social graph UI | View current edges. Create new trust/watch/silence edges by pubkey. Set topic scope and weight for trust. Revoke edges. |
| 1.8.6 | Feed view (basic) | Feed UI | Chronological feed of stubs from trust edges. Watch edge headers shown separately. Silence edges excluded. |
| 1.8.7 | Profile view | Profile UI | Show pubkey, trust density, watch count. List branches participated in. |
| 1.8.8 | Network status | Status panel | Show connected peers, sync state, storage usage, node type. |
| 1.8.9 | Settings | Settings UI | Storage budget. Relay behavior. Trust propagation toggle. Hop decay factor. |

**Dependencies:** 1.1-1.7 (all core crates)

---

### Milestone 1.9 — Integration Testing and Hardening (Months 9-12)

End-to-end validation of the full Phase 1 system.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 1.9.1 | Multi-node test harness | Test infrastructure | Spin up N nodes in isolated network. Configurable topology. Automated stub creation and verification. |
| 1.9.2 | Geographic sync test | Integration test | Two nodes on different continents exchange stubs. Verify branch coherence within 30 seconds of reconnection. |
| 1.9.3 | Partition and recovery test | Integration test | Partition network into two islands. Create stubs on both sides. Reconnect. Verify CRDT convergence — no data loss, no conflicts. |
| 1.9.4 | Storage budget stress test | Load test | Run node at 95% storage budget with continuous stub ingestion. Verify eviction fires correctly. Verify no data corruption. Verify authored stubs retained. |
| 1.9.5 | Merkle sync efficiency test | Performance test | Node offline for 7 days rejoins. Measure sync data volume. Target: ≤ 85MB at 1M user scale simulation. |
| 1.9.6 | Seed node failure test | Resilience test | Kill all seed nodes. Verify existing connected peers continue operating. Verify new nodes cannot join (expected). Restart seeds. Verify new nodes can join. |
| 1.9.7 | Fuzz testing on stub ingestion | Security test | Fuzz the stub validation pipeline with malformed, oversized, and adversarial inputs. Zero crashes, zero panics. |
| 1.9.8 | Performance benchmarks | Benchmark suite | Stub creation throughput. Signature verification throughput. DHT lookup latency. Gossip propagation latency. Merkle sync throughput. |

**Dependencies:** 1.1-1.8 (full Phase 1 stack)

---

## Phase 2 — Graph Intelligence (Months 13-24)

**Goal:** Branch attachment inference, quality scoring, SNR, feed construction, filtering.
**Team:** 8-10 engineers
**Success metric:** A dormant branch correctly reactivates and surfaces previous participants when a new relevant event occurs.

---

### Milestone 2.1 — Branch Attachment Inference (Months 13-17)

The core novel mechanism — automatic topic-based branch assignment per Section 3.2a.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 2.1.1 | Embedding model integration | Model runtime | Integrate MiniLM-L6-v2-int8 (22MB). Run on desktop. Produce 384-dim embeddings from stub content. |
| 2.1.2 | PQ compression for embeddings | Compression module | Compress 384-dim float → 128-byte PQ representation. Acceptable fidelity loss measured against uncompressed cosine similarity. |
| 2.1.3 | Multi-model support | Model registry | Support multiple recognized models concurrently. Stub carries embeddings from all locally available models. |
| 2.1.4 | Topic-keyed DHT queries | DHT extension | Publish topic embeddings to DHT. Query DHT for branches with similar topic embeddings. Return top-k candidates. |
| 2.1.5 | Attachment scoring algorithm | Scoring engine | Implement weighted scoring: `w_topic(0.5) * cosine_sim + w_user(0.3) * participation + w_ref(0.2) * reference_overlap`. |
| 2.1.6 | Threshold-based attachment decision | Decision engine | ATTACH_THRESHOLD (0.72), BRIDGE_THRESHOLD (0.55). Score > attach → attach. Score > bridge → bridge stub (top-2). Else → new branch. |
| 2.1.7 | Ambiguity detection and author override | Ambiguity flow | Multiple branches within 0.05 of each other → SEMANTICALLY_AMBIGUOUS. Present author with branch options. Author's signed choice overrides inference. |
| 2.1.8 | Trust-weighted consensus across models | Consensus engine | When multiple models produce different attachment decisions, trust-weighted voting determines consensus. Per Section 3.2a. |
| 2.1.9 | Branch centroid maintenance | Centroid updater | Running mean of attached stubs' embeddings per model. Incremental update on each attachment. Track centroid drift from root. |
| 2.1.10 | Cold start behavior | New branch rules | No candidate above 0.4 → new branch. Branch invisible until second stub attaches. Orphan stubs tracked but not displayed as branches. |
| 2.1.11 | Entity extraction (basic) | NER module | Extract PERSON, PLACE, EVENT, CONCEPT entities from stub content. Produce EntityRef records for branch vector. |

**Dependencies:** Phase 1 complete

---

### Milestone 2.2 — Content Objects and Long-Form (Months 13-15)

Enable long-form content beyond the 160-char inline limit.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 2.2.1 | Content object creation | Content object module | Create arbitrary-length UTF-8 documents. BLAKE3 hash as content_ref. Store separately from stub. |
| 2.2.2 | IPFS integration for content objects | IPFS storage layer | Store content objects on local IPFS node. Content-addressed retrieval. Pin locally. |
| 2.2.3 | Stub ↔ content object linking | Link resolution | Stub with content_ref resolves to content object on render. Lazy loading — fetch on demand. |
| 2.2.4 | Content object memory decay | Decay engine | Content objects subject to same pinning weight and decay as stubs. Independent decay — stub can outlive its content object (hash remains, content gone). |
| 2.2.5 | Long-form composition UI | Desktop client update | Compose long-form text. Preview. Publish as stub + content object. Inline for ≤ 160 chars, content object for longer. Seamless to user. |

**Dependencies:** 1.3 (storage), 1.4 (network)

---

### Milestone 2.3 — Quality Scoring (Months 15-19)

Tiered client-side quality evaluation per Section 7.2.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 2.3.1 | Tier 1 — heuristic scoring rules | Rule engine (~50KB) | Detect logical structure markers, epistemic qualifiers, citation presence, assertion density. Score 0.0-1.0. Runs on any device. |
| 2.3.2 | Tier 2 — MiniLM scoring | On-device inference | MiniLM-L6-v2-int8 evaluates coherence, logical structure, epistemic honesty. ~80ms inference on mid-range hardware. |
| 2.3.3 | Tier 3 — SLM scoring | Opt-in SLM | Phi-3-mini on NPU-equipped devices. Batch scoring mode (not per-read). User-initiated. |
| 2.3.4 | Tier auto-detection | Device capability probe | Detect RAM, NPU presence, thermal headroom, battery. Auto-select appropriate tier. User manual override. |
| 2.3.5 | Score stub creation | Score publishing | Scores signed by scoring keypair. Published as score stubs attached to evaluated stub. Declare model version and tier. |
| 2.3.6 | Model attestation protocol | Attestation verification | Score stubs declare which attested model produced them. Verify attestation before trusting score. |
| 2.3.7 | Tier 0 — peer aggregation | Score aggregation | Aggregate score stubs from trust graph peers. Trust-weighted mean. Filter by declared tier if reader prefers. |
| 2.3.8 | Score display | Desktop client update | Show quality score per stub. Expandable detail: which features contributed positively/negatively. Tier indicator. |
| 2.3.9 | Built-upon score tracking | Structural scoring | Track how many stubs reference a given stub as parent or explicit ref. Update built-upon score incrementally. Strongest quality signal. |

**Dependencies:** 2.1 (embedding infrastructure), Phase 1 trust graph

---

### Milestone 2.4 — SNR Layer (Months 18-20)

Signal-to-noise ratio at post, branch, and feed level per Section 8.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 2.4.1 | Post-level SNR calculation | SNR engine | Signal = quality_score + attachment_confidence + build_upon_rate + reference_density. Noise = emotional_trigger_density + ungrounded_assertion + orphan_penalty. |
| 2.4.2 | Branch-level SNR | Branch SNR | Ratio of substantive contribution to reflexive reaction across full branch history. Tracks depth growth vs. volume growth. |
| 2.4.3 | Feed-level SNR | Feed SNR | Ambient readout of trust graph's current quality. Compute as weighted mean of visible stub SNRs. |
| 2.4.4 | SNR visual rendering | Desktop client update | Feed-level SNR as ambient visual state — subtle density/texture change at low SNR. Not a number. Not gamified. |
| 2.4.5 | Squelch mode | Feed squelch | User-configured SNR threshold. Below threshold → feed goes quiet. Empty feed displayed as intentional state, not error. |

**Dependencies:** 2.3 (quality scores feed SNR)

---

### Milestone 2.5 — Feed Construction (Months 17-20)

User-facing feed algorithm per Section 9.1.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 2.5.1 | Default feed algorithm | Feed engine | Priority: (1) trust edge stubs, (2) reactivated dormant branches, (3) bridge stubs in interest graph, (4) watch headers, (5) nothing else. |
| 2.5.2 | Interest vector computation | Interest module | Derive user's interest vector from their trust edges' topic scopes, branch participation history, and recent engagement. |
| 2.5.3 | Dormant branch reactivation surfacing | Reactivation detector | Detect when dormant branch receives new activity. Match against user interest vector. Surface in feed if relevant. |
| 2.5.4 | Bridge stub surfacing | Bridge detector | Identify bridge stubs connecting branches in user's interest graph. Surface with context about which branches are being connected. |
| 2.5.5 | Feed algorithm as swappable module | Plugin architecture | Feed algorithm is open-source, user-readable, user-replaceable. WASM sandbox for community-built algorithms. |
| 2.5.6 | Feed UI overhaul | Desktop client update | Replace chronological feed (Phase 1) with priority-ordered feed. Branch navigation. Temporal metabolism display — "this conversation has been developing for 3 months." |

**Dependencies:** 2.1 (branch inference for reactivation), 2.4 (SNR for squelch)

---

### Milestone 2.6 — Filtering (Basic) (Months 19-22)

Black marker filtering model per Section 10.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 2.6.1 | Lexical filter engine | Word-level filter | Match specific words/phrases. Render as inline redaction bars proportional to word length. Sentence structure preserved. |
| 2.6.2 | Source filter engine | Pubkey filter | Filter by specific keypairs or trust-graph-distance thresholds. Post-level rendering. |
| 2.6.3 | Post-level black marker rendering | Post filter UI | Full post behind solid bar. Metadata visible: author position, timestamp, built-upon count, quality score, SNR. |
| 2.6.4 | Branch-level black marker rendering | Branch filter UI | Branch node visible in graph with shape/size. Contents masked. |
| 2.6.5 | Reveal mechanic | Tap-to-reveal | Single tap reveals temporarily (session) or permanently. Thin border on revealed content. Filter activity private, never transmitted. |
| 2.6.6 | Filter management UI | Filter settings | Create, edit, delete filters. Lexical, source types. Preview filter effect before applying. |

**Dependencies:** Phase 1 client, 2.3 (quality scores for metadata display)

---

### Milestone 2.7 — Branch Lifecycle (Months 20-24)

Dormancy, reactivation, memory decay — the temporal dynamics.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 2.7.1 | Memory decay engine | Decay calculator | Compute pinning weight per stub: references + built-upon + branch activation + trust neighborhood count. |
| 2.7.2 | Pinning weight propagation | Weight updater | Pinning weight updated on every reference, build-upon, branch activation. Decremented on inactivity schedule. |
| 2.7.3 | Content expiry enforcement | Expiry engine | Stubs with expiry flag set: remove from relay storage after configured period. Hash remains in Merkle tree permanently. |
| 2.7.4 | Dormant branch reactivation | Reactivation engine | Dormant branch receives new stub → state transitions to ACTIVE. Notify previous participants (via feed surfacing, not push). |
| 2.7.5 | Branch navigation UI | Branch viewer update | Show current stubs, dormancy indicator, previous activations, bridge connections, participant map with trust density. |
| 2.7.6 | Permanence intent at write time | Stub creation update | Author sets optional expiry at stub creation. Encoded in flags. Immutable after creation. UI clearly communicates permanence implications. |

**Dependencies:** 2.1 (branch inference), 2.5 (feed for reactivation surfacing)

---

## Phase 3 — Mobile (Months 25-36)

**Goal:** Full mobile clients with light node behavior and graceful storage management.
**Team:** 10-12 engineers
**Success metric:** A mobile device with 10GB storage budget operates as a genuine network participant for 90+ days without manual storage management.

---

### Milestone 3.1 — Rust Core FFI Layer (Months 25-28)

Shared Rust core exposed to mobile platforms via FFI.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 3.1.1 | C-ABI FFI boundary definition | `pulse-ffi` crate | Define stable C ABI for all core operations: identity, stub creation, signing, storage, network, trust graph. |
| 3.1.2 | Swift bindings generation | Swift package | Auto-generated Swift bindings from C headers. Type-safe wrappers. Memory management handled correctly across boundary. |
| 3.1.3 | Kotlin bindings generation | Kotlin/JNI package | Auto-generated Kotlin bindings via JNI. Type-safe wrappers. No memory leaks across JNI boundary. |
| 3.1.4 | FFI integration tests | Cross-platform tests | All core operations exercised through FFI from Swift and Kotlin. Round-trip data integrity verified. |

**Dependencies:** Phase 1-2 Rust crates stable

---

### Milestone 3.2 — Light Node Behavior (Months 26-30)

Topic-filtered Merkle tree and graceful degradation for constrained devices.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 3.2.1 | Topic-filtered subtree | Filtered tree module | Maintain Merkle subtree for subscribed branches only. Local lookup for subscribed topics. DHT fallback for others. |
| 3.2.2 | DHT fallback with honest UI | Fallback handler | Stubs outside local subtree show "loading from network" state. 1-3 round trips, 100-500ms latency. No pretense of local certainty. |
| 3.2.3 | Dynamic relay mode switching | Mode manager | Light mode on cellular. Full relay on WiFi + charging. No relay below 20% battery. Auto-switch with user overrides. |
| 3.2.4 | Storage budget management (mobile) | Mobile storage manager | Default 10GB budget. Memory-decay-based eviction. Background eviction during idle. |
| 3.2.5 | Background sync scheduling | Sync scheduler | Merkle tree sync batched during charging/WiFi. Trust neighborhood prefetch during idle on WiFi. Gossip throttled by battery. |
| 3.2.6 | Bloom filter peer queries | Bandwidth optimizer | Query peers with Bloom filters before content requests. Eliminate unnecessary round trips. |

**Dependencies:** 3.1 (FFI layer), 1.5 (Merkle sync)

---

### Milestone 3.3 — iOS Client (Months 27-33)

Native iOS application.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 3.3.1 | iOS app scaffold | Xcode project | Swift UI app. Rust core via FFI. Builds for iOS 16+. |
| 3.3.2 | Identity management (iOS) | iOS keychain integration | Keypair stored in Secure Enclave where available. Biometric unlock. Recovery key export. |
| 3.3.3 | Stub composition (iOS) | iOS post creation | Inline and long-form composition. Publish flow. Preview. |
| 3.3.4 | Feed and branch views (iOS) | iOS feed/branch UI | Full feed with priority ordering. Branch navigation. SNR visual rendering. Black marker filters. |
| 3.3.5 | Social graph management (iOS) | iOS trust/watch/silence UI | Create, view, revoke edges. Profile metrics. |
| 3.3.6 | Background network tasks (iOS) | iOS background execution | BGAppRefreshTask for Merkle sync. BGProcessingTask for trust neighborhood prefetch. Respect iOS background limits. |
| 3.3.7 | Neural Engine quality scoring (iOS) | Core ML integration | Tier 2/3 scoring via Core ML on Neural Engine. Tier auto-detection. Battery-aware scheduling. |
| 3.3.8 | Notification system (iOS) | iOS notifications | Notify on trust graph activity, branch reactivation, direct replies. Respect silence edges. |

**Dependencies:** 3.1 (FFI), 3.2 (light node)

---

### Milestone 3.4 — Android Client (Months 27-33)

Native Android application.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 3.4.1 | Android app scaffold | Android Studio project | Kotlin + Compose UI. Rust core via JNI. Targets Android 10+ (API 29). |
| 3.4.2 | Identity management (Android) | Android Keystore integration | Keypair in hardware-backed Keystore where available. Biometric unlock. Recovery key export. |
| 3.4.3 | Stub composition (Android) | Android post creation | Inline and long-form. Publish. Preview. |
| 3.4.4 | Feed and branch views (Android) | Android feed/branch UI | Full feed. Branch navigation. SNR visual. Filters. |
| 3.4.5 | Social graph management (Android) | Android trust/watch/silence UI | Create, view, revoke edges. Profile metrics. |
| 3.4.6 | Background network tasks (Android) | WorkManager integration | Periodic Merkle sync. Trust neighborhood prefetch. Battery-aware scheduling via WorkManager constraints. |
| 3.4.7 | On-device scoring (Android) | NNAPI / TFLite integration | Tier 1-2 scoring. Tier auto-detection across device spectrum (budget → flagship). Graceful degradation to Tier 0. |
| 3.4.8 | Notification system (Android) | Android notifications | Trust graph activity, branch reactivation, direct replies. Silence edge respect. |

**Dependencies:** 3.1 (FFI), 3.2 (light node)

---

### Milestone 3.5 — NAT Traversal and Mobile Networking (Months 28-32)

Reliable connectivity for mobile nodes behind carrier-grade NAT.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 3.5.1 | libp2p hole punching (mobile) | NAT traversal module | Simultaneous TCP/UDP open via signaling relay. Success rate measurement across major carriers. |
| 3.5.2 | Circuit relay fallback | Relay module | When direct connection fails, relay traffic through intermediary. Content push to peers rather than serve on demand. |
| 3.5.3 | Connectivity state machine | Connection manager | Track connectivity state: direct / relayed / unreachable. Auto-switch. Expose state to UI. |
| 3.5.4 | Mobile data budget | Bandwidth manager | Track cellular data usage. Configurable monthly limit. No relay on cellular by default. Warnings at 80% budget. |

**Dependencies:** 1.4 (libp2p base), 3.2 (mobile node behavior)

---

### Milestone 3.6 — Browser Client (Months 30-34)

Zero-installation web client via WebRTC and WASM.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 3.6.1 | WASM core compilation | `pulse-wasm` crate | Compile Rust core to WebAssembly. All core operations available in browser. |
| 3.6.2 | WebRTC peer connectivity | Browser networking | Connect to Pulse network via WebRTC. DHT participation. Direct content retrieval. |
| 3.6.3 | Ephemeral session management | Session handler | No persistent storage. Session-only state. Clean teardown on tab close. |
| 3.6.4 | Browser UI | Web application | Read-only feed view. Branch navigation. Stub viewing. Identity import (paste keypair) for write access. |
| 3.6.5 | Shareable branch links | Deep linking | `pulse://branch/<branch_id>` links open in browser client. Zero-to-reading in one click. |

**Dependencies:** Phase 1-2 Rust crates, 3.5 (WebRTC for NAT traversal)

---

### Milestone 3.7 — Media Support (Months 31-35)

Content-addressed media objects via IPFS.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 3.7.1 | Media object creation | Media module | Content-addressed media (images, video, audio). BLAKE3 hash. Store on IPFS. |
| 3.7.2 | Media reference in content objects | Content link | Content objects reference media by hash only. No embedding. |
| 3.7.3 | Media retrieval and caching | Media cache | Fetch media by content hash from IPFS. Local cache with LRU eviction within storage budget. |
| 3.7.4 | Media composition UI (all platforms) | Upload flow | Attach media to stub. Preview. Publish. Storage cost indicator. |
| 3.7.5 | Media-free relay support | Relay config | Operators can run text-only relays. No media storage obligation. Advertise media support in node capabilities. |

**Dependencies:** 2.2 (content objects), IPFS integration

---

### Milestone 3.8 — Reply Eligibility (Months 32-34)

Harassment mitigation via structural reply controls per Section 6.3.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 3.8.1 | Reply eligibility enforcement | Protocol enforcement | OPEN / TRUST_GRAPH / TRUSTED_ONLY per stub flags. Ineligible replies exist in graph but don't nest under thread. |
| 3.8.2 | Account-level default setting | Account config | Set default reply eligibility for all new stubs. Override per-stub at creation time. |
| 3.8.3 | Trust graph hop calculation for eligibility | Hop calculator | TRUST_GRAPH mode: compute N-hop distance from author. Configurable N. Efficient calculation using cached trust graph. |
| 3.8.4 | Reply eligibility UI (all platforms) | UI controls | Set eligibility at stub creation. Visual indicator on stubs with restricted eligibility. |

**Dependencies:** 1.6 (trust graph), 1.7 (branch structure)

---

## Phase 4 — Maturity (Months 37-48)

**Goal:** Verified identity, advanced filtering, storage market, censorship resistance, protocol freeze.
**Team:** 10-12 engineers
**Success metric:** A user in a high-censorship environment can access and participate in the network without their traffic being identifiable as Pulse.

---

### Milestone 4.1 — Credential Attestation Framework (Months 37-40)

Verified identity with domain-specific credentials per Section 5.3.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 4.1.1 | Attestation stub type | Attestation protocol | Implement attestation schema (Section 5.3). Verifier signs credential claim for subject pubkey. |
| 4.1.2 | Credential type registry | URI-based registry | Standard credential type URIs for common credentials (academic, professional, institutional). Extensible. |
| 4.1.3 | Attestation verification | Verifier trust | Verify attestation signature. Trust in attestation weighted by trust in verifier identity. Expired attestations flagged. |
| 4.1.4 | Verified profile display | Profile UI update | Show verified credentials on profile. Domain-specific trust density incorporates credential attestations. |
| 4.1.5 | Verifier onboarding | Verifier tooling | Tools for institutions to become verifiers: generate verifier keypair, issue attestations, manage credential lifecycle. |

**Dependencies:** 1.1 (identity), 1.6 (trust graph)

---

### Milestone 4.2 — Group Identity (Months 37-42)

Threshold signatures for collective entities per Section 5.5.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 4.2.1 | FROST threshold signature integration | `pulse-frost` crate | FROST m-of-n threshold signatures producing Ed25519-compatible output. Library evaluation and integration. |
| 4.2.2 | Group genesis stub | Group creation flow | Create group identity with threshold params, member pubkeys (optional), policy hash. All n members sign genesis. |
| 4.2.3 | Threshold signing coordination | Signing protocol | m members contribute partial signatures. Assemble into valid stub signature. Broadcast normally. |
| 4.2.4 | Group member rotation | Rotation flow | New genesis stub signed by current m-of-n. Old group key issues rotation stub. Trust edges migrate via grace period. |
| 4.2.5 | Disclosure spectrum | Group privacy options | Transparent (full member list), pseudonymous (pubkeys only), anonymous (no member info). All valid. |
| 4.2.6 | Group management UI | Group admin interface | Create group, manage members, set threshold, coordinate signing sessions, rotate members. |

**Dependencies:** 1.1 (identity), 5.4 (key rotation). Blocked on FROST library maturity for mobile.

---

### Milestone 4.3 — Advanced Filtering (Months 38-42)

Semantic filtering and moderation subscriptions per Sections 10.4-10.5.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 4.3.1 | Semantic filter engine | Topic classification | Classify stubs by topic category (sports, finance, politics, etc.). Client-side classification using existing embedding models. |
| 4.3.2 | Structural filter engine | Branch-level filter | Filter entire branches or recurring event types. Branch-level black marker rendering. |
| 4.3.3 | Moderation subscription layer | Subscription system | Subscribe to community moderation layers (blocklists, trust standards). Applied as filter layers before personal filters. |
| 4.3.4 | Moderation layer creation tools | Moderator tooling | Create and maintain moderation layers. Publish blocklists as signed stubs. Version and update. |
| 4.3.5 | Filter composition | Filter pipeline | Multiple filter types compose: moderation subscriptions → semantic → structural → lexical → source. Clear rendering at each level. |

**Dependencies:** 2.6 (basic filtering), 2.1 (embedding infrastructure)

---

### Milestone 4.4 — Storage Market (Months 39-44)

Protocol-native storage market per Section 14.2a.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 4.4.1 | Pinning commitment protocol | Commitment stubs | Operator and author sign pinning commitment. Duration, challenge interval, payment ref, escrow ref. Immutable on-graph record. |
| 4.4.2 | Proof-of-storage challenges | Challenge protocol | Random byte-range requests with Merkle inclusion proof. Configurable timeout. Challenge and response published as paired stubs. |
| 4.4.3 | Penalty escrow mechanism | Escrow stubs | Signed escrow stubs acknowledged by both parties. Release conditions: successful completion or documented breach. |
| 4.4.4 | Breach detection and documentation | Breach protocol | Three consecutive failed challenges in 72-hour window = breach. Breach stub with nonces, expected responses, actual responses. Permanent record. |
| 4.4.5 | Operator reputation from commitments | Reputation engine | Breach stubs degrade operator reputation. Successful commitments build it. Visible in operator profile. |
| 4.4.6 | Storage market UI | Market interface | Browse operators. Compare pricing and reputation. Create pinning commitments. Monitor challenge responses. |
| 4.4.7 | Archival node recognition | Archival node type | Recognized node type for institutions. Pin without social reinforcement criteria. Archival pinning visible in network. |

**Dependencies:** 1.3 (storage), 1.4 (network), 1.6 (trust graph for reputation)

---

### Milestone 4.5 — Censorship Resistance (Months 40-45)

Anonymity tools and pluggable transports per Section 11.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 4.5.1 | Onion-routed post submission | Onion routing module | Posts routed through multiple relay hops. Originating IP not visible to first relay. Configurable hop count. User-opted. |
| 4.5.2 | Pluggable transports | Transport plugins | Pulse traffic indistinguishable from generic HTTPS. Pluggable transport framework. At least one production transport (e.g., domain fronting, meek). |
| 4.5.3 | Bridge node distribution | Bridge registry | Unlisted bridge node addresses distributed through trusted channels. For high-censorship environments. |
| 4.5.4 | Jurisdictional blocklist framework | Operator compliance tooling | Operators subscribe to jurisdiction-appropriate hash blocklists. Compliance via hash matching only. No semantic evaluation. |

**Dependencies:** 1.4 (network layer)

---

### Milestone 4.6 — Operator Tooling (Months 41-45)

Tools for running and managing Pulse relay nodes.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 4.6.1 | Operator node configuration | Operator setup tooling | Streamlined setup for always-on relay nodes. Storage, bandwidth, and pinning policy configuration. |
| 4.6.2 | Pinning policy management | Policy engine | Topic-specific pinning. Community trust graph pinning. Broad pinning. CSAM blocklist enforcement (mandatory). |
| 4.6.3 | Operator monitoring dashboard | Admin UI | Connected peers, storage usage, bandwidth, sync state, pinning commitment status, challenge compliance. |
| 4.6.4 | Operator economics dashboard | Revenue tracking | Active pinning commitments, revenue, breach status, reputation score. |
| 4.6.5 | Operator federation tools | Multi-operator coordination | Tools for operator communities: shared pinning policies, coordinated storage market pricing, collective health monitoring. |

**Dependencies:** 4.4 (storage market)

---

### Milestone 4.7 — Protocol Freeze and Foundation (Months 44-48)

Finalize protocol specification and establish governance.

| # | Task | Deliverable | Acceptance Criteria |
|---|------|-------------|---------------------|
| 4.7.1 | Protocol v1.0 specification | Formal spec document | Complete, unambiguous protocol specification. Wire formats, state machines, validation rules, conformance requirements. |
| 4.7.2 | Conformance test suite | Test suite | Any implementation can run conformance tests to verify protocol compliance. Stub format, signature verification, CRDT convergence, sync protocol. |
| 4.7.3 | CSAM blocklist infrastructure | Blocklist distribution | Secure blocklist distribution mechanism. Update propagation. Operator compliance verification. |
| 4.7.4 | Seed node diversity enforcement | Diversity audit tooling | Automated monitoring of seed node diversity thresholds (no country > 25%, no org > 15%, 6+ continents). Alerts on violation. |
| 4.7.5 | Pulse Foundation establishment | Legal entity | Non-profit entity. Mandate: protocol spec, CSAM blocklist, seed node registry, reference implementation. No content authority. |
| 4.7.6 | Model attestation governance process | Governance framework | Process for evaluating new embedding/scoring model proposals. Bias evaluation. Revocation of compromised attestations. |
| 4.7.7 | Third-party implementation guide | Developer documentation | Guide for building alternative Pulse clients and operators. API documentation. Conformance test instructions. |

**Dependencies:** All prior milestones

---

## Cross-Cutting Concerns

These are not milestones but ongoing responsibilities across all phases.

### Security

- Fuzz testing on all ingestion pipelines (stubs, edges, attestations, sync messages)
- Signature verification on every received object — no exceptions
- Key material never leaves encrypted storage except during signing operations
- Adversarial testing: malformed inputs, oversized messages, replay attacks, signature forgery attempts

### Testing Strategy

- **Unit tests:** Every crate, every module. Property-based tests for CRDT convergence and Merkle tree invariants.
- **Integration tests:** Multi-node test harness. Partition testing. Sync verification. Geographic distribution simulation.
- **Load tests:** Scale simulation at 1M, 10M, 100M stubs. Storage budget stress. Network bandwidth under load.
- **Conformance tests:** Protocol compliance verification for any implementation.

### Documentation

- Architecture document (this repo: `docs/architecture.md`) is the source of truth
- Protocol specification developed incrementally alongside implementation
- API documentation for FFI boundary, network protocols, and storage interfaces
- Operator documentation for node setup and management

---

## Dependency Graph Summary

```
Phase 1 (Foundation)
├── 1.1 Identity ──────────────────────────────────┐
├── 1.2 Stub Format ──── depends on 1.1 ──────────┤
├── 1.3 Storage ──── depends on 1.2 ───────────────┤
├── 1.4 Network ──── depends on 1.1, 1.2 ──────────┤
├── 1.5 Merkle Sync ──── depends on 1.3, 1.4 ──────┤
├── 1.6 Social Graph ──── depends on 1.2, 1.3, 1.4 ┤
├── 1.7 Branches ──── depends on 1.2, 1.3, 1.5 ────┤
├── 1.8 Desktop Client ──── depends on 1.1-1.7 ────┤
└── 1.9 Integration ──── depends on 1.1-1.8 ───────┘

Phase 2 (Graph Intelligence)
├── 2.1 Branch Inference ──── depends on Phase 1
├── 2.2 Content Objects ──── depends on 1.3, 1.4
├── 2.3 Quality Scoring ──── depends on 2.1
├── 2.4 SNR ──── depends on 2.3
├── 2.5 Feed ──── depends on 2.1, 2.4
├── 2.6 Filtering ──── depends on Phase 1, 2.3
└── 2.7 Branch Lifecycle ──── depends on 2.1, 2.5

Phase 3 (Mobile)
├── 3.1 FFI Layer ──── depends on Phase 1-2 crates
├── 3.2 Light Node ──── depends on 3.1
├── 3.3 iOS ──── depends on 3.1, 3.2
├── 3.4 Android ──── depends on 3.1, 3.2
├── 3.5 NAT Traversal ──── depends on 1.4, 3.2
├── 3.6 Browser ──── depends on Phase 1-2, 3.5
├── 3.7 Media ──── depends on 2.2
└── 3.8 Reply Eligibility ──── depends on 1.6, 1.7

Phase 4 (Maturity)
├── 4.1 Attestations ──── depends on 1.1, 1.6
├── 4.2 Group Identity ──── depends on 1.1, FROST libs
├── 4.3 Advanced Filtering ──── depends on 2.6, 2.1
├── 4.4 Storage Market ──── depends on 1.3, 1.4, 1.6
├── 4.5 Censorship Resistance ──── depends on 1.4
├── 4.6 Operator Tooling ──── depends on 4.4
└── 4.7 Protocol Freeze ──── depends on all
```

---

*Pulse Development Plan — Working Document*
*Derived from architecture.md*
