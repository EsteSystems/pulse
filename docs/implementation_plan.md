# Pulse — Implementation Plan (Post-MVP)

*April 2026*

---

## Completed

- [x] M1: Core crates — identity, stubs, storage (72 tests)
- [x] M2: P2P networking — libp2p, GossipSub, Kademlia, history republish
- [x] M3: Social graph — trust/watch/silence edges, trust-shaped feed, profiles
- [x] M4: Desktop client — Tauri + Svelte, onboarding, feed, branches, people, settings
- [x] M5: CI/CD — GitHub Actions for Windows/macOS/Linux, GitHub Release v0.1.0
- [x] Bug fixes — RocksDB locks, WebKitGTK button colors, history sync, auto-reconnect

---

## Next: v0.1.1 — Polish and UX

Implementation order reflects dependencies. Username system is foundational — post cards, people search, and profile edit all depend on it.

### 1. Feed auto-refresh
Poll for new feed items every 3-5 seconds. New stubs appear without navigating away and back. Merge new items into existing feed without scroll jump.

### 2. Optional username (display name)
Any UTF-8 string. Stored locally and published as a signed name stub so other nodes display it. Shown as "username (shortid)" everywhere — feed, profile, people. If no username set, show shortid only.

### 3. Post card redesign with icon actions
Restructure feed items:
- Text content on top
- Below it, one compact line: author (who), time (human-readable "3h ago", full datetime on hover), and action icons
- Icons: thread, reply, spread — with tooltip/action name on hover
- Clean, minimal, icon-based

### 4. Spread action (repost)
Republish someone else's stub through your identity so your trust graph sees it. Creates a spread stub that references the original. The original author and content are preserved and attributed. Name: "spread" — fits the radio/signal metaphor.

### 5. See-all feed mode
Toggle between "My Graph" (trust/watch only) and "All" (every stub from every known identity). All mode still excludes silenced authors. Default: My Graph.

### 6. People search
Search/filter the People view by display name (partial match) or public key (prefix match). Real-time filtering as user types.

### 7. Profile edit — username and avatar
Edit mode on profile page: set display name, set optional avatar/icon (emoji, initials, or small image). Published to network so other nodes display it.

### 8. Clickable profile counters
When > 0, profile metrics (trust density, watchers, built upon, stubs) become clickable. Each opens a list: who trusts you, who watches you, which stubs were built upon, all your stubs.

### 9. Persistent peer discovery in Settings
Keep the peer address input visible at all times, even after network starts. "Connect" button to add peers while running. Show connected peers list.

---

## Future: Peer Connectivity Strategy

**Problem:** If nodes only auto-connect to trusted peers, you get echo chambers at the network topology level.

**Proposed model:**
- Minimum peer target: 8-12
- Priority tiers for peer slots:
  - Trusted peers: auto-connect, highest priority, never evicted (3-4 slots)
  - Watched peers: connect if discovered, medium priority (2-3 slots)
  - Random/DHT peers: always maintain for network diversity (3-5 slots)
- Random slots are non-negotiable — they prevent island formation
- Maps to architecture doc's "network diversity score" concept

**Risk:** Forming islands of only trusted peers with no bridge to the wider network. The random peer slots are the structural remedy.

**Status:** Design discussion. Manual bootstrap peer approach is sufficient for MVP. Implement after core UX polish is complete.
