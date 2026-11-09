const invoke = (command, args = {}) => window.__TAURI__.core.invoke(command, args);
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
let appView = "library";
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
  gateSignin.disabled = false;
  gateSignin.focus();
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

function managedCard(game, compact = false) {
  const appId = game.metadata_app_id;
  const cover = appId ? coverUrl(appId) : "";
  const meta = appId ? `Steam App ${appId}` : "Managed entry";
  const readiness = managedReady(game) ? "Ready" : "Setup needed";
  const selectedClass = game.id === managedSelectedId ? " active" : "";
  return `<button class="game game-card${selectedClass}${compact ? " compact" : ""}" data-managed="${escape(game.id)}" title="${escape(game.title)}">
    <span class="cover" aria-hidden="true">${cover ? `<img loading="lazy" src="${escape(cover)}" alt="">` : ""}</span>
    <span class="game-meta">
      <strong>${escape(game.title)}</strong>
      <span>${escape(meta)}</span>
      <span class="badges"><span class="badge${managedReady(game) ? " ok" : " warn"}">${escape(readiness)}</span></span>
    </span>
  </button>`;
}

function renderManagedList() {
  const query = searchEl.value.trim().toLowerCase();
  gamesEl.className = "games list";
  gamesEl.replaceChildren();
  const visible = managedLibrary.games.filter((game) => !query || `${game.title} ${game.metadata_app_id || ""}`.toLowerCase().includes(query));
  for (const game of visible) {
    const wrapper = document.createElement("div");
    wrapper.innerHTML = managedCard(game);
    gamesEl.appendChild(wrapper.firstElementChild);
  }
  if (!gamesEl.children.length) {
    const empty = document.createElement("div");
    empty.className = "empty";
    empty.textContent = managedLibrary.games.length ? "No games match your search." : "Your library is empty. Add a game to get started.";
    gamesEl.appendChild(empty);
  }
}

function managedDetails(game) {
  const drive = managedDrive(game);
  return `<div class="status">
    <div class="row"><div class="label">Library entry</div><div class="value">${escape(game.id)}</div></div>
    <div class="row"><div class="label">Installation</div><div class="value">${escape(game.installation?.root || "Not configured")}</div></div>
    <div class="row"><div class="label">Prefix</div><div class="value">${escape(game.installation?.prefix || "Created on first Play")}</div></div>
    <div class="row"><div class="label">Drive</div><div class="value">${escape(drive ? `${drive.label || drive.id} · ${drive.root}` : "Not on a registered drive")}</div></div>
    <div class="row"><div class="label">Prefix source</div><div class="value">${escape(game.launch?.prefix_mode === "steam" ? "Steam compatdata" : "SteamTron prefix")}</div></div>
    <div class="row"><div class="label">Steam</div><div class="value">${escape(game.metadata_app_id ? `App ${game.metadata_app_id}` : "No App ID")}${game.steam ? ` · ${escape(game.steam.state || "unregistered")}` : ""}</div></div>
    <div class="row"><div class="label">Launch mode</div><div class="value">${escape(game.steam?.launch_mode || "emulator")}</div></div>
  </div>`;
}

function renderManagedPreview(game) {
  const appId = game.metadata_app_id;
  const hero = appId ? headerUrl(appId) : "";
  detailEl.innerHTML = `<div class="detail-hero${hero ? "" : " no-artwork"}"${hero ? ` style="background-image:url('${escape(hero)}')"` : ""}><div class="detail-hero-shade"></div><div class="detail-hero-content"><h2>${escape(game.title)}</h2><p class="muted">${escape(appId ? `Steam App ${appId}` : "Managed entry")}</p></div></div>${managedDetails(game)}`;
}

function renderManagedHome() {
  const installed = managedLibrary.games.filter((game) => managedReady(game));
  const pending = managedLibrary.games.filter((game) => !managedReady(game));
  const featured = installed[0] || managedLibrary.games[0];
  const appId = featured?.metadata_app_id;
  const hero = appId ? headerUrl(appId) : "";
  const rail = (title, items, empty) => `<section class="managed-rail"><h2>${escape(title)}</h2>${items.length ? `<div class="rail-list">${items.map((game) => managedCard(game, true)).join("")}</div>` : `<div class="status">${escape(empty)}</div>`}</section>`;
  detailEl.innerHTML = `<div class="detail-hero${hero ? "" : " no-artwork"}"${hero ? ` style="background-image:url('${escape(hero)}')"` : ""}><div class="detail-hero-shade"></div><div class="detail-hero-content"><p class="muted">Your library</p><h2>${escape(featured?.title || "Your next game")}</h2></div></div>${rail("Ready to play", installed.slice(0, 8), "No installed games yet.")}${rail("Needs setup", pending.slice(0, 8), "Every library entry is ready.")}`;
}

function renderManagedMode() {
  if (!hasManagedLibrary()) return;
  if (!managedLibrary.games.some((game) => game.id === managedSelectedId)) {
    managedSelectedId = managedLibrary.games[0]?.id || null;
  }
  renderManagedList();
  if (appView === "home") {
    renderManagedHome();
    return;
  }
  const game = managedLibrary.games.find((item) => item.id === managedSelectedId);
  if (game) renderManagedPreview(game);
}

function setView(view) {
  appView = view;
  document.querySelector("#nav-home").classList.toggle("active", view === "home");
  document.querySelector("#nav-library").classList.toggle("active", view === "library");
  document.querySelector("#nav-home").setAttribute("aria-current", view === "home" ? "page" : "false");
  document.querySelector("#nav-library").setAttribute("aria-current", view === "library" ? "page" : "false");
  if (hasManagedLibrary()) {
    renderManagedMode();
  } else if (view === "library") {
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
  managedSelectedId = button.dataset.managed;
  appView = "library";
  setView("library");
});

detailEl.addEventListener("click", (event) => {
  const button = event.target.closest("[data-managed]");
  if (!button) return;
  managedSelectedId = button.dataset.managed;
  setView("library");
});

async function boot() {
  try {
    const auth = await invoke("profile_status");
    if (!auth.authenticated) { showAuthGate("Sign in to continue."); return; }
    hideAuthGate();
    await load();
    await checkMigration();
  } catch (error) {
    showAuthGate(`Could not check sign-in: ${error}`);
  }
}

gateSignin.onclick = async () => {
  const stop = busy(gateSignin, "Signing in…");
  gateStatus.textContent = "Complete sign-in in your browser…";
  try {
    const auth = await invoke("profile_login");
    stop();
    if (!auth.authenticated) { showAuthGate("Sign-in did not complete."); return; }
    hideAuthGate();
    await load();
  } catch (error) {
    stop();
    showAuthGate(`Sign-in failed: ${error}`);
  }
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
    try { managedLibrary = await invoke("library_snapshot"); } catch (_error) { managedLibrary = null; }
    if (hasManagedLibrary()) {
      renderManagedMode();
      void loadArtwork();
      return true;
    }
    if (!games.some((game) => game.appid === selected)) {
      selected = games[0]?.appid || null;
      detailTab = "general";
    }
    renderGames();
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

async function renderProfile(host) {
  host.innerHTML = `<div class="status">Loading profile…</div>`;
  try {
    const auth = await invoke("profile_status");
    host.innerHTML = auth.authenticated
      ? `<div class="label">Signed in as</div><div class="profile-email">${escape(auth.email || "SteamTron account")}</div><p class="muted">Session expires ${auth.expires_at ? new Date(auth.expires_at * 1000).toLocaleString() : "later"}.</p><div class="profile-actions"><button class="danger" id="signout">Sign out</button></div>`
      : `<div class="label">SteamTron account</div><p class="muted">Sign in to add games and download manifests.</p><div class="profile-actions"><button class="primary" id="signin">Sign in</button></div>`;
    const signin = host.querySelector("#signin");
    const signout = host.querySelector("#signout");
    if (signin) signin.onclick = async () => {
      signin.disabled = true;
      host.innerHTML = `<div class="status">Complete sign-in in your browser…</div>`;
      try { await invoke("profile_login"); hideAuthGate(); await boot(); await renderProfile(host); }
      catch (error) { host.innerHTML = `<div class="status error">${escape(error)}</div>`; }
    };
    if (signout) signout.onclick = async () => {
      const stop = busy(signout, "Signing out…");
      try { await invoke("profile_logout"); stop(); showAuthGate("Signed out. Sign in to continue."); }
      catch (error) { stop(); host.innerHTML = `<div class="status error">${escape(error)}</div>`; }
    };
  } catch (error) {
    host.innerHTML = `<div class="status error">${escape(error)}</div>`;
  }
}

function renderSettingsMessage(message, isError = false) {
  const node = document.querySelector("#settings-content .settings-message");
  if (node) { node.textContent = message; node.classList.toggle("error", isError); }
}

function renderSettings() {
  detailEl.innerHTML = `<div class="doctor-head"><div><h2>Settings</h2><p class="muted">Account and Steam diagnostics.</p></div><div class="update-actions"><button id="settings-backup">Backup library</button><button id="settings-restore">Restore backup</button><button id="settings-shortcut">Install desktop shortcut</button><button id="settings-restart">Restart Steam</button></div></div><div class="settings-tabs"><button class="settings-tab active" id="tab-profile">Profile</button><button class="settings-tab" id="tab-doctor">Doctor</button></div><div id="settings-content"><div class="settings-message status"></div><div id="settings-panel"></div></div>`;
  const panel = detailEl.querySelector("#settings-panel");
  const profileTab = detailEl.querySelector("#tab-profile");
  const doctorTab = detailEl.querySelector("#tab-doctor");
  const settingsRestart = detailEl.querySelector("#settings-restart");
  settingsRestart.onclick = () => restartSteam(settingsRestart);
  const settingsBackup = detailEl.querySelector("#settings-backup");
  settingsBackup.onclick = async () => {
    const stop = busy(settingsBackup, "Backing up…");
    try {
      const result = await invoke("export_backup");
      stop();
      if (!result) { renderSettingsMessage("Backup cancelled."); return; }
      renderSettingsMessage(`Backup written: ${result.path} (${result.games} games, ${result.files} files).`);
    } catch (error) { stop(); renderSettingsMessage(String(error), true); }
  };
  const settingsRestore = detailEl.querySelector("#settings-restore");
  settingsRestore.onclick = async () => {
    if (!window.confirm("Restore metadata from a backup? Steam must be stopped. Existing Lua, appmanifest, and fixes files may be overwritten.")) return;
    const stop = busy(settingsRestore, "Restoring…");
    try {
      const result = await invoke("restore_backup");
      stop();
      if (!result) { renderSettingsMessage("Restore cancelled."); return; }
      markRestartRequired("Library metadata restored. Start Steam to apply the restored files.");
      await load();
      renderSettingsMessage(`Restored ${result.games} games (${result.lua_files} Lua, ${result.appmanifests} appmanifest, ${result.fixes} fixes).`);
    } catch (error) { stop(); renderSettingsMessage(String(error), true); }
  };
  const settingsShortcut = detailEl.querySelector("#settings-shortcut");
  settingsShortcut.onclick = async () => {
    const stop = busy(settingsShortcut, "Installing…");
    try { const path = await invoke("install_desktop_entry"); stop(); renderSettingsMessage(`Desktop shortcut installed: ${path}. Pin SteamTron from the application menu.`); }
    catch (error) { stop(); renderSettingsMessage(String(error), true); }
  };
  const select = (active) => {
    profileTab.classList.toggle("active", active === "profile");
    doctorTab.classList.toggle("active", active === "doctor");
  };
  profileTab.onclick = () => { select("profile"); renderProfile(panel); };
  doctorTab.onclick = () => { select("doctor"); loadDoctor(panel); };
  select("profile");
  renderProfile(panel);
}


async function showManagedAddGame() {
  try { managedLibrary = await invoke("library_snapshot"); } catch (_error) { /* keep the last snapshot */ }
  const drives = managedLibrary?.drives || [];
  const backdrop = document.createElement("div");
  backdrop.className = "modal-backdrop";
  backdrop.innerHTML = `<div class="modal"><div class="modal-head"><h2>Add game</h2><button class="modal-close" id="managed-add-close" aria-label="Close">×</button></div>
    <p class="muted">Download the Lua manifest, register the App ID with the platform backend, and create the managed library entry. Content is downloaded separately.</p>
    <label for="managed-add-appid">App ID</label><input id="managed-add-appid" inputmode="numeric" pattern="[0-9]+" placeholder="3468650">
    <label for="managed-add-title">Title (optional)</label><input id="managed-add-title" placeholder="Fetched from Steam when empty">
    <label for="managed-add-drive">Drive</label><select id="managed-add-drive">${drives.map((drive) => `<option value="${escape(drive.root)}">${escape(drive.label || drive.root)}</option>`).join("")}<option value="__new">Register a new drive…</option></select>
    <div id="managed-add-new-drive-wrap" style="display:none"><label for="managed-add-new-drive">Drive root</label><input id="managed-add-new-drive" placeholder="/mnt/games"></div>
    <label for="managed-add-provider">Manifest source</label><select id="managed-add-provider"><option value="steamml">SteamML (public, no account)</option><option value="api">UMUTron API (signed in)</option></select>
    <label for="managed-add-proton">Proton runner (optional)</label><input id="managed-add-proton" placeholder="/path/to/proton">
    <div class="modal-status" role="status"></div>
    <div class="modal-actions"><button id="managed-add-cancel">Cancel</button><button id="managed-add-submit" class="primary">Add game</button></div></div>`;
  document.body.appendChild(backdrop);
  const appIdInput = backdrop.querySelector("#managed-add-appid");
  const titleInput = backdrop.querySelector("#managed-add-title");
  const driveSelect = backdrop.querySelector("#managed-add-drive");
  const newDriveWrap = backdrop.querySelector("#managed-add-new-drive-wrap");
  const newDriveInput = backdrop.querySelector("#managed-add-new-drive");
  const providerSelect = backdrop.querySelector("#managed-add-provider");
  const protonInput = backdrop.querySelector("#managed-add-proton");
  const submit = backdrop.querySelector("#managed-add-submit");
  const statusNode = backdrop.querySelector(".modal-status");
  const setStatus = (message, error = false) => { statusNode.textContent = message; statusNode.classList.toggle("error", error); };
  const close = () => backdrop.remove();
  backdrop.querySelector("#managed-add-close").onclick = close;
  backdrop.querySelector("#managed-add-cancel").onclick = close;
  backdrop.addEventListener("click", (event) => { if (event.target === backdrop) close(); });
  driveSelect.onchange = () => {
    const fresh = driveSelect.value === "__new";
    newDriveWrap.style.display = fresh ? "" : "none";
    if (fresh) newDriveInput.focus();
  };
  submit.onclick = async () => {
    const appId = Number(appIdInput.value.trim());
    if (!Number.isInteger(appId) || appId <= 0) { setStatus("Enter a numeric Steam App ID.", true); appIdInput.focus(); return; }
    const fresh = driveSelect.value === "__new";
    const driveRoot = fresh ? newDriveInput.value.trim() : driveSelect.value;
    if (!driveRoot) { setStatus("Choose or enter a drive root.", true); return; }
    const label = fresh ? "" : (driveSelect.selectedOptions[0]?.textContent || "");
    const stop = busy(submit, "Adding…");
    setStatus("Downloading the manifest and registering the App ID…");
    await nextPaint();
    try {
      const result = await invoke("add_managed_game", { request: {
        app_id: appId,
        title: titleInput.value.trim(),
        drive_root: driveRoot,
        drive_label: label,
        provider: providerSelect.value,
        proton: protonInput.value.trim(),
      } });
      markRestartRequired(`Added ${result.title}. Restart Steam to see it.`);
      showToast(`Added ${result.title} · ${result.backend}`);
      close();
      await load();
    } catch (error) {
      stop();
      setStatus(String(error), true);
    }
  };
  appIdInput.focus();
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

document.querySelector("#nav-library").onclick = () => { setView("library"); if (searchEl) searchEl.focus(); };
document.querySelector("#nav-home").onclick = () => setView("home");
document.querySelector("#add-game").onclick = () => { if (hasManagedLibrary()) showManagedAddGame(); else showAddGame(); };
document.querySelector("#settings").onclick = renderSettings;
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
