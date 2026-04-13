<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount, onDestroy } from "svelte";

  // Polling
  let feedPollInterval = null;
  const FEED_POLL_MS = 4000;

  function startFeedPoll() {
    stopFeedPoll();
    feedPollInterval = setInterval(async () => {
      if (view === "feed") {
        try { feedItems = await invoke("get_feed", { limit: 100, showAll: feedShowAll }); } catch {}
      }
    }, FEED_POLL_MS);
  }

  function stopFeedPoll() {
    if (feedPollInterval) { clearInterval(feedPollInterval); feedPollInterval = null; }
  }

  onDestroy(() => stopFeedPoll());

  // App state
  let view = $state("loading"); // loading | onboard | unlock | feed | branch | profile | people | settings
  let myPubkey = $state("");
  let myShortId = $state("");
  let hasKeystore = $state(false);

  // Form state
  let passphrase = $state("");
  let composeText = $state("");
  let composeReplyTo = $state(null);
  let edgeTargetPubkey = $state("");
  let editDisplayName = $state("");
  let feedShowAll = $state(false);
  let peopleSearch = $state("");
  let edgeType = $state("trust");
  let edgeWeight = $state(0.8);
  let bootstrapPeers = $state("");

  // Data
  let feedItems = $state([]);
  let branchStubs = $state([]);
  let currentBranchId = $state("");
  let outboundEdges = $state([]);
  let profileData = $state(null);
  let profileTarget = $state(null);
  let knownIdentities = $state([]);
  let networkStatus = $state({ peer_count: 0, is_running: false, local_peer_id: null, listen_addrs: [] });

  // UI state
  let error = $state("");
  let statusMsg = $state("");

  onMount(async () => {
    try {
      hasKeystore = await invoke("has_keystore");
      const existing = await invoke("check_identity");
      if (existing) {
        myPubkey = existing;
        myShortId = existing.slice(0, 16);
        view = "feed";
        await refreshFeed();
      } else if (hasKeystore) {
        view = "unlock";
      } else {
        view = "onboard";
      }
    } catch (e) {
      error = e.toString();
      view = "onboard";
    }
  });

  async function createIdentity() {
    if (!passphrase || passphrase.length < 4) {
      error = "Passphrase must be at least 4 characters";
      return;
    }
    try {
      error = "";
      myPubkey = await invoke("create_identity", { passphrase });
      myShortId = myPubkey.slice(0, 16);
      passphrase = "";
      view = "feed";
      statusMsg = "Identity created";
    } catch (e) { error = e.toString(); }
  }

  async function unlockIdentity() {
    try {
      error = "";
      myPubkey = await invoke("unlock_identity", { passphrase });
      myShortId = myPubkey.slice(0, 16);
      passphrase = "";
      view = "feed";
      await refreshFeed();
      statusMsg = "Unlocked";
    } catch (e) { error = e.toString(); }
  }

  async function startNetwork() {
    try {
      error = "";
      const peers = bootstrapPeers.split("\n").map(p => p.trim()).filter(p => p.length > 0);
      const peerId = await invoke("start_network", { bootstrapPeers: peers });
      statusMsg = `Network started. Peer ID: ${peerId.slice(0, 16)}...`;
      await refreshNetworkStatus();
    } catch (e) { error = e.toString(); }
  }

  async function publishStub() {
    if (!composeText.trim()) return;
    try {
      error = "";
      const stub = await invoke("publish_stub", {
        text: composeText,
        parentHash: composeReplyTo,
        replyEligibility: null,
      });
      composeText = "";
      composeReplyTo = null;
      statusMsg = "Published";
      await refreshFeed();
    } catch (e) { error = e.toString(); }
  }

  async function createEdge() {
    if (!edgeTargetPubkey.trim()) return;
    try {
      error = "";
      await invoke("create_edge", {
        targetPubkey: edgeTargetPubkey.trim(),
        edgeType: edgeType,
        weight: edgeType === "trust" ? edgeWeight : null,
      });
      edgeTargetPubkey = "";
      statusMsg = `${edgeType} edge created`;
      await refreshEdges();
      await refreshFeed();
    } catch (e) { error = e.toString(); }
  }

  async function removeEdge(targetPubkey, type_) {
    try {
      error = "";
      await invoke("remove_edge", { targetPubkey, edgeType: type_ });
      statusMsg = "Edge removed";
      await refreshEdges();
      await refreshFeed();
    } catch (e) { error = e.toString(); }
  }

  async function refreshFeed() {
    try {
      feedItems = await invoke("get_feed", { limit: 100, showAll: feedShowAll });
      startFeedPoll();
    } catch (e) { error = e.toString(); }
  }

  async function refreshEdges() {
    try {
      outboundEdges = await invoke("get_outbound_edges");
    } catch (e) { error = e.toString(); }
  }

  async function refreshNetworkStatus() {
    try {
      networkStatus = await invoke("get_network_status");
    } catch (e) {}
  }

  async function openBranch(branchId) {
    try {
      currentBranchId = branchId;
      branchStubs = await invoke("get_branch_stubs", { branchId });
      view = "branch";
    } catch (e) { error = e.toString(); }
  }

  async function openProfile(pubkey) {
    try {
      profileTarget = pubkey;
      profileData = await invoke("get_profile", { pubkey: pubkey || null });
      view = "profile";
    } catch (e) { error = e.toString(); }
  }

  async function openPeople() {
    try {
      knownIdentities = await invoke("get_known_identities");
      view = "people";
    } catch (e) { error = e.toString(); }
  }

  function navigateTo(v) {
    view = v;
    error = "";
    if (v === "feed") { refreshFeed(); }
    else { stopFeedPoll(); }
    if (v === "people") openPeople();
    if (v === "profile") openProfile(null);
    if (v === "settings") { refreshEdges(); refreshNetworkStatus(); }
  }

  function timeAgo(ts) {
    const seconds = Math.floor((Date.now() - ts) / 1000);
    if (seconds < 60) return "just now";
    if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
    if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
    return `${Math.floor(seconds / 86400)}d ago`;
  }

  function charCount(text) {
    return [...text].length;
  }

  function authorLabel(displayName, shortId) {
    if (displayName) return displayName;
    return shortId;
  }

  async function spreadStub(contentHash) {
    // TODO: implement spread action (task #15)
    statusMsg = "Spread coming soon";
  }

  async function saveDisplayName() {
    try {
      error = "";
      const name = editDisplayName.trim() || null;
      await invoke("set_display_name", { name });
      statusMsg = name ? `Display name set to "${name}"` : "Display name cleared";
      if (view === "profile") await openProfile(null);
    } catch (e) { error = e.toString(); }
  }
</script>

<main>
  <!-- Navigation -->
  {#if view !== "loading" && view !== "onboard" && view !== "unlock"}
    <nav>
      <button class="nav-btn" class:active={view === "feed"} onclick={() => navigateTo("feed")}>Feed</button>
      <button class="nav-btn" class:active={view === "people"} onclick={() => navigateTo("people")}>People</button>
      <button class="nav-btn" class:active={view === "profile"} onclick={() => navigateTo("profile")}>Profile</button>
      <button class="nav-btn" class:active={view === "settings"} onclick={() => navigateTo("settings")}>Settings</button>
      <span class="nav-status">
        {#if networkStatus.is_running}
          <span class="dot online"></span> {networkStatus.peer_count} peers
        {:else}
          <span class="dot offline"></span> offline
        {/if}
      </span>
    </nav>
  {/if}

  {#if error}
    <div class="error">{error}</div>
  {/if}

  {#if statusMsg}
    <div class="status">{statusMsg}</div>
  {/if}

  <!-- Loading -->
  {#if view === "loading"}
    <div class="center-page"><p class="muted">Loading...</p></div>
  {/if}

  <!-- Onboarding: Create Identity -->
  {#if view === "onboard"}
    <div class="center-page">
      <h1>Pulse</h1>
      <p class="muted">A trust-topological discourse network</p>
      <div class="card">
        <h2>Create your identity</h2>
        <p class="hint">Your identity is an Ed25519 keypair. It's yours — no platform can revoke it. Choose a passphrase to encrypt it on disk.</p>
        <input type="password" placeholder="Passphrase (4+ characters)" bind:value={passphrase} onkeydown={(e) => e.key === "Enter" && createIdentity()} />
        <button onclick={createIdentity}>Create Identity</button>
      </div>
    </div>
  {/if}

  <!-- Unlock -->
  {#if view === "unlock"}
    <div class="center-page">
      <h1>Pulse</h1>
      <p class="muted">Welcome back</p>
      <div class="card">
        <h2>Unlock your identity</h2>
        <input type="password" placeholder="Passphrase" bind:value={passphrase} onkeydown={(e) => e.key === "Enter" && unlockIdentity()} />
        <button onclick={unlockIdentity}>Unlock</button>
      </div>
    </div>
  {/if}

  <!-- Feed -->
  {#if view === "feed"}
    <div class="page">
      <!-- Feed mode toggle -->
      <div class="feed-toggle">
        <button class="toggle-btn" class:active={!feedShowAll} onclick={() => { feedShowAll = false; refreshFeed(); }}>My Graph</button>
        <button class="toggle-btn" class:active={feedShowAll} onclick={() => { feedShowAll = true; refreshFeed(); }}>All</button>
      </div>
      <!-- Compose -->
      <div class="compose-box">
        {#if composeReplyTo}
          <div class="reply-indicator">
            Replying to {composeReplyTo.slice(0, 16)}...
            <button class="link-btn" onclick={() => composeReplyTo = null}>cancel</button>
          </div>
        {/if}
        <textarea
          placeholder="What's on your mind? (300 chars)"
          bind:value={composeText}
          maxlength="300"
          rows="3"
        ></textarea>
        <div class="compose-footer">
          <span class="char-count" class:over={charCount(composeText) > 300}>{charCount(composeText)}/300</span>
          <button onclick={publishStub} disabled={!composeText.trim()}>Publish</button>
        </div>
      </div>

      <!-- Feed items -->
      {#if feedItems.length === 0}
        <div class="empty-state">
          <p>Nothing from your trust graph right now.</p>
          <p class="hint">Trust or watch someone to see their posts here. Visit People to discover identities on the network.</p>
        </div>
      {:else}
        {#each feedItems as item}
          <div class="post-card" class:own-item={item.item_type === "own"} class:trust-item={item.item_type === "trust"} class:watch-item={item.item_type === "watch"}>
            <!-- Content -->
            {#if (item.item_type === "trust" || item.item_type === "own") && item.stub}
              <p class="stub-text">{item.stub.text || ""}</p>
            {:else}
              <p class="stub-text muted">Header only — content on demand</p>
            {/if}
            <!-- Meta line: who, when, actions -->
            <div class="post-meta">
              <button class="link-btn author" onclick={() => openProfile(item.author_pubkey)}>
                {item.item_type === "own" ? "you" : authorLabel(item.author_display_name, item.author_short_id)}
              </button>
              <span class="edge-dot" class:edge-own={item.item_type === "own"} class:edge-trust={item.item_type === "trust"} class:edge-watch={item.item_type === "watch"}></span>
              <span class="timestamp" title={new Date(item.timestamp).toLocaleString()}>{timeAgo(item.timestamp)}</span>
              <span class="meta-spacer"></span>
              <button class="icon-btn" title="Thread" onclick={() => openBranch(item.branch_id)}>&#x1F5E8;</button>
              {#if (item.item_type === "trust" || item.item_type === "own") && item.stub}
                <button class="icon-btn" title="Reply" onclick={() => { composeReplyTo = item.stub.content_hash; }}>&#x21A9;</button>
                <button class="icon-btn" title="Spread" onclick={() => { spreadStub(item.stub.content_hash); }}>&#x1F4E1;</button>
              {/if}
            </div>
          </div>
        {/each}
      {/if}
    </div>
  {/if}

  <!-- Branch / Thread View -->
  {#if view === "branch"}
    <div class="page">
      <button class="link-btn back" onclick={() => navigateTo("feed")}>&larr; Back to feed</button>
      <h2>Thread</h2>
      {#each branchStubs as stub}
        <div class="post-card">
          <p class="stub-text">{stub.text || ""}</p>
          <div class="post-meta">
            <button class="link-btn author" onclick={() => openProfile(stub.author_pubkey)}>{authorLabel(stub.author_display_name, stub.author_short_id)}</button>
            {#if stub.is_root}<span class="edge-dot edge-root"></span>{/if}
            <span class="timestamp" title={new Date(stub.timestamp).toLocaleString()}>{timeAgo(stub.timestamp)}</span>
            <span class="meta-spacer"></span>
            <button class="icon-btn" title="Reply" onclick={() => { composeReplyTo = stub.content_hash; view = "feed"; }}>&#x21A9;</button>
            <button class="icon-btn" title="Spread" onclick={() => { spreadStub(stub.content_hash); }}>&#x1F4E1;</button>
          </div>
        </div>
      {/each}
      {#if branchStubs.length === 0}
        <p class="muted">No stubs in this branch yet.</p>
      {/if}
    </div>
  {/if}

  <!-- Profile -->
  {#if view === "profile"}
    <div class="page">
      {#if profileData}
        <div class="card profile-card">
          <h2>{profileData.pubkey === myPubkey ? "Your Profile" : "Profile"}</h2>
          {#if profileData.display_name}
            <p class="display-name-label">{profileData.display_name}</p>
          {/if}
          <div class="pubkey-display">
            <span class="mono">{profileData.short_id}</span>
            <button class="link-btn" onclick={() => navigator.clipboard.writeText(profileData.pubkey)}>copy full key</button>
          </div>
          {#if profileData.pubkey === myPubkey}
            <div class="edit-name">
              <input type="text" placeholder="Set display name (any text)" bind:value={editDisplayName} onkeydown={(e) => e.key === "Enter" && saveDisplayName()} />
              <button onclick={saveDisplayName}>{editDisplayName.trim() ? "Save" : "Clear"}</button>
            </div>
          {/if}
          <div class="metrics">
            <div class="metric">
              <span class="metric-value">{profileData.trust_density}</span>
              <span class="metric-label">trust density</span>
            </div>
            <div class="metric">
              <span class="metric-value">{profileData.watch_count}</span>
              <span class="metric-label">watchers</span>
            </div>
            <div class="metric">
              <span class="metric-value">{profileData.built_upon_count}</span>
              <span class="metric-label">built upon</span>
            </div>
            <div class="metric">
              <span class="metric-value">{profileData.stub_count}</span>
              <span class="metric-label">stubs</span>
            </div>
          </div>
          {#if profileData.pubkey !== myPubkey}
            <div class="profile-actions">
              <button onclick={() => { edgeTargetPubkey = profileData.pubkey; edgeType = "trust"; createEdge(); }}>Trust</button>
              <button onclick={() => { edgeTargetPubkey = profileData.pubkey; edgeType = "watch"; createEdge(); }}>Watch</button>
              <button class="danger" onclick={() => { edgeTargetPubkey = profileData.pubkey; edgeType = "silence"; createEdge(); }}>Silence</button>
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {/if}

  <!-- People -->
  {#if view === "people"}
    <div class="page">
      <h2>People</h2>
      <!-- Search -->
      <input type="text" placeholder="Search by name or public key..." bind:value={peopleSearch} class="search-input" />

      <!-- Add edge manually -->
      <div class="card">
        <h3>Add by public key</h3>
        <input type="text" placeholder="Paste a public key (64 hex chars)" bind:value={edgeTargetPubkey} />
        <div class="edge-form">
          <select bind:value={edgeType}>
            <option value="trust">Trust</option>
            <option value="watch">Watch</option>
            <option value="silence">Silence</option>
          </select>
          {#if edgeType === "trust"}
            <input type="range" min="0.1" max="1.0" step="0.1" bind:value={edgeWeight} />
            <span>{edgeWeight.toFixed(1)}</span>
          {/if}
          <button onclick={createEdge}>Add</button>
        </div>
      </div>

      <!-- Known identities -->
      {#if knownIdentities.length === 0}
        <p class="muted">No other identities discovered yet. Connect to the network and wait for stubs to arrive.</p>
      {:else}
        {#each knownIdentities.filter(p => {
          if (p.pubkey === myPubkey) return false;
          if (!peopleSearch.trim()) return true;
          const q = peopleSearch.toLowerCase();
          return (p.display_name && p.display_name.toLowerCase().includes(q)) || p.pubkey.startsWith(q) || p.short_id.startsWith(q);
        }) as person}
          {#if true}
            <div class="person-row">
              <button class="link-btn author" onclick={() => openProfile(person.pubkey)}>{authorLabel(person.display_name, person.short_id)}</button>
              <span class="person-stats">{person.stub_count} stubs / {person.trust_density} trusted</span>
              <div class="person-actions">
                <button class="small-btn" onclick={() => { edgeTargetPubkey = person.pubkey; edgeType = "trust"; createEdge(); }}>trust</button>
                <button class="small-btn" onclick={() => { edgeTargetPubkey = person.pubkey; edgeType = "watch"; createEdge(); }}>watch</button>
                <button class="small-btn danger" onclick={() => { edgeTargetPubkey = person.pubkey; edgeType = "silence"; createEdge(); }}>silence</button>
              </div>
            </div>
          {/if}
        {/each}
      {/if}
    </div>
  {/if}

  <!-- Settings -->
  {#if view === "settings"}
    <div class="page">
      <h2>Settings</h2>

      <!-- Identity -->
      <div class="card">
        <h3>Identity</h3>
        <div class="pubkey-display">
          <span class="mono">{myShortId}</span>
          <button class="link-btn" onclick={() => navigator.clipboard.writeText(myPubkey)}>copy full key</button>
        </div>
      </div>

      <!-- Network -->
      <div class="card">
        <h3>Network</h3>
        {#if networkStatus.is_running}
          <p>Connected to {networkStatus.peer_count} peer(s)</p>
          {#if networkStatus.listen_addrs.length > 0}
            <p class="hint">Share one of these addresses with another tester so they can connect to you:</p>
            {#each networkStatus.listen_addrs as addr}
              <div class="addr-row">
                <code class="mono small">{addr}</code>
                <button class="link-btn" onclick={() => navigator.clipboard.writeText(addr)}>copy</button>
              </div>
            {/each}
          {/if}
        {:else}
          <textarea
            placeholder="Bootstrap peer addresses (one per line)"
            bind:value={bootstrapPeers}
            rows="3"
          ></textarea>
          <button onclick={startNetwork}>Start Network</button>
        {/if}
        <button class="link-btn" onclick={refreshNetworkStatus}>refresh</button>
      </div>

      <!-- Edges -->
      <div class="card">
        <h3>Your edges ({outboundEdges.length})</h3>
        {#each outboundEdges as edge}
          <div class="edge-row">
            <span class="edge-badge {edge.edge_type}">{edge.edge_type}</span>
            <span class="mono">{edge.target_pubkey.slice(0, 16)}</span>
            {#if edge.edge_type === "trust"}
              <span>w={edge.weight.toFixed(1)}</span>
            {/if}
            <button class="link-btn danger" onclick={() => removeEdge(edge.target_pubkey, edge.edge_type)}>remove</button>
          </div>
        {/each}
        {#if outboundEdges.length === 0}
          <p class="muted">No edges yet.</p>
        {/if}
      </div>
    </div>
  {/if}
</main>

<style>
  :root {
    --bg: #fafafa;
    --bg-card: #ffffff;
    --text: #1a1a1a;
    --text-muted: #888;
    --border: #e5e5e5;
    --accent: #2563eb;
    --trust: #059669;
    --watch: #d97706;
    --silence: #dc2626;
    --danger: #dc2626;
    font-family: "Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    font-size: 15px;
    line-height: 1.6;
    color: var(--text);
    background: var(--bg);
  }

  @media (prefers-color-scheme: dark) {
    :root {
      --bg: #111;
      --bg-card: #1a1a1a;
      --text: #e5e5e5;
      --text-muted: #777;
      --border: #333;
    }
  }

  main {
    max-width: 640px;
    margin: 0 auto;
    padding: 0 16px 80px;
  }

  nav {
    display: flex;
    gap: 4px;
    align-items: center;
    padding: 12px 0;
    border-bottom: 1px solid var(--border);
    margin-bottom: 16px;
    position: sticky;
    top: 0;
    background: var(--bg);
    z-index: 10;
  }


  .nav-status {
    margin-left: auto;
    font-size: 12px;
    color: var(--text-muted);
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    display: inline-block;
  }
  .dot.online { background: var(--trust); }
  .dot.offline { background: var(--text-muted); }

  .center-page {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-height: 60vh;
    text-align: center;
  }

  h1 { font-size: 32px; font-weight: 700; margin: 0 0 4px; letter-spacing: -0.5px; }
  h2 { font-size: 20px; font-weight: 600; margin: 0 0 12px; }
  h3 { font-size: 16px; font-weight: 600; margin: 0 0 8px; }

  .card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 20px;
    margin-bottom: 16px;
  }

  input[type="text"], input[type="password"], textarea {
    width: 100%;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg);
    color: var(--text);
    font-size: 14px;
    font-family: inherit;
    margin-bottom: 10px;
    box-sizing: border-box;
    resize: vertical;
  }

  button {
    padding: 8px 16px;
    border: none;
    border-radius: 8px;
    background-color: #2563eb;
    color: #ffffff;
    font-size: 14px;
    font-weight: 500;
    cursor: pointer;
  }
  button:hover { opacity: 0.9; }
  button:disabled { opacity: 0.4; cursor: default; }
  button.danger { background-color: #dc2626; color: #ffffff; }

  .link-btn {
    background-color: transparent;
    color: #2563eb;
    padding: 2px 4px;
    font-size: 13px;
  }
  .link-btn:hover { text-decoration: underline; }
  .link-btn.danger { color: #dc2626; }
  .link-btn.back { margin-bottom: 12px; padding: 0; }
  .link-btn.author { font-family: "SF Mono", "Fira Code", monospace; font-size: 13px; }

  .nav-btn {
    background-color: transparent;
    border: none;
    padding: 6px 12px;
    border-radius: 6px;
    cursor: pointer;
    color: var(--text-muted);
    font-size: 14px;
    font-weight: 500;
  }
  .nav-btn:hover { background-color: var(--border); color: var(--text); }
  .nav-btn.active { color: var(--text); background-color: var(--border); }

  .small-btn {
    padding: 3px 8px;
    font-size: 12px;
    border-radius: 4px;
    background-color: #2563eb;
    color: #ffffff;
  }
  .small-btn.danger { background-color: #dc2626; }

  .muted { color: var(--text-muted); }
  .mono { font-family: "SF Mono", "Fira Code", monospace; font-size: 13px; }
  .small { font-size: 12px; }
  .hint { font-size: 13px; color: var(--text-muted); margin: 4px 0 16px; }

  .error {
    background: #fef2f2;
    color: var(--danger);
    border: 1px solid #fecaca;
    padding: 8px 12px;
    border-radius: 8px;
    margin-bottom: 12px;
    font-size: 13px;
  }
  @media (prefers-color-scheme: dark) {
    .error { background: #2d1111; border-color: #5c2020; }
  }

  .status {
    color: var(--trust);
    font-size: 13px;
    margin-bottom: 8px;
  }

  .feed-toggle {
    display: flex;
    gap: 2px;
    margin-bottom: 12px;
    background-color: var(--border);
    border-radius: 8px;
    padding: 2px;
  }
  .toggle-btn {
    flex: 1;
    padding: 6px 12px;
    border: none;
    border-radius: 6px;
    background-color: transparent;
    color: var(--text-muted);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
  }
  .toggle-btn.active {
    background-color: var(--bg-card);
    color: var(--text);
  }

  .compose-box {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 12px;
    margin-bottom: 16px;
  }
  .compose-box textarea { margin-bottom: 4px; }
  .compose-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .char-count { font-size: 12px; color: var(--text-muted); }
  .char-count.over { color: var(--danger); }
  .reply-indicator {
    font-size: 12px;
    color: var(--text-muted);
    margin-bottom: 6px;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .post-card {
    border-bottom: 1px solid var(--border);
    padding: 12px 0;
  }
  .post-card:last-child { border-bottom: none; }

  .stub-text { margin: 0 0 6px; white-space: pre-wrap; word-break: break-word; line-height: 1.5; }

  .post-meta {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
  }
  .meta-spacer { flex: 1; }
  .timestamp { font-size: 12px; color: var(--text-muted); cursor: default; }

  .edge-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    display: inline-block;
    flex-shrink: 0;
  }
  .edge-dot.edge-own { background-color: #2563eb; }
  .edge-dot.edge-trust { background-color: #059669; }
  .edge-dot.edge-watch { background-color: #d97706; }
  .edge-dot.edge-root { background-color: #2563eb; }

  .icon-btn {
    background-color: transparent;
    border: none;
    padding: 2px 6px;
    cursor: pointer;
    font-size: 14px;
    opacity: 0.4;
    border-radius: 4px;
    line-height: 1;
  }
  .icon-btn:hover { opacity: 1; background-color: var(--border); }

  .edge-badge {
    font-size: 11px;
    padding: 1px 6px;
    border-radius: 4px;
    font-weight: 500;
  }
  .edge-badge.trust { background: #ecfdf5; color: var(--trust); }
  .edge-badge.watch { background: #fffbeb; color: var(--watch); }
  .edge-badge.silence { background: #fef2f2; color: var(--silence); }
  @media (prefers-color-scheme: dark) {
    .edge-badge.trust { background: #052e16; }
    .edge-badge.watch { background: #2d2006; }
    .edge-badge.silence { background: #2d1111; }
  }

  .empty-state {
    text-align: center;
    padding: 40px 0;
    color: var(--text-muted);
  }

  .metrics {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 12px;
    margin: 16px 0;
  }
  .metric { text-align: center; }
  .metric-value { display: block; font-size: 24px; font-weight: 700; }
  .metric-label { font-size: 11px; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.5px; }

  .profile-actions {
    display: flex;
    gap: 8px;
    margin-top: 12px;
  }

  .pubkey-display {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }

  .edge-form {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .edge-form select {
    padding: 6px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--text);
  }

  .edge-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
  }
  .edge-row:last-child { border-bottom: none; }

  .search-input { margin-bottom: 12px; }

  .person-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 0;
    border-bottom: 1px solid var(--border);
  }
  .person-stats { font-size: 12px; color: var(--text-muted); flex: 1; }
  .person-actions { display: flex; gap: 4px; }

  .profile-card { text-align: center; }
  .display-name-label { font-size: 20px; font-weight: 600; margin: 0 0 4px; }
  .edit-name { display: flex; gap: 8px; margin-top: 12px; }
  .edit-name input { flex: 1; margin-bottom: 0; }
  .profile-card .pubkey-display { justify-content: center; }

  .addr-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 0;
    word-break: break-all;
  }
  .addr-row code { flex: 1; font-size: 11px; }

  .page { padding-top: 8px; }
</style>
