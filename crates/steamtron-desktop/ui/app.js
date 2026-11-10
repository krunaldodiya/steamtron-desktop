const invoke = (command, args = {}) => window.__TAURI__.core.invoke(command, args);
const convertFileSrc = (path) => window.__TAURI__.core.convertFileSrc(path);
const gamesEl = document.querySelector("#games");
const detailEl = document.querySelector("#detail");
const searchEl = document.querySelector("#search");
let games = [];
let fixes = new Map();
let playStates = new Map();
let heroPlayTimer = null;
let selected = null;
let detailTab = "general";
let artworkLoading = false;
let managedLibrary = null;
let managedSelectedId = null;
let runningManaged = new Set();
let libraryPage = 1;
let homeFocusSection = "recent";
const LIBRARY_PAGE_SIZE = 60;
let appView = "library";
let gameDetailOrigin = "library";
let pendingRestart = loadPendingRestart();


function loadPendingRestart() {
  try { return JSON.parse(localStorage.getItem("steamtron.pendingRestart.v1") || "null"); } catch (_error) { return null; }
}

function savePendingRestart(value) {
  pendingRestart = value;
  try {
    if (value) localStorage.setItem("steamtron.pendingRestart.v1", JSON.stringify(value));
    else localStorage.removeItem("steamtron.pendingRestart.v1");
  } catch (_error) {}
  renderRestartBanner();
}

function markRestartRequired(message = "SteamTron changed Steam library files.") {
  savePendingRestart({ message, at: new Date().toISOString() });
}

function clearRestartRequired() {
  savePendingRestart(null);
}

function renderRestartBanner() {
  const banner = document.querySelector("#restart-banner");
  if (!banner) return;
  banner.classList.toggle("hidden", !pendingRestart);
  const text = document.querySelector("#restart-banner-text");
  if (text) text.textContent = pendingRestart?.message || "SteamTron changed Steam library files.";
}

function coverUrl(appid) {
  return `https://cdn.cloudflare.steamstatic.com/steam/apps/${encodeURIComponent(appid)}/library_600x900.jpg`;
}

function headerUrl(appid) {
  return `https://cdn.cloudflare.steamstatic.com/steam/apps/${encodeURIComponent(appid)}/header.jpg`;
}

function heroUrl(appid) {
  return `https://cdn.cloudflare.steamstatic.com/steam/apps/${encodeURIComponent(appid)}/library_hero.jpg`;
}

function bindCoverImage(image, appid) {
  image.addEventListener("error", () => {
    if (image.dataset.fallback === "header") {
      image.dataset.fallback = "shared";
      image.src = `https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/${encodeURIComponent(appid)}/header.jpg`;
      return;
    }
    if (image.dataset.fallback === "shared") {
      image.classList.add("missing");
      image.removeAttribute("src");
      return;
    }
    image.dataset.fallback = "header";
    image.src = headerUrl(appid);
  });
}

function quicklookMarkup(game) {
  const chips = [];
  chips.push(`<span class="quicklook ${game.installed ? "ok" : "warn"}">${game.installed ? "Installed" : "Not installed"}</span>`);
  if (game.managed) chips.push('<span class="quicklook">Managed</span>');
  if (game.build_id && game.build_id !== "0") chips.push(`<span class="quicklook">build ${escape(game.build_id)}</span>`);
  if (game.pinned) chips.push(`<span class="quicklook pin">Pinned ${escape(game.pinned)}</span>`);
  const fix = fixes.get(game.appid);
  if (fix) chips.push(`<span class="quicklook ${fix.enabled ? "ok" : "muted"}">Fixes ${fix.enabled ? "on" : "off"}</span>`);
  return chips.join("");
}

const escape = (value) => String(value ?? "").replace(/[&<>"']/g, (char) => ({
  "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
}[char]));

function nextPaint() {
  return new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
}

function busy(button, label) {
  if (!button) return () => {};
  const original = button.textContent;
  button.classList.add("loading");
  button.disabled = true;
  button.setAttribute("aria-busy", "true");
  if (label) button.textContent = label;
  return () => {
    button.classList.remove("loading");
    button.removeAttribute("aria-busy");
    button.textContent = original;
    button.disabled = false;
  };
}

function showErrorDialog(message, title = "Action failed") {
  const existing = document.querySelector(".error-dialog-backdrop");
  if (existing) existing.remove();
  const backdrop = document.createElement("div");
  backdrop.className = "modal-backdrop error-dialog-backdrop";
  backdrop.innerHTML = `<div class="modal"><div class="modal-head"><h2>${escape(title)}</h2><button class="modal-close" aria-label="Close">×</button></div>
    <p>${escape(String(message))}</p>
    <div class="modal-actions"><button class="primary" id="error-dialog-ok">OK</button></div>
  </div>`;
  document.body.appendChild(backdrop);
  const close = () => backdrop.remove();
  backdrop.querySelector(".modal-close").onclick = close;
  backdrop.querySelector("#error-dialog-ok").onclick = close;
  backdrop.addEventListener("click", (event) => { if (event.target === backdrop) close(); });
  backdrop.querySelector("#error-dialog-ok").focus();
}

function showSteamRunningDialog(action) {
  const existing = document.querySelector(".steam-running-backdrop");
  if (existing) existing.remove();
  const backdrop = document.createElement("div");
  backdrop.className = "modal-backdrop steam-running-backdrop";
  backdrop.innerHTML = `<div class="modal"><div class="modal-head"><h2>Steam is running</h2><button class="modal-close" aria-label="Close">×</button></div>
    <p>Steam must be fully closed before you can ${escape(action)}.</p>
    <p class="muted">Close Steam completely, then try again. This prevents Steam from overwriting fixes files and launch options.</p>
    <div class="modal-actions"><button class="primary" id="steam-running-ok">OK</button></div>
  </div>`;
  document.body.appendChild(backdrop);
  const close = () => backdrop.remove();
  backdrop.querySelector(".modal-close").onclick = close;
  backdrop.querySelector("#steam-running-ok").onclick = close;
  backdrop.addEventListener("click", (event) => { if (event.target === backdrop) close(); });
  backdrop.querySelector("#steam-running-ok").focus();
}

async function ensureSteamClosed(action) {
  try {
    if (!await invoke("steam_running")) return true;
  } catch (error) {
    status(String(error), true);
    return false;
  }
  showSteamRunningDialog(action);
  return false;
}

function status(message, error = false) {
  if (error) {
    showErrorDialog(message);
    return;
  }
  const node = detailEl.querySelector(".status");
  if (node) {
    node.textContent = message;
    node.classList.remove("error");
  }
}

const authGate = document.querySelector("#auth-gate");
const gateSignin = document.querySelector("#gate-signin");
const gateStatus = document.querySelector("#gate-status");
const appSections = [document.querySelector(".topbar"), document.querySelector(".layout")];

function showAuthGate(message = "") {
  authGate.classList.remove("hidden");
  for (const node of appSections) { node.inert = true; node.setAttribute("aria-hidden", "true"); }
  gateStatus.textContent = message;
  gateSpinner.classList.add("hidden");
  document.querySelector("#gate-retry").disabled = false;
}

function hideAuthGate() {
  authGate.classList.add("hidden");
  for (const node of appSections) { node.inert = false; node.removeAttribute("aria-hidden"); }
}


async function checkMigration() {
  try {
    const status = await invoke("library_migration_status");
    if (!status || status.library.games > 0 || !status.sources?.length) return;
    showMigration(status.sources);
  } catch (error) {
    console.warn("Migration check failed", error);
  }
}

function migrationSummary(plan) {
  const actions = plan?.actions || [];
  const kind = (action) => Object.keys(action)[0];
  const count = (name) => actions.filter((action) => kind(action) === name).length;
  return {
    games: count("AddGame"),
    drives: count("AddDrive"),
    registrations: count("RegisterSteam"),
    copies: count("CopyFixes"),
    needs: count("NeedsInput"),
  };
}

function showMigration(sources) {
  const backdrop = document.createElement("div");
  backdrop.className = "modal-backdrop";
  const rows = sources.map((source) => {
    const title = source.kind === "umutron" ? "UmuTron library" : "SteamTron config";
    if (source.error) {
      return `<div class="row"><strong>${escape(title)}</strong><div class="muted">${escape(source.path)}</div><div class="status error">${escape(source.error)}</div></div>`;
    }
    const summary = migrationSummary(source.plan);
    const parts = [`${summary.games} game${summary.games === 1 ? "" : "s"}`];
    if (summary.drives) parts.push(`${summary.drives} drive${summary.drives === 1 ? "" : "s"}`);
    if (summary.registrations) parts.push(`${summary.registrations} Steam registration${summary.registrations === 1 ? "" : "s"}`);
    if (summary.copies) parts.push(`${summary.copies} fixes cop${summary.copies === 1 ? "y" : "ies"}`);
    if (summary.needs) parts.push(`${summary.needs} needing a folder`);
    const warnings = (source.plan.warnings || []).length;
    return `<div class="row"><strong>${escape(title)}</strong><div class="muted">${escape(source.path)}</div><div>${escape(parts.join(" · "))}</div>${warnings ? `<div class="muted">${warnings} warning${warnings === 1 ? "" : "s"}</div>` : ""}</div>`;
  }).join("");
  backdrop.innerHTML = `<div class="modal"><div class="modal-head"><h2>Bring your library over</h2><button class="modal-close" id="migrate-close" aria-label="Close">×</button></div><p class="muted">SteamTron keeps games in its own library under SteamTronLibrary/steamtronapps. Existing entries are imported once, fixes are copied (never deleted from the source), and Steam registrations stay pending until the backend is available.</p><div class="migration-sources">${rows}</div><div class="modal-status"></div><div class="modal-actions"><button id="migrate-later">Not now</button><button id="migrate-import" class="primary">Import</button></div></div>`;
  document.body.appendChild(backdrop);
  const statusNode = backdrop.querySelector(".modal-status");
  const close = () => backdrop.remove();
  backdrop.querySelector("#migrate-close").onclick = close;
  backdrop.querySelector("#migrate-later").onclick = close;
  backdrop.querySelector("#migrate-import").onclick = async () => {
    const button = backdrop.querySelector("#migrate-import");
    const stop = busy(button, "Importing…");
    const reports = [];
    try {
      for (const source of sources) {
        if (source.error) continue;
        const result = await invoke("library_import_commit", { path: source.path });
        reports.push(result.report || {});
      }
      stop();
      const games = reports.reduce((sum, report) => sum + (report.games_added?.length || 0), 0);
      const skipped = reports.reduce((sum, report) => sum + (report.games_skipped?.length || 0), 0);
      const pending = reports.reduce((sum, report) => sum + (report.pending_registrations?.length || 0), 0);
      statusNode.textContent = `Imported ${games} game${games === 1 ? "" : "s"}` +
        (skipped ? `, skipped ${skipped} existing` : "") +
        (pending ? `; ${pending} Steam registration${pending === 1 ? "" : "s"} pending.` : ".");
      button.disabled = true;
      await load();
    } catch (error) {
      stop();
      statusNode.textContent = String(error);
      statusNode.classList.add("error");
    }
  };
}


async function refreshManagedRunning() {
  try { runningManaged = new Set(await invoke("running_managed")); }
  catch (_error) { runningManaged = new Set(); }
}

function hasManagedLibrary() {
  return Boolean(managedLibrary?.games?.length);
}

function managedDrive(game) {
  const root = game.installation?.root || "";
  return (managedLibrary?.drives || []).find((drive) => {
    const base = String(drive.root || "").replace(/\/+$/, "");
    return base && (root === base || root.startsWith(base + "/"));
  }) || null;
}

function managedReady(game) {
  return Boolean(game.installation?.root);
}

const artworkCache = new Map();

async function artworkFor(gameId) {
  if (!gameId) return null;
  if (artworkCache.has(gameId)) return artworkCache.get(gameId);
  let artwork = null;
  try { artwork = await invoke("managed_artwork", { gameId }); }
  catch (_error) { artwork = null; }
  artworkCache.set(gameId, artwork);
  return artwork;
}

function markCoverMissing(img) {
  const frame = img.parentElement;
  img.remove();
  if (frame && !frame.classList.contains("cover-missing")) {
    frame.classList.add("cover-missing");
  }
}

async function hydrateCovers(host) {
  const images = [...host.querySelectorAll("img[data-cover-game]")].filter((img) => !img.dataset.started);
  await Promise.all(images.map(async (img) => {
    img.dataset.started = "1";
    const game = managedLibrary?.games.find((item) => item.id === img.dataset.coverGame);
    if (!game?.metadata_app_id) { markCoverMissing(img); return; }
    let artwork = null;
    try { artwork = await artworkFor(game.id); } catch (_error) { artwork = null; }
    if (!img.isConnected) return;
    if (artwork?.cover) {
      img.src = convertFileSrc(artwork.cover);
    } else {
      img.src = coverUrl(game.metadata_app_id);
    }
    img.addEventListener("error", () => {
      if (!img.isConnected) return;
      if (img.dataset.remoteFallback !== "1") {
        img.dataset.remoteFallback = "1";
        img.src = headerUrl(game.metadata_app_id);
        return;
      }
      markCoverMissing(img);
    });
  }));
}

async function hydrateHero(game) {
  if (!game?.metadata_app_id) return;
  try {
    const artwork = await artworkFor(game.id);
    if (!artwork?.hero) return;
    const node = document.querySelector("#home-hero, .game-page .detail-hero");
    if (node && managedSelectedId === game.id) {
      node.style.backgroundImage = `url('${convertFileSrc(artwork.hero)}')`;
    }
  } catch (_error) {
    // The CDN background stays when the local cache is unavailable.
  }
}

function coverCard(game, section) {
  const appId = game.metadata_app_id;
  const selected = game.id === managedSelectedId
    && (!section || section === homeFocusSection);
  const active = selected ? " active" : "";
  return `<button class="cover-card${active}" data-managed="${escape(game.id)}" title="${escape(game.title)}">
    <span class="cover">${appId ? `<img loading="lazy" data-cover-game="${escape(game.id)}" alt="">` : ""}<span class="play-glyph" aria-hidden="true">▶</span></span>
    <span class="cover-title">${escape(game.title)}</span>
  </button>`;
}

function managedFilteredGames() {
  return managedLibrary.games;
}

function libraryPages() {
  return Math.max(1, Math.ceil(managedFilteredGames().length / LIBRARY_PAGE_SIZE));
}

function renderManagedLibrary() {
  const filtered = managedFilteredGames();
  const pages = libraryPages();
  libraryPage = Math.min(Math.max(1, libraryPage), pages);
  const start = (libraryPage - 1) * LIBRARY_PAGE_SIZE;
  const visible = filtered.slice(start, start + LIBRARY_PAGE_SIZE);
  const total = managedLibrary.games.length;
  detailEl.innerHTML = `<div class="page library-page">
    <header class="page-head"><h1>Your library</h1><p>Every game, one place. Pick up where you left off.</p></header>
    <p class="library-count">${total} games</p>
    ${visible.length ? `<div class="cover-grid">${visible.map((game) => coverCard(game)).join("")}</div>` : '<div class="empty">Your library is empty. Add a game from the Store.</div>'}
    <nav class="pager" aria-label="Library pages">
      <button data-page="first"${libraryPage === 1 ? " disabled" : ""}>First</button>
      <button data-page="prev"${libraryPage === 1 ? " disabled" : ""}>Previous</button>
      <span class="pager-page">${libraryPage}</span>
      <button data-page="next"${libraryPage === pages ? " disabled" : ""}>Next</button>
      <button data-page="last"${libraryPage === pages ? " disabled" : ""}>Last</button>
    </nav>
    <p class="pager-hint">Page ${libraryPage} of ${pages} · PgUp / PgDn</p>
  </div>`;
  void hydrateCovers(detailEl);
  for (const button of detailEl.querySelectorAll("[data-page]")) {
    button.onclick = () => {
      if (button.dataset.page === "first") libraryPage = 1;
      else if (button.dataset.page === "prev") libraryPage = Math.max(1, libraryPage - 1);
      else if (button.dataset.page === "next") libraryPage = Math.min(libraryPages(), libraryPage + 1);
      else libraryPage = libraryPages();
      renderManagedLibrary();
    };
  }
}

async function startManagedGame(game, button) {
  const stop = button ? busy(button, "Starting…") : () => {};
  try {
    const result = await invoke("play_managed", { gameId: game.id });
    if (result.mode !== "steam") runningManaged.add(game.id);
    showToast(result.mode === "steam"
      ? `Starting ${game.title} through Steam`
      : `Started ${game.title} · ${result.log}`);
    renderManagedMode();
  } catch (error) {
    showToast(String(error), true);
    stop();
  }
}

async function stopManagedGame(game, button) {
  const stop = button ? busy(button, "Stopping…") : () => {};
  try {
    await invoke("stop_managed", { gameId: game.id });
    runningManaged.delete(game.id);
    showToast(`Stopped ${game.title}`);
    renderManagedMode();
  } catch (error) {
    showToast(String(error), true);
    stop();
  }
}

function managedSetupMarkup(game) {
  const target = game.launch_targets?.find((item) => item.primary) || game.launch_targets?.[0];
  const executable = target?.executable || "";
  const argumentsText = (target?.arguments || []).join(" ");
  return `<div class="status" id="managed-setup"><div class="row"><div class="label">Launch target</div><div class="value">${escape(executable || "Not configured")}</div></div><label for="managed-exe">Executable</label><div class="search-row"><input id="managed-exe" placeholder="Game.exe" value="${escape(executable)}"><button id="managed-exe-browse">Browse…</button></div><label for="managed-args">Arguments</label><input id="managed-args" placeholder="-windowed (space separated)" value="${escape(argumentsText)}"><label for="managed-proton">Proton runner (optional)</label><input id="managed-proton" value="${escape(game.launch?.proton || game.installation?.proton || "")}"><div class="update-actions"><button class="primary" id="managed-setup-save">Save setup</button></div></div>`;
}

async function wireManagedSetup(game, host = detailEl, onSaved = () => renderManagedMode()) {
  const box = host.querySelector("#managed-setup");
  if (!box) return;
  const exeInput = box.querySelector("#managed-exe");
  const argsInput = box.querySelector("#managed-args");
  const protonInput = box.querySelector("#managed-proton");
  box.querySelector("#managed-exe-browse").onclick = async () => {
    try {
      const path = await invoke("pick_managed_executable", { gameId: game.id });
      if (path) exeInput.value = path;
    } catch (error) { showToast(String(error), true); }
  };
  box.querySelector("#managed-setup-save").onclick = async () => {
    const executable = exeInput.value.trim();
    if (!executable) { showToast("Choose the game executable first.", true); return; }
    const button = box.querySelector("#managed-setup-save");
    const stop = busy(button, "Saving…");
    try {
      const result = await invoke("set_managed_launch_target", { request: {
        game_id: game.id,
        executable,
        arguments: argsInput.value.trim().split(/\s+/).filter(Boolean),
        proton: protonInput.value.trim(),
        name: "",
      } });
      showToast(`Launch target saved: ${result.name}`);
      onSaved();
    } catch (error) {
      showToast(String(error), true);
      stop();
    }
  };
}

async function loadManagedFixes(gameId, host = detailEl) {
  const box = host.querySelector("#managed-fixes");
  if (!box) return;
  try {
    const view = await invoke("managed_fixes", { gameId });
    const value = view.files.length
      ? `${view.files.length} file${view.files.length === 1 ? "" : "s"} · ${view.path}`
      : `None installed · ${view.path}`;
    box.innerHTML = `<div class="row"><div class="label">Fixes</div><div class="value">${escape(value)}</div></div><div class="update-actions"><button id="managed-fixes-install">Install ZIP…</button>${view.files.length ? '<button id="managed-fixes-remove">Remove</button>' : ""}</div>`;
    box.querySelector("#managed-fixes-install").onclick = async () => {
      try {
        const installed = await invoke("install_managed_fixes", { gameId });
        if (!installed) return;
        showToast(`Installed ${installed.files.length} fix file${installed.files.length === 1 ? "" : "s"}`);
        await loadManagedFixes(gameId, host);
      } catch (error) { showToast(String(error), true); }
    };
    const remove = box.querySelector("#managed-fixes-remove");
    if (remove) remove.onclick = async () => {
      if (!window.confirm("Remove the installed fixes for this game?")) return;
      try {
        const removed = await invoke("remove_managed_fixes", { gameId });
        showToast(`Removed ${removed.files} fix file${removed.files === 1 ? "" : "s"}`);
        await loadManagedFixes(gameId, host);
      } catch (error) { showToast(String(error), true); }
    };
  } catch (error) {
    box.innerHTML = `<div class="row"><div class="label">Fixes</div><div class="value">${escape(String(error))}</div></div>`;
  }
}

const protonTierCache = new Map();

async function protonTier(appid) {
  if (!appid) return null;
  if (protonTierCache.has(appid)) return protonTierCache.get(appid);
  let tier = null;
  try { tier = await invoke("proton_tier", { appid }); }
  catch (_error) { tier = null; }
  protonTierCache.set(appid, tier);
  return tier;
}

function renderGameChips(node, chips) {
  if (!node) return;
  node.innerHTML = chips.map((chip) => chip.copy
    ? `<button class="game-chip" data-copy="${escape(String(chip.copy))}" data-copy-label="${escape(chip.label || "Value")}" title="${escape(chip.title || `Click to copy ${chip.label || "value"}`)}">${escape(chip.text)}</button>`
    : `<span class="game-chip"${chip.title ? ` title="${escape(chip.title)}"` : ""}>${escape(chip.text)}</span>`).join("");
}

async function copyToClipboard(value, label) {
  try {
    await navigator.clipboard.writeText(value);
    showToast(`${label} copied`);
  } catch (_error) {
    try {
      const area = document.createElement("textarea");
      area.value = value;
      area.style.position = "fixed";
      area.style.opacity = "0";
      document.body.appendChild(area);
      area.select();
      document.execCommand("copy");
      area.remove();
      showToast(`${label} copied`);
    } catch (fallbackError) {
      showToast(String(fallbackError), true);
    }
  }
}

document.addEventListener("click", (event) => {
  const chip = event.target.closest("[data-copy]");
  if (!chip) return;
  void copyToClipboard(chip.dataset.copy, chip.dataset.copyLabel || "Value");
});

function renderManagedPreview(game) {
  const appId = game.metadata_app_id;
  const target = game.launch_targets?.find((item) => item.primary) || game.launch_targets?.[0];
  const canPlay = managedReady(game) && Boolean(target);
  const steamMode = Boolean(game.launch?.requires_steam || game.steam?.launch_mode === "steam_client");
  const running = runningManaged.has(game.id);
  const action = running
    ? '<button id="managed-stop" class="primary">Stop</button>'
    : `<button id="managed-play" class="primary"${canPlay ? "" : " disabled"}>${steamMode ? "Play via Steam" : "Play"}</button>`;
  detailEl.innerHTML = `<div class="page game-page game-detail">
    <div class="game-page-top"><button id="managed-back" class="secondary">← ${gameDetailOrigin === "store" ? "Store" : "Library"}</button></div>
    <section class="game-hero${appId ? "" : " no-artwork"}" id="game-hero">
      <div class="game-hero-shade"></div>
      <div class="game-hero-card">
        <img class="game-cover" id="game-cover" alt="" loading="lazy">
        <div class="game-info">
          <h1>${escape(game.title)}</h1>
          <p class="game-meta" id="game-meta">${escape(appId ? `Steam App ${appId}` : "Managed entry")}</p>
          <p class="game-description hidden" id="game-description"></p>
          <p class="game-credits" id="game-credits"></p>
          <div class="game-actions">${action}<button id="managed-options" class="game-icon-button" title="Game options" aria-label="Game options"><svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><circle cx="12" cy="12" r="3"></circle><path d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1.03 1.56V21a2 2 0 1 1-4 0v-.09a1.7 1.7 0 0 0-1.11-1.56 1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.7 1.7 0 0 0 .34-1.87 1.7 1.7 0 0 0-1.56-1.03H3a2 2 0 1 1 0-4h.09a1.7 1.7 0 0 0 1.56-1.11 1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.87.34H9a1.7 1.7 0 0 0 1.03-1.56V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1.03 1.56 1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.34 1.87V9c.23.63.83 1.03 1.56 1.03H21a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.56 1.03z"></path></svg></button><span class="game-size" id="game-size">Size: …</span></div>
          ${steamMode ? '<p class="muted game-note">Steam client required to launch.</p>' : ""}
          <div class="game-chips" id="game-chips"></div>
        </div>
      </div>
    </section>
  </div>`;
  detailEl.querySelector("#managed-back").onclick = () => setView(gameDetailOrigin);
  detailEl.querySelector("#managed-options").onclick = () => openManagedGameOptions(game);
  void hydrateGameDetail(game);
  const play = detailEl.querySelector("#managed-play");
  if (play && canPlay) play.onclick = () => startManagedGame(game, play);
  const stopButton = detailEl.querySelector("#managed-stop");
  if (stopButton) stopButton.onclick = () => stopManagedGame(game, stopButton);
}

async function hydrateGameDetail(game) {
  const appId = game.metadata_app_id;
  const heroNode = detailEl.querySelector("#game-hero");
  const coverNode = detailEl.querySelector("#game-cover");
  const metaNode = detailEl.querySelector("#game-meta");
  const descriptionNode = detailEl.querySelector("#game-description");
  const creditsNode = detailEl.querySelector("#game-credits");
  const sizeNode = detailEl.querySelector("#game-size");
  const chipsNode = detailEl.querySelector("#game-chips");
  try {
    const artwork = await artworkFor(game.id);
    if (heroNode && artwork?.hero) heroNode.style.backgroundImage = `url('${convertFileSrc(artwork.hero)}')`;
    if (coverNode) {
      coverNode.addEventListener("error", () => { coverNode.style.visibility = "hidden"; }, { once: true });
      if (artwork?.cover) coverNode.src = convertFileSrc(artwork.cover);
      else if (appId) coverNode.src = coverUrl(appId);
    }
  } catch (_error) { /* keep the gradient */ }
  const chips = [{ text: "Proton: …" }];
  if (appId) chips.push({ text: `STEAM_ID: ${appId}`, copy: String(appId), label: "STEAM_ID" });
  renderGameChips(chipsNode, chips);
  if (!appId) {
    chips[0] = { text: "Proton: Unknown", title: "ProtonDB rates Steam builds; this entry has no Steam App ID." };
    renderGameChips(chipsNode, chips);
  }
  if (appId) {
    void (async () => {
      const tier = await protonTier(appId).catch(() => null);
      chips[0].text = `Proton: ${tier || "Unknown"}`;
      try {
        const igdb = await invoke("igdb_id_for", { appid: appId, title: game.title });
        if (igdb) chips.splice(1, 0, { text: `IGDB_ID: ${igdb}`, copy: String(igdb), label: "IGDB_ID" });
      } catch (_error) { /* IGDB mapping is optional */ }
      renderGameChips(chipsNode, chips);
    })();
  }
  if (appId) {
    try {
      const details = await appDetails(appId);
      const meta = [details.release_date, details.genres].filter(Boolean).join(" · ");
      if (meta && metaNode) metaNode.textContent = meta;
      if (details.description && descriptionNode) {
        descriptionNode.textContent = details.description;
        descriptionNode.classList.remove("hidden");
      }
      const credits = [details.developers, details.publishers].filter(Boolean).join(" · ");
      if (credits && creditsNode) creditsNode.textContent = credits;
    } catch (_error) { /* keep the fallback meta */ }
  }
  try {
    const size = await invoke("managed_game_size", { gameId: game.id });
    if (sizeNode) sizeNode.textContent = `Size: ${formatBytes(size)}`;
  } catch (_error) {
    if (sizeNode) sizeNode.textContent = "Size: unavailable";
  }
}

function openManagedGameOptions(game) {
  const target = game.launch_targets?.find((item) => item.primary) || game.launch_targets?.[0];
  const backdrop = document.createElement("div");
  backdrop.className = "modal-backdrop";
  backdrop.innerHTML = `<div class="modal" style="max-width:700px">
    <div class="modal-head"><h2>Game options</h2><button class="modal-close" aria-label="Close">×</button></div>
    <p class="muted">${escape(game.title)}</p>
    <div class="game-options-paths">
      <div class="row"><div class="label">Installation</div><div class="value">${escape(game.installation?.root || "")}</div></div>
      <div class="row"><div class="label">Prefix</div><div class="value">${escape(game.installation?.prefix || "")}</div></div>
      <div class="row"><div class="label">Launch target</div><div class="value">${escape(target?.executable || "Not configured")}</div></div>
    </div>
    ${managedSetupMarkup(game)}
    <div class="status" id="managed-fixes"><div class="row"><div class="label">Fixes</div><div class="value">Loading…</div></div></div>
  </div>`;
  document.body.appendChild(backdrop);
  const close = () => backdrop.remove();
  backdrop.querySelector(".modal-close").onclick = close;
  backdrop.addEventListener("click", (event) => { if (event.target === backdrop) close(); });
  void wireManagedSetup(game, backdrop, () => { close(); renderManagedMode(); });
  void loadManagedFixes(game.id, backdrop);
  const first = backdrop.querySelector("#managed-setup-save") || backdrop.querySelector("#managed-exe");
  if (first) focusControl(first);
}

function pad2(value) {
  return String(value).padStart(2, "0");
}

function renderManagedHome() {
  const games = managedLibrary.games;
  const installed = games.filter((game) => managedReady(game));
  const featured = games.find((game) => game.id === managedSelectedId) || installed[0] || games[0];
  const appId = featured?.metadata_app_id;
  const hero = appId ? heroUrl(appId) : "";
  const running = featured ? runningManaged.has(featured.id) : false;
  const recentIndex = Math.max(1, games.findIndex((game) => game.id === featured?.id) + 1);
  const railCounter = `${pad2(games.length ? recentIndex : 0)} / ${pad2(games.length)}`;
  detailEl.innerHTML = `<div class="page home-page">
    <section class="home-hero${hero ? "" : " no-artwork"}" id="home-hero"${hero ? ` style="background-image:url('${escape(hero)}')"` : ""}>
      <div class="home-hero-shade"></div>
      <div class="home-hero-content">
        <p class="eyebrow" id="home-eyebrow">Recently played</p>
        <h1 id="home-title">${escape(featured?.title || "Your next game")}</h1>
        <p class="home-meta" id="home-related">${escape(appId ? `Steam App ${appId}` : "Managed entry")}${featured && managedReady(featured) ? " · Ready to play" : ""}</p>
        <p class="home-description hidden" id="home-description"></p>
        <div class="home-actions"><button class="primary" id="home-play"${featured ? "" : " disabled"}>${running ? "Stop" : "Play"}</button><button id="home-view"${featured ? "" : " disabled"}>View game</button></div>
      </div>
    </section>
    <section class="managed-rail" data-rail="recent">
      <div class="rail-head"><h2>Recently played</h2><span class="muted" id="home-counter">${railCounter}</span></div>
      <div class="rail-scroll">${games.map((game) => coverCard(game, "recent")).join("")}</div>
    </section>
    <section class="managed-rail" data-rail="setup">
      <div class="rail-head"><h2>Recently ready to play</h2></div>
      <p class="muted">Your ready games, newest completed setups first.</p>
      ${installed.length ? `<div class="rail-scroll">${installed.map((game) => coverCard(game, "setup")).join("")}</div>` : '<div class="status">No installed games yet.</div>'}
    </section>
  </div>`;
  const play = detailEl.querySelector("#home-play");
  if (play && featured) {
    play.onclick = () => running
      ? stopManagedGame(featured, play)
      : startManagedGame(featured, play);
  }
  const view = detailEl.querySelector("#home-view");
  if (view && featured) view.onclick = () => openManagedGame(featured.id);
  if (featured) void applyHeroDetails(featured);
  void hydrateCovers(detailEl);
  void hydrateHero(featured);
}

function renderManagedMode() {
  if (!hasManagedLibrary()) return;
  document.body.classList.add("managed");
  if (!managedLibrary.games.some((game) => game.id === managedSelectedId)) {
    managedSelectedId = managedLibrary.games[0]?.id || null;
  }
  if (appView === "home") {
    renderManagedHome();
    return;
  }
  if (appView === "game") {
    const game = managedLibrary.games.find((item) => item.id === managedSelectedId);
    if (game) {
      renderManagedPreview(game);
      return;
    }
  }
  renderManagedLibrary();
}

function openManagedGame(id, origin = "library") {
  managedSelectedId = id;
  gameDetailOrigin = origin;
  appView = "game";
  renderManagedMode();
}

function setView(view) {
  const returningToStore = view === "store" && appView === "store-game";
  appView = view;
  if (returningToStore) restoreStoreScroll = true;
  document.body.classList.toggle("store-open", view === "store");
  const navView = view === "game" ? "library" : view === "store-game" ? "store" : view;
  for (const name of ["home", "library", "store"]) {
    const button = document.querySelector(`#nav-${name}`);
    button.classList.toggle("active", navView === name);
    button.setAttribute("aria-current", navView === name ? "page" : "false");
  }
  if (view === "store") {
    if (!storeTried && !storeLoading) void openStore();
    else renderStorePage();
    return;
  }
  if (hasManagedLibrary()) {
    renderManagedMode();
  } else if (view === "library" || view === "game") {
    renderGames();
    const game = games.find((item) => item.appid === selected);
    if (game) renderDetail(game, detailTab);
    else detailEl.innerHTML = `<div class="empty">No Steam games found.</div>`;
  } else {
    detailEl.innerHTML = `<div class="empty">Add a game to build your library.</div>`;
  }
}

gamesEl.addEventListener("click", (event) => {
  const button = event.target.closest("[data-managed]");
  if (!button) return;
  openManagedGame(button.dataset.managed);
});

detailEl.addEventListener("click", (event) => {
  const button = event.target.closest("[data-managed]");
  if (!button) return;
  openManagedGame(button.dataset.managed);
});



// Keyboard and controller navigation, mirroring UmuTron's focus model:
// arrow keys move to the nearest control in that direction, Enter activates,
// Escape goes back, PageUp/PageDown page collections, and a gamepad maps to
// the same actions with the same debounce/repeat timing.
const navState = {
  focused: null,
  held: new Set(),
  nextRepeat: new Map(),
};

const NAV_ACTIONS = {
  ArrowLeft: "left",
  ArrowRight: "right",
  ArrowUp: "up",
  ArrowDown: "down",
  Enter: "select",
  " ": "select",
  Escape: "back",
  PageUp: "pageup",
  PageDown: "pagedown",
};

const GAMEPAD_ACTIONS = new Map([
  [0, "select"],
  [1, "back"],
  [2, "play"],
  [6, "desktop"],
  [9, "pageup"],
  [10, "pagedown"],
  [11, "up"],
  [12, "down"],
  [13, "left"],
  [14, "right"],
]);

function headerControls() {
  return [...document.querySelectorAll(".topbar .nav-item, .topbar .top-actions button")]
    .filter((node) => !node.disabled && node.getClientRects().length > 0);
}

function activeNavControl() {
  const view = appView === "game" ? "library" : appView === "store-game" ? "store" : appView;
  return document.querySelector(`.topbar #nav-${view}`) || document.querySelector(".topbar #nav-home");
}

function enterContentFromHeader() {
  if (appView === "home") {
    const card = document.querySelector(`.home-page .cover-card[data-managed="${CSS.escape(managedSelectedId || "")}"]`)
      || document.querySelector(".home-page .cover-card");
    if (card) { focusControl(card); return true; }
  }
  const search = document.querySelector("#store-search");
  if (search) { focusControl(search); return true; }
  const play = document.querySelector("#managed-play, #managed-stop");
  if (play) { focusControl(play); return true; }
  const card = document.querySelector(".library-page .cover-card");
  if (card) { focusControl(card); return true; }
  return false;
}

function navigationSurface() {
  const settings = document.querySelector("#settings-page");
  if (settings && !settings.classList.contains("hidden")) return settings;
  return document.querySelector("#detail");
}

function navigationControls() {
  const settings = document.querySelector("#settings-page");
  const modal = document.querySelector(".modal-backdrop");
  const dialog = document.querySelector("#profile-dialog");
  const root = dialog && !dialog.classList.contains("hidden")
    ? dialog
    : settings && !settings.classList.contains("hidden")
      ? settings
      : modal || document;
  return [...root.querySelectorAll("button, input, select, textarea, a[href], [tabindex='0']")]
    .filter((node) => !node.disabled
      && node.getClientRects().length > 0
      && !node.closest(".hidden")
      && !node.closest("#auth-gate"));
}

function scrollControlIntoView(node) {
  const margin = 28;
  // Horizontal rails scroll themselves; the page must not move sideways.
  let horizontal = node.parentElement;
  while (horizontal && horizontal !== document.body && horizontal !== document.documentElement) {
    const style = getComputedStyle(horizontal);
    if (/(auto|scroll|hidden)/.test(style.overflowX) && horizontal.scrollWidth > horizontal.clientWidth + 1) break;
    horizontal = horizontal.parentElement;
  }
  if (horizontal && horizontal !== document.body && horizontal !== document.documentElement) {
    const box = horizontal.getBoundingClientRect();
    const rect = node.getBoundingClientRect();
    if (horizontal.classList.contains("rail-scroll")) {
      // UmuTron keeps the focused card anchored at the rail start and slides
      // the row underneath it, so the outline stays put while cards move.
      // The adjustment is applied immediately; only the outline starts
      // travelling right once the rail can no longer scroll.
      horizontal.scrollBy({ left: Math.round(rect.left - box.left), behavior: "auto" });
    } else {
      let delta = 0;
      if (rect.left < box.left + margin) delta = rect.left - box.left - margin;
      else if (rect.right > box.right - margin) delta = rect.right - box.right + margin;
      if (delta) horizontal.scrollBy({ left: Math.round(delta), behavior: "smooth" });
    }
  }
  // Vertical reveal inside the nearest vertical scroller (settings) or the page.
  let vertical = node.parentElement;
  while (vertical && vertical !== document.body && vertical !== document.documentElement) {
    const style = getComputedStyle(vertical);
    if (/(auto|scroll)/.test(style.overflowY) && vertical.scrollHeight > vertical.clientHeight + 1) break;
    vertical = vertical.parentElement;
  }
  const header = document.querySelector(".topbar")?.getBoundingClientRect().height || 0;
  const rect = node.getBoundingClientRect();
  if (vertical && vertical !== document.body && vertical !== document.documentElement) {
    const box = vertical.getBoundingClientRect();
    let delta = 0;
    if (rect.top < box.top + 16) delta = rect.top - box.top - 16;
    else if (rect.bottom > box.bottom - 16) delta = rect.bottom - box.bottom + 16;
    if (delta) vertical.scrollBy({ top: Math.round(delta), behavior: "smooth" });
  } else if (rect.top < header + 16) {
    window.scrollBy({ top: rect.top - header - 16, behavior: "smooth" });
  } else if (rect.bottom > window.innerHeight - 16) {
    window.scrollBy({ top: rect.bottom - (window.innerHeight - 16), behavior: "smooth" });
  }
}

function focusStoreContent() {
  const card = document.querySelector("#store-results .cover-card")
    || document.querySelector(".store-content .cover-card")
    || document.querySelector(".store-content button");
  if (card) { focusControl(card); return true; }
  return false;
}

function focusStoreSidebar() {
  const candidate = navState.lastSidebar;
  const target = candidate && document.contains(candidate)
    ? candidate
    : document.querySelector(".store-sidebar input, .store-sidebar .store-option, .store-sidebar .store-clear-link");
  if (target) { focusControl(target); return true; }
  return false;
}

function focusControl(node) {
  if (!node) return;
  if (navState.focused && navState.focused !== node) navState.focused.classList.remove("nav-focus");
  navState.focused = node;
  node.classList.add("nav-focus");
  node.focus({ preventScroll: true });
  scrollControlIntoView(node);
  if (node.closest(".store-sidebar")) navState.lastSidebar = node;
  if (node.matches?.(".cover-card[data-managed]")) {
    if (node.closest(".home-page")) {
      syncHomeHero(node);
    } else if (node.closest(".library-page")) {
      // Selection follows focus: the previous card loses its outline.
      managedSelectedId = node.dataset.managed;
      for (const card of document.querySelectorAll(".library-page .cover-card")) {
        card.classList.toggle("active", card.dataset.managed === managedSelectedId);
      }
    }
  }
}

const appDetailsCache = new Map();

async function appDetails(appid) {
  if (!appid) return null;
  if (appDetailsCache.has(appid)) return appDetailsCache.get(appid);
  const details = await invoke("app_details", { appid });
  appDetailsCache.set(appid, details);
  return details;
}

async function applyHeroDetails(game) {
  const related = document.querySelector("#home-related");
  const description = document.querySelector("#home-description");
  if (!related || !description || !game) return;
  const appId = game.metadata_app_id;
  const fallback = `${appId ? `Steam App ${appId}` : "Managed entry"}${managedReady(game) ? " · Ready to play" : ""}`;
  related.textContent = fallback;
  description.textContent = "";
  description.classList.add("hidden");
  if (!appId) return;
  try {
    const details = await appDetails(appId);
    if (managedSelectedId !== game.id) return;
    const meta = [details.release_date, details.genres].filter(Boolean).join(" · ");
    if (meta) related.textContent = meta;
    if (details.description) {
      description.textContent = details.description;
      description.classList.remove("hidden");
    }
  } catch (_error) {
    // The fallback line stays; store metadata is optional.
  }
}

function setHomeSelection(section, id) {
  homeFocusSection = section;
  managedSelectedId = id;
  for (const card of document.querySelectorAll(".home-page .cover-card")) {
    const railSection = card.closest(".managed-rail")?.dataset.rail;
    card.classList.toggle("active", card.dataset.managed === id && railSection === section);
  }
}

function syncHomeHero(card) {
  if (!managedLibrary) return;
  const game = managedLibrary.games.find((item) => item.id === card.dataset.managed);
  if (!game) return;
  const rail = card.closest(".managed-rail");
  const railKind = rail?.dataset.rail || "recent";
  const cards = rail ? [...rail.querySelectorAll(".cover-card")] : [];
  const index = Math.max(1, cards.indexOf(card) + 1);
  const total = cards.length;
  setHomeSelection(railKind, game.id);
  const appId = game.metadata_app_id;
  const heroNode = document.querySelector("#home-hero");
  if (heroNode) {
    const hero = appId ? heroUrl(appId) : "";
    heroNode.style.backgroundImage = hero ? `url('${hero}')` : "";
    heroNode.classList.toggle("no-artwork", !hero);
  }
  const eyebrow = document.querySelector("#home-eyebrow");
  if (eyebrow) eyebrow.textContent = railKind === "setup" ? "Recently ready to play" : "Recently played";
  const title = document.querySelector("#home-title");
  if (title) title.textContent = game.title;
  void applyHeroDetails(game);
  const counter = document.querySelector("#home-counter");
  if (counter) counter.textContent = railKind === "setup" ? `${total} ready games` : `${pad2(index)} / ${pad2(total)}`;
  const play = document.querySelector("#home-play");
  const running = runningManaged.has(game.id);
  if (play) {
    play.textContent = running ? "Stop" : "Play";
    play.disabled = false;
    play.onclick = () => running ? stopManagedGame(game, play) : startManagedGame(game, play);
  }
  const view = document.querySelector("#home-view");
  if (view) {
    view.disabled = false;
    view.onclick = () => openManagedGame(game.id);
  }
}

function moveNavigation(action) {
  const controls = navigationControls();
  if (!controls.length) return false;
  const active = document.activeElement;
  const current = controls.includes(active)
    ? active
    : navState.focused && controls.includes(navState.focused)
      ? navState.focused
      : null;
  if (!current) {
    focusControl(controls[0]);
    return true;
  }
  const header = headerControls();
  if (header.includes(current)) {
    if (action === "left" || action === "right") {
      const index = header.indexOf(current);
      const next = Math.max(0, Math.min(header.length - 1, index + (action === "right" ? 1 : -1)));
      focusControl(header[next]);
      return true;
    }
    if (action === "down") return enterContentFromHeader();
    if (action === "up") return true;
  }
  if (action === "next" || action === "previous") {
    const index = controls.indexOf(current);
    const step = action === "next" ? 1 : -1;
    focusControl(controls[(index + step + controls.length) % controls.length]);
    return true;
  }
  // UmuTron's home rail wraps with modulo; the scroll clamps at the end and
  // focus moves across the last visible cards before looping to the first.
  if ((action === "left" || action === "right") && current.matches?.(".cover-card[data-managed]")) {
    const rail = current.closest(".rail-scroll");
    if (rail && rail.closest(".home-page") && rail.closest(".managed-rail")?.dataset.rail === "recent") {
      const cards = [...rail.querySelectorAll(".cover-card")];
      const index = cards.indexOf(current);
      if (index >= 0) {
        if (action === "right" && index === cards.length - 1) {
          focusControl(cards[0]);
          return true;
        }
        if (action === "left" && index === 0) {
          focusControl(cards[cards.length - 1]);
          return true;
        }
      }
    }
  }
  const center = (node) => {
    const rect = node.getBoundingClientRect();
    return [rect.left + rect.width / 2, rect.top + rect.height / 2];
  };
  const regionOf = (node) => node.closest(".store-content")
    ? "content"
    : node.closest(".store-sidebar")
      ? "sidebar"
      : node.closest(".topbar")
        ? "header"
        : node.closest("#settings-page")
          ? "settings"
          : node.closest(".modal-backdrop")
            ? "modal"
            : "page";
  const [x, y] = center(current);
  const bestIn = (nodes) => {
    let best = null;
    for (const node of nodes) {
      if (node === current) continue;
      const [cx, cy] = center(node);
      const dx = cx - x;
      const dy = cy - y;
      const along = action === "right" ? dx : action === "left" ? -dx : action === "down" ? dy : -dy;
      const across = action === "left" || action === "right" ? Math.abs(dy) : Math.abs(dx);
      if (along <= 1) continue;
      const score = along + across * 3;
      if (!best || score < best.score) best = { score, node };
    }
    return best;
  };
  // Keep focus inside the current region first (store sidebar vs catalogue),
  // so pressing Down at the end of the grid reaches the pager instead of the
  // sidebar. Cross-region moves only happen when the region has no candidate.
  const region = regionOf(current);
  const regional = controls.filter((node) => regionOf(node) === region);
  const best = bestIn(regional) || bestIn(controls);
  if (best) {
    focusControl(best.node);
    return true;
  }
  if (action === "right" && current.closest(".store-sidebar")) {
    return focusStoreContent();
  }
  if (action === "left" && current.closest(".store-content")) {
    return focusStoreSidebar();
  }
  if (action === "up") {
    const nav = activeNavControl();
    if (nav) { focusControl(nav); return true; }
  }
  return false;
}

function focusedManagedGame() {
  const card = document.activeElement?.closest?.("[data-managed]") || navState.focused?.closest?.("[data-managed]");
  if (!card || !managedLibrary) return null;
  return managedLibrary.games.find((game) => game.id === card.dataset.managed) || null;
}

function dispatchNavigation(action, fromGamepad = false) {
  const active = document.activeElement;
  const tag = active?.tagName;
  const typing = active && ["INPUT", "TEXTAREA", "SELECT"].includes(tag);
  if (action === "back") {
    if (typing) {
      document.activeElement.blur();
      if (!document.body.classList.contains("settings-open")) {
        const nav = activeNavControl();
        if (nav) focusControl(nav);
      }
      return;
    }
    if (profileDialogOpen()) { closeProfileDialog(); return; }
    if (document.body.classList.contains("settings-open")) { closeSettingsPage(); return; }
    const modal = document.querySelector(".modal-backdrop");
    if (modal) { modal.remove(); return; }
    if (document.body.classList.contains("fullscreen-mode")) { openFullscreenOptions(); return; }
    if (appView === "store-game") { setView("store"); return; }
    if (appView === "game") { setView(gameDetailOrigin); return; }
    if (navState.focused) navState.focused.classList.remove("nav-focus");
    navState.focused = null;
    document.activeElement?.blur?.();
    return;
  }
  if (typing) {
    if (action === "select") return;
    if (!fromGamepad) {
      // Native keys stay inside selects, textareas, and horizontal caret moves.
      if (tag === "SELECT" || tag === "TEXTAREA") return;
      if (action === "left" || action === "right") return;
    }
  }
  if (action === "select") {
    const control = document.activeElement;
    if (control && typeof control.click === "function" && !control.disabled) control.click();
    else moveNavigation("next");
    return;
  }
  if (action === "play") {
    const game = focusedManagedGame();
    if (game) { void startManagedGame(game, null); return; }
    const play = document.querySelector("#managed-play, #home-play");
    if (play && !play.disabled) play.click();
    return;
  }
  if (action === "pageup" || action === "pagedown") {
    if (appView === "store") {
      const next = action === "pagedown" ? storePage + 1 : Math.max(1, storePage - 1);
      if (next !== storePage) void ensureStorePage(next);
      return;
    }
    if (hasManagedLibrary() && appView === "library") {
      const pages = libraryPages();
      libraryPage = action === "pagedown"
        ? Math.min(pages, libraryPage + 1)
        : Math.max(1, libraryPage - 1);
      renderManagedLibrary();
    } else {
      window.scrollBy({ top: (action === "pagedown" ? 1 : -1) * window.innerHeight * 0.9 });
    }
    return;
  }
  moveNavigation(action);
}

function sampleGamepad(pressed, now) {
  const directions = new Set(["left", "right", "up", "down"]);
  const actions = [];
  for (const action of [...pressed].sort()) {
    if (!navState.held.has(action)) {
      actions.push(action);
      navState.nextRepeat.set(action, now + 400);
    } else if (directions.has(action) && now >= (navState.nextRepeat.get(action) || now)) {
      actions.push(action);
      navState.nextRepeat.set(action, now + 160);
    }
  }
  navState.held = new Set(pressed);
  for (const action of [...navState.nextRepeat.keys()]) {
    if (!pressed.has(action)) navState.nextRepeat.delete(action);
  }
  return actions;
}

function pollGamepad(now) {
  requestAnimationFrame(pollGamepad);
void refreshFullscreenState();
void applyUiScale(uiScale, false);
  if (!document.hasFocus()) {
    navState.held = new Set();
    navState.nextRepeat.clear();
    return;
  }
  const gate = document.querySelector("#auth-gate");
  if (gate && !gate.classList.contains("hidden")) {
    navState.held = new Set();
    navState.nextRepeat.clear();
    return;
  }
  const pads = navigator.getGamepads ? [...navigator.getGamepads()] : [];
  const pressed = new Set();
  for (const pad of pads) {
    if (!pad || !pad.connected) continue;
    for (const [index, action] of GAMEPAD_ACTIONS) {
      if (pad.buttons[index]?.pressed) pressed.add(action);
    }
    const [ax = 0, ay = 0] = pad.axes || [];
    if (Math.abs(ax) > 0.5 || Math.abs(ay) > 0.5) {
      pressed.add(Math.abs(ax) >= Math.abs(ay)
        ? (ax > 0 ? "right" : "left")
        : (ay > 0 ? "down" : "up"));
    }
  }
  for (const action of sampleGamepad(pressed, now)) dispatchNavigation(action, true);
}

window.addEventListener("keydown", (event) => {
  if (event.ctrlKey || event.metaKey || event.altKey) return;
  if (event.key === "F11") { event.preventDefault(); void toggleFullscreenMode(); return; }
  const action = NAV_ACTIONS[event.key];
  if (!action) return;
  const gate = document.querySelector("#auth-gate");
  if (gate && !gate.classList.contains("hidden")) return;
  const typing = event.target && ["INPUT", "TEXTAREA", "SELECT"].includes(event.target.tagName);
  if (typing) {
    if (action === "select" || event.key === " ") return;
    if (event.target.tagName === "SELECT") return;
    if (action === "left" || action === "right") {
      const caret = event.target.selectionStart;
      if (action === "right" && (caret === null || caret >= event.target.value.length)) {
        event.preventDefault();
        focusStoreContent();
        return;
      }
      if (action === "left" && caret === 0) {
        event.preventDefault();
        focusStoreSidebar();
        return;
      }
      return;
    }
  }
  event.preventDefault();
  dispatchNavigation(action);
});

requestAnimationFrame(pollGamepad);

const profileDialog = document.querySelector("#profile-dialog");

function profileDialogOpen() {
  return Boolean(profileDialog && !profileDialog.classList.contains("hidden"));
}

function closeProfileDialog() {
  if (!profileDialog) return;
  profileDialog.classList.add("hidden");
  if (navState.focused && profileDialog.contains(navState.focused)) {
    navState.focused.classList.remove("nav-focus");
    navState.focused = null;
  }
}

async function openProfileDialog() {
  let profile = null;
  try { profile = await invoke("profile_status"); } catch (_error) { profile = null; }
  document.querySelector("#profile-email").textContent =
    profile?.email || (profile?.authenticated ? "Signed in" : "Signed out");
  document.querySelector("#profile-base").textContent = profile?.base_url || "";
  profileDialog.classList.remove("hidden");
  focusControl(document.querySelector("#profile-logout"));
}

document.querySelector("#profile").onclick = () => { void openProfileDialog(); };
document.querySelector("#profile-close").onclick = closeProfileDialog;
document.querySelector("#profile-logout").onclick = async () => {
  try {
    await invoke("profile_logout");
    closeProfileDialog();
    showAuthGate("Signed out. Sign in to continue.");
  } catch (error) {
    showToast(String(error), true);
  }
};
profileDialog.addEventListener("click", (event) => {
  if (event.target === profileDialog) closeProfileDialog();
});

function applyFullscreenState(fullscreen) {
  document.body.classList.toggle("fullscreen-mode", fullscreen);
  const button = document.querySelector("#window-mode");
  if (button) {
    const label = fullscreen ? "Exit Fullscreen mode (F11)" : "Switch to Fullscreen mode (F11)";
    button.title = label;
    button.setAttribute("aria-label", label);
  }
}

async function refreshFullscreenState() {
  try { applyFullscreenState(await invoke("fullscreen_state")); }
  catch (_error) { applyFullscreenState(false); }
}

async function toggleFullscreenMode() {
  try { applyFullscreenState(await invoke("toggle_fullscreen")); }
  catch (error) { showToast(String(error), true); }
}

let uiScale = (() => {
  const stored = parseFloat(localStorage.getItem("steamtron.uiScale.v1"));
  return Number.isFinite(stored) && stored > 0 ? stored : 1;
})();

function uiScaleLabel() {
  return Math.abs(uiScale - 1) < 0.001 ? "Auto" : `${Math.round(uiScale * 100)}%`;
}

async function applyUiScale(scale, persist = true) {
  uiScale = Math.min(1.5, Math.max(0.75, Math.round(scale * 100) / 100));
  try { await invoke("set_ui_scale", { scale: uiScale }); } catch (_error) { /* keep the label */ }
  if (persist) {
    try { localStorage.setItem("steamtron.uiScale.v1", String(uiScale)); } catch (_error) { /* ignore */ }
  }
  const label = document.querySelector("#fullscreen-scale-label");
  if (label) label.textContent = `UI Scale: ${uiScaleLabel()}`;
}

function openFullscreenOptions() {
  const backdrop = document.createElement("div");
  backdrop.className = "modal-backdrop";
  backdrop.innerHTML = `<div class="modal" style="max-width:380px">
    <div class="modal-head"><h2>Fullscreen options</h2><button class="modal-close" aria-label="Close">×</button></div>
    <div class="fullscreen-options">
      <button id="fullscreen-reboot">Reboot</button>
      <button id="fullscreen-shutdown">Shutdown</button>
      <button id="fullscreen-exit">Exit Fullscreen</button>
      <button id="fullscreen-quit">Quit SteamTron</button>
      <div class="fullscreen-scale">
        <button id="fullscreen-scale-down" class="scale-button" aria-label="Decrease UI scale">−</button>
        <span id="fullscreen-scale-label">UI Scale: ${uiScaleLabel()}</span>
        <button id="fullscreen-scale-up" class="scale-button" aria-label="Increase UI scale">+</button>
      </div>
    </div>
  </div>`;
  document.body.appendChild(backdrop);
  const close = () => backdrop.remove();
  backdrop.querySelector(".modal-close").onclick = close;
  backdrop.addEventListener("click", (event) => { if (event.target === backdrop) close(); });
  backdrop.querySelector("#fullscreen-reboot").onclick = async () => {
    if (!window.confirm("Reboot the system? Save any work in other applications first.")) return;
    try { await invoke("reboot_system"); } catch (error) { showToast(String(error), true); }
  };
  backdrop.querySelector("#fullscreen-shutdown").onclick = async () => {
    if (!window.confirm("Shut down the system? Save any work in other applications first.")) return;
    try { await invoke("shutdown_system"); } catch (error) { showToast(String(error), true); }
  };
  backdrop.querySelector("#fullscreen-exit").onclick = () => {
    close();
    void toggleFullscreenMode();
  };
  backdrop.querySelector("#fullscreen-quit").onclick = () => {
    void invoke("quit_app").catch(() => window.close());
  };
  backdrop.querySelector("#fullscreen-scale-down").onclick = () => { void applyUiScale(uiScale - 0.05); };
  backdrop.querySelector("#fullscreen-scale-up").onclick = () => { void applyUiScale(uiScale + 0.05); };
  focusControl(backdrop.querySelector("#fullscreen-exit"));
}

document.querySelector("#window-mode").onclick = () => { void toggleFullscreenMode(); };

async function boot() {
  try {
    const auth = await invoke("profile_status");
    if (!auth.authenticated) {
      showAuthGate("Opening your browser to sign in…");
      void runSignIn();
      return;
    }
    hideAuthGate();
    await load();
    await checkMigration();
  } catch (error) {
    showAuthGate(`Could not check sign-in: ${error}`);
  }
}

const gateRetry = document.querySelector("#gate-retry");
const gateQuit = document.querySelector("#gate-quit");
const gateSpinner = document.querySelector("#gate-spinner");
let signInRunning = false;

async function runSignIn() {
  if (signInRunning) return;
  signInRunning = true;
  gateSpinner.classList.remove("hidden");
  gateRetry.disabled = true;
  gateStatus.textContent = "Opening your browser to sign in…";
  try {
    const auth = await invoke("profile_login");
    if (!auth.authenticated) {
      gateStatus.textContent = "Sign-in did not complete. Retry to open the browser again.";
      return;
    }
    hideAuthGate();
    await load();
    await checkMigration();
  } catch (error) {
    gateStatus.textContent = String(error);
  } finally {
    signInRunning = false;
    gateSpinner.classList.add("hidden");
    gateRetry.disabled = false;
  }
}

gateRetry.onclick = () => { void runSignIn(); };
gateQuit.onclick = () => {
  invoke("quit_app").catch(() => window.close());
};

let toastTimer = null;
function showToast(message, error = false) {
  if (error) {
    showErrorDialog(message);
    return;
  }
  const node = document.querySelector("#toast");
  node.textContent = message;
  node.classList.remove("error");
  node.classList.add("show");
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => node.classList.remove("show"), 2600);
}

function showLibraryLoading() {
  gamesEl.className = "games list";
  gamesEl.innerHTML = `<div class="loading-list">${Array.from({ length: 6 }, () => '<div class="loading-row"><span class="loading-cover"></span><span class="loading-lines"><i></i><i></i><i></i></span></div>').join("")}</div>`;
  detailEl.innerHTML = `<div class="empty">Loading Steam library…</div>`;
}

async function loadArtwork() {
  if (artworkLoading) return;
  const appids = games.filter((game) => !game.artworkLoaded).map((game) => game.appid);
  if (!appids.length) return;
  artworkLoading = true;
  try {
    const artwork = await invoke("game_artwork", { appids });
    for (const game of games) {
      const art = artwork[game.appid];
      game.artworkLoaded = true;
      if (!art) continue;
      if (art.cover) {
        game.cover = art.cover;
        updateCardCover(game);
      }
      if (art.hero) {
        game.hero = art.hero;
        if (selected === game.appid) updateDetailHero(game);
      }
    }
  } catch (_error) {
    // Standard Steam CDN URLs and the placeholder cover remain usable.
  } finally {
    artworkLoading = false;
  }
}

function updateCardCover(game) {
  const image = gamesEl.querySelector(`.game-card[data-appid="${game.appid}"] img`);
  if (!image) return;
  image.classList.remove("missing");
  delete image.dataset.fallback;
  image.src = game.cover || coverUrl(game.appid);
}

function updateDetailHero(game) {
  const hero = detailEl.querySelector(".detail-hero");
  if (!hero) return;
  const source = game.hero || game.cover;
  if (!source) return;
  hero.classList.remove("no-artwork");
  hero.style.backgroundImage = `url(${JSON.stringify(source)})`;
}

function highlightSelected() {
  for (const card of gamesEl.querySelectorAll(".game-card")) {
    card.classList.toggle("active", card.dataset.appid === selected);
  }
}

function renderGames() {
  const query = searchEl.value.trim().toLowerCase();
  gamesEl.className = "games list";
  gamesEl.replaceChildren();
  for (const game of games.filter((item) => !query || `${item.name} ${item.appid}`.toLowerCase().includes(query))) {
    const button = document.createElement("button");
    button.className = `game game-card${selected === game.appid ? " active" : ""}`;
    button.dataset.appid = game.appid;
    button.title = `${game.name} · App ${game.appid}`;
    const build = game.build_id && game.build_id !== "0"
      ? `build ${game.build_id}`
      : game.version ? `latest ${game.version}` : "version unknown";
    const fix = fixes.get(game.appid);
    const badges = [
      !game.installed ? '<span class="badge warn">Not installed</span>' : "",
      game.managed ? '<span class="badge">Managed</span>' : "",
      game.pinned ? '<span class="badge pin">Pinned</span>' : "",
      fix?.enabled ? '<span class="badge ok">Fixes</span>' : "",
    ].filter(Boolean).join("");
    button.innerHTML = `
      <span class="cover" aria-hidden="true">
        <img loading="lazy" src="${escape(game.cover || coverUrl(game.appid))}" alt="">
      </span>
      <span class="game-meta">
        <strong>${escape(game.name)}</strong>
        <span>${escape(game.appid)} · ${escape(build)}</span>
        <span class="badges">${badges}</span>
      </span>`;
    const image = button.querySelector("img");
    bindCoverImage(image, game.appid);
    gamesEl.appendChild(button);
  }
  if (!gamesEl.children.length) {
    const empty = document.createElement("div");
    empty.className = "empty library-empty";
    empty.textContent = games.length ? "No games match your search." : "No Steam games found.";
    gamesEl.appendChild(empty);
  }
}

function selectGame(game) {
  if (!game) return;
  try {
    selected = game.appid;
    highlightSelected();
    renderDetail(game);
  } catch (error) {
    console.error("Could not open game", error);
    showToast(`Could not open ${game.name}: ${error}`, true);
  }
}

gamesEl.addEventListener("click", (event) => {
  const button = event.target.closest(".game-card");
  if (!button || button.dataset.managed) return;
  selectGame(games.find((game) => game.appid === button.dataset.appid));
});

function renderDetail(game, tab = "general") {
  detailTab = tab;
  const current = fixes.get(game.appid);
  const tabs = [["general", "General"]];
  // Launch targets are independent of fixes: a game may need a specific
  // executable/arguments even when the fixes overlay is not involved.
  if (game.installed) tabs.push(["launch", "Launch options"]);
  if (game.installed) tabs.push(["fixes", "Fixes"]);
  if (game.managed) tabs.push(["updates", "Updates"]);
  const active = tabs.some(([key]) => key === tab) ? tab : "general";
  const hero = game.hero || game.cover || "";
  detailEl.innerHTML = `<div class="detail-hero${hero ? "" : " no-artwork"}"${hero ? ` style="background-image:url('${escape(hero)}')"` : ""}><div class="detail-hero-shade"></div><div class="detail-hero-content"><h2>${escape(game.name)}</h2><p class="muted">Steam App ${escape(game.appid)}</p><div class="quicklooks">${quicklookMarkup(game)}</div><div class="detail-hero-actions"><button id="hero-play" class="primary">Play</button></div></div></div><div class="game-tabs">${tabs.map(([key, label]) => `<button class="game-tab${key === active ? " active" : ""}" data-tab="${key}">${label}</button>`).join("")}</div><div class="game-panel"></div>`;
  const panel = detailEl.querySelector(".game-panel");
  for (const button of detailEl.querySelectorAll(".game-tab")) button.onclick = () => renderDetail(game, button.dataset.tab);
  setupHeroPlay(game);
  if (active === "updates" && game.managed) { renderUpdatesPanel(panel, game); return; }
  if (active === "launch") { renderLaunchOptionsPanel(panel, game, current); return; }
  if (active === "fixes") { renderFixesPanel(panel, game, current); return; }
  renderGeneralPanel(panel, game);
}

function parseLaunchArguments(value) {
  const result = [];
  const regex = /"([^"]*)"|'([^']*)'|(\S+)/g;
  let match;
  while ((match = regex.exec(value || ""))) {
    const part = match[1] ?? match[2] ?? match[3];
    if (part) result.push(part);
  }
  return result;
}

function targetLabel(target) {
  const name = (target?.name || "").trim();
  if (name) return name;
  const path = target?.executable || "";
  return path.split(/[\\/]/).pop() || "Launcher";
}

function showLaunchChooser(game, targets) {
  if (!targets.length) return Promise.resolve(null);
  if (targets.length === 1) return Promise.resolve(targets[0].id);
  return new Promise((resolve) => {
    const backdrop = document.createElement("div");
    backdrop.className = "modal-backdrop";
    backdrop.innerHTML = `<div class="modal"><div class="modal-head"><h2>Choose launcher</h2><button class="modal-close" aria-label="Close">×</button></div><p class="muted">More than one launch target is configured for ${escape(game.name)}.</p><div class="launch-choice-list"></div><div class="modal-actions"><button class="secondary" id="launch-choice-cancel">Cancel</button></div></div>`;
    document.body.appendChild(backdrop);
    const list = backdrop.querySelector(".launch-choice-list");
    for (const target of targets) {
      const button = document.createElement("button");
      button.className = "launch-choice";
      const args = target.arguments?.length ? ` · ${target.arguments.join(" ")}` : "";
      button.innerHTML = `<strong>${escape(targetLabel(target))}</strong><small>${escape(target.executable || "")}${escape(args)}</small>`;
      button.onclick = () => { backdrop.remove(); resolve(target.id); };
      list.appendChild(button);
    }
    const close = () => { backdrop.remove(); resolve(null); };
    backdrop.querySelector(".modal-close").onclick = close;
    backdrop.querySelector("#launch-choice-cancel").onclick = close;
    backdrop.addEventListener("click", (event) => { if (event.target === backdrop) close(); });
  });
}

async function playSelected(game) {
  const current = fixes.get(game.appid);
  const targets = current?.launch_targets || [];
  if (!targets.length) {
    if (!current?.enabled || current?.launch_only) {
      try {
        await invoke("play_steam_game", { appid: game.appid });
        status("Game started through Steam.");
        return true;
      } catch (error) { status(String(error), true); return false; }
    }
    try {
      // Fixes are enabled but no explicit target is configured. Steam will
      // use the managed launch-option wrapper and its original %command%.
      await invoke("play_game", { appid: game.appid });
      status("Game started through Steam with fixes.");
      return true;
    } catch (error) { status(String(error), true); return false; }
  }
  const targetId = await showLaunchChooser(game, targets);
  if (!targetId) return false;
  try {
    await invoke("play_game", { appid: game.appid, targetId });
    status("Game started through SteamTron.");
    return true;
  } catch (error) { status(String(error), true); return false; }
}

function showLaunchEditor(game, target, onSaved, executableOverride = "") {
  const editing = Boolean(target);
  const executablePath = target?.executable || executableOverride;
  const backdrop = document.createElement("div");
  backdrop.className = "modal-backdrop";
  backdrop.innerHTML = `<div class="modal launch-editor"><div class="modal-head"><h2>${editing ? "Edit launcher" : "Add launcher"}</h2><button class="modal-close" aria-label="Close">×</button></div>
    <label for="launch-name">Launcher name</label>
    <input id="launch-name" placeholder="Optional; defaults to the executable name" value="${escape(target?.name || "")}">
    <label>Executable</label>
    <div class="launch-editor-path">${escape(executablePath)}</div>
    <label for="launch-arguments">Launch arguments</label>
    <input id="launch-arguments" placeholder="Optional, for example -nolauncher" value="${escape((target?.arguments || []).join(" "))}">
    <div class="modal-status" id="launch-editor-status"></div>
    <div class="modal-actions"><button id="launch-cancel">Cancel</button><button class="primary" id="launch-save">Save</button></div>
  </div>`;
  document.body.appendChild(backdrop);
  const statusNode = backdrop.querySelector("#launch-editor-status");
  const setStatus = (message, error = false) => { statusNode.textContent = message; statusNode.classList.toggle("error", error); };
  const close = () => backdrop.remove();
  backdrop.querySelector(".modal-close").onclick = close;
  backdrop.querySelector("#launch-cancel").onclick = close;
  backdrop.addEventListener("click", (event) => { if (event.target === backdrop) close(); });
  backdrop.querySelector("#launch-save").onclick = async () => {
    if (!executablePath.trim()) { setStatus("Choose an executable.", true); return; }
    const name = backdrop.querySelector("#launch-name").value;
    const arguments = parseLaunchArguments(backdrop.querySelector("#launch-arguments").value);
    const button = backdrop.querySelector("#launch-save");
    const stop = busy(button, "Saving…");
    try {
      const result = editing
        ? await invoke("update_launch_target", { appid: game.appid, id: target.id, name, arguments })
        : await invoke("add_launch_target", { appid: game.appid, executable: executablePath, name, arguments });
      stop();
      close();
      onSaved(result);
    } catch (error) { stop(); setStatus(String(error), true); }
  };
}

function renderLaunchOptionsPanel(panel, game, current) {
  const targets = current?.launch_targets || [];
  const rows = targets.map((target) => {
    const args = target.arguments?.length ? target.arguments.join(" ") : "No arguments";
    return `<div class="launch-option-row" data-id="${escape(target.id)}">
      <div class="launch-option-copy">
        <strong>${escape(targetLabel(target))}${target.primary ? ' <span class="badge ok">Primary</span>' : ""}</strong>
        <small>${escape(target.executable)}</small>
        <small>${escape(args)}</small>
      </div>
      <div class="actions"><button data-edit="${escape(target.id)}">Edit</button><button class="danger" data-delete="${escape(target.id)}">Delete</button></div>
    </div>`;
  }).join("");
  const needsFixes = Boolean(current?.enabled);
  const note = current?.launch_only
    ? "This game has no fixes. Play launches it through Steam and its native launcher."
    : "Launches through the SteamTron overlay and existing Proton prefix.";
  panel.innerHTML = `<div class="fixes-section">
    <div class="row"><div class="label">Launch options</div><div class="value">${targets.length} configured</div></div>
    <p class="muted">Add one or more executables. Play asks which launcher to use when more than one is configured. Arguments such as <code>-nolauncher</code> are passed only to that launcher.</p>
    <div class="launch-options-list">${rows || '<div class="status">No launch options configured. Add the game executable, or a launcher executable you want to use.</div>'}</div>
    <div class="actions"><button class="primary" id="add-launch-option">Add launcher…</button></div>
    <div class="status" id="launch-options-status">${note}</div>
  </div>`;
  const statusNode = panel.querySelector("#launch-options-status");
  const setStatus = (message, error = false) => { statusNode.textContent = message; statusNode.classList.toggle("error", error); };
  const refresh = (result) => {
    fixes.set(game.appid, result);
    renderDetail(game, "launch");
  };
  const add = panel.querySelector("#add-launch-option");
  if (add) add.onclick = async () => {
    try {
      const path = await invoke("pick_launch_executable", { appid: game.appid });
      if (!path) return;
      showLaunchEditor(game, null, (result) => {
        refresh(result);
      }, path);
    } catch (error) { setStatus(String(error), true); }
  };
  for (const button of panel.querySelectorAll("[data-edit]")) {
    button.onclick = () => {
      const target = targets.find((item) => item.id === button.dataset.edit);
      if (target) showLaunchEditor(game, target, refresh);
    };
  }
  for (const button of panel.querySelectorAll("[data-delete]")) {
    button.onclick = async () => {
      if (!window.confirm("Delete this launch option?")) return;
      const stop = busy(button, "Deleting…");
      try {
        const result = await invoke("remove_launch_target", { appid: game.appid, id: button.dataset.delete });
        stop();
        refresh(result);
      } catch (error) { stop(); setStatus(String(error), true); }
    };
  }
}

function setupHeroPlay(game) {
  if (heroPlayTimer) { clearInterval(heroPlayTimer); heroPlayTimer = null; }
  const button = detailEl.querySelector("#hero-play");
  if (!button) return;
  let running = Boolean(playStates.get(game.appid)?.running);
  let pending = null;
  let transitionedAt = 0;

  const render = (isRunning, targets, current) => {
    playStates.set(game.appid, { running: isRunning });
    button.classList.toggle("danger", isRunning);
    if (isRunning) {
      button.textContent = "Stop";
      button.disabled = false;
      button.title = "Stop the game";
    } else if (!targets.length && (!current?.enabled || current?.launch_only)) {
      button.textContent = "Play";
      button.disabled = false;
      button.title = "Launch through Steam. No SteamTron fixes are required.";
    } else if (!targets.length) {
      button.textContent = "Play";
      button.disabled = false;
      button.title = current?.enabled
        ? "Launch through Steam using the configured fixes launch option."
        : "Launch through Steam.";
    } else {
      button.textContent = "Play";
      button.disabled = false;
      button.title = targets.length > 1 ? "Choose a launcher and start through SteamTron." : "Launch with the fixes overlay and existing Proton prefix.";
    }
  };

  const update = async () => {
    if (!button.isConnected || pending) return;
    const current = fixes.get(game.appid);
    const targets = current?.launch_targets || [];
    let actual = false;
    try { actual = await invoke("game_running", { appid: game.appid }); } catch (_error) { return; }
    if (!button.isConnected || pending) return;
    if (Date.now() - transitionedAt < 1200) return;
    running = Boolean(actual);
    render(running, targets, current);
  };

  button.onclick = async () => {
    if (pending) return;
    if (running) {
      // Stop immediately in the UI; reconcile with the backend afterwards.
      running = false;
      transitionedAt = Date.now();
      pending = "stopping";
      render(false, fixes.get(game.appid)?.launch_targets || [], fixes.get(game.appid));
      button.disabled = true;
      try {
        await invoke("stop_game", { appid: game.appid });
        status("Game stopped.");
      } catch (error) {
        status(String(error), true);
      } finally {
        pending = null;
        transitionedAt = Date.now();
        render(false, fixes.get(game.appid)?.launch_targets || [], fixes.get(game.appid));
      }
      return;
    }

    // Steam must be running before a SteamTron/Steam launch. Do not start it
    // silently; report the requirement and let the user start Steam.
    transitionedAt = Date.now();
    pending = "starting";
    const current = fixes.get(game.appid);
    button.textContent = "Checking Steam…";
    button.disabled = true;
    try {
      await invoke("ensure_steam_running");
    } catch (error) {
      pending = null;
      running = false;
      render(false, current?.launch_targets || [], current);
      status(String(error), true);
      return;
    }
    render(true, current?.launch_targets || [], current);
    button.disabled = true;
    const started = await playSelected(game);
    pending = null;
    transitionedAt = Date.now();
    if (started) {
      running = true;
      render(true, fixes.get(game.appid)?.launch_targets || [], fixes.get(game.appid));
    } else {
      running = false;
      render(false, fixes.get(game.appid)?.launch_targets || [], fixes.get(game.appid));
      button.disabled = false;
    }
  };

  render(running, fixes.get(game.appid)?.launch_targets || [], fixes.get(game.appid));
  update();
  heroPlayTimer = setInterval(update, 2000);
}

function renderGeneralPanel(panel, game) {
  panel.innerHTML = `
    <div class="row"><div class="label">Game directory</div><div class="value">${escape(game.common)}</div></div>
    <div class="row"><div class="label">Library status</div><div class="value">${game.managed ? "Managed through SteamTron" : "Managed by Steam"} · ${game.installed ? "content installed" : "content not installed"}</div></div>
    <div class="actions">
      <button class="danger" id="remove-library" ${game.managed ? "" : "disabled"}>Remove from library</button>
    </div>
    <div class="status">${game.managed ? "Removing this entry deletes the Lua, appmanifest, manifests, and managed symlink. Game content is left intact." : "This game is not managed by SteamTron."}</div>`;
  const removeLibrary = panel.querySelector("#remove-library");
  if (removeLibrary && game.managed) removeLibrary.onclick = async () => {
    if (!window.confirm(`Remove ${game.name} from the Steam library?`)) return;
    const stop = busy(removeLibrary, "Removing…");
    try {
      const result = await invoke("remove_game", { appid: game.appid });
      selected = null;
      detailEl.innerHTML = `<div class="empty">Select a Steam game.</div>`;
      await load();
      if (result.steam_restart_required) markRestartRequired(`Removed ${game.name}. Restart Steam to finish removing the library entry.`);
      showToast(result.steam_restart_required
        ? `Removed ${game.name} · restart Steam from Settings to apply`
        : `Removed ${game.name} · no library files were present`);
    } catch (error) { stop(); status(String(error), true); }
  };
}

function renderFixesPanel(panel, game, current) {
  const hasFixes = Boolean(current && !current.launch_only);
  panel.innerHTML = `
    <div class="fixes-section security-block" id="security-block" hidden>
      <div class="row"><div class="label">Windows Security</div><div class="value" id="security-value">Checking…</div></div>
      <div class="actions">
        <button id="security-apply">Add exception</button>
        <button id="security-remove" disabled>Remove exception</button>
        <button id="security-repair">Repair</button>
      </div>
      <div class="status" id="security-status"></div>
    </div>
    <div class="fixes-separator"></div>
    <div class="fixes-section">
      <div class="actions">
        <button class="primary" id="install">Import fixes ZIP</button>
        <button id="export" ${hasFixes ? "" : "disabled"}>Export fixes ZIP</button>
      </div>
    </div>
    <div class="fixes-separator"></div>
    <div class="fixes-section">
      <div class="row"><div class="label">Current fixes location</div><div class="value">${hasFixes ? escape(current.fixes) : "No fixes installed"}</div></div>
      <div class="actions"><button id="open-folder" ${hasFixes ? "" : "disabled"}>Open fixes folder</button><button id="use-folder">Change fixes folder…</button></div>
    </div>
    <div class="fixes-separator"></div>
    <div class="fixes-section">
      <div class="row"><div class="label">Steam launch option</div><div class="value">${current?.launch_option_set ? "Set" : "Not set"}</div></div>
      <div class="actions">
        <button id="toggle" ${hasFixes ? "" : "disabled"}>${current?.enabled ? "Disable fixes" : "Enable fixes"}</button>
        <button class="danger" id="remove" ${hasFixes ? "" : "disabled"}>Remove fixes entry</button>
        <button id="launch-option" ${hasFixes ? "" : "disabled"}>${current?.launch_option_set ? "Clear launch option" : "Set launch option"}</button>
      </div>
    </div>
    <div class="status${hasFixes && current.enabled && !current.launch_option_set ? " error" : ""}">${current?.launch_option_error ? escape(current.launch_option_error) : hasFixes ? (current.enabled && !current.launch_option_set ? "Fixes are enabled, but the Steam launch option is not set. Click Set launch option, then fully restart Steam." : current.enabled ? "Fixes enabled." : "Fixes installed but disabled.") : "No fixes installed. Launch options are available independently."}</div>`;
  const securityBlock = panel.querySelector("#security-block");
  const securityValue = panel.querySelector("#security-value");
  const securityStatus = panel.querySelector("#security-status");
  const securityApply = panel.querySelector("#security-apply");
  const securityRemove = panel.querySelector("#security-remove");
  const securityRepair = panel.querySelector("#security-repair");
  const loadSecurity = async () => {
    try {
      const state = await invoke("windows_security_status", { appid: game.appid });
      securityBlock.hidden = false;
      securityValue.textContent = state.missing ? `${state.missing} path(s) not excluded` : "Active";
      securityStatus.textContent = state.message + (state.detections.length ? ` Detections: ${state.detections.map((item) => item.threat).join(", ")}.` : "");
      securityStatus.classList.toggle("error", state.missing > 0 || !state.defender);
      securityApply.disabled = state.missing === 0;
      securityRemove.disabled = state.missing === state.paths.length;
    } catch (error) {
      securityBlock.hidden = false;
      securityValue.textContent = "Unavailable";
      securityStatus.textContent = String(error);
      securityStatus.classList.add("error");
    }
  };
  const ensureSecurity = async () => {
    try {
      if (!await invoke("windows_security_supported")) return true;
      const state = await invoke("windows_security_status", { appid: game.appid });
      if (state.defender && state.realtime && state.missing > 0) {
        status("Add the Windows Security exception for this game before applying fixes.", true);
        return false;
      }
      return true;
    } catch (error) {
      status(String(error), true);
      return false;
    }
  };
  invoke("windows_security_supported").then((supported) => { if (supported) loadSecurity(); }).catch(() => {});
  securityApply.onclick = async () => {
    if (!window.confirm(`Add scoped Microsoft Defender exclusions for ${game.name}? This does not disable antivirus. It requires Windows admin approval.`)) return;
    const stop = busy(securityApply, "Adding…");
    try {
      await invoke("windows_security_apply", { appid: game.appid, includeGame: true });
      await loadSecurity();
      status("Windows Security exception added for this game.");
    } catch (error) { status(String(error), true); }
    finally { stop(); }
  };
  securityRemove.onclick = async () => {
    if (!window.confirm(`Remove the Windows Security exclusions for ${game.name}?`)) return;
    const stop = busy(securityRemove, "Removing…");
    try {
      await invoke("windows_security_remove", { appid: game.appid });
      await loadSecurity();
      status("Windows Security exception removed.");
    } catch (error) { status(String(error), true); }
    finally { stop(); }
  };
  securityRepair.onclick = async () => {
    const stop = busy(securityRepair, "Repairing…");
    try {
      await invoke("windows_security_repair", { appid: game.appid });
      await loadSecurity();
      status("Windows Security exception repaired.");
    } catch (error) { status(String(error), true); }
    finally { stop(); }
  };
  panel.querySelector("#install").onclick = async () => {
    const button = panel.querySelector("#install");
    if (!await ensureSteamClosed("import fixes")) return;
    if (!await ensureSecurity()) return;
    const stop = busy(button, "Installing…");
    try {
      status("Choose a fixes ZIP…");
      const result = await invoke("install_fixes", { appid: game.appid });
      if (result) {
        fixes.set(game.appid, result);
        await loadFixes();
        markRestartRequired(`Fixes installed for ${game.name}. Restart Steam to apply the launch option.`);
        stop();
        renderDetail(game, "fixes");
        renderGames();
        status(result.launch_option_error
          ? `Fixes installed, but the Steam launch option failed: ${result.launch_option_error}`
          : "Fixes installed and Steam launch option set. Start Steam manually.", !!result.launch_option_error);
      } else { stop(); status("Install cancelled."); }
    } catch (error) { stop(); status(String(error), true); }
  };
  panel.querySelector("#open-folder").onclick = async () => {
    const button = panel.querySelector("#open-folder");
    const stop = busy(button, "Opening…");
    try { await invoke("open_fixes_folder", { appid: game.appid }); stop(); }
    catch (error) { stop(); status(String(error), true); }
  };
  panel.querySelector("#use-folder").onclick = async () => {
    const button = panel.querySelector("#use-folder");
    if (!await ensureSteamClosed("change the fixes folder")) return;
    if (!await ensureSecurity()) return;
    const stop = busy(button, "Opening…");
    try {
      const result = await invoke("use_fixes_folder", { appid: game.appid });
      if (!result) { stop(); status("Folder selection cancelled."); return; }
      fixes.set(game.appid, result);
      await loadFixes();
      markRestartRequired(`Fixes folder changed for ${game.name}. Restart Steam to apply the launch option.`);
      stop();
      renderDetail(game, "fixes");
      renderGames();
      status(result.launch_option_error
        ? `Fixes folder set, but the Steam launch option failed: ${result.launch_option_error}`
        : "Fixes folder set and Steam launch option applied. Start Steam manually.", !!result.launch_option_error);
    } catch (error) { stop(); status(String(error), true); }
  };
  panel.querySelector("#export").onclick = async () => {
    const button = panel.querySelector("#export");
    if (!await ensureSteamClosed("export fixes")) return;
    const stop = busy(button, "Exporting…");
    try { status("Preparing fixes ZIP…"); const result = await invoke("export_fixes", { appid: game.appid, title: game.name }); status(result ? `Exported ${result}` : "Export cancelled."); }
    catch (error) { status(String(error), true); }
    finally { stop(); }
  };
  panel.querySelector("#toggle").onclick = async () => {
    const button = panel.querySelector("#toggle");
    if (!await ensureSteamClosed(current.enabled ? "disable fixes" : "enable fixes")) return;
    const stop = busy(button, current.enabled ? "Disabling…" : "Enabling…");
    try {
      const message = await invoke("set_enabled", { appid: game.appid, enabled: !current.enabled });
      await loadFixes();
      markRestartRequired(`${game.name} fixes changed. Restart Steam to apply.`);
      stop();
      renderDetail(game, "fixes");
      status(message);
    } catch (error) {
      stop();
      try { await loadFixes(); renderDetail(game, "fixes"); } catch {}
      status(String(error), true);
    }
  };
  panel.querySelector("#launch-option").onclick = async () => {
    const button = panel.querySelector("#launch-option");
    if (!await ensureSteamClosed(current?.launch_option_set ? "clear the launch option" : "set the launch option")) return;
    const enabled = !current?.launch_option_set;
    const stop = busy(button, enabled ? "Setting…" : "Clearing…");
    try { const message = await invoke("apply_launch_option", { appid: game.appid, enabled }); await loadFixes(); markRestartRequired(`${game.name} launch option changed. Restart Steam to apply.`); stop(); renderDetail(game, "fixes"); status(message); }
    catch (error) { stop(); status(String(error), true); }
  };
  panel.querySelector("#remove").onclick = async () => {
    if (!window.confirm("Remove this fixes entry and delete its fixes folder from disk?")) return;
    const button = panel.querySelector("#remove");
    if (!await ensureSteamClosed("remove fixes")) return;
    const stop = busy(button, "Removing fixes…");
    try { await invoke("remove_fixes", { appid: game.appid }); fixes.delete(game.appid); await loadFixes(); markRestartRequired(`${game.name} fixes removed. Restart Steam to apply.`); stop(); renderDetail(game, "fixes"); renderGames(); status("Fixes entry removed."); }
    catch (error) { stop(); status(String(error), true); }
  };
}

function renderUpdatesPanel(host, game) {
  host.innerHTML = `<div class="updates-panel"><div class="update-summary" id="updates-summary">Loading archived builds…</div><div class="update-list" id="updates-list"></div><div class="update-actions"><button id="updates-auto">Use automatic updates</button><button class="primary" id="updates-pin" disabled>Pin selected build</button><button id="updates-refresh">Refresh</button></div></div>`;
  const summary = host.querySelector("#updates-summary");
  const list = host.querySelector("#updates-list");
  const auto = host.querySelector("#updates-auto");
  const pin = host.querySelector("#updates-pin");
  let data = null;
  let selectedVersion = "";
  const load = async () => {
    const button = host.querySelector("#updates-refresh");
    const stop = busy(button, "Checking…");
    summary.classList.remove("error");
    summary.textContent = "Loading archived builds…";
    list.replaceChildren();
    try {
      data = await invoke("update_versions", { appid: game.appid });
      selectedVersion = data.pinned || data.installed || data.latest || "";
      render();
    } catch (error) { summary.textContent = String(error); summary.classList.add("error"); }
    finally { stop(); }
  };
  const render = () => {
    if (!data) return;
    summary.classList.remove("error");
    if (!data.versions.length) {
      summary.textContent = "No archived builds were found in the SLSsteam or depotcache stores.";
      auto.disabled = false;
      pin.disabled = true;
      return;
    }
    const installed = data.installed_present ? `installed ${data.installed}${data.installed_buildid ? ` · build ${data.installed_buildid}` : ""}` : "not installed";
    const pinned = data.pinned || "automatic";
    summary.textContent = `Content: ${installed}\nLatest archived: ${data.latest || "unknown"} · Pinned: ${pinned}${data.pinned && data.installed_present && data.pinned !== data.installed ? " (uninstall and reinstall to apply)" : ""}`;
    auto.disabled = false;
    pin.disabled = !selectedVersion;
    list.replaceChildren();
    for (const version of data.versions) {
      const row = document.createElement("button");
      row.className = `update-row${version.date === selectedVersion ? " selected" : ""}`;
      const badges = [
        version.latest ? "Latest" : "",
        version.installed ? "Installed" : "",
        version.pinned ? "Pinned" : "",
      ].filter(Boolean);
      const build = version.buildid ? `build ${escape(version.buildid)}` : "build not archived";
      row.innerHTML = `<span class="update-version"><span class="date">${escape(version.date)}</span><small>${version.depots} depot${version.depots === 1 ? "" : "s"} · ${build}</small></span><span class="update-badges">${badges.map((badge) => `<span class="${badge.toLowerCase()}">${badge}</span>`).join("")}</span>`;
      row.onclick = () => { selectedVersion = version.date; render(); };
      list.appendChild(row);
    }
  };
  host.querySelector("#updates-refresh").onclick = load;
  auto.onclick = async () => {
    const stop = busy(auto, "Clearing pin…");
    try { data = await invoke("clear_update_pin", { appid: game.appid }); selectedVersion = data.installed || data.latest || ""; markRestartRequired(`${game.name} update pin cleared. Restart Steam to apply.`); stop(); render(); }
    catch (error) { stop(); summary.textContent = String(error); summary.classList.add("error"); }
  };
  pin.onclick = async () => {
    if (!selectedVersion) return;
    const stop = busy(pin, "Pinning…");
    try { data = await invoke("update_pin", { appid: game.appid, date: selectedVersion }); markRestartRequired(`${game.name} update pin changed. Restart Steam to apply.`); stop(); render(); }
    catch (error) { stop(); summary.textContent = String(error); summary.classList.add("error"); }
  };
  load();
}

async function loadBackend() {
  const node = document.querySelector("#backend");
  if (!node) return;
  try {
    const status = await invoke("backend_status");
    node.textContent = `${status.backend}: ${status.ready ? "ready" : "not ready"}`;
    node.title = `${status.details}\nSteam: ${status.steam_root}\nLua: ${status.lua_dir}`;
    node.classList.toggle("ready", status.ready);
    node.classList.toggle("error", !status.ready);
  } catch (error) {
    node.textContent = "Library backend unavailable";
    node.title = String(error);
    node.classList.add("error");
  }
}

async function loadFixes() {
  fixes = new Map((await invoke("list_fixes")).map((item) => [item.appid, item]));
  try {
    const repaired = await invoke("repair_launch_options");
    if (repaired.length) {
      fixes = new Map((await invoke("list_fixes")).map((item) => [item.appid, item]));
    }
  } catch (_error) {
    // The Fixes tab surfaces any launch-option problem for the selected game.
  }
}

async function load() {
  if (!games.length) showLibraryLoading();
  await loadBackend();
  try {
    games = await invoke("list_games");
    await loadFixes();
    try {
      const added = await invoke("auto_launch_targets");
      if (added.length) await loadFixes();
    } catch (_error) { /* non-fatal */ }
    try { await invoke("auto_pin_installed"); } catch (_error) { /* non-fatal */ }
    try { await invoke("reconcile_registrations"); } catch (_error) { /* keep stored states */ }
    try { managedLibrary = await invoke("library_snapshot"); } catch (_error) { managedLibrary = null; }
    await refreshManagedRunning();
    if (hasManagedLibrary()) {
      document.body.classList.add("managed");
      renderManagedMode();
      void loadArtwork();
      setTimeout(() => { void warmStore(); }, 1200);
      return true;
    }
    document.body.classList.remove("managed");
    if (!games.some((game) => game.appid === selected)) {
      selected = games[0]?.appid || null;
      detailTab = "general";
    }
    renderGames();
    setTimeout(() => { void warmStore(); }, 1200);
    if (selected) {
      const game = games.find((item) => item.appid === selected);
      if (game) renderDetail(game, detailTab);
    } else {
      detailEl.innerHTML = `<div class="empty">No Steam games found.</div>`;
    }
    void loadArtwork();
    return true;
  } catch (error) {
    detailEl.innerHTML = `<div class="status error">${escape(error)}</div>`;
    return false;
  }
}

async function restartSteam(button) {
  if (!button) return;
  const stop = busy(button, "Restarting…");
  showToast("Restarting Steam…");
  try {
    const message = await invoke("restart_steam");
    clearRestartRequired();
    showToast(message);
  }
  catch (error) { showToast(String(error), true); }
  finally { setTimeout(stop, 1500); }
}

async function refreshLibrary() {
  const button = document.querySelector("#refresh");
  const stop = busy(button, "Refreshing…");
  const ok = await load();
  stop();
  if (ok) showToast(`Library refreshed · ${games.length} game${games.length === 1 ? "" : "s"}`);
  else showToast("Refresh failed. Check Settings → Doctor.", true);
}

async function loadDoctor(target = null) {
  const host = target || detailEl;
  host.innerHTML = `<div class="doctor-head"><div><h2>Doctor</h2><p class="muted">Validate Steam and fixes requirements.</p></div><div class="update-actions"><button class="primary" id="doctor-repair">Repair Steam library</button><button id="doctor-refresh">Recheck</button></div></div><div class="doctor-list"><div class="status">Running checks…</div></div>`;
  host.querySelector("#doctor-refresh").onclick = () => loadDoctor(host);
  host.querySelector("#doctor-repair").onclick = async () => {
    if (!window.confirm("Repair will install SLSsteam. Start Steam manually afterward. Continue?")) return;
    const button = host.querySelector("#doctor-repair");
    const stop = busy(button, "Repairing…");
    try {
      const message = await invoke("repair_steam_library");
      markRestartRequired("Steam library repaired. Start Steam manually.");
      stop();
      renderSettingsMessage(message);
      await loadBackend();
      await loadDoctor(host);
    } catch (error) { stop(); renderSettingsMessage(String(error), true); }
  };
  try {
    const report = await invoke("doctor_report");
    const list = host.querySelector(".doctor-list");
    list.replaceChildren();
    for (const item of report.checks) {
      const row = document.createElement("div");
      row.className = `doctor-item ${item.status}`;
      row.innerHTML = `<div class="doctor-title">${escape(item.title)}</div><div class="doctor-detail">${escape(item.detail)}</div>`;
      list.appendChild(row);
    }
    const summary = document.createElement("div");
    summary.className = report.healthy ? "status" : "status error";
    summary.textContent = report.healthy ? "All checks passed." : "Some checks need attention.";
    list.prepend(summary);
  } catch (error) {
    host.innerHTML = `<div class="status error">${escape(error)}</div>`;
  }
}


let settingsTab = "general";
let settingsLibrary = null;

function formatBytes(bytes) {
  if (!bytes) return "0 B";
  const units = ["B", "KiB", "MiB", "GiB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) { value /= 1024; unit += 1; }
  return `${value.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
}

function settingsRow(title, detail, control = "") {
  return `<div class="settings-row"><div class="row-text"><strong>${escape(title)}</strong>${detail ? `<span>${escape(detail)}</span>` : ""}</div><div class="row-control">${control}</div></div>`;
}

function settingsStatusBadge(status) {
  const cls = status === "ok" ? " ok" : status === "error" || status === "warning" ? " error" : "";
  return `<span class="settings-badge${cls}">${escape(status)}</span>`;
}

function settingsPage() {
  return document.querySelector("#settings-page");
}

async function openSettingsPage() {
  document.body.classList.add("settings-open");
  settingsPage().classList.remove("hidden");
  await renderSettingsPage();
  const activeTab = settingsPage().querySelector(".settings-tab.active");
  if (activeTab) focusControl(activeTab);
}

function closeSettingsPage() {
  const page = settingsPage();
  if (page) page.classList.add("hidden");
  document.body.classList.remove("settings-open");
  if (navState.focused) navState.focused.classList.remove("nav-focus");
  navState.focused = null;
}

async function refreshSettingsLibrary() {
  try { settingsLibrary = await invoke("library_snapshot"); }
  catch (_error) { settingsLibrary = null; }
  return settingsLibrary;
}

async function renderSettingsPage() {
  const page = settingsPage();
  if (!page) return;
  for (const tab of page.querySelectorAll("[data-settings-tab]")) {
    tab.classList.toggle("active", tab.dataset.settingsTab === settingsTab);
  }
  const body = page.querySelector("#settings-body");
  body.innerHTML = '<div class="settings-loading">Loading…</div>';
  try {
    if (settingsTab === "general") await renderGeneralSettings(body);
    else if (settingsTab === "storage") await renderStorageSettings(body);
    else await renderProtonSettings(body);
  } catch (error) {
    body.innerHTML = `<div class="status error">${escape(error)}</div>`;
  }
}

async function renderGeneralSettings(host) {
  const [snapshot, diagnostics, profile, version] = await Promise.all([
    invoke("library_snapshot").catch(() => null),
    invoke("diagnostics_summary").catch(() => null),
    invoke("profile_status").catch(() => null),
    invoke("app_version").catch(() => ""),
  ]);
  settingsLibrary = snapshot;
  const settings = snapshot?.settings || {};
  const mode = settings.default_display_mode || "desktop";
  const gamemode = settings.gamemode !== false;
  const email = profile?.email || (profile?.authenticated ? "Signed in" : "Signed out");
  const activity = diagnostics
    ? `${diagnostics.games} games · ${diagnostics.files} files · ${formatBytes(diagnostics.bytes)}`
    : "Unavailable";
  host.innerHTML = `
    <section class="settings-section"><h2>Display mode</h2><p>Default mode applies on the next app start. Enter fullscreen from the mode button; leave through fullscreen options or the tray.</p><div class="settings-card">
      ${settingsRow("Default launch mode", "Desktop or fullscreen when a game starts.", `<select id="setting-display-mode"><option value="desktop"${mode === "desktop" ? " selected" : ""}>Desktop</option><option value="fullscreen"${mode === "fullscreen" ? " selected" : ""}>Fullscreen</option></select>`)}
      ${settingsRow("Feral GameMode", "Automatically optimize CPU governor, GPU performance, and process priorities while gaming, then revert on exit.", `<label class="switch"><input type="checkbox" id="setting-gamemode"${gamemode ? " checked" : ""}><span></span></label>`)}
    </div></section>
    <section class="settings-section"><h2>Account</h2><p>Session used for manifests, library actions and Steam registration.</p><div class="settings-card">
      ${settingsRow(email, profile?.base_url || "SteamTron account", '<button id="setting-signout">Sign out</button>')}
    </div></section>
    <section class="settings-section"><h2>Maintenance</h2><p>Library backup, restore and desktop integration.</p><div class="settings-card">
      <div class="settings-row"><div class="row-text"><strong>Library metadata</strong><span>Back up or restore Lua manifests, appmanifests and fixes.</span></div><div class="row-control settings-inline"><button id="setting-backup">Backup…</button><button id="setting-restore">Restore…</button></div></div>
      ${settingsRow("Desktop shortcut", "Add SteamTron to the application menu.", '<button id="setting-shortcut">Install</button>')}
      ${settingsRow("Steam client", "Restart Steam to apply library changes.", '<button id="setting-restart">Restart Steam</button>')}
    </div></section>
    <section class="settings-section"><h2>Steam diagnostics</h2><p>Validate Steam, fixes and the registration backend.</p><div class="settings-card">
      ${settingsRow("Doctor", "Checks Steam, the Lua folder, fixes and the registration backend.", '<button id="setting-doctor-run">Run checks</button>')}
      <div id="setting-doctor-results"></div>
    </div></section>
    <section class="settings-section"><h2>Diagnostic logs</h2><p>One local folder per game. Completed logs: 8 MiB per file, 100 MiB total, 14-day retention. Cleanup runs before and after launches; active logs are never cleared.</p><div class="settings-card">
      ${settingsRow("Activity", activity, '<button id="setting-open-logs">Open logs folder</button>')}
      <button id="setting-clear-logs" class="danger-row">Clear all diagnostic logs</button>
    </div></section>
    <p class="settings-version">SteamTron ${escape(version)}</p>`;

  host.querySelector("#setting-display-mode").onchange = async (event) => {
    try {
      await invoke("update_library_settings", { defaultDisplayMode: event.target.value });
      showToast(`Default launch mode: ${event.target.value}`);
    } catch (error) { showToast(String(error), true); }
  };
  host.querySelector("#setting-gamemode").onchange = async (event) => {
    try {
      await invoke("update_library_settings", { gamemode: event.target.checked });
      showToast(event.target.checked ? "GameMode optimization enabled" : "GameMode optimization disabled");
    } catch (error) { showToast(String(error), true); }
  };
  host.querySelector("#setting-signout").onclick = async () => {
    try {
      await invoke("profile_logout");
      closeSettingsPage();
      showAuthGate("Signed out. Sign in to continue.");
    } catch (error) { showToast(String(error), true); }
  };
  host.querySelector("#setting-backup").onclick = async (event) => {
    const stop = busy(event.target, "Backing up…");
    try {
      const result = await invoke("export_backup");
      stop();
      if (!result) { showToast("Backup cancelled."); return; }
      showToast(`Backup written: ${result.path} (${result.games} games, ${result.files} files)`);
    } catch (error) { stop(); showToast(String(error), true); }
  };
  host.querySelector("#setting-restore").onclick = async (event) => {
    if (!window.confirm("Restore metadata from a backup? Steam must be stopped. Existing Lua, appmanifest and fixes files may be overwritten.")) return;
    const stop = busy(event.target, "Restoring…");
    try {
      const result = await invoke("restore_backup");
      stop();
      if (!result) { showToast("Restore cancelled."); return; }
      markRestartRequired("Library metadata restored. Start Steam to apply the restored files.");
      await load();
      showToast(`Restored ${result.games} games (${result.lua_files} Lua, ${result.appmanifests} appmanifest, ${result.fixes} fixes)`);
    } catch (error) { stop(); showToast(String(error), true); }
  };
  host.querySelector("#setting-shortcut").onclick = async (event) => {
    const stop = busy(event.target, "Installing…");
    try {
      const path = await invoke("install_desktop_entry");
      stop();
      showToast(`Desktop shortcut installed: ${path}`);
    } catch (error) { stop(); showToast(String(error), true); }
  };
  host.querySelector("#setting-restart").onclick = () => restartSteam(host.querySelector("#setting-restart"));
  host.querySelector("#setting-open-logs").onclick = async () => {
    if (!diagnostics?.path) return;
    try { await invoke("open_path", { path: diagnostics.path }); }
    catch (error) { showToast(String(error), true); }
  };
  host.querySelector("#setting-clear-logs").onclick = async (event) => {
    if (!window.confirm("Clear all diagnostic logs? Active logs are kept.")) return;
    const stop = busy(event.target, "Clearing…");
    try {
      const result = await invoke("clear_diagnostics");
      stop();
      showToast(`Cleared ${result.removed_files} files (${formatBytes(result.removed_bytes)})`);
      await renderSettingsPage();
    } catch (error) { stop(); showToast(String(error), true); }
  };
  host.querySelector("#setting-doctor-run").onclick = async (event) => {
    const stop = busy(event.target, "Checking…");
    try {
      const report = await invoke("doctor_report");
      stop();
      const rows = (report.checks || []).map((check) =>
        `<div class="settings-row"><div class="row-text"><strong>${escape(check.title)}</strong><span>${escape(check.detail)}</span></div><div class="row-control">${settingsStatusBadge(check.status)}</div></div>`
      ).join("");
      host.querySelector("#setting-doctor-results").innerHTML = rows || '<div class="settings-row"><div class="row-text"><span>No checks reported.</span></div></div>';
      showToast(report.healthy ? "Steam checks passed" : "Steam checks need attention", !report.healthy);
    } catch (error) { stop(); showToast(String(error), true); }
  };
}

async function renderStorageSettings(host) {
  const snapshot = await refreshSettingsLibrary();
  const drives = snapshot?.drives || [];
  const games = snapshot?.games || [];
  const libraryPath = snapshot?.path || "";
  const dataDir = libraryPath.replace(/\/[^/]+$/, "");
  const gamesOn = (drive) => games.filter((game) => {
    const root = game.installation?.root || "";
    return root === drive.root || root.startsWith(`${drive.root.replace(/\/+$/, "")}/`);
  }).length;
  const rows = drives.map((drive) => settingsRow(
    `${drive.label || drive.id}${drive.default ? " · Default" : ""}`,
    `${drive.root} · ${gamesOn(drive)} game${gamesOn(drive) === 1 ? "" : "s"}`,
    `<button data-open-drive="${escape(drive.root)}">Open</button>${drive.default ? "" : `<button data-default-drive="${escape(drive.id)}">Make default</button>`}`,
  )).join("");
  host.innerHTML = `
    <section class="settings-section"><h2>Drives</h2><p>Game content, prefixes and fixes live under <code>SteamTronLibrary/steamtronapps</code> on each drive. Adding a drive never moves existing games.</p><div class="settings-card">
      ${rows || '<div class="settings-row"><div class="row-text"><span>No drives registered yet.</span></div></div>'}
      <div class="settings-row"><div class="row-text"><strong>Add drive</strong><span>Choose a mounted drive or folder for new games.</span></div><div class="row-control"><button id="setting-add-drive">Add drive…</button></div></div>
    </div></section>
    <section class="settings-section"><h2>Data folder</h2><p>Library index, tools, diagnostics and launch state for this installation.</p><div class="settings-card">
      ${settingsRow("SteamTron data", dataDir || "Unavailable", `<button id="setting-open-data">Open folder</button>`)}
      ${settingsRow("Library file", libraryPath || "Unavailable", "")}
    </div></section>`;

  for (const button of host.querySelectorAll("[data-open-drive]")) {
    button.onclick = async () => {
      try { await invoke("open_path", { path: button.dataset.openDrive }); }
      catch (error) { showToast(String(error), true); }
    };
  }
  for (const button of host.querySelectorAll("[data-default-drive]")) {
    button.onclick = async () => {
      try {
        await invoke("set_default_library_drive", { id: button.dataset.defaultDrive });
        await load();
        await renderSettingsPage();
        showToast("Default drive updated");
      } catch (error) { showToast(String(error), true); }
    };
  }
  const add = host.querySelector("#setting-add-drive");
  if (add) add.onclick = async () => {
    const stop = busy(add, "Choosing…");
    try {
      const result = await invoke("add_library_drive");
      stop();
      if (!result) return;
      managedLibrary = result;
      await renderSettingsPage();
      showToast("Drive added");
    } catch (error) { stop(); showToast(String(error), true); }
  };
  const openData = host.querySelector("#setting-open-data");
  if (openData) openData.onclick = async () => {
    try { await invoke("open_path", { path: dataDir }); }
    catch (error) { showToast(String(error), true); }
  };
}

async function renderProtonSettings(host) {
  const data = await invoke("proton_runners").catch(() => ({ runners: [], default: "" }));
  const runners = data.runners || [];
  const defaultPath = data.default || "";
  const options = runners.map((runner) =>
    `<option value="${escape(runner.path)}"${runner.path === defaultPath ? " selected" : ""}>${escape(runner.name)}</option>`
  ).join("");
  const rows = runners.map((runner) => settingsRow(
    runner.name,
    `${runner.path} · ${runner.source}${runner.exists ? "" : " · missing"}`,
    `${runner.path === defaultPath ? "" : `<button data-use-runner="${escape(runner.path)}">Use</button>`}${runner.source === "custom" ? `<button data-remove-runner="${escape(runner.path)}">Remove</button>` : ""}`,
  )).join("");
  host.innerHTML = `
    <section class="settings-section"><h2>Default Proton runner</h2><p>Used for new managed games unless a game overrides it.</p><div class="settings-card">
      <div class="settings-row"><div class="row-text"><strong>Default runner</strong><span>${escape(defaultPath || "Not selected")}</span></div><div class="row-control"><select id="setting-default-runner">${options || '<option value="">No runners found</option>'}</select></div></div>
    </div></section>
    <section class="settings-section"><h2>Runners</h2><p>Detected from Steam and UmuTron, plus folders you add.</p><div class="settings-card">
      ${rows || '<div class="settings-row"><div class="row-text"><span>No Proton runners detected.</span></div></div>'}
      <div class="settings-row"><div class="row-text"><strong>Add runner</strong><span>Choose a Proton folder or the proton executable.</span></div><div class="row-control"><button id="setting-add-runner">Add runner…</button></div></div>
    </div></section>`;

  const select = host.querySelector("#setting-default-runner");
  select.onchange = async () => {
    if (!select.value) return;
    try {
      await invoke("set_default_proton_runner", { path: select.value });
      await renderSettingsPage();
      showToast("Default Proton runner updated");
    } catch (error) { showToast(String(error), true); }
  };
  for (const button of host.querySelectorAll("[data-use-runner]")) {
    button.onclick = async () => {
      try {
        await invoke("set_default_proton_runner", { path: button.dataset.useRunner });
        await renderSettingsPage();
        showToast("Default Proton runner updated");
      } catch (error) { showToast(String(error), true); }
    };
  }
  for (const button of host.querySelectorAll("[data-remove-runner]")) {
    button.onclick = async () => {
      try {
        await invoke("remove_proton_runner", { path: button.dataset.removeRunner });
        await renderSettingsPage();
        showToast("Runner removed");
      } catch (error) { showToast(String(error), true); }
    };
  }
  host.querySelector("#setting-add-runner").onclick = async (event) => {
    const stop = busy(event.target, "Choosing…");
    try {
      const result = await invoke("add_proton_runner");
      stop();
      if (!result) return;
      await renderSettingsPage();
      showToast("Proton runner added");
    } catch (error) { stop(); showToast(String(error), true); }
  };
}

for (const tab of document.querySelectorAll("[data-settings-tab]")) {
  tab.onclick = () => {
    settingsTab = tab.dataset.settingsTab;
    void renderSettingsPage();
  };
}
document.querySelector("#settings-close").onclick = closeSettingsPage;

let storeQuery = "";
let storeGenre = "";
let storeSort = "popular";
let storePage = 1;
const STORE_PAGE_SIZE = 24;
let storeBuffer = [];
let storeServerPage = 0;
let storeExhausted = false;
let storeGenres = [];
let storeLoading = false;
let storeRefreshing = false;
let storeError = "";
let storeTried = false;
let storeDetail = null;
let storeDraft = "";
let storeScrollTop = 0;
let restoreStoreScroll = false;
let storeRecent = (() => { try { return JSON.parse(localStorage.getItem("steamtron.storeRecent.v1") || "[]"); } catch (_error) { return []; } })();

function saveStoreRecent() {
  try { localStorage.setItem("steamtron.storeRecent.v1", JSON.stringify(storeRecent)); } catch (_error) { /* ignore */ }
}

function rememberStoreItem(item) {
  storeRecent = [item, ...storeRecent.filter((entry) => entry.id !== item.id)].slice(0, 12);
  saveStoreRecent();
}

function storeCard(item) {
  const cover = item.images?.portrait || "";
  return `<button class="cover-card" data-store-id="${escape(String(item.id))}" title="${escape(item.name)}">
    <span class="cover">${cover ? `<img loading="lazy" data-store-cover="${escape(cover)}" alt="">` : ""}<span class="play-glyph" aria-hidden="true">▶</span></span>
    <span class="cover-title">${escape(item.name)}</span>
  </button>`;
}

function hydrateStoreCovers(host) {
  for (const image of host.querySelectorAll("img[data-store-cover]")) {
    image.src = image.dataset.storeCover;
    image.addEventListener("error", () => {
      const frame = image.parentElement;
      image.remove();
      if (frame) frame.classList.add("cover-missing");
    });
  }
}

function managedGameForStoreItem(item) {
  if (!item?.steam_app_id || !managedLibrary) return null;
  return managedLibrary.games.find((game) => game.metadata_app_id === item.steam_app_id) || null;
}

async function openStoreItem(id) {
  const base = [...storeBuffer, ...storeRecent].find((entry) => String(entry.id) === String(id));
  if (!base) return;
  if (appView === "store") {
    const content = detailEl.querySelector(".store-content");
    if (content) storeScrollTop = content.scrollTop;
    const search = detailEl.querySelector("#store-search");
    if (search) storeDraft = search.value;
  }
  rememberStoreItem(base);
  if (!managedLibrary) {
    try { managedLibrary = await invoke("library_snapshot"); } catch (_error) { /* keep going */ }
  }
  const known = managedGameForStoreItem(base);
  if (known) {
    openManagedGame(known.id, "store");
    return;
  }
  let item = base;
  try {
    const full = await invoke("store_game", { id: Number(id) });
    if (full && full.id) item = full;
  } catch (_error) { /* the list entry is enough for the detail page */ }
  storeDetail = item;
  appView = "store-game";
  renderStoreDetail();
}

function openAddProgress(title) {
  const backdrop = document.createElement("div");
  backdrop.className = "modal-backdrop";
  backdrop.innerHTML = `<div class="modal add-progress" style="max-width:420px;text-align:center">
    <div class="modal-head"><h2>Add to library</h2></div>
    <h3 class="add-progress-title">${escape(title)}</h3>
    <div class="auth-spinner" id="add-progress-spinner"></div>
    <p class="add-progress-status" id="add-progress-status">Preparing…</p>
    <div class="add-progress-actions" id="add-progress-actions"></div>
  </div>`;
  document.body.appendChild(backdrop);
  const spinner = backdrop.querySelector("#add-progress-spinner");
  const status = backdrop.querySelector("#add-progress-status");
  const actions = backdrop.querySelector("#add-progress-actions");
  return {
    setStatus(message) {
      status.textContent = message;
      status.classList.remove("error");
    },
    close() { backdrop.remove(); },
    fail(message) {
      spinner.classList.add("hidden");
      status.textContent = message;
      status.classList.add("error");
      actions.innerHTML = '<button id="add-progress-close" class="primary">Close</button>';
      const close = actions.querySelector("#add-progress-close");
      close.onclick = () => backdrop.remove();
      focusControl(close);
    },
  };
}

async function addStoreItemToLibrary(item, button) {
  if (!item?.steam_app_id) {
    showToast("This title has no Steam App ID and cannot be added.", true);
    return;
  }
  const stop = busy(button, "Adding…");
  const progress = openAddProgress(item.name);
  progress.setStatus("Checking the library…");
  try {
    const snapshot = await invoke("library_snapshot");
    managedLibrary = snapshot;
    const drive = snapshot.drives?.find((entry) => entry.default) || snapshot.drives?.[0];
    if (!drive) {
      throw new Error("No drive registered yet. Add one in Settings → Storage.");
    }
    let provider = "steamml";
    try {
      const profile = await invoke("profile_status");
      if (profile?.authenticated) provider = "api";
    } catch (_error) { /* fall back to the public provider */ }
    progress.setStatus("Downloading the Lua manifest and registering the App ID…");
    const result = await invoke("add_managed_game", { request: {
      app_id: item.steam_app_id,
      title: item.name,
      drive_root: drive.root,
      drive_label: drive.label || "",
      provider,
      proton: "",
    } });
    managedLibrary = await invoke("library_snapshot").catch(() => managedLibrary);
    progress.close();
    clearRestartRequired();
    showToast(`Added ${result.title} to your library`);
    openManagedGame(result.game_id, "store");
  } catch (error) {
    progress.fail(String(error));
    stop();
    showToast(String(error), true);
  }
}

function renderStoreDetail() {
  const item = storeDetail;
  if (!item) { setView("store"); return; }
  // Managed titles always open the library detail directly.
  const managedEntry = managedGameForStoreItem(item);
  if (managedEntry) {
    openManagedGame(managedEntry.id, "store");
    return;
  }
  const hero = item.images?.hero || item.images?.portrait || "";
  const cover = item.images?.portrait || "";
  const genres = (item.genres || []).map((genre) => genre.name).filter(Boolean).join(", ");
  const meta = [item.release_date, genres].filter(Boolean).join(" · ");
  const credits = [item.developers, item.publishers].filter(Boolean).join(" · ");
  const chips = [{ text: "Proton: …" }, { text: `IGDB_ID: ${item.id}`, copy: String(item.id), label: "IGDB_ID" }];
  if (item.steam_app_id) chips.push({ text: `STEAM_ID: ${item.steam_app_id}`, copy: String(item.steam_app_id), label: "STEAM_ID" });
  const action = item.steam_app_id
    ? '<button class="primary" id="store-add">Add to library</button>'
    : '<button class="primary" disabled title="This title has no Steam release, so it cannot be added to the Steam library.">Not available on Steam</button>';
  const availabilityNote = !item.steam_app_id
    ? '<p class="game-note">This title has no Steam release, so SteamTron cannot add it to the Steam library or rate it on ProtonDB.</p>'
    : "";
  detailEl.innerHTML = `<div class="page game-page game-detail">
    <div class="game-page-top"><button id="store-back" class="secondary">← Store</button></div>
    <section class="game-hero${hero ? "" : " no-artwork"}" id="game-hero"${hero ? ` style="background-image:url('${escape(hero)}')"` : ""}>
      <div class="game-hero-shade"></div>
      <div class="game-hero-card">
        ${cover ? `<img class="game-cover" src="${escape(cover)}" alt="" loading="lazy">` : ""}
        <div class="game-info">
          <h1>${escape(item.name)}</h1>
          <p class="game-meta">${escape(meta || "IGDB catalogue entry")}</p>
          <p class="game-description">${escape(item.description || "")}</p>
          <p class="game-credits">${escape(credits)}</p>
          <div class="game-actions">${action}</div>
          ${availabilityNote}
          <div class="game-chips" id="store-chips"></div>
        </div>
    </section>
  </div>`;
  detailEl.querySelector("#store-back").onclick = () => setView("store");
  renderGameChips(detailEl.querySelector("#store-chips"), chips);
  if (!item.steam_app_id) {
    void invoke("store_resolve_steam", { title: item.name }).then((appid) => {
      if (!appid || appView !== "store-game" || storeDetail?.id !== item.id) return;
      storeDetail = { ...storeDetail, steam_app_id: appid };
      const managed = managedGameForStoreItem(storeDetail);
      if (managed) { openManagedGame(managed.id, "store"); return; }
      renderStoreDetail();
    }).catch(() => {});
  }
  if (item.steam_app_id) {
    void protonTier(item.steam_app_id).then((tier) => {
      chips[0].text = `Proton: ${tier || "Unknown"}`;
      renderGameChips(detailEl.querySelector("#store-chips"), chips);
    }).catch(() => {});
  } else {
    chips[0] = {
      text: "Proton: No Steam build",
      title: "ProtonDB rates Steam builds; this title has no Steam release.",
    };
    renderGameChips(detailEl.querySelector("#store-chips"), chips);
  }
  const add = detailEl.querySelector("#store-add");
  if (add) {
    add.onclick = () => addStoreItemToLibrary(item, add);
  }
  const coverNode = detailEl.querySelector(".game-cover");
  if (coverNode) coverNode.addEventListener("error", () => { coverNode.style.visibility = "hidden"; });
}

function isPcCatalogItem(item) {
  const platforms = item.platforms || [];
  if (!platforms.length) return true; // unknown platform data: keep the title
  return platforms.some((platform) => {
    const name = String(platform.name || "").toLowerCase();
    return name.includes("windows") || name.includes("linux") || name.includes("macos") || name === "mac";
  });
}

function resetStorePaging() {
  storeBuffer = [];
  storeServerPage = 0;
  storeExhausted = false;
  storePage = 1;
  storeError = "";
}

async function ensureStorePage(page, render = true) {
  storePage = Math.max(1, page);
  storeLoading = true;
  storeRefreshing = false;
  storeError = "";
  if (render) renderStorePage();
  const target = storePage * STORE_PAGE_SIZE;
  let guard = 0;
  while (storeBuffer.length < target && !storeExhausted && guard < 8) {
    guard += 1;
    const nextServerPage = storeServerPage + 1;
    let data;
    try {
      if (!storeGenres.length) {
        try { storeGenres = await invoke("store_genres"); } catch (_error) { storeGenres = []; }
      }
      data = await invoke("store_catalog", {
        query: storeQuery,
        genre: storeGenre,
        sort: storeSort,
        page: nextServerPage,
      });
    } catch (error) {
      storeError = String(error);
      break;
    }
    storeServerPage = nextServerPage;
    const items = data.items || [];
    for (const item of items) {
      if (!isPcCatalogItem(item)) continue;
      if (!storeBuffer.some((entry) => entry.id === item.id)) storeBuffer.push(item);
    }
    const totalPages = data.total_pages || 1;
    if (!items.length || storeServerPage >= totalPages) storeExhausted = true;
  }
  storeLoading = false;
  storeTried = true;
  if (render) renderStorePage();
}

async function openStore() {
  if (!storeBuffer.length) {
    resetStorePaging();
    await ensureStorePage(1);
  } else {
    renderStorePage();
  }
}

async function warmStore() {
  if (storeTried || storeLoading) return;
  storeTried = true;
  await ensureStorePage(1, false);
}

function renderStorePage() {
  const previousActive = document.activeElement;
  const previousInDetail = previousActive && detailEl.contains(previousActive);
  const previousId = previousInDetail ? previousActive.id : "";
  const previousStoreId = previousInDetail ? previousActive.dataset?.storeId || "" : "";
  const previousPage = previousInDetail ? previousActive.dataset?.storePage || "" : "";
  const previousScroll = detailEl.querySelector(".store-content")?.scrollTop || 0;
  const genreOptions = storeGenres.map((genre) =>
    `<button class="store-option${String(genre.id) === storeGenre ? " selected" : ""}" data-store-genre="${escape(String(genre.id))}">${escape(genre.name)}</button>`
  ).join("");
  const sortOptions = [["popular", "Most popular"], ["rating", "Top rated"], ["latest", "Newest"], ["name_asc", "A–Z"], ["name_desc", "Z–A"]]
    .map(([value, label]) => `<button class="store-option${value === storeSort ? " selected" : ""}" data-store-sort="${value}">${label}</button>`)
    .join("");
  const visible = storeBuffer.slice((storePage - 1) * STORE_PAGE_SIZE, storePage * STORE_PAGE_SIZE);
  const hasMore = !storeExhausted || storeBuffer.length > storePage * STORE_PAGE_SIZE;
  const count = storeLoading ? "Loading…" : storeError ? "Store unavailable" : `${storeBuffer.length} PC titles`;
  detailEl.innerHTML = `<div class="page store-page">
    <header class="page-head"><h1>Discover your next game</h1><p>PC and Steam-playable titles only. Browse the catalogue by genre and rating. Game data by IGDB.</p></header>
    <div class="store-body">
      <aside class="store-sidebar">
        <div class="store-sidebar-head"><label for="store-search">Search</label><button type="button" tabindex="0" class="store-clear-link" id="store-clear-search">Clear</button></div>
        <input id="store-search" placeholder="Search the catalogue and press Enter" value="${escape(storeDraft)}" aria-label="Search the catalogue">
        <div class="store-sidebar-head"><label>Genre</label><button type="button" tabindex="0" class="store-clear-link" id="store-clear-genre">Clear</button></div>
        <div class="store-option-list" data-list="genre" role="listbox" aria-label="Genre">
          ${genreOptions}
        </div>
        <div class="store-sidebar-head"><label>Sort</label><button type="button" tabindex="0" class="store-clear-link" id="store-clear-sort">Clear</button></div>
        <div class="store-option-list" data-list="sort" role="listbox" aria-label="Sort">${sortOptions}</div>
        <p class="library-count">${escape(count)}</p>
      </aside>
      <section class="store-content">
        ${storeRecent.length ? `<div class="store-recent"><span class="store-recent-label">Recently viewed</span>${storeRecent.map((item) => `<button class="store-chip" data-store-id="${escape(String(item.id))}">${escape(item.name)}</button>`).join("")}</div>` : ""}
        <div id="store-results" class="cover-grid store-grid">
          ${storeError
            ? `<div class="status error">${escape(storeError)}</div><button id="store-retry">Retry</button>`
            : visible.length
              ? visible.map(storeCard).join("")
              : storeLoading ? '<div class="status">Loading the catalogue…</div>' : '<div class="status">No games match these filters.</div>'}
        </div>
        <nav class="pager" aria-label="Store pages">
          <button data-store-page="first"${storePage === 1 ? " disabled" : ""}>First</button>
          <button data-store-page="prev"${storePage === 1 ? " disabled" : ""}>Previous</button>
          <span class="pager-page">${storePage}</span>
          <button data-store-page="next"${hasMore ? "" : " disabled"}>Next</button>
        </nav>
        <p class="pager-hint">Page ${storePage} · ${STORE_PAGE_SIZE} titles per page · PgUp / PgDn</p>
      </section>
    </div>
  </div>`;

  const input = detailEl.querySelector("#store-search");
  input.addEventListener("input", () => { storeDraft = input.value; });
  const run = () => {
    storeDraft = input.value;
    storeQuery = storeDraft.trim();
    resetStorePaging();
    void ensureStorePage(1);
  };
  input.addEventListener("keydown", (event) => { if (event.key === "Enter") run(); });
  for (const button of detailEl.querySelectorAll("[data-store-genre]")) {
    button.onclick = () => { storeGenre = button.dataset.storeGenre; resetStorePaging(); void ensureStorePage(1); };
  }
  for (const button of detailEl.querySelectorAll("[data-store-sort]")) {
    button.onclick = () => { storeSort = button.dataset.storeSort; resetStorePaging(); void ensureStorePage(1); };
  }
  detailEl.querySelector("#store-clear-search").onclick = () => {
    storeQuery = "";
    storeDraft = "";
    input.value = "";
    resetStorePaging();
    void ensureStorePage(1);
  };
  detailEl.querySelector("#store-clear-genre").onclick = () => {
    storeGenre = "";
    resetStorePaging();
    void ensureStorePage(1);
  };
  detailEl.querySelector("#store-clear-sort").onclick = () => {
    storeSort = "popular";
    resetStorePaging();
    void ensureStorePage(1);
  };
  const retry = detailEl.querySelector("#store-retry");
  if (retry) retry.onclick = () => { void ensureStorePage(storePage); };
  for (const button of detailEl.querySelectorAll("[data-store-page]")) {
    button.onclick = () => {
      const action = button.dataset.storePage;
      let next = storePage;
      if (action === "first") next = 1;
      else if (action === "prev") next = Math.max(1, storePage - 1);
      else if (action === "next") next = storePage + 1;
      if (next !== storePage) void ensureStorePage(next);
    };
  }
  for (const card of detailEl.querySelectorAll("[data-store-id]")) {
    card.onclick = () => openStoreItem(card.dataset.storeId);
  }
  hydrateStoreCovers(detailEl);
  for (const list of detailEl.querySelectorAll(".store-option-list")) {
    const selected = list.querySelector(".store-option.selected");
    if (selected) {
      list.scrollTop = Math.max(0, selected.offsetTop - (list.clientHeight - selected.offsetHeight) / 2);
    }
  }
  const content = detailEl.querySelector(".store-content");
  if (content) content.scrollTop = restoreStoreScroll ? storeScrollTop : previousScroll;
  restoreStoreScroll = false;
  if (previousInDetail) {
    let restore = null;
    if (previousStoreId) restore = detailEl.querySelector(`[data-store-id="${CSS.escape(previousStoreId)}"]`);
    else if (previousPage) restore = detailEl.querySelector(`[data-store-page="${previousPage}"]`);
    else if (previousId) restore = detailEl.querySelector(`#${CSS.escape(previousId)}`);
    if (restore) focusControl(restore);
  }
}

function showAddGame() {
  const backdrop = document.createElement("div");
  backdrop.className = "modal-backdrop";
  backdrop.innerHTML = `<div class="modal"><div class="modal-head"><h2>Add game</h2><button class="modal-close" id="add-close" aria-label="Close">×</button></div><p class="muted">Search the Steam catalogue by game name, App ID, or Steam URL. Select the exact result before adding.</p><label for="add-search">Search</label><div class="search-row"><input id="add-search" placeholder="Game name, 3468650, or Steam URL"><button id="add-search-button">Search</button></div><div class="search-results"></div><div class="selected-game" style="display:none"></div><div class="modal-status"></div><div class="security-option" id="add-security-option" hidden><label><input type="checkbox" id="add-security"> Add a Windows Security exception before fixes</label><div class="muted">Adds scoped Microsoft Defender exclusions for SteamTron's fix, upper-layer, and mounted-overlay paths, plus the game folder. It does not disable antivirus. Requires Windows admin approval.</div></div><div class="modal-actions" id="add-actions" hidden><button id="add-import" disabled>Import ZIP/Lua for selected game</button><button id="add-submit" class="primary" disabled>Add selected game</button></div></div>`;
  document.body.appendChild(backdrop);
  const queryInput = backdrop.querySelector("#add-search");
  const searchButton = backdrop.querySelector("#add-search-button");
  const resultsNode = backdrop.querySelector(".search-results");
  const selectedNode = backdrop.querySelector(".selected-game");
  const statusNode = backdrop.querySelector(".modal-status");
  const submit = backdrop.querySelector("#add-submit");
  const importButton = backdrop.querySelector("#add-import");
  const actionRow = backdrop.querySelector("#add-actions");
  const securityOption = backdrop.querySelector("#add-security-option");
  const securityCheck = backdrop.querySelector("#add-security");
  let selected = null;
  invoke("windows_security_supported").then((supported) => { if (supported) securityOption.hidden = false; }).catch(() => {});
  const close = () => backdrop.remove();
  const setStatus = (message, error = false) => { statusNode.textContent = message; statusNode.classList.toggle("error", error); };
  const clearSelected = () => {
    selected = null;
    selectedNode.style.display = "none";
    actionRow.hidden = true;
    submit.disabled = true;
    importButton.disabled = true;
  };
  const choose = (result) => {
    selected = result;
    selectedNode.style.display = "";
    selectedNode.innerHTML = `<div class="name">${escape(result.name)}</div><div class="muted">Steam App ${escape(result.appid)} · ${escape(result.kind)}</div>`;
    actionRow.hidden = false;
    submit.disabled = false;
    importButton.disabled = false;
    for (const node of resultsNode.querySelectorAll(".search-result")) node.classList.toggle("selected", node.dataset.appid === result.appid);
    setStatus(`Selected ${result.name}.`);
  };
  const runSearch = async () => {
    clearSelected();
    resultsNode.replaceChildren();
    const stop = busy(searchButton, "Searching…");
    setStatus("Searching Steam…");
    await nextPaint();
    try {
      const results = await invoke("search_games", { query: queryInput.value });
      if (!results.length) { setStatus("No matching Steam results.", true); return; }
      setStatus(`${results.length} result${results.length === 1 ? "" : "s"}. Select the correct game.`);
      for (const result of results) {
        const button = document.createElement("button");
        button.className = `search-result${result.installed ? " installed" : ""}`;
        button.dataset.appid = result.appid;
        button.disabled = result.installed;
        button.innerHTML = `${result.image ? `<img src="${escape(result.image)}" alt="">` : ""}<span><strong>${escape(result.name)}${result.installed ? '<span class="in-library">In library</span>' : ""}</strong><small>App ${escape(result.appid)} · ${escape(result.kind)}</small></span>`;
        if (!result.installed) button.onclick = () => choose(result);
        resultsNode.appendChild(button);
      }
    } catch (error) { setStatus(String(error), true); }
    finally { stop(); }
  };
  const finish = async (result) => {
    if (!result) return;
    submit.disabled = true;
    importButton.disabled = true;
    const pinned = result.pinned ? ` · pinned latest ${result.pinned}` : result.pin_error ? ` · latest pin failed: ${result.pin_error}` : "";
    markRestartRequired(`Added ${result.title}. Restart Steam to see it in the library.`);
    setStatus(`Added ${result.title}${pinned}. Restart Steam from Settings to see it in the library.`, !!result.pin_error);
    await load();
    if (result.appmanifest) setStatus(`Added ${result.title}${pinned}. Appmanifest: ${result.appmanifest}`, !!result.pin_error);
  };
  backdrop.querySelector("#add-close").onclick = close;
  backdrop.addEventListener("click", (event) => { if (event.target === backdrop) close(); });
  searchButton.onclick = runSearch;
  queryInput.addEventListener("keydown", (event) => { if (event.key === "Enter") runSearch(); });
  submit.onclick = async () => {
    if (!selected) return;
    const stop = busy(submit, "Adding…");
    importButton.disabled = true;
    setStatus(`Downloading manifest bundle for ${selected.name}…`);
    await nextPaint();
    try {
      const result = await invoke("add_game", { appidOrUrl: selected.appid, title: selected.name });
      let securityWarning = "";
      if (securityCheck.checked) {
        setStatus(`Approve the Windows Security exception for ${selected.name}…`);
        await nextPaint();
        try {
          await invoke("windows_security_apply", { appid: result.appid, includeGame: true });
        } catch (error) {
          securityWarning = `Game added, but the Windows Security exception was not applied: ${error}`;
        }
      }
      stop();
      await finish(result);
      if (securityWarning) setStatus(securityWarning, true);
    } catch (error) {
      stop();
      setStatus(String(error), true);
      submit.disabled = false;
      importButton.disabled = false;
    }
  };
  importButton.onclick = async () => {
    if (!selected) return;
    const stop = busy(importButton, "Choose file…");
    submit.disabled = true;
    setStatus(`Choose the Lua manifest or ZIP for ${selected.name}…`);
    await nextPaint();
    try {
      const result = await invoke("import_game_file", { appidOrUrl: selected.appid, title: selected.name });
      stop();
      if (!result) { setStatus("Import cancelled."); submit.disabled = false; importButton.disabled = false; return; }
      await finish(result);
    }
    catch (error) { stop(); setStatus(String(error), true); submit.disabled = false; importButton.disabled = false; }
  };
  setStatus("Search to find a game, then select a result.");
  queryInput.focus();
}

document.querySelector("#nav-library").onclick = () => setView("library");
document.querySelector("#nav-home").onclick = () => setView("home");
document.querySelector("#nav-store").onclick = () => setView("store");
window.addEventListener("focus", async () => {
  if (!hasManagedLibrary()) return;
  await refreshManagedRunning();
  if (appView === "library" || appView === "home" || appView === "game") renderManagedMode();
});
document.querySelector("#settings").onclick = () => {
  if (document.body.classList.contains("fullscreen-mode")) openFullscreenOptions();
  else void openSettingsPage();
};
document.querySelector("#refresh").onclick = refreshLibrary;
searchEl.oninput = () => { if (hasManagedLibrary()) renderManagedList(); else renderGames(); };
document.querySelector("#restart-banner-button").onclick = (event) => restartSteam(event.currentTarget);
document.querySelector("#restart-banner-dismiss").onclick = () => {
  clearRestartRequired();
};
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    document.querySelectorAll(".modal-backdrop").forEach((node) => node.remove());
  }
});
window.addEventListener("error", (event) => {
  console.error(event.error || event.message);
  showToast(`UI error: ${event.message}`, true);
});
window.addEventListener("unhandledrejection", (event) => {
  console.error(event.reason);
  showToast(`UI error: ${event.reason}`, true);
});
renderRestartBanner();
boot();
