# Pulse — Architecture Document
### A Trust-Topological Public Discourse Network

*Working Document — April 2026*

---

## Table of Contents

1. [Vision](#1-vision)
2. [Core Principles](#2-core-principles)
3. [Data Model](#3-data-model)
4. [Network Layer](#4-network-layer)
5. [Identity and Cryptography — Individual and Group](#5-identity-and-cryptography)
6. [Social Graph — Trust, Watch, Silence and Reputation](#6-social-graph--trust-and-watch)
7. [Quality Layer](#7-quality-layer)
8. [SNR — Signal to Noise Ratio](#8-snr--signal-to-noise-ratio)
9. [Client Layer](#9-client-layer)
10. [Filtering Model](#10-filtering-model)
11. [Anonymity and Censorship Resistance](#11-anonymity-and-censorship-resistance)
12. [Mobile-First Architecture](#12-mobile-first-architecture)
13. [Storage Model](#13-storage-model)
14. [Operator and Relay Model](#14-operator-and-relay-model)
15. [Governance](#15-governance)
16. [Anti-Manipulation Properties](#16-anti-manipulation-properties)
17. [Phased Implementation](#17-phased-implementation)
18. [Technology Stack](#18-technology-stack)

---

## 1. Vision

Pulse is a trust-topological public discourse network where the architecture is a direct expression of social epistemology. Every technical decision maps to an honest model of how knowledge, trust, and speech actually work between humans.

It is not a platform. It has no center. It cannot be bought, regulated into irrelevance, or deplatformed. It has no feed to manipulate, no algorithm to bias, no infrastructure to seize.

The core value it delivers is **proximity to consequence** — the felt sensation of being adjacent to where things are actually happening and being decided. Unlike existing platforms that manufacture this sensation through engagement manipulation, Pulse produces it structurally through honest social graph topology.

**What Pulse is not:**

- Not a platform with an algorithmic feed
- Not a content moderator
- Not an engagement optimizer
- Not an advertising vehicle
- Not a central authority over speech
- Not a company that can be bought, pressured, or regulated into controlling what you read

---

## 2. Core Principles

These are non-negotiable constraints on every architectural decision.

**P1 — Sovereign identity.** Your identity is a keypair. No platform can deplatform you. Your social graph and history are yours, portable to any client.

**P2 — The platform has no feed.** You choose your feed. The platform has no ranking function, no engagement optimization, no push mechanism. Accusations of algorithmic bias against the platform are structurally impossible.

**P3 — Proximity is earned.** By reasoning well in public over time. Not by follower count, not by outrage velocity, not by paid promotion.

**P4 — Speech is consequential.** Posts are immutable from the moment of creation. What you said is what you said. The platform treats speech as real.

**P5 — The network remembers what the social graph values.** Content persists through reinforcement by trusted communities. Nothing else is guaranteed permanence.

**P6 — Trust and relevance are structurally distinct.** Following someone because you trust their judgment and following someone because you need to know what they say are different social acts. The architecture encodes this distinction at the edge level.

**P7 — All filtering is client-side.** The protocol has one mandatory content control — a CSAM hash blocklist. Everything else is operator policy and user choice.

**P8 — The branch has memory.** Conversations are not lost. They are dormant. They wake when the world makes them relevant again.

---

## 3. Data Model

### 3.1 The Stub — Atomic Unit

Every post is a **stub** — the minimal semantic unit of a speech act. A stub is immutable from creation. Its total size is hard-capped at **256 bytes** for text-only content.

```
Stub Schema (binary, fixed header + variable content)
─────────────────────────────────────────────────────
content_hash        32 bytes   BLAKE3 hash of canonical content
author_pubkey       32 bytes   Ed25519 public key
author_signature    64 bytes   Signs (content_hash + timestamp + branch_vector)
timestamp            8 bytes   Unix epoch, millisecond precision
branch_vector       variable   Compressed topic sig + parent ref + explicit refs (max 128 bytes)
content_ref         32 bytes   BLAKE3 hash of content object (zero if inline)
inline_content      variable   UTF-8, LZ4 compressed, only if ≤ 160 chars (optional)
─────────────────────────────────────────────────────
Total              ~320 bytes  max for inline stubs; header-only stubs ~148 bytes
```

Quality metadata, SNR scores, and trust weights are **derived from the graph** — none stored in the stub. The stub is the speech act and the cryptographic proof of it. Nothing else.

**Content objects — long-form:** A stub is a pointer, not a container. Short content (≤ 160 characters) may be inlined in the stub. Longer content lives in a separately content-addressed **content object** — an arbitrary-length UTF-8 document referenced by the stub's `content_ref` hash. Content objects are stored on IPFS and subject to independent memory decay. The stub remains the atomic graph unit regardless of content length. Threads, essays, and long arguments are first-class — they do not require chained stubs.

**Media** is never embedded in a stub or content object. A content object referencing media contains only content hashes of separately stored media objects. An operator can run a text-only relay without any media or long-form content storage obligation.

### 3.2 The Branch — Conversation with Memory

A branch is not created explicitly. It **emerges** from the graph when stubs share structural position across three dimensions simultaneously:

- **User dimension** — author's history of participation in related branches
- **Topic dimension** — semantic content signature matched against existing branch signatures
- **Reference dimension** — explicit or implicit citations of entities, events, and named elements

When all three vectors converge on a dormant branch, attachment is inferred automatically. When vectors point to no existing branch, a new branch is born. When vectors sit at the intersection of two existing branches, the post is a **bridge stub** — a first-class structural type indicating conceptual connection between previously separate conversations.

Branches are **Merkle-CRDTs** — append-only, conflict-free, eventually consistent. Immutability at the stub level makes CRDT convergence trivially correct: no conflicting mutations, only concurrent additions.

### 3.2a Branch Attachment Inference — Design Specification

This is the hardest technical problem in the system. The following specifies the mechanism concretely.

**The single-model problem:**

Using one embedding model for branch attachment means the model's biases about semantic similarity become the network's topology — topics the model considers unrelated will never bridge, topics it conflates will incorrectly merge. Model governance for attachment is more critical than for quality scoring, because attachment determines the shape of the knowledge graph itself. The solution is **model plurality with attachment reconciliation**.

**Recognized embedding models:**

The Pulse Foundation maintains a versioned list of recognized embedding models, each independently evaluated for bias characteristics. Multiple models are recognized simultaneously. No single model is mandatory. Each model holds an attestation keypair — score stubs declare which model produced them.

Recognized models at protocol v1.0 (proposed):
- `miniLM-L6-v2-int8` — 22MB, general purpose, English-primary
- `bge-small-en-int8` — 33MB, stronger retrieval performance
- `nomic-embed-multilingual-int8` — 65MB, multilingual coverage
- `paraphrase-multilingual-int8` — 70MB, cross-lingual semantic similarity

Clients run whichever recognized models their device tier supports. A stub may carry multiple branch vectors from different models. The network reconciles these into consensus attachment through trust-weighted voting among nodes that have evaluated the stub.

**Topic signature computation:**

Each stub's topic signature is a dense embedding vector computed client-side at write time, stored as a 128-byte PQ-compressed representation per model. Two stubs about the same event in different languages or phrasings will have high cosine similarity under multilingual models even with no shared vocabulary.

**Attachment decision algorithm:**

```
For each new stub S, for each recognized model M:
  embedding_M = M.embed(S.content)
  candidates  = DHT.query(topic_key(embedding_M), k=20)

  For each candidate branch B:
    score_M(B) = w_topic × cosine_sim(embedding_M, B.centroid_M)
               + w_user  × user_participation_score(S.author, B)
               + w_ref   × reference_overlap_score(S.refs, B.known_entities)

consensus_attachment = trust_weighted_vote(
  scores per model, weighted by model attestation trust in local graph
)

if consensus > ATTACH_THRESHOLD:    attach S to consensus branch
elif consensus > BRIDGE_THRESHOLD:  S is a bridge stub (attach to top-2)
else:                               S creates new branch
```

Default weights: `w_topic = 0.5`, `w_user = 0.3`, `w_ref = 0.2`

Default thresholds: `ATTACH_THRESHOLD = 0.72`, `BRIDGE_THRESHOLD = 0.55`

**When models disagree:** A stub where models assign it to different branches above threshold is flagged `SEMANTICALLY_AMBIGUOUS` and presented to the author for manual branch selection at write time. The author's signed choice supersedes all model consensus. Manual attachment always takes precedence.

**Branch centroid maintenance:**

Each branch maintains a running centroid embedding per model — the mean of all attached stubs' embeddings for that model, updated incrementally. Centroid drift from the root stub's embedding signals topic evolution. Significant drift flags a potential branch split — a future protocol extension.

**Cold start for new topics:**

A stub with no candidate branch scoring above 0.4 under any recognized model creates a new branch. New branches are invisible until a second stub attaches — a single orphan stub does not constitute a branch. This prevents branch explosion from one-off posts.

### 3.3 Branch State

Each branch maintains a compact state representation:

```
Branch State
────────────────────────────────────────
branch_id           32 bytes   Derived from root stub content hash
depth               4 bytes    Levels of sub-branches
stub_count          8 bytes    Total attached stubs
last_activity       8 bytes    Timestamp of most recent stub
root_hash           32 bytes   Merkle root of full branch state
activation_state    1 byte     ACTIVE | DORMANT | BRIDGING
────────────────────────────────────────
```

Two nodes comparing branch state exchange root hashes first. Hash mismatch triggers delta sync — only the diverging stubs are exchanged, not the full branch.

### 3.5 The Merkle Tree — Two-Tier Consistency Model

Every node maintains a Merkle tree of stub existence records — hashes and addresses, not content. The tree is the network's map. The "universal" property holds strictly only for full nodes. Light nodes carry topic-filtered subsets and degrade gracefully to DHT queries for unknown stubs. This two-tier model must be stated honestly:

**Full nodes** hold the complete tree. Arbitrary stub lookup is instant and local. Target storage: ~12GB at 100M stubs.

**Light nodes** hold a topic-filtered subtree covering their subscribed branches. Stub lookup within subscribed topics is instant and local. Stub lookup outside subscribed topics requires a DHT query — 1-3 network round trips, ~100-500ms latency. This is a different consistency model than full nodes: **eventually reachable** rather than **locally certain**.

Light nodes display this honestly in the UI — a stub outside the local subtree shows a "loading from network" state rather than pretending local certainty.

```
Merkle Tree Entry per Stub
──────────────────────────────────────
content_hash        32 bytes   Primary key
author_pubkey_ref   32 bytes   Reference to author identity
branch_position     32 bytes   Position in branch graph
address_hint        18 bytes   Author's last known network address (IPv6 + port)
timestamp            8 bytes
──────────────────────────────────────
Total              ~122 bytes  per entry
```

**Storage cost:**

| Network scale | Full tree | 1% topic slice |
|---|---|---|
| 10M stubs | 1.2 GB | 12 MB |
| 100M stubs | 12 GB | 120 MB |
| 1B stubs | 122 GB | 1.2 GB |

Full nodes and dedicated operators maintain the complete tree. Light nodes carry meaningful slices. The network's structural integrity depends on sufficient full node density — a growth sequencing concern addressed in Section 17.

---

## 4. Network Layer

### 4.1 Node Types

**Full nodes** — always-on, generous storage and bandwidth. Store complete Merkle tree plus full trust neighborhood content. Form the stable backbone.

**Regular nodes** — desktop clients, online most of the day. Full relay during active hours, light mode when idle. The bulk of network capacity.

**Light nodes** — mobile clients, constrained environments. Topic-filtered Merkle tree. Trust neighborhood content stored locally. Participate in DHT routing and gossip forwarding. Conservative relay behavior by default.

**Ephemeral nodes** — browser clients, temporary sessions. No persistent storage. Real network participants — DHT routing and read access — zero persistence contribution.

All node types are peers. The protocol makes no privilege distinction. The difference is contribution level, not access rights.

### 4.2 Peer Discovery

**Bootstrap — seed nodes:**
A small set of well-known, independently operated seed nodes distributed with client software. Seed nodes maintain a live peer registry — who is online and reachable. They hold no content state. If all seeds go offline, existing connected peers continue functioning normally. New nodes cannot join until seeds return.

**Protocol-identity rendezvous:**
Every node implementing the Pulse protocol publishes its existence to a shared DHT key: `BLAKE3("pulse-network-v1")`. Any new node looking for peers queries this key. No seed list required for initial discovery — protocol identity is the only coordination needed. Islands running the correct protocol find the main network automatically.

**Kademlia DHT:**
Peer routing uses Kademlia — the same DHT underlying BitTorrent and IPFS. Node IDs are assigned at join. Routing is O(log n) hops. DHT keys include both node IDs and **topic signatures** — nodes interested in a topic find each other by querying topic-keyed DHT entries.

**Seed attestations:**
When nodes connect they exchange signed attestation records — "I have successfully synced with this peer and found it to be a valid Pulse node." Attestations propagate through gossip. A seed with thousands of independent attestations from diverse geographies is trustworthy. A seed attested only by nodes in one geographic or political cluster is flagged.

### 4.3 Content Propagation

**GossipSub** (libp2p implementation) handles topic-addressed content propagation. Each topic maintains a mesh of peers. New stubs propagate by gossip within their topic mesh. Propagation is topic-filtered — nodes forward only stubs matching their connected peers' declared interest vectors.

**Epidemic broadcast trees** optimize propagation paths: the network learns which paths are most efficient and preferentially uses them while maintaining fallback paths for resilience.

**Content retrieval:**
When a user navigates to a stub outside their trust graph:

1. Client looks up the stub hash in the local Merkle tree — instant, local
2. Tree entry contains the author's last known network address
3. Client contacts the author's node directly — peer to peer
4. If author is offline, request fans out to their trust neighborhood
5. Content arrives, renders, is not cached beyond the session

**Address hint staleness** is handled by DHT fallback — if the stored address hint fails, the client queries the DHT for the author's current address by public key. Authors publish their current address to the DHT keyed by public key on every address change.

### 4.4 Consistency Model

**CRDTs** — Conflict-free Replicated Data Types — ensure branch convergence without coordination. Append-only operations mean any two nodes with the same set of stubs converge to identical branch state regardless of operation order.

**Eventual consistency** is the target. Two nodes may briefly hold different branch states when network partitioned. They converge within seconds of reconnection. For a discourse platform this latency is entirely acceptable.

**Merkle tree sync:**
Delta sync on tree reconnection. Two nodes exchange root hashes. If equal — nothing to sync, two hash round trip. If different — binary search to find divergence, exchange only differing entries. A node offline for one week syncs its tree with ~85MB of deltas at 1M active users network scale.

### 4.5 Island Detection and Prevention

**Network diversity score:** Every node continuously measures how many distinct geographic regions, autonomous systems, and seed lineages are represented in its current peer connections. Low diversity score triggers active peer seeking across lineages.

**Intentional islands:** Communities that want separation can configure isolated operation. Their isolation flag is visible in their node state. Their users know they are operating in island mode. Branch content uses the same data model — if they reconnect, history merges cleanly via CRDT. Intentional isolation is allowed. Invisible isolation is not.

---

## 5. Identity and Cryptography

### 5.1 Keypair Identity

Every identity is an **Ed25519 keypair**. Your public key is your identity. Your private key signs your speech acts. Nothing in the protocol requires linking your keypair to your legal name, phone number, or any real-world identity.

Identity is portable — you carry your keypair, you carry your identity. No platform holds your account. No platform can revoke it.

### 5.2 Identity Tiers

**Pseudonymous (default):** Keypair with accumulated history. You are your public key and the branches you've contributed to. Re-identifiable through behavioral analysis over time — this is disclosed honestly to users. Pseudonymity is not anonymity.

**Anonymous:** Throwaway keypair, used once, no history. Permitted by the protocol. Epistemically discounted by the graph — no trust weight, no quality history, no branch position credibility. Anonymity is free. It costs epistemic standing.

**Verified:** Optionally linkable to real-world credentials through signed attestations from trusted verifiers. A bar association membership, medical license, or institutional affiliation can be cryptographically attached to your keypair. You publish the attestation. It becomes part of your identity layer. Domain-specific credibility that means something specific.

### 5.3 Attestations

An attestation is a signed record from a verifier claiming a specific credential for a specific keypair:

```
Attestation
─────────────────────────────────────
subject_pubkey      32 bytes   Who is being attested
verifier_pubkey     32 bytes   Who is attesting
credential_type     variable   URI identifying credential type
credential_hash     32 bytes   Hash of credential evidence
verifier_signature  64 bytes   Signs the above
timestamp            8 bytes
expiry               8 bytes   Optional
─────────────────────────────────────
```

Attestations are published as stubs attached to a credential branch. They are subject to the same immutability and memory decay as all stubs.

### 5.4 Key Rotation and Compromise Recovery

Ed25519 keypairs are not rotatable in place — a new key is a new identity. Pulse handles this through **signed rotation stubs**:

A rotation stub is a special stub type signed by the old keypair that declares a new keypair as the continuation identity. It contains:

```
Rotation Stub
─────────────────────────────────────
old_pubkey          [32]u8   Signing key
new_pubkey          [32]u8   Continuation identity
rotation_signature  [64]u8   Old key signs (new_pubkey + timestamp)
timestamp           u64
─────────────────────────────────────
```

During a configurable grace period (default: 30 days), both old and new keypairs are recognized as the same identity for trust graph purposes. After the grace period, only the new key is active. Trust edges pointing to the old key are migrated to the new key automatically by clients that have observed the rotation stub.

**Compromise recovery:** At identity creation time, users are strongly encouraged to generate a **recovery keypair** stored offline. The recovery keypair's public key is committed into the identity's genesis stub. In the event of key compromise, the recovery keypair can sign a recovery rotation stub — identical to a normal rotation stub but signed by the recovery key rather than the active key. Recovery rotations are flagged distinctly in the graph and may trigger trust re-evaluation from cautious peers.

### 5.5 Group Identity

News organizations, research groups, editorial teams, and anonymous collectives require collective identity — a keypair that no single individual controls. This is a real adoption gap for the early adopter population most likely to drive initial network quality.

Group identity uses **threshold signatures** (FROST — Flexible Round-Optimized Schnorr Threshold Signatures) to produce Ed25519-compatible signatures from m-of-n keyholders. The group keypair is indistinguishable from an individual keypair on the network. The threshold structure is internal to the group.

**Group genesis stub:**

```
Group Identity Stub (genesis)
────────────────────────────────────────────
group_pubkey         [32]u8     The threshold keypair's public key
threshold_m          u8         Signatures required (e.g. 3)
threshold_n          u8         Total keyholders (e.g. 5)
member_pubkeys       [][32]u8   Individual member public keys (optional — may be omitted for anonymous collectives)
group_policy_hash    [32]u8     Hash of off-graph governance document
founding_signatures  [][64]u8   All n members sign the genesis stub
────────────────────────────────────────────
```

The genesis stub establishes the group. Subsequent stubs are signed by the threshold keypair — any m members contribute partial signatures, the stub is assembled and broadcast normally. Individual authorship within the group may be optionally disclosed via a signed attribution field within the stub, or remain anonymous to the network.

**Member rotation:** Adding or removing members requires a new genesis stub signed by the current m-of-n threshold, declaring the new member set and keypair. The old group keypair issues a rotation stub pointing to the new group keypair. Trust graph edges migrate during the grace period, identical to individual key rotation.

**Disclosure spectrum:** A group may publish its full member list (transparent collective), publish member public keys without real-world identity links (pseudonymous collective), or publish no member information at all (anonymous collective). All three are valid. The group_policy_hash in the genesis stub points to whatever governance document the group chooses to make available — or nothing.

---

## 6. Social Graph — Trust and Watch

### 6.1 Two Edge Types

The social graph has two structurally distinct edge types. These are not UI flavors of the same connection — they are different data types with different semantic meaning and different effects on content delivery and metric calculation.

**Trust edge — epistemic endorsement:**
"I have found this person's reasoning to be reliable over time. Their arguments are load-bearing for my thinking."

- Their full stub content reaches your local storage
- Their trust weight propagates through the graph weighted by your own credibility in the relevant topic graph
- Trust is contextual — you trust someone differently in different domains
- Trust edges contribute to the trusted party's **trust density score**

**Watch edge — informational relevance without endorsement:**
"I need to know what this person is saying. This is not an endorsement of their judgment."

- Only stub headers (136 bytes, no content) reach your local storage
- Content retrieved on demand, not cached
- Watch edges contribute to the watched party's **watch count** — reach without epistemic endorsement
- "Follow ≠ endorsement" disappears as a verbal disclaimer. The architecture says it structurally.

### 6.2 Trust Weight Propagation

Trust is not flat. Your trust of someone carries weight proportional to your own established credibility in the topic graph where trust is being evaluated.

```
effective_trust_weight = trust_edge_weight × truster_credibility_in_topic
```

A domain expert's trust endorsement in their domain is worth more than a general user's trust endorsement in the same domain. Trust weight is contextual, not universal.

One-hop propagation (optional, user-configured):

```
derived_trust_weight = direct_trust_weight × intermediary_credibility × hop_decay_factor
```

Where `hop_decay_factor` is a configurable constant (default 0.7) that prevents trust from propagating indefinitely.

### 6.3 The Silence Edge

A third edge type for explicit non-reception:

**Silence edge:** "I have chosen not to receive posts from this person. They exist. I know it. I've closed the channel."

Different from a block (which implies a hostile act) and different from unwatch (which implies indifference). Silence is an explicit, deliberate boundary. The person's structural position in the graph remains visible. Their content does not reach you. Both parties can in principle know this. The graph reflects it accurately.

**Why Pulse has no block:** You cannot block someone in public discourse for the same reason you cannot prevent someone from hearing your public speech. That right does not exist in any real public space. What you have is sovereignty over your own reception — not authority over theirs. Silence expresses the right you actually have. Block pretends to grant a right you don't.

A silenced account can still read your public stubs, build upon them, and respond in public branches. Their response exists in the branch — complete, visible to anyone who navigates there. You will not receive it in your feed. You chose not to. The branch has integrity. Your feed is your curated experience of it. These are correctly separate.

**Reply eligibility — harassment mitigation:** Silence handles your reception. It does not by itself prevent branch degradation from a harasser replying to every stub. The structural remedy is **reply eligibility**, a per-stub or account-level setting:

```
reply_eligibility = OPEN          | anyone may nest replies under this stub
                  | TRUST_GRAPH   | only accounts within N hops of author's trust graph
                  | TRUSTED_ONLY  | only direct trust edges
```

Outside the eligibility threshold, replies to your stub exist in the graph — they can quote you, reference you, build on you — but they do not nest under your stub's reply thread. Your branch discussion is not degraded. The speech act still exists publicly. It simply does not attach to your thread without your structural permission.

This preserves the public speech principle: you cannot erase someone's response to your public statement. You can determine whether their response nests structurally within your conversation thread.

### 6.4 Weighted Silence Score

Silence edges from credible accounts accumulate into a **weighted silence score** — a manipulation-resistant signal of how a person is actually received by people whose reception is itself worth something.

```
weighted_silence = Σ (silence_weight_i)

silence_weight_i = truster_density_in_topic
                 × content_quantity_factor
                 × built_upon_factor
```

The `content_quantity_factor` and `built_upon_factor` prevent bots — a silencing account must have demonstrated genuine participation to contribute meaningful weight. A coordinated bot farm silencing someone produces near-zero weighted score despite high raw count. The weighting exposes brigading automatically.

**Three patterns the score reveals:**

**Broad topic distribution** — high-trust accounts across many unrelated domains independently silencing someone. Unrelated communities don't coordinate. Convergence is meaningful. Indicates genuine offensiveness or noise.

**Narrow topic distribution** — silenced primarily within one domain. Requires scrutiny — could indicate domain-specific offensive behavior, or could indicate a legitimate contrarian voice that domain's establishment finds threatening. The narrow distribution is a flag, not a verdict.

**High raw count, low weighted score** — many silences, low truster credibility. The brigading fingerprint. The weighting collapses the signal to near zero regardless of volume.

**Display:** Visible to the person themselves — always, with domain breakdown. This is actionable self-knowledge. Visible to others — available on full profile inspection, not foregrounded. A due diligence signal, not a public shaming metric.

### 6.5 Profile Metrics

Four numbers replace the single follower count. Together they give a complete, ungameable portrait of how a person's public speech is actually received.

**Trust density** — weighted trust endorsements per topic domain. Earned through consistent quality reasoning over time. Displayed per domain, not as a single number.

**Watch count** — raw watch edges. Reach without epistemic endorsement. Honest about what it measures.

**Built-upon score** — how many stubs across the network use yours as structural foundation. The purest positive signal. Cannot be faked — requires genuine humans to voluntarily build on your reasoning.

**Weighted silence score** — how many credible people have independently chosen not to receive your content. The purest negative signal. Cannot be brigaded — weighting by truster credibility collapses coordinated low-quality attacks to near zero.

---

## 7. Quality Layer

### 7.1 Argument Quality Signal

Every stub carries a transparent, contestable argument quality signal. What it measures:

- **Internal coherence** — does the argument contradict itself
- **Logical structure** — is the conclusion supported by stated premises
- **Epistemic honesty** — are claims qualified appropriately, uncertainty acknowledged
- **Evidence grounding** — are factual claims supported or at minimum supportable
- **Relevance** — does the argument address what it claims to address

What it explicitly does **not** measure:
- Whether the conclusion is correct
- Whether you agree with it
- Whether it is popular

A perfectly coherent argument for a wrong conclusion scores well. The quality signal measures the **form of reasoning**, not the content of the position. This is the only defensible version of a quality signal.

### 7.2 Scoring Architecture

Quality scoring is **entirely client-side**. No operator evaluates content. No central model produces authoritative scores. This is non-negotiable — operator-side scoring would make operators content evaluators, contradicting the common carrier position.

**Tiered local analysis — device capability determines tier, not access:**

The previous design (Phi-3-mini on every device) was flagship-centric and therefore dishonest for a global platform. Budget Android devices — which dominate global markets — cannot run 2GB models at read time without unacceptable battery and thermal impact. The correct model degrades gracefully across device tiers rather than binary enable/disable.

**Tier 0 — peer aggregation only.** No local model. Score stubs from trust graph peers are aggregated and displayed. The user sees others' attested scores, not local computation. Available on any device. Zero battery impact. Still meaningful — the score reflects the trust-weighted view of the user's social graph.

**Tier 1 — heuristic scoring.** No neural model. Rule-based detection of logical structure markers, epistemic qualifiers, citation presence, and assertion density. ~50KB of compiled rules. Negligible compute. Runs on any Android device from 2018 forward. Covers approximately 70% of the quality signal at under 5% of the cost of Tier 2.

**Tier 2 — quantized MiniLM.** `all-MiniLM-L6-v2` quantized to int8: 22MB on device, ~80ms inference on mid-range hardware, ~150mAh/hour battery impact during active use. Suitable for devices with 3GB+ RAM from approximately 2020 forward. Default for capable mid-range devices.

**Tier 3 — full SLM.** Phi-3-mini or equivalent on flagship devices with dedicated NPU. Opt-in, not default. User-initiated batch scoring rather than per-read inference to manage thermal load.

Device auto-detects available tier based on RAM, NPU presence, available thermal headroom, and battery level. User can manually cap their tier. **Tier is declared in every score stub** — recipients know exactly what level of analysis produced the score they are reading.

Feature inequality is real but minimized: a Tier 0 device still receives quality scores through peer aggregation. The score it sees is shaped by whose scores reach it through the trust graph, introducing a social bias distinct from a compute bias — but not an access gap. Every device participates in the quality layer.

**Score stubs:**
Scores are computed locally, signed by the scoring keypair, and published as **score stubs** attached to the evaluated stub. Score stubs are first-class graph objects subject to the same immutability and memory decay as content stubs. Score comparability is achieved through **model attestation** — a score stub declares model version and tier. The reader's displayed quality score is the trust-weighted mean of attested scores from nodes in their trust graph, filtered by declared tier if the reader prefers higher-confidence scores.

**Layer 2 — Trust-weighted human calibration:**
Feedback from high-trust nodes in the relevant topic graph. Users who have themselves demonstrated quality reasoning are better judges of quality than users who haven't. Their quality feedback carries weight proportional to their trust density in the relevant domain.

**Layer 3 — Structural reinforcement:**
Posts that receive substantive replies — replies that build arguments — score better than posts that generate emotional reactions. A stub used as foundation for further reasoning gets its quality score updated upward. This is the strongest signal and the hardest to fake.

### 7.3 Transparency Requirements

The scoring model is:

- **Open source** — the full model is publicly readable
- **Forkable** — communities can run different quality models appropriate to their context
- **Self-explaining** — a post's score shows exactly which features contributed positively or negatively
- **Non-gating** — low quality scores don't hide posts, they label them. The reader decides what to do with that information

### 7.4 Bridge Stubs

A stub that sits at the structural intersection of two previously separate branches is a **bridge stub** — a first-class type carrying its own quality signal. Bridge stubs represent synthesis — the connection of previously separate understanding. Over time the map of bridge stubs across the network is the map of where understanding is being connected and by whom.

---

## 8. SNR — Signal to Noise Ratio

Borrowed from radio communications. A unified readout of content quality at three levels simultaneously.

### 8.1 Post-Level SNR

```
SNR_post = signal_components / noise_components

Signal: quality_score + structural_attachment_confidence + build_upon_rate + reference_density
Noise:  emotional_trigger_density + assertion_without_grounding + orphan_penalty
```

### 8.2 Branch-Level SNR

Ratio of substantive contribution to reflexive reaction across the branch's full history. Measures whether the branch is growing in depth or only in volume. Whether new information is entering versus existing assertions being recirculated.

### 8.3 Feed-Level SNR

The unified ambient readout of your trust graph's current quality. Not a number — an ambient visual state. The feed feels clean at high SNR. A subtle density or texture change signals SNR degradation. Not alarming. Not gamified. Honest feedback about the quality of what you are currently receiving.

### 8.4 Squelch

A first-class feed mode. When feed-level SNR drops below a user-configured threshold, the feed goes quiet rather than filling with low-quality content. An empty Pulse feed means the world is currently quiet on things that matter to you. That is information, not a failure state.

---

## 9. Client Layer

### 9.1 Feed Construction

The feed algorithm is **code the user can read, modify, and replace**. The platform ships a default implementation. Users can install community-built algorithms. Users can write their own. The platform has no secret ranking function.

Default feed construction:
1. All new stubs from direct trust edges — highest priority
2. Reactivated dormant branches relevant to current events — topic-matched against interest vector
3. Bridge stubs connecting branches in your interest graph — synthesis surfacing
4. Watched account stub headers — awareness without depth
5. Nothing else — the feed earns its presence or goes quiet

### 9.2 Branch Navigation

Users navigate branches as living structures with memory, not as archived threads. The branch view shows:

- Current active stubs — new growth
- Dormant period indicator — how long the branch has been quiet
- Previous activation events — when and why it woke up before
- Bridge connections — which other branches this one connects to
- Participant map — who has contributed and their trust density in this topic

### 9.3 Temporal Metabolism

Pulse does not impose an artificial event tempo. Branches persist through dormancy without pressure to resolve or be replaced. A slow-moving crisis remains visible without being forced into an artificial arc. Topic threads accumulate over time rather than disappearing into a stream. Temporal context is displayed explicitly — this conversation has been developing for three months, not just trending for four hours.

---

## 10. Filtering Model

### 10.1 Philosophy

All filtering is client-side. The platform has no content policy beyond the mandatory CSAM hash blocklist. You protect yourself. The platform does not protect you from content. This is the only model consistent with preserving the authenticity that makes proximity to consequence feel real.

### 10.2 The Black Marker Model

Filters render as **black markers** — the declassified document model. Content exists. It was said. The marker acknowledges presence while withholding detail.

At **word level:** Inline redaction bars proportional to word length. The sentence structure remains readable. You understand the argument's shape without the specific triggering content.

At **post level:** Full post content behind a solid bar. Structural metadata remains visible — author's position in the branch, timestamp, built-upon count, quality score, SNR contribution. You know the post exists and what weight it carries without reading it.

At **branch level:** Branch node present in the graph with shape and size visible. Contents masked. You see that a significant structure exists without reading it.

**The shape of what is filtered is always visible.** You chose to filter. You always know how much you are not reading.

### 10.3 Reveal Mechanic

Single tap reveals masked content temporarily (session) or permanently (your choice at reveal time). Not a settings menu — inline, immediate, reversible. A revealed stub shows a thin border indicating it passed through your filter. Your filter activity is private data on your device, never transmitted to the network.

### 10.4 Filter Taxonomy

**Lexical** — specific words or phrases. Word-level marker rendering.

**Semantic** — topic categories via content classification. Post-level rendering. Sports, cooking, financial advice, religious content, political content by region or ideology.

**Structural** — entire branches or recurring event types. Branch-level rendering.

**Source** — specific keypairs or trust-graph-distance thresholds.

### 10.5 Moderation Subscriptions

Beyond personal filters, users subscribe to community moderation layers — blocklists and trust standards maintained by whoever they choose to trust. These operate as additional filter layers applied before personal filters. Subscribing to a moderation layer is a trust edge to the entity maintaining it — subject to the same trust weight evaluation as any other trust relationship.

---

## 11. Anonymity and Censorship Resistance

### 11.1 Threat Model

**Legal pressure on node operators** — operators are infrastructure, not publishers. Content-addressed storage maximizes the technical truth of the common carrier position. Operators store content-addressed objects whose semantic meaning they do not evaluate.

**Network-level blocking** — pluggable transports make Pulse traffic indistinguishable from generic HTTPS. Bridge nodes with unlisted addresses distributed through trusted channels for high-censorship environments.

**Content hash banning** — the hash blocklist mechanism allows operators to comply with jurisdiction-specific legal requirements without semantic content evaluation. Operators match hashes against lists. They do not read posts.

**Identity deanonymization** — optional onion-routed post submission routes posts through multiple relay hops before entering the gossip network. The originating IP is not visible to the first relay. Users who need strong anonymity accept the latency cost. Direct submission is the default for users who do not.

### 11.2 CSAM — The Non-Negotiable

The BLAKE3 hash blocklist of known CSAM is a **protocol conformance requirement**, not an operator option. An implementation that does not check this blocklist is not Pulse. It is a fork. The social and legal cost of running that fork falls entirely on its operators.

This is the one content-based protocol-level requirement. Everything else is operator policy and user choice. The line is drawn here and nowhere else at the protocol layer.

### 11.3 Jurisdictional Model

The Pulse protocol is jurisdiction-neutral. Operators choose which jurisdictions they operate in and comply with local law. Users choose which operators to connect to. Content legal in one jurisdiction but illegal in another may be stored on compliant relays in the permissive jurisdiction and not on relays in the restrictive jurisdiction. French users connecting to French relays see a French-compliant view. French users connecting to Moldovan relays see a different view. The protocol does not impose one view. Relay selection policy determines what is accessible.

---

## 12. Mobile-First Architecture

### 12.1 The Mobile Opportunity

Modern mobile devices — 256GB to 1TB storage, always-on connectivity, significant compute — are capable of being genuine network participants rather than thin clients. This reframes Pulse's deployment model entirely.

A phone with 256GB storage allocated 20GB for Pulse can store:
- Full Merkle tree for a topic-filtered 5% network slice: ~600MB
- 90 days of full trust graph content at 200 trusted contacts: ~2-8GB depending on posting volume
- Media cache for frequently accessed content: remaining budget

The phone is not a consumer of the network. It is the network.

### 12.2 Mobile Node Behavior

**Default (light) mode:**
- Topic-filtered Merkle tree — subscribed branches only
- Trust neighborhood content stored locally
- DHT participation at minimal level — discoverable and routing basic queries
- Gossip forwarding for active subscribed topics only
- No active inbound relay on mobile data
- No active relay on battery below 20%

**WiFi + charging mode (optional):**
- Full relay behavior for trust neighborhood
- Active inbound connections accepted
- Gossip forwarding expanded to full topic subscription set
- Merkle tree sync with full peers

**Settings exposed to user:**
- Storage budget (default: 10GB, configurable)
- Relay behavior toggle (manual override)
- Mobile data relay (default: off)
- Minimum battery threshold for relay (default: 20%)

### 12.3 NAT Traversal

Most mobile connections are behind carrier-grade NAT. libp2p handles this through:

- **Hole punching** — simultaneous TCP/UDP open from both ends via a signaling relay
- **Relay-assisted connection** — content pushed to peers via a relay node when direct connection is impossible
- **Circuit relay** — libp2p's protocol for relaying traffic through intermediary nodes when direct connection fails

A mobile node behind NAT can still participate as a relay for content delivery by pushing to peers rather than serving on demand.

### 12.4 Battery and Bandwidth Optimization

**Content delivery:**
- LZ4 compression on all stub content (fast decompression, ~2:1 ratio)
- Bloom filter peer queries before content requests (eliminate unnecessary round trips)
- Delta sync on Merkle tree (never download what you already have)
- Lazy loading of branch history (load current active stubs first, history on demand)

**Background operation:**
- Merkle tree sync batched during charging/WiFi windows
- Trust neighborhood content prefetch during idle on WiFi
- Gossip participation throttled proportionally to battery level

### 12.5 Browser Client

A browser-based client via WebRTC enables zero-installation network participation. Browser clients are ephemeral — no persistent storage beyond the session, no background operation. They participate in DHT routing and read content via direct retrieval. Their contribution to the network is real but minimal.

The browser client is the entry point that converts visitors into participants. Someone shares a Pulse branch link. You open it in a browser. You are in the network.

---

## 13. Storage Model

### 13.1 What Every Node Stores

**Universal Merkle tree (all nodes):**
Every stub's existence — hash, address, branch position, timestamp. The map of everything. No content.

**Trust neighborhood content (all nodes except ephemeral):**
Full stubs from direct trust edges. Optionally one hop out, weighted by trust propagation score. This is the node's contribution to the network's memory.

**Nothing else is stored without explicit action.**

Content outside the trust graph is retrieved on demand and not cached beyond the session. A node cannot be compelled to store content from outside its trust graph. Storage is a social commitment, not a network obligation.

### 13.2 Cache Eviction Policy

When storage budget is approached, eviction priority (lowest priority evicted first):

1. One-hop derived trust content below threshold weight
2. Dormant branch content not accessed in 90+ days
3. Dormant branch content not accessed in 30+ days
4. Active branch content with low built-upon scores
5. Active branch content with high built-upon scores — last evicted
6. Your own authored stubs — never evicted

The user's own speech acts are always retained locally. They authored them. They are responsible for them.

### 13.3 Memory Decay

Content persists through reinforcement. A stub accumulates pinning weight proportional to how often it is referenced, built upon, part of an activated dormant branch, or in the trust neighborhood of many nodes. When pinning weight drops to zero across all nodes that held it, the content is gone. Its hash remains in the Merkle tree permanently — the fact of its existence is immutable. The content is mortal.

**Permanence intent at write time:** Authors can configure a stub to expire from relays after a set period (e.g., 90 days). This setting is encoded in the stub at creation and cannot be changed retroactively.

**The accountability tension:** Memory decay is a deliberate design choice with a real cost. Important historical content — a public figure's statement later denied, early documentation of a developing scandal, a scientist's retracted claim — can disappear if social interest wanes before accountability is established. Unlike the web where archival services can preserve content independently of its popularity, content-addressed P2P storage with decay depends on continued social reinforcement.

This is not a flaw to paper over. It is an honest tradeoff: the network prioritizes living collective memory over permanent archive. The mitigation is **archival nodes** — a recognized node type for institutions, journalists, historians, and accountability organizations that pin content without social reinforcement criteria.

Archival nodes are not mandated by the protocol. They are recognized by it. Their pinning decisions are their own. The Library of Congress, the Internet Archive, investigative journalism organizations, and academic institutions are natural archival node operators. The protocol acknowledges their function without controlling their scope.

### 13.4 Delta Encoding

Branch state is stored as deltas between Merkle checkpoints. A checkpoint is a compact Merkle root encoding full branch state at a point in time.

- Active branches: weekly checkpoints
- Dormant branches: monthly checkpoints
- Full history reconstructable from checkpoint + subsequent deltas
- No node required to hold full reconstruction history

### 13.5 Storage Arithmetic

```
Per stub (inline, max):        ~320 bytes
Per stub (header-only):        ~148 bytes
Per active user per day:       ~2.6 KB  (10 stubs/day, mixed inline/header)
Per million users per day:     ~2.6 GB  new stubs
Per year at 1M users:          ~950 GB  total network stubs

Node storage requirements (1M active users):
─────────────────────────────────────────────
Full Merkle tree:              ~12 GB
30 days trust neighborhood
  (200 trusted contacts):      ~2-8 GB
Topic-filtered tree (5%):      ~600 MB
─────────────────────────────────────────────
Typical desktop full node:     ~15-20 GB
Typical mobile light node:     ~3-5 GB
```

---

## 14. Operator and Relay Model

### 14.1 What an Operator Is

An operator runs a Pulse node that is always online, has generous storage and bandwidth, and serves as a stable backbone peer. Operators are infrastructure, not publishers. They store and forward content-addressed objects. They do not curate, moderate, or editorialize.

### 14.2 Operator Economics

**The protocol is free and open.** No licensing. No platform fee.

Operator costs: bandwidth, storage, electricity.

Operator revenue options:
- Subscription from users who want guaranteed relay availability for their content
- Community funding for nodes serving specific communities
- Institutional operation (universities, media organizations, civic institutions)
- Personal contribution — running a node as a network citizenship act

No advertising model. Advertising selects for attention capture which selects for outrage. The protocol has no mechanism for advertising integration.

**Storage market:** The primary operator revenue model is a **protocol-native storage market**. Authors who want guaranteed content persistence beyond social reinforcement pay operators directly for pinning commitments — a time-bounded contract: "pin this content hash for 12 months." Payment is made in any mutually agreed currency. The operator signs a pinning commitment stub, which is an on-graph verifiable record of the obligation.

### 14.2a Storage Market — Protocol Design

Pinning commitments without verification are promises. The storage market requires three elements: proof of storage, penalty escrow, and dispute resolution.

**Pinning commitment stub:**

```
Pinning Commitment Stub
────────────────────────────────────────────
content_hash        [32]u8    What is being pinned
operator_pubkey     [32]u8    Who is pinning
duration_days       u32       Commitment length
challenge_interval  u32       Days between proof-of-storage challenges
payment_ref         [32]u8    Hash of payment evidence stub
penalty_escrow_ref  [32]u8    Hash of escrowed penalty stub
operator_signature  [64]u8
author_signature    [64]u8
timestamp           u64
────────────────────────────────────────────
```

**Proof of storage:**

At each challenge interval, the author (or a delegated monitor) issues a cryptographic challenge: a random byte-range request of the pinned content. The operator must return the correct bytes plus a Merkle inclusion proof within a configurable timeout. An operator who has dropped content cannot fabricate the correct response. The challenge and response are published as paired stubs — the permanent on-graph record of compliance.

**Penalty escrow:**

At commitment time, the operator locks an escrow amount into a signed escrow stub acknowledged by both parties. If the operator fails three consecutive challenges across a 72-hour window — accounting for transient network failures — the author publishes a **breach stub**: a signed record of the failed challenges including nonces, expected responses, and actual responses or timeouts. The breach stub is permanent in the graph, attached to the operator's identity. Escrow release to the author follows from the documented breach.

The financial penalty is secondary to the reputational one. An operator identity with multiple breach stubs is visibly degraded in the storage market. Trust edges toward that operator for storage purposes will be weighted accordingly by clients evaluating storage partners.

**Dispute resolution:**

A single failed challenge triggers a warning state, not immediate breach. Transient network issues — confirmed by the operator publishing a signed unavailability declaration within the timeout window — do not count toward breach thresholds. Three consecutive unchallenged failures without a declared unavailability constitute breach. The 72-hour window is the minimum; commitment stubs may negotiate longer windows for operators in regions with less reliable connectivity.

This is not a trustless system. It requires the author to monitor challenges or delegate to a trusted monitoring party. It is a **verifiable** system — every commitment, challenge, response, and breach is an immutable on-graph record auditable by any node.

### 14.3 Pinning Policy

Operators choose what they pin. Minimum conformance: CSAM blocklist enforcement. Beyond that: their choice.

A topic-specific operator pins only content from their topic domain. A community operator pins content from their community's trust graph. A general operator pins broadly. The network's resilience comes from the diversity of pinning policies across operators, not from any single operator pinning everything.

### 14.4 Legal Compliance

Operators subscribe to jurisdiction-appropriate blocklists — hash lists, not semantic filters. Compliance is hash matching. Operators do not evaluate the meaning of what they store. This maximizes the technical defensibility of the common carrier position.

An operator in Germany subscribes to NetzDG-relevant blocklists. An operator in the EU subscribes to DSA-relevant blocklists. An operator in a permissive jurisdiction subscribes only to the mandatory CSAM blocklist. The protocol accommodates all of these without requiring the protocol itself to encode any of them.

---

## 15. Governance

### 15.1 Protocol Governance

The Pulse protocol is governed by a **Pulse Foundation** — a non-profit entity whose mandate is the maintenance of the protocol specification, the CSAM blocklist, and the seed node registry. The Foundation has no authority over content and no ownership of network infrastructure.

Foundation responsibilities:
- Protocol versioning and backward compatibility
- CSAM blocklist maintenance and distribution
- Seed node diversity — ensuring the seed list represents geographically and organizationally diverse operators
- Reference implementation maintenance

The Foundation has no power to deplatform users, restrict content (beyond the CSAM mandate), or favor any client implementation over another.

### 15.2 Forking

The protocol is forkable. Any community can fork Pulse, modify the protocol, and run their own network. Their network is not Pulse — it is a fork. Forks are encouraged. They are not compatible with the main network by default. Cross-network bridges are possible if both networks support them.

### 15.3 Seed Node Diversity Requirements

The canonical seed node list must maintain minimum diversity thresholds:
- No single country represents more than 25% of seed nodes
- No single organization operates more than 15% of seed nodes
- Geographic distribution across a minimum of 6 continents
- Mix of institutional, community, and individual operators

Violation of these thresholds triggers Foundation intervention to recruit additional diverse operators before the imbalance becomes a governance risk.

---

## 16. Anti-Manipulation Properties

### 16.1 Why Bot Farms Fail

On Pulse, a trust edge from a bot carries weight proportional to the bot's own established credibility in relevant topic graphs. A bot with no branch history, no built-upon score, and no consistent engagement pattern has zero credibility weight to propagate.

**A million weightless trust edges sum to zero.**

### 16.2 The Consistency Requirements Bots Cannot Fake

**Temporal consistency** — genuine engagement develops over time with organic ebb and flow matching topic activity cycles. Bot farms show unnatural temporal patterns — bursts without dormancy, activity without the organic rhythm of genuine interest.

**Topical consistency** — real people have coherent topic vectors. Bot accounts have either incoherent vectors (randomly distributed engagement) or suspiciously perfect vectors (engaging only with the target's branches).

**Structural consistency** — genuine engagement produces load-bearing stubs that real humans build upon. Bot farms produce volume. They cannot produce stubs that real humans voluntarily use as foundations for further reasoning. The built-upon score is the metric bots structurally cannot fake.

**Relational consistency** — real trust graphs are dense meshes with organic community clustering. Bot farm trust graphs are stars — all pointing at the target with no internal community relationships. The topology of artificial trust inflation is visible in the graph structure alone.

### 16.3 Sybil Attack Mitigations

**Time-weighted trust:** Trust weight accrues slowly. A new account carries minimal weight regardless of stated engagement. Manufacturing a convincing history requires months of sustained operation per bot — expensive at scale.

**Graph distance weighting:** Trust through multiple hops of established high-credibility accounts is worth more than trust one hop from unknown accounts. Bot farms cannot inject into the high-credibility core without being vouched for by expensive-to-manufacture accounts.

**Behavioral entropy:** Genuine human behavior is unpredictable in ways expensive to simulate. Aggregate entropy of posting times, topic drift, engagement patterns, and dormancy periods is high. Bot farms optimizing for credibility scores converge on locally optimal but globally unnatural behavioral patterns. Anomaly detection on behavioral entropy is a strong signal.

---

## 17. Phased Implementation

**Honest timeline note:** The full scope described in this document requires 36–48 months with a team of 8–12 engineers. A 24-month timeline is achievable only for Phases 1 and 2 with a focused team. The phases below reflect realistic sequencing, not aspirational compression.

### Phase 1 — Foundation (Months 1–12, team of 5–8)

**Goal:** A working P2P network with basic stub creation, branch emergence, and trust graph edges.

- Ed25519 identity generation and management
- Stub schema implementation and signing
- Basic Kademlia DHT with Pulse topic extensions
- GossipSub integration for stub propagation
- Merkle tree implementation and delta sync
- Basic trust and watch edge creation
- Text-only stubs, no media
- Desktop client only (macOS, Linux, Windows)
- Default chronological branch view
- No quality scoring yet

**Success metric:** Two nodes in different geographic locations can exchange stubs and maintain coherent branch state.

### Phase 2 — Graph Intelligence (Months 13–24, team of 8–10)

**Goal:** Branch attachment inference, quality scoring, and SNR layer.

- Three-dimensional branch attachment inference per Section 3.2a
- Branch reactivation detection and notification
- Dormancy and memory decay implementation
- On-device quality scoring with model attestation
- Trust-weighted quality calibration
- Built-upon score tracking
- SNR calculation at post and branch level
- Bridge stub detection and classification
- Feed construction algorithm (open source, swappable)
- Basic filtering with black marker rendering
- Content objects for long-form content

**Success metric:** A dormant branch correctly reactivates and surfaces previous participants when a new relevant event occurs.

### Phase 3 — Mobile (Months 25–36, team of 10–12)

**Goal:** Full mobile client with light node behavior and graceful storage management.

- iOS and Android clients
- Topic-filtered Merkle tree for light nodes with honest DHT fallback
- Dynamic relay mode switching (WiFi/cellular/battery)
- NAT traversal via libp2p hole punching
- Storage budget management with memory-decay-based eviction
- LZ4 compression throughout
- WebRTC browser client (read access + DHT participation)
- Media support (IPFS content-addressed, separate from stubs)
- Reply eligibility settings and enforcement

**Success metric:** A mobile device with 10GB storage budget operates as a genuine network participant for 90+ days without manual storage management.

### Phase 4 — Maturity (Months 37–48, team of 10–12)

**Goal:** Verified identity attestations, advanced filtering, operator tooling, storage market.

- Credential attestation framework
- Verified identity profiles with domain-specific trust
- Advanced semantic filtering (topic category classification)
- Moderation subscription layer
- Onion-routed post submission (optional, user-configured)
- Pluggable transport for censorship-resistant environments
- Operator tooling and storage market implementation
- Archival node recognition and tooling
- Protocol v1.0 specification freeze
- Pulse Foundation establishment and seed node diversity enforcement

**Success metric:** A user in a high-censorship environment can access and participate in the network without their traffic being identifiable as Pulse.

---

## 18. Technology Stack

### Core Protocol

| Component | Technology | Rationale |
|---|---|---|
| Transport + P2P | libp2p | Battle-tested, multi-platform, handles NAT, DHT, gossip |
| Peer routing | Kademlia DHT | Powers BitTorrent and IPFS at scale |
| Content propagation | GossipSub | Topic-addressed, mesh-based, efficient |
| Content addressing | BLAKE3 | Fast, parallel, modern hash function |
| Identity | Ed25519 | Fast, small keys, widely supported |
| Signatures | Ed25519 | Same keypair, compact signatures |
| CRDT | Merkle-CRDT | Combines Merkle proofs with CRDT semantics |
| Media storage | IPFS | Content-addressed, distributed, proven |
| Compression | LZ4 | Extremely fast decompression, adequate ratio |

### Reference Implementation

| Layer | Technology | Rationale |
|---|---|---|
| Core daemon | Rust | Memory safety, performance, async ecosystem |
| Mobile (iOS) | Swift + Rust FFI | Native performance, Rust core reuse |
| Mobile (Android) | Kotlin + Rust FFI | Native performance, Rust core reuse |
| Desktop client | Tauri (Rust + Web) | Cross-platform, native performance, small bundle |
| Browser client | WebAssembly + WebRTC | Zero installation, genuine P2P in browser |
| Quality scoring | Tiered: heuristics / MiniLM-int8 / Phi-3-mini (Section 7.2) | Client-side only, device-adaptive, no operator content evaluation |
| Feed algorithm | User-provided | Any language, sandboxed execution via WASM |

### Data Storage

| Purpose | Technology |
|---|---|
| Local Merkle tree | RocksDB (embedded) |
| Trust graph | SQLite (embedded) |
| Stub content cache | RocksDB with LZ4 compression |
| Branch state | Merkle-CRDT on RocksDB |
| Media cache | IPFS local datastore |

---

## Appendix A — Stub Binary Format

```
Offset  Length   Field
──────────────────────────────────────────────────
0       32       content_hash (BLAKE3)
32      32       author_pubkey (Ed25519)
64      64       author_signature (Ed25519)
128     8        timestamp (Unix ms, big-endian)
136     2        flags (immutability/expiry/inline-content bits)
138     2        branch_vector_length (max 128)
140     N≤128    branch_vector (variable, LZ4 compressed)
140+N   32       content_ref (BLAKE3 of content object; zero if inline)
172+N   2        inline_content_length (zero if content_ref used)
174+N   M≤146    inline_content (UTF-8, LZ4 compressed; omitted if content_ref used)
──────────────────────────────────────────────────
Total            320 bytes max (header-only stub: ~148 bytes)
```

**Flags field (16 bits):**
```
bit 0:   content_is_inline    (1 = inline content present, 0 = content_ref used)
bit 1:   expiry_set           (1 = stub has relay expiry configured)
bit 2:   reply_eligibility_0  } 2-bit encoding:
bit 3:   reply_eligibility_1  } 00=OPEN, 01=TRUST_GRAPH, 10=TRUSTED_ONLY, 11=reserved
bits 4-15: reserved
```

## Appendix B — Branch Attachment Vector Format

```
Field               Type           Description
──────────────────────────────────────────────────────
model_count         u8             Number of embedding models used
embeddings          []ModelEmbed   One PQ-compressed embedding per model
parent_hash         [32]u8         Hash of parent stub (zero if root)
explicit_refs       [][32]u8       Hashes of explicitly referenced stubs
entity_refs         []EntityRef    Named entities extracted from content
──────────────────────────────────────────────────────

ModelEmbed {
  model_id:     [8]u8     Truncated hash of attested model identifier
  embedding:    [128]u8   PQ-compressed dense embedding (384 → 128 dims)
}

EntityRef {
  entity_type:  u8        (PERSON | PLACE | EVENT | CONCEPT)
  entity_hash:  [16]u8    Truncated hash of canonical entity name
}
```

## Appendix C — Trust Edge Schema

```
Field               Type        Description
──────────────────────────────────────────────
source_pubkey       [32]u8      Truster
target_pubkey       [32]u8      Trusted
edge_type           u8          TRUST | WATCH | SILENCE
topic_scope         [32]u8      Topic hash (zero = universal)
weight              f32         0.0 – 1.0
timestamp           u64         Creation time
signature           [64]u8      Source signs the above
──────────────────────────────────────────────
```

---

## 19. Open Questions and Known Limitations

These are unresolved design questions requiring further work before protocol v1.0.

**Branch centroid drift and splitting.** When a branch evolves significantly — its topic centroid drifts far from its root stub — it may need splitting into two. The split protocol, merge semantics, and how existing stubs are reassigned are not specified. This is a future protocol extension.

**Model attestation governance.** Both branch attachment and quality scoring depend on model attestation. The Foundation's process for evaluating new model proposals, detecting modified model versions, and revoking compromised attestations is not yet defined. This is a governance design problem as much as a technical one.

**Storage market payment layer.** The pinning commitment protocol is specified. The payment layer is not — what currency, what settlement mechanism, how cross-border payments are handled. The protocol deliberately leaves payment mechanism open to avoid encoding a specific financial system, but the gap means the storage market requires out-of-band payment coordination until a payment layer is designed.

**Cross-fork bridges.** How two Pulse forks with diverged protocols can optionally interoperate is not specified. Bridge nodes connecting incompatible networks face data model translation problems that require their own design.

**Ephemeral branch visibility lag.** A branch does not become visible until a second stub attaches (Section 3.2a). A genuine first stub on a new topic is invisible until someone else independently covers the same topic. For fast-moving breaking events this creates a discovery lag. The severity is unquantified and may require a modified visibility rule for time-sensitive content.

**FROST threshold signature tooling maturity.** Group identity (Section 5.5) depends on FROST threshold signatures. FROST is cryptographically sound but production library maturity across mobile platforms (iOS Swift, Android Kotlin) is not fully established as of this writing. Phase 4 implementation depends on library availability.

**Scoring tier inequality measurement.** The tiered quality scoring model (Section 7.2) minimizes but does not eliminate device-correlated feature inequality. The actual difference in score quality between Tier 0 (peer aggregation only) and Tier 2 (local MiniLM) has not been empirically quantified. This needs measurement before claiming the tiers are meaningfully comparable.

---
*This document is a living specification subject to revision.*
*Protocol version: pre-alpha*
