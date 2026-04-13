# Pulse — MVP Plan

*April 2026*

---

## Goal

Ship a desktop application to a small group of early testers that delivers the core Pulse experience: identity as a keypair, stubs as speech acts, trust/watch/silence as structurally distinct social edges, and a feed shaped entirely by trust topology. No algorithms. No engagement optimization. The feed earns its presence or goes quiet.

The MVP proves the thesis: **trust-topological feed construction feels meaningfully different from algorithmic or chronological feeds.**

---

## Scope

### In

| Feature | Notes |
|---|---|
| Ed25519 identity | Generate, persist (encrypted), sign, verify |
| Stubs | Inline text up to 300 chars, immutable, signed, BLAKE3 hashed |
| P2P networking | libp2p, Kademlia DHT, GossipSub, topic-filtered propagation |
| Merkle tree + delta sync | Stub existence map, efficient reconnection sync |
| Trust edges | Full content delivery, topic-scoped, weighted |
| Watch edges | Header-only delivery, content on demand |
| Silence edges | No delivery, graph position preserved |
| Explicit branches | Reply via parent_hash, branch state tracking, chronological thread view |
| Trust-shaped feed | Trust = full stubs, watch = headers, silence = excluded |
| Profile metrics | Trust density (basic), watch count, built-upon count (basic) |
| Desktop client | Tauri (macOS, Linux, Windows), polished enough for real daily use |
| Multi-node test deployment | 3-5 nodes on VPS cluster for tester group |

### Out (deferred)

| Feature | Why deferred |
|---|---|
| Branch attachment inference | Explicit replies sufficient for MVP; inference is Phase 2 complexity |
| Quality scoring / SNR / squelch | Feed shaping by trust topology is the MVP thesis, not quality signals |
| Filtering / black markers | Small trusted tester group doesn't need content filtering yet |
| Content objects / long-form | 300 chars covers substantial micro-posts; long-form is Phase 2 |
| Memory decay | Store everything; decay matters at scale, not with early testers |
| Key rotation / recovery | Acceptable risk for early testers; document manual recovery procedure |
| Mobile / browser clients | Desktop-only; testers use laptops |
| Media | Text-only; the discourse experience doesn't require images to prove the thesis |
| Group identity / attestations | Phase 4 features |
| Storage market / operator economics | Not relevant at tester scale |
| Censorship resistance | Not needed for initial test deployment |

---

## Architecture Decisions (MVP-specific)

**Inline content limit: 300 characters.** This changes the stub binary format from the architecture spec:
- Max inline content after LZ4 compression: ~250 bytes (300 chars typical Latin text compresses to ~200-250 bytes)
- Max stub size: ~430 bytes (header ~180 + inline ~250)
- This is an MVP parameter — the protocol spec will finalize the limit based on tester feedback

**No content objects.** All content is inline. No IPFS dependency for MVP. This removes an entire subsystem.

**No branch inference.** Branches are explicit: you reply to a stub (parent_hash set) or you create a root stub (new branch). The three-dimensional inference engine is deferred entirely.

**Simplified trust weight.** `effective_trust_weight = edge_weight` for MVP. Topic-scoped credibility weighting deferred — requires quality scoring infrastructure that's out of scope.

**No one-hop trust propagation.** Direct edges only. Propagation requires calibration with real social graph data.

**Storage: keep everything.** No eviction, no decay. At tester scale (~10-50 users, months of usage), total storage will be under 100MB. Not worth the complexity.

**Deployment: VPS cluster.** 3-5 always-on nodes on VPS instances. Testers' desktop clients connect to these as bootstrap peers. Nodes also connect to each other. The VPS nodes act as seed nodes and full relay nodes for the test network.

---

## Milestones

### M1 — Core Crates (Week 1)

The foundational Rust libraries. No networking, no UI — pure data model and cryptography.

| # | Task | Deliverable | Done when |
|---|------|-------------|-----------|
| M1.1 | Project scaffolding | Cargo workspace with crate structure | `pulse-identity`, `pulse-stub`, `pulse-store`, `pulse-network`, `pulse-graph`, `pulse-core` crates created. CI runs `cargo check`. |
| M1.2 | Ed25519 keypair generation | `pulse-identity` crate | Generate keypair, serialize/deserialize, deterministic from seed (testing). Uses `ed25519-dalek`. |
| M1.3 | Encrypted keystore | Identity persistence | Keypair encrypted at rest with Argon2id-derived key from user passphrase. Load, unlock, lock. |
| M1.4 | BLAKE3 content hashing | Hash module in `pulse-stub` | Deterministic content hashing. Same content = same hash, always. |
| M1.5 | Stub binary format | `pulse-stub` crate | Serialize/deserialize per Appendix A (adjusted for 300 char inline limit). Fixed header + variable content. |
| M1.6 | Stub signing and verification | Sign/verify module | Author signature covers content_hash + timestamp + branch_vector. Verify on receipt. Reject invalid. |
| M1.7 | Stub validation pipeline | Ingestion validator | Verify hash, verify signature, verify timestamp sanity, verify size constraints. Accept or reject with reason. |
| M1.8 | LZ4 compression | Compression module | Compress/decompress inline UTF-8. Round-trip fidelity verified. |
| M1.9 | RocksDB stub store | `pulse-store` crate | Store/retrieve stubs by content_hash. LZ4 values. Batch writes. |
| M1.10 | SQLite trust graph store | Graph storage | Trust/watch/silence edges with topic scope and weight. Query by source, target, type. |
| M1.11 | Branch state store | Branch storage | branch_id, stub_count, last_activity, root_hash. Update on stub attachment. |
| M1.12 | Unit + property-based tests | Test suite | Full coverage on serialization round-trips. Property tests: any valid stub serializes and deserializes to identical bytes. |

---

### M2 — P2P Networking (Weeks 2-3)

Nodes find each other, exchange stubs, and maintain consistent state.

| # | Task | Deliverable | Done when |
|---|------|-------------|-----------|
| M2.1 | libp2p node initialization | `pulse-network` crate | Initialize libp2p host with TCP + QUIC. Listen on configurable port. Node identity derived from Pulse keypair. |
| M2.2 | Kademlia DHT integration | DHT module | Join DHT, publish presence, query by key. Configurable bootstrap peers list. |
| M2.3 | Hardcoded seed peer list | Bootstrap config | Configurable seed peer list (will be VPS node addresses for MVP). Fallback behavior if seeds unreachable. |
| M2.4 | GossipSub integration | Pub/sub module | Subscribe to topic meshes. Publish stubs to topic. Receive from subscribed topics. |
| M2.5 | Stub propagation | Gossip handler | New locally-created stub → sign → publish to GossipSub. Received stub → validate → store → forward to interested peers. |
| M2.6 | Trust edge propagation | Edge gossip | Trust/watch/silence edges published and received via gossip. Validated on receipt (signature check). |
| M2.7 | Merkle tree construction | Tree module | Insert stub existence entries (content_hash, author_pubkey_ref, branch_position, address_hint, timestamp). Compute root hash. |
| M2.8 | Incremental tree updates | Tree updater | New stub → tree updated without rebuild. Root hash updated incrementally. |
| M2.9 | Delta sync protocol | Sync engine | Root hash comparison → binary search divergence → exchange only differing entries. |
| M2.10 | Content retrieval (direct) | Retrieval protocol | Lookup stub hash → address hint → direct peer request → content returned. DHT fallback if address stale. |
| M2.11 | Two-node sync test | Integration test | Two nodes on localhost create stubs independently. Connect. Verify full convergence within 5 seconds. |
| M2.12 | Five-node mesh test | Integration test | Five nodes, various connection topologies. Stubs created on any node reach all nodes. Partition one node, reconnect, verify sync. |

---

### M3 — Social Graph and Feed (Week 3-4)

Trust/watch/silence edges and the trust-shaped feed — the core differentiator.

| # | Task | Deliverable | Done when |
|---|------|-------------|-----------|
| M3.1 | Trust edge creation | Trust CRUD | Create signed trust edge (source, target, type=TRUST, topic_scope, weight). Store locally. Publish to network. |
| M3.2 | Watch edge creation | Watch CRUD | Create signed watch edge. Distinct delivery behavior from trust. |
| M3.3 | Silence edge creation | Silence CRUD | Create signed silence edge. Content from silenced pubkey excluded from feed. Graph position preserved. |
| M3.4 | Edge revocation | Revocation stubs | Revoke any edge via signed revocation stub. Propagate. |
| M3.5 | Trust neighborhood content delivery | Delivery engine | Trust edges: full stub content stored locally on receipt. Watch edges: headers only (hash, author, timestamp, branch_position). Silence: nothing delivered. |
| M3.6 | On-demand content retrieval for watch edges | Pull-on-demand | Watch edge stub header visible in feed. User action retrieves full content from network. Not cached beyond session. |
| M3.7 | Feed construction engine | Feed module in `pulse-core` | Priority: (1) new stubs from trust edges, (2) watch edge headers, (3) nothing else. Chronological within priority tier. |
| M3.8 | Explicit branch creation | Branch creation | Root stub (no parent_hash) creates new branch. Branch ID = root stub content_hash. |
| M3.9 | Explicit reply | Reply flow | Stub with parent_hash set attaches to parent's branch. Branch stub_count and last_activity updated. |
| M3.10 | Built-upon count tracking | Basic metric | Count how many stubs reference a given stub as parent. Increment on receipt. Display on profile and stub metadata. |
| M3.11 | Profile metrics computation | Profile engine | Trust density = count of inbound trust edges. Watch count = count of inbound watch edges. Built-upon count = total references to authored stubs. |
| M3.12 | Feed integration test | End-to-end test | Node A trusts B, watches C, silences D. B, C, D all publish stubs. A's feed shows B's full content, C's headers, nothing from D. |

---

### M4 — Desktop Client (Weeks 4-6)

Tauri application — polished enough for daily use by real testers.

| # | Task | Deliverable | Done when |
|---|------|-------------|-----------|
| M4.1 | Tauri app scaffold | App skeleton | Tauri + web frontend (Svelte or React — whichever is faster to build with). Rust backend exposes all pulse-core operations. Builds on macOS, Linux, Windows. |
| M4.2 | Onboarding flow | Identity creation UI | Generate keypair. Set passphrase. Show public key (copyable). Explain what a keypair is in plain language. Smooth first-run experience. |
| M4.3 | Stub composition | Post creation UI | Text input (300 char limit with counter). Preview. Publish button. Confirmation with "posted" state. Reply-to flow (compose from within a branch). |
| M4.4 | Feed view | Main feed UI | Trust-shaped feed: full stubs from trusted accounts with author, timestamp, branch context. Watch headers shown distinctly (muted, expandable). Clear visual distinction between trust content and watch content. Empty feed state: "Nothing new from your trust graph" — not an error. |
| M4.5 | Branch / thread view | Branch UI | Chronological thread of stubs in a branch. Root stub at top. Replies nested. Author pubkey (truncated) + timestamp on each stub. Reply button per stub. |
| M4.6 | Profile view | Profile UI | Public key (truncated + copyable full key). Trust density count. Watch count. Built-upon count. List of branches participated in. |
| M4.7 | Social graph management | Trust/watch/silence UI | Add trust/watch/silence by pasting a pubkey. Set weight for trust edges (0.0-1.0 slider). View all outbound edges. Revoke edges. Clear labels: "Trust — their content reaches you in full", "Watch — you see headers, pull content on demand", "Silence — you won't see their content." |
| M4.8 | People directory | Discovery UI | List of known identities on the network (discovered via DHT/gossip). Show pubkey, stub count, trust density, watch count. Actions: trust, watch, silence. |
| M4.9 | Network status panel | Status UI | Connected peers count. Sync state (synced / syncing / offline). Total stubs in local store. Merkle tree size. Last sync timestamp. |
| M4.10 | Settings | Settings UI | Display name (local-only label for own identity, not protocol-level). Passphrase change. Bootstrap peer list. Storage location. |
| M4.11 | Notification indicators | Activity indicators | Visual indicator when new stubs arrive in feed. Per-branch "new activity" indicator. No push notifications — pull-based awareness only. |
| M4.12 | Visual polish pass | UI refinement | Consistent typography, spacing, color. Light and dark mode. The app should feel intentional and calm — not flashy, not bare. The visual language should communicate "this is a quiet, high-signal space." |

---

### M5 — Deployment and Hardening (Week 6-7)

Multi-node VPS deployment and real-world readiness.

| # | Task | Deliverable | Done when |
|---|------|-------------|-----------|
| M5.1 | VPS node provisioning | 3-5 VPS instances | Nodes running on cloud VPS (e.g., Hetzner, OVH, DigitalOcean). Geographically distributed if possible but not required. Always-on. |
| M5.2 | Seed node configuration | Bootstrap config | VPS nodes configured as seed peers. Desktop client ships with VPS addresses as default bootstrap list. |
| M5.3 | Node-to-node mesh verification | Deployment test | All VPS nodes connected. Stub created on any node propagates to all others within 5 seconds. |
| M5.4 | Desktop client → VPS connectivity | Connectivity test | Fresh desktop client install connects to VPS seed nodes. Discovers other peers. Sends and receives stubs. |
| M5.5 | Multi-client feed test | End-to-end test | 5+ desktop clients connected to VPS mesh. Each creates identity, publishes stubs, creates trust/watch/silence edges. Feed reflects edge types correctly for each client. |
| M5.6 | Reconnection / offline resilience test | Resilience test | Client goes offline for 1 hour. Reconnects. Delta sync brings client up to date. No data loss. No UI errors. |
| M5.7 | Concurrent write test | Consistency test | Multiple clients posting to the same branch simultaneously. CRDT convergence verified. All clients see all stubs. No ordering violations in branch view. |
| M5.8 | Fuzz testing on ingestion | Security test | Fuzz stub validation pipeline. Fuzz edge validation. Fuzz sync protocol messages. Zero crashes, zero panics. |
| M5.9 | Performance baseline | Benchmarks | Stub creation latency. Propagation latency (creation → visible on remote peer). Sync time after 1-hour offline window. Document baselines. |
| M5.10 | Tester onboarding documentation | User guide | How to install. How to create identity. How to trust/watch/silence. How to post and reply. How to read the feed. What the metrics mean. Plain language, no jargon. |
| M5.11 | Tester feedback mechanism | Feedback channel | Simple way for testers to report bugs and impressions. Could be a dedicated branch on the network itself, or external (email/form). |
| M5.12 | Release packaging | Distributable builds | Signed macOS .dmg, Linux .AppImage, Windows .msi. One-click install. No terminal required for testers. |

---

## Timeline

```
Week 1         M1 — Core Crates
                 Identity, stub format, storage, crypto
                 All pure Rust, no networking, no UI

Weeks 2-3      M2 — P2P Networking
                 libp2p, DHT, GossipSub, Merkle sync
                 Multi-node tests passing by end of week 3

Weeks 3-4      M3 — Social Graph and Feed  (overlaps M2)
                 Trust/watch/silence edges
                 Feed construction engine
                 End-to-end feed test passing

Weeks 4-6      M4 — Desktop Client
                 Tauri app, all views, visual polish
                 Usable by non-technical testers

Weeks 6-7      M5 — Deployment and Hardening
                 VPS nodes live, clients connected
                 Fuzz testing, perf baselines
                 Tester documentation and packaged builds

Week 7         Ship to testers
```

**Total: ~7 weeks with 24/7 Claude Code**

Buffer for unexpected integration issues: +1-2 weeks.

**Realistic ship date: 8-9 weeks from start.**

---

## Risk Register

| Risk | Impact | Likelihood | Mitigation |
|---|---|---|---|
| libp2p GossipSub + Kademlia integration complexity | Schedule slip on M2 | Medium | Start M2 early. Prototype peer connectivity before full stub propagation. |
| Tauri cross-platform build issues | Can't ship on all 3 OSes | Low | Prioritize one platform (Linux or macOS), ship others as fast-follow. |
| Delta sync correctness bugs | Data inconsistency for testers | Medium | Heavy property-based testing on Merkle tree. Partition/reconnect test suite. |
| NAT issues for testers behind home routers | Testers can't connect | Medium | VPS nodes act as relay. libp2p relay protocol as fallback. Document port forwarding if needed. |
| 300 char limit feels too short for testers | Feedback: "I can't express a full thought" | Low-Medium | Monitor feedback. Content objects (long-form) is first post-MVP feature if needed. |
| Tester onboarding friction (keypair concept unfamiliar) | Testers bounce during setup | Medium | Invest in M4.2 onboarding flow. Plain language. No crypto jargon in UI. |

---

## Post-MVP Priority Queue

Features to add based on tester feedback, in likely priority order:

1. **Content objects (long-form)** — if 300 chars feels limiting
2. **Branch attachment inference** — once enough branches exist to test topic clustering
3. **Silence weighted score** — once enough social graph data exists
4. **Quality scoring (Tier 1 heuristics)** — cheapest quality signal
5. **Basic filtering (lexical + source)** — as network grows beyond trusted circle
6. **One-hop trust propagation** — once direct trust graph feels too small
7. **SNR and squelch** — once feed volume warrants it
8. **Mobile client** — once desktop experience is validated

---

*Pulse MVP Plan — April 2026*
