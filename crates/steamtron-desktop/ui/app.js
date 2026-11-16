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
const runningGrace = new Map();
let libraryPage = 1;
// Every route owns its selection so Home and Library never share an active
// card. The detail page keeps using managedSelectedId independently.
const routeSelection = { home: null, library: null };
const LIBRARY_PAGE_SIZE = 60;
let appView = "home";
let gameDetailOrigin = "library";
let gameDetailSeed = null;
// Steam-restart banners were removed by request. Keep the call sites inert.
function markRestartRequired() {
  try { localStorage.removeItem("steamtron.pendingRestart.v1"); } catch (_error) {}
}

function clearRestartRequired() {}

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

const dialogWindows = [];
let dialogZ = 300;
let dialogSeq = 0;

function dialogTitleOf(modal) {
  const node = modal.querySelector(".modal-head h2, .game-settings-head h2, h2");
  return (node?.textContent || "Dialog").trim() || "Dialog";
}

function dialogEntryFor(backdrop) {
  return dialogWindows.find((entry) => entry.backdrop === backdrop) || null;
}

function raiseDialog(entry) {
  dialogZ += 1;
  entry.backdrop.style.zIndex = String(dialogZ);
}

function cleanupDialog(entry) {
  if (!entry) return;
  entry.observer?.disconnect();
  const index = dialogWindows.indexOf(entry);
  if (index >= 0) dialogWindows.splice(index, 1);
  syncDialogBlocking();
  restoreDialogFocus(entry);
}

function openDialogEntry() {
  for (let index = dialogWindows.length - 1; index >= 0; index -= 1) {
    const entry = dialogWindows[index];
    if (document.body.contains(entry.backdrop)
      && !entry.backdrop.classList.contains("hidden")) {
      return entry;
    }
  }
  return null;
}

function restoreDialogFocus(entry) {
  let target = entry?.returnFocus;
  if (!target) return;
  if (!document.contains(target)) {
    if (target.id) target = document.getElementById(target.id);
    else if (target.dataset?.storeId) {
      target = document.querySelector(`[data-store-id="${CSS.escape(target.dataset.storeId)}"]`);
    } else {
      target = null;
    }
  }
  if (!target || !document.contains(target) || target.disabled) return;
  const top = openDialogEntry();
  if (top) {
    if (top.backdrop.contains(target)) focusControl(target);
    return;
  }
  if (!authGate.classList.contains("hidden")) return;
  if (document.body.classList.contains("settings-open")) {
    const page = settingsPage();
    if (page && page.contains(target)) focusControl(target);
    return;
  }
  if (appSections.some((node) => node?.contains(target) && node.inert)) return;
  if (target.getClientRects && target.getClientRects().length === 0) return;
  focusControl(target);
}

function dialogTop() {
  for (let index = dialogWindows.length - 1; index >= 0; index -= 1) {
    const entry = dialogWindows[index];
    if (!document.body.contains(entry.backdrop)) {
      cleanupDialog(entry);
      continue;
    }
    if (entry.backdrop.classList.contains("hidden")) continue;
    return entry;
  }
  return null;
}

function dismissDialog(entry) {
  if (!entry) return;
  if (document.body.contains(entry.backdrop)) entry.backdrop.remove();
  cleanupDialog(entry);
}

function syncDialogBlocking() {
  const open = dialogWindows.some((entry) =>
    document.body.contains(entry.backdrop) && !entry.backdrop.classList.contains("hidden"));
  document.body.classList.toggle("dialog-open", open);
  syncAppInert();
}

function toggleDialogMaximize(entry) {
  if (!entry) return;
  entry.maximized = !entry.maximized;
  entry.modal.classList.toggle("dialog-maximized", entry.maximized);
  const button = entry.modal.querySelector(".dialog-maximize");
  if (button) {
    const label = `${entry.maximized ? "Restore down" : "Maximize"} ${entry.title}`;
    button.title = entry.maximized ? "Restore down" : "Maximize";
    button.setAttribute("aria-label", label);
  }
}

function registerDialog(backdrop, options = {}) {
  const modal = backdrop.querySelector(".modal");
  if (!modal) return null;
  const existing = dialogEntryFor(backdrop);
  if (existing) {
    raiseDialog(existing);
    return existing;
  }
  modal.classList.add("dialog-window");
  if (options.confirm) modal.classList.add("dialog-confirm");
  const title = options.title || dialogTitleOf(modal);
  const header = modal.querySelector(".modal-head, .game-settings-head");
  if (header) header.classList.add("dialog-head");
  const active = document.activeElement;
  const entry = {
    id: ++dialogSeq,
    backdrop,
    modal,
    title,
    maximized: false,
    observer: null,
    returnFocus: active && active !== document.body && !backdrop.contains(active) ? active : null,
  };
  dialogWindows.push(entry);
  syncDialogBlocking();
  backdrop.dataset.dialogId = String(entry.id);
  raiseDialog(entry);
  if (header && !modal.dataset.dialogControls) {
    const controls = document.createElement("div");
    controls.className = "dialog-controls";
    const max = document.createElement("button");
    max.type = "button";
    max.className = "dialog-control dialog-maximize";
    max.title = "Maximize";
    max.setAttribute("aria-label", `Maximize ${title}`);
    max.innerHTML = '<svg viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.4" aria-hidden="true"><rect x="2" y="2" width="8" height="8" rx="1"></rect></svg>';
    max.onclick = () => toggleDialogMaximize(entry);
    let close = modal.querySelector(".modal-close");
    if (!close) {
      close = document.createElement("button");
      close.type = "button";
      close.className = "modal-close";
      close.innerHTML = "×";
      close.setAttribute("aria-label", `Close ${title}`);
      close.onclick = () => dismissDialog(entry);
    }
    close.classList.add("dialog-control", "dialog-close");
    controls.append(max, close);
    header.appendChild(controls);
    modal.dataset.dialogControls = "1";
  } else if (header) {
    // Re-registered static dialog: rebind the controls to the new entry.
    const controls = header.querySelector(".dialog-controls");
    if (controls) {
      const max = controls.querySelector(".dialog-maximize");
      if (max) max.onclick = () => toggleDialogMaximize(entry);
    }
  }
  entry.observer = new MutationObserver(() => {
    if (!document.body.contains(backdrop)) cleanupDialog(entry);
  });
  entry.observer.observe(document.body, { childList: true });
  return entry;
}

function closeTopDialog() {
  const entry = dialogTop();
  if (!entry) return false;
  const close = entry.modal.querySelector(".modal-close");
  if (close) close.click();
  else dismissDialog(entry);
  if (document.body.contains(entry.backdrop) && entry.backdrop.classList.contains("hidden")) {
    cleanupDialog(entry);
  }
  return true;
}

function confirmDialog(options = {}) {
  const title = options.title || "Confirm";
  const message = options.message || "";
  const confirmLabel = options.confirmLabel || "Continue";
  const cancelLabel = options.cancelLabel || "Cancel";
  return new Promise((resolve) => {
    const backdrop = document.createElement("div");
    backdrop.className = "modal-backdrop";
    backdrop.innerHTML = `<div class="modal">
      <div class="modal-head"><h2>${escape(title)}</h2><button class="modal-close" aria-label="Close">×</button></div>
      <div class="dialog-confirm-body">${escape(message)}</div>
      <div class="modal-actions"><button data-confirm-cancel>${escape(cancelLabel)}</button><button class="${options.danger ? "danger" : "primary"}" data-confirm-ok>${escape(confirmLabel)}</button></div>
    </div>`;
    document.body.appendChild(backdrop);
    const entry = registerDialog(backdrop, { title, confirm: true });
    let settled = false;
    const finish = (value) => {
      if (settled) return;
      settled = true;
      dismissDialog(entry);
      resolve(value);
    };
    backdrop.querySelector("[data-confirm-cancel]").onclick = () => finish(false);
    backdrop.querySelector("[data-confirm-ok]").onclick = () => finish(true);
    backdrop.querySelector(".modal-close").onclick = () => finish(false);
    focusControl(backdrop.querySelector("[data-confirm-ok]") || backdrop.querySelector("[data-confirm-cancel]"));
  });
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
  registerDialog(backdrop);
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
  registerDialog(backdrop);
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

function showAuthGate(message = "Sign in to SteamTron to manage your Steam library.") {
  authGate.classList.remove("hidden");
  gateStatus.textContent = message;
  gateSpinner.classList.add("hidden");
  gateSignin.disabled = false;
  syncAppInert();
  gateSignin.focus();
}

function hideAuthGate() {
  authGate.classList.add("hidden");
  syncAppInert();
}

function syncAppInert() {
  const modalOpen = dialogWindows.some((entry) =>
    document.body.contains(entry.backdrop) && !entry.backdrop.classList.contains("hidden"));
  const inert = !authGate.classList.contains("hidden")
    || document.body.classList.contains("settings-open")
    || modalOpen;
  for (const node of appSections) {
    if (!node) continue;
    node.inert = inert;
    if (inert) node.setAttribute("aria-hidden", "true");
    else node.removeAttribute("aria-hidden");
  }
}


async function refreshManagedRunning() {
  try { runningManaged = new Set(await invoke("running_managed")); }
  catch (_error) { runningManaged = new Set(); }
}

function hasManagedLibrary() {
  return Boolean(managedLibrary?.games?.length);
}

function managedShell() {
  return managedLibrary !== null;
}

function managedDrive(game) {
  const root = game.installation?.root || "";
  return (managedLibrary?.drives || []).find((drive) => {
    const base = String(drive.root || "").replace(/\/+$/, "");
    return base && (root === base || root.startsWith(base + "/"));
  }) || null;
}

function managedReady(game) {
  if (!game) return false;
  // Always trust the snapshot's live filesystem check; a bare root string is
  // recorded when the game is added and does not mean content exists.
  return game.installed === true;
}

function managedPlayable(game) {
  if (!managedReady(game)) return false;
  const target = game.launch_targets?.find((item) => item.primary) || game.launch_targets?.[0];
  const steam = Boolean(game.launch?.requires_steam || game.steam?.launch_mode === "steam_client");
  return steam || Boolean(target);
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
    const currentId = node?.id === "home-hero"
      ? routeSelection.home?.id
      : managedSelectedId;
    if (node && currentId === game.id) {
      node.style.backgroundImage = `url('${convertFileSrc(artwork.hero)}')`;
    }
  } catch (_error) {
    // The CDN background stays when the local cache is unavailable.
  }
}

function coverCard(game, section, caption = "") {
  const appId = game.metadata_app_id;
  // Home rails pass their rail name as section; the Library grid passes none.
  // Each route reads only its own selection.
  const selection = section ? routeSelection.home : routeSelection.library;
  const selected = Boolean(selection
    && selection.id === game.id
    && (!section || selection.section === section));
  const active = selected ? " active" : "";
  return `<button class="cover-card${active}" data-managed="${escape(game.id)}" title="${escape(game.title)}">
    <span class="cover">${appId ? `<img loading="lazy" data-cover-game="${escape(game.id)}" alt="">` : ""}<span class="play-glyph" aria-hidden="true">▶</span></span>
    <span class="cover-title">${escape(game.title)}</span>${caption ? `<span class="cover-state">${escape(caption)}</span>` : ""}
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
  if (!(await confirmDialog({ title: "Play game", message: `Play ${game.title}?`, confirmLabel: "Play" }))) return;
  const stop = button ? busy(button, "Starting…") : () => {};
  try {
    const result = await invoke("play_managed", { gameId: game.id });
    runningManaged.add(game.id);
    runningGrace.set(game.id, Date.now() + 20000);
    showToast(`Started ${game.title} · ${result.log}`);
    managedLibrary = await invoke("library_snapshot").catch(() => managedLibrary);
    renderManagedMode();
  } catch (error) {
    showToast(String(error), true);
    stop();
  }
}

async function stopManagedGame(game, button) {
  if (!(await confirmDialog({ title: "Stop game", message: `Stop ${game.title}? Unsaved game progress may be lost.`, confirmLabel: "Stop", danger: true }))) return;
  const stop = button ? busy(button, "Stopping…") : () => {};
  try {
    await invoke("stop_managed", { gameId: game.id });
    runningGrace.delete(game.id);
    runningManaged.delete(game.id);
    showToast(`Stopped ${game.title}`);
    renderManagedMode();
  } catch (error) {
    showToast(String(error), true);
    stop();
  }
}

function linkFileName(path) {
  const parts = String(path || "").split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

function parseLauncherArguments(text) {
  const matches = String(text || "").match(/"[^"]*"|'[^']*'|\S+/g) || [];
  return matches.map((value) => value.replace(/^["']|["']$/g, ""));
}

function renderManagedLink(pane, game, refreshGame, renderTab) {
  const linked = Boolean(game.installed && game.installation?.root);
  const draft = {
    root: linked ? (game.installation?.root || "") : "",
    launchers: (game.launch_targets || []).map((target) => ({
      name: target.name || "",
      executable: target.executable,
      arguments: [...(target.arguments || [])],
      primary: Boolean(target.primary),
    })),
  };
  if (draft.launchers.length && !draft.launchers.some((launcher) => launcher.primary)) {
    draft.launchers[0].primary = true;
  }
  pane.innerHTML = `<h3>Game Link</h3>
    <p class="pane-note">${linked
      ? "Correct the game folder or launcher if the wrong files were linked. This never moves, copies, or deletes game files. Choosing a different folder keeps the former managed folder untouched. Linked games are read-only references: downloads, updates and DLC changes are disabled."
      : "Link an installed game folder and every executable you want Play to offer. This never moves, copies, or deletes the selected game files."}</p>
    <label for="gs-link-root">Linked game folder</label>
    <div class="game-link-folder-row">
      <input id="gs-link-root" value="${escape(draft.root)}" placeholder="/path/to/installed/game" readonly>
      <button id="gs-link-choose">Browse…</button>
    </div>
    <h3 class="game-link-section-title">Launchers</h3>
    <p class="pane-note">This list is the only source of truth for what Play starts. The default launcher starts when there is one entry; otherwise Play asks which launcher to start. Collections such as Uncharted 4 + The Lost Legacy share one folder with separate executables; add each one here. Each launcher can have its own arguments, added after the shared options in General.</p>
    ${linked ? "" : '<p class="pane-note">Choose the Proton compatibility layer in the Compatibility tab. The current selection is used when this game is linked.</p>'}
    <div id="gs-link-launchers"></div>
    <div class="pane-actions"><button id="gs-link-add">Add launcher…</button></div>
    <p class="pane-note" id="gs-link-status">A game with at least one launcher is linked automatically.</p>`;
  const rootInput = pane.querySelector("#gs-link-root");
  const list = pane.querySelector("#gs-link-launchers");
  let savingLink = false;

  async function persistLink() {
    const root = rootInput.value.trim();
    if (!root || !draft.launchers.length || savingLink) return false;
    savingLink = true;
    try {
      const result = await invoke("link_managed_game", { request: {
        game_id: game.id,
        root,
        launchers: draft.launchers.map((launcher) => ({
          name: launcher.name,
          executable: launcher.executable,
          arguments: launcher.arguments,
          primary: launcher.primary,
        })),
      } });
      managedLibrary = result.library;
      await refreshGame();
      const status = pane.querySelector("#gs-link-status");
      if (status) {
        status.textContent = `Linked automatically · ${result.report.launchers} launcher${result.report.launchers === 1 ? "" : "s"}`;
      }
      return true;
    } catch (error) {
      showToast(String(error), true);
      return false;
    } finally {
      savingLink = false;
    }
  }

  const renderLaunchers = () => {
    list.innerHTML = draft.launchers.length
      ? draft.launchers.map((launcher, index) => `<div class="game-link-row" data-launcher="${index}">
          <div class="game-link-copy">
            <strong>${escape(launcher.name || linkFileName(launcher.executable))}</strong>
            <span class="muted" title="${escape(launcher.executable)}">${escape(launcher.executable)}</span>
            <span class="muted">${launcher.arguments.length ? `Arguments: ${escape(launcher.arguments.join(" "))}` : "No launcher-specific arguments"}</span>
          </div>
          <div class="game-link-actions">
            <button data-link-primary${launcher.primary ? " disabled" : ""}>Default</button>
            <button data-link-edit>Edit</button>
            <button class="danger" data-link-delete>Delete</button>
          </div>
        </div>`).join("")
      : '<p class="muted">Add at least one launcher executable.</p>';
    for (const row of list.querySelectorAll("[data-launcher]")) {
      const index = Number(row.dataset.launcher);
      row.querySelector("[data-link-primary]").onclick = () => {
        draft.launchers.forEach((launcher, position) => { launcher.primary = position === index; });
        renderLaunchers();
        void persistLink();
      };
      row.querySelector("[data-link-edit]").onclick = () => editLauncher(index);
      row.querySelector("[data-link-delete]").onclick = async () => {
        const launcher = draft.launchers[index];
        if (draft.launchers.length <= 1) {
          showToast("A linked game needs at least one launcher.", true);
          return;
        }
        if (!(await confirmDialog({
          title: "Remove launcher",
          message: `Remove "${launcher.name || linkFileName(launcher.executable)}" from this game? The game files are not deleted.`,
          confirmLabel: "Remove",
          danger: true,
        }))) return;
        draft.launchers.splice(index, 1);
        if (!draft.launchers.some((entry) => entry.primary)) {
          draft.launchers[0].primary = true;
        }
        renderLaunchers();
        void persistLink();
      };
    }
  };

  const editLauncher = (index) => {
    const existing = index >= 0 ? draft.launchers[index] : null;
    const executable = existing?.executable || "";
    const backdrop = document.createElement("div");
    backdrop.className = "modal-backdrop";
    backdrop.innerHTML = `<div class="modal"><div class="modal-head"><h2>${existing ? "Edit launcher" : "Add launcher"}</h2><button class="modal-close" aria-label="Close">×</button></div>
      <p class="muted">${escape(linkFileName(executable))}</p>
      <label for="gs-launcher-executable">Executable</label>
      <input id="gs-launcher-executable" value="${escape(executable)}" readonly>
      <label for="gs-launcher-name">Launcher name</label>
      <input id="gs-launcher-name" value="${escape(existing?.name || "")}" placeholder="${escape(linkFileName(executable))}">
      <p class="muted">Leave this empty to show the launcher's file name.</p>
      <label for="gs-launcher-arguments">Launcher arguments</label>
      <input id="gs-launcher-arguments" value="${escape((existing?.arguments || []).join(" "))}" placeholder="Optional, for example -nolauncher">
      <p class="muted">Passed directly to this launcher. Shell commands are not evaluated.</p>
      <div class="modal-actions"><button id="gs-launcher-cancel">Cancel</button><button class="primary" id="gs-launcher-save">Save</button></div></div>`;
    document.body.appendChild(backdrop);
    registerDialog(backdrop);
    const close = () => backdrop.remove();
    backdrop.querySelector(".modal-close").onclick = close;
    backdrop.querySelector("#gs-launcher-cancel").onclick = close;
    backdrop.querySelector("#gs-launcher-save").onclick = () => {
      const value = {
        name: backdrop.querySelector("#gs-launcher-name").value.trim(),
        executable,
        arguments: parseLauncherArguments(backdrop.querySelector("#gs-launcher-arguments").value),
        primary: existing?.primary || draft.launchers.length === 0,
      };
      if (existing) draft.launchers[index] = value;
      else draft.launchers.push(value);
      close();
      renderLaunchers();
      void persistLink();
    };
    backdrop.querySelector("#gs-launcher-name").focus();
  };

  rootInput.addEventListener("change", () => {
    if (draft.launchers.length) void persistLink();
  });
  pane.querySelector("#gs-link-choose").onclick = async () => {
    try {
      const path = await invoke("pick_managed_folder", { gameId: game.id });
      if (path) {
        rootInput.value = path;
        if (draft.launchers.length) void persistLink();
      }
    } catch (error) { showToast(String(error), true); }
  };
  pane.querySelector("#gs-link-add").onclick = async () => {
    const root = rootInput.value.trim();
    if (!root) { showToast("Choose the linked game folder first.", true); return; }
    try {
      const executable = await invoke("pick_managed_executable", { gameId: game.id, start: root });
      if (!executable) return;
      const normalizedRoot = root.replace(/\/+$/, "");
      if (!executable.startsWith(normalizedRoot + "/") && !executable.startsWith(normalizedRoot + "\\")) {
        showToast("Choose a launcher inside the linked game folder.", true);
        return;
      }
      draft.launchers.push({ name: "", executable, arguments: [], primary: draft.launchers.length === 0 });
      renderLaunchers();
      void persistLink();
    } catch (error) { showToast(String(error), true); }
  };
  renderLaunchers();
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

async function updateManagedDlcAction(game) {
  const button = detailEl.querySelector("#managed-play");
  if (!button) return;
  try {
    const state = await invoke("managed_dlc_state", { gameId: game.id });
    const installed = new Set(state.installed || []);
    const pending = (state.selected || []).some((id) => !installed.has(id));
    const current = detailEl.querySelector("#managed-play");
    if (current) {
      if (pending) {
        current.textContent = "Update";
        current.onclick = () => startManagedDlcUpdate(game);
      } else {
        current.textContent = "Play";
        current.onclick = () => startManagedGame(game, current);
      }
    }
  } catch (_error) { /* DLC state is optional */ }
}

async function startManagedDlcUpdate(game) {
  managedDownloads.set(game.id, {
    gameId: game.id,
    status: "starting",
    percent: 0,
    completed: 0,
    total: 0,
    speed: 0,
    message: "Downloading DLC…",
  });
  openDownloadManager(game);
  try {
    await invoke("start_managed_dlc_download", { gameId: game.id });
    watchManagedDownload(game.id);
  } catch (error) {
    managedDownloads.delete(game.id);
    if (appView === "game" && managedSelectedId === game.id) renderManagedMode();
    showToast(String(error), true);
  }
}

function renderManagedPreview(game) {
  const appId = game.metadata_app_id;
  const target = game.launch_targets?.find((item) => item.primary) || game.launch_targets?.[0];
  const canPlay = managedReady(game) && Boolean(target);
  const requiresSteam = Boolean(game.launch?.requires_steam);
  const running = runningManaged.has(game.id);
  const seed = gameDetailSeed;
  const provisionalHero = seed?.images?.hero || seed?.images?.portrait || (appId ? heroUrl(appId) : "");
  const provisionalCover = seed?.images?.portrait || (appId ? coverUrl(appId) : "");
  const provisionalMeta = seed
    ? [seed.release_date, (seed.genres || []).map((genre) => genre.name).join(", ")].filter(Boolean).join(" · ")
    : (appId ? `Steam App ${appId}` : "Managed entry");
  const provisionalCredits = seed ? [seed.developers, seed.publishers].filter(Boolean).join(" · ") : "";
  const installed = managedReady(game);
  const download = managedDownloads.get(game.id);
  const downloading = Boolean(download && !downloadTerminal(download));
  const action = running
    ? '<button id="managed-stop" class="danger">Stop</button>'
    : downloading
      ? `<button id="managed-downloading" class="primary">${download.status === "paused" ? "Paused" : "Downloading"}</button>`
      : !installed
        ? '<button id="managed-download" class="primary">Download</button>'
        : `<button id="managed-play" class="primary"${canPlay ? "" : " disabled"}>Play</button>`;
  detailEl.innerHTML = `<div class="page game-page game-detail">
    <div class="game-page-top"><button id="managed-back" class="secondary">← ${gameDetailOrigin === "store" ? "Store" : gameDetailOrigin === "home" ? "Home" : "Library"}</button></div>
    <section class="game-hero${provisionalHero ? "" : " no-artwork"}" id="game-hero"${provisionalHero ? ` style="background-image:url('${escape(provisionalHero)}')"` : ""}>
      <div class="game-hero-shade"></div>
      <div class="game-hero-card">
        <img class="game-cover" id="game-cover" alt="" loading="lazy"${provisionalCover ? ` src="${escape(provisionalCover)}"` : ""}>
        <div class="game-info">
          <h1>${escape(game.title)}</h1>
          <p class="game-meta" id="game-meta">${escape(provisionalMeta || (appId ? `Steam App ${appId}` : "Managed entry"))}</p>
          <p class="game-description${seed?.description ? "" : " hidden"}" id="game-description">${seed?.description ? escape(seed.description) : ""}</p>
          <p class="game-credits" id="game-credits">${escape(provisionalCredits)}</p>
          <div class="game-actions">${action}<button id="managed-remove" class="danger">Remove from library</button><button id="managed-options" class="game-icon-button" title="${document.body.classList.contains("fullscreen-mode") ? "Game settings are available in desktop mode" : "Game options"}" aria-label="Game options"${document.body.classList.contains("fullscreen-mode") ? " disabled" : ""}><svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><circle cx="12" cy="12" r="3"></circle><path d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1.03 1.56V21a2 2 0 1 1-4 0v-.09a1.7 1.7 0 0 0-1.11-1.56 1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.7 1.7 0 0 0 .34-1.87 1.7 1.7 0 0 0-1.56-1.03H3a2 2 0 1 1 0-4h.09a1.7 1.7 0 0 0 1.56-1.11 1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.87.34H9a1.7 1.7 0 0 0 1.03-1.56V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1.03 1.56 1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.34 1.87V9c.23.63.83 1.03 1.56 1.03H21a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.56 1.03z"></path></svg></button><span class="game-size" id="game-size">Size: …</span></div>
          ${downloading ? '<p class="muted game-note">Downloading game files…</p>' : (!installed ? '<p class="muted game-note">Game files are not downloaded yet.</p>' : (!target ? '<p class="muted game-note">Choose the game executable in Game options to enable Play.</p>' : requiresSteam ? '<p class="muted game-note">Steam must be running in the background for Steamworks. The game still launches directly through Proton.</p>' : ""))}
          <div class="download-inline hidden" id="managed-download-inline">
            <div class="download-inline-track"><span></span></div>
            <div class="download-inline-meta"><span class="download-inline-meta-text"></span></div>
          </div>
          <div class="game-chips" id="game-chips"></div>
        </div>
      </div>
    </section>
  </div>`;
  detailEl.querySelector("#managed-back").onclick = () => setView(gameDetailOrigin, { focus: "restore" });
  const downloadButton = detailEl.querySelector("#managed-download");
  if (downloadButton) downloadButton.onclick = () => startManagedDownload(game);
  const downloadingButton = detailEl.querySelector("#managed-downloading");
  if (downloadingButton) downloadingButton.onclick = () => openDownloadManager(game);
  if (downloading) {
    applyInlineDownload(game.id);
    watchManagedDownload(game.id);
  }
  else if (!managedDownloads.has(game.id) && !downloadStatusChecked.has(game.id)) {
    downloadStatusChecked.add(game.id);
    void invoke("managed_download_status", { gameId: game.id }).then((state) => {
      if (state && !downloadTerminal(state)) {
        managedDownloads.set(game.id, state);
        if (appView === "game" && managedSelectedId === game.id) renderManagedMode();
      }
    }).catch(() => {});
  }
  detailEl.querySelector("#managed-options").onclick = () => openManagedGameOptions(game);
  const remove = detailEl.querySelector("#managed-remove");
  if (remove) {
    remove.onclick = async () => {
      if (runningManaged.has(game.id)) { showToast("Stop the game before removing it.", true); return; }
      if (!(await confirmDialog({
        title: "Remove from library",
        message: `Remove ${game.title} from your library?\n\nThe Steam registration is removed. Game files stay on disk.`,
        confirmLabel: "Remove",
        danger: true,
      }))) return;
      const stop = busy(remove, "Removing…");
      try {
        await invoke("remove_managed_game", { gameId: game.id });
        managedLibrary = await invoke("library_snapshot").catch(() => managedLibrary);
        showToast(`Removed ${game.title} from your library`);
        setView(gameDetailOrigin === "store" ? "store" : gameDetailOrigin === "home" ? "home" : "library");
      } catch (error) {
        stop();
        showToast(String(error), true);
      }
    };
  }
  void hydrateGameDetail(game);
  const play = detailEl.querySelector("#managed-play");
  if (play && canPlay) play.onclick = () => startManagedGame(game, play);
  if (installed) void updateManagedDlcAction(game);
  const stopButton = detailEl.querySelector("#managed-stop");
  if (stopButton) stopButton.onclick = () => stopManagedGame(game, stopButton);
}

const managedDownloads = new Map();
const downloadStatusChecked = new Set();

function downloadTerminal(state) {
  return ["done", "error", "cancelled"].includes(state?.status);
}

function downloadSpeed(state) {
  return Number(state?.averageSpeed ?? state?.average_speed ?? state?.speed) || 0;
}

function downloadEta(state) {
  const total = Number(state?.total) || 0;
  const completed = Number(state?.completed) || 0;
  const speed = downloadSpeed(state);
  if (!total || total <= completed || speed <= 0) return "";
  const seconds = Math.ceil((total - completed) / speed);
  if (seconds >= 3600) return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`;
  return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
}

function downloadSummary(state) {
  const percent = Math.max(0, Math.min(100, Number(state.percent) || 0));
  if (!state.total) {
    const parts = [];
    if (state.percent > 0) parts.push(`${percent.toFixed(0)}%`);
    if (state.completed > 0) parts.push(`${formatBytes(state.completed)} downloaded`);
    if (state.message) parts.push(state.message);
    return parts.join(" · ") || "Preparing download…";
  }
  const parts = [
    `${percent.toFixed(1)}%`,
    `${formatBytes(state.completed || 0)} of ${formatBytes(state.total || 0)}`,
  ];
  const speed = downloadSpeed(state);
  if (speed > 0) parts.push(`${formatBytes(speed)}/s`);
  const eta = downloadEta(state);
  if (eta) parts.push(`ETA ${eta}`);
  if ((Number(state.completed) || 0) === 0 && state.message) parts.push(state.message);
  return parts.join(" · ");
}

function applyInlineDownload(gameId) {
  const state = managedDownloads.get(gameId);
  const inline = document.querySelector("#managed-download-inline");
  if (!inline) return;
  if (!state || downloadTerminal(state)) {
    inline.classList.add("hidden");
    return;
  }
  inline.classList.remove("hidden");
  const percent = Math.max(0, Math.min(100, Number(state.percent) || 0));
  const bar = inline.querySelector(".download-inline-track span");
  const meta = inline.querySelector(".download-inline-meta-text");
  if (bar) bar.style.width = `${percent}%`;
  if (meta) meta.textContent = downloadSummary(state);
}

async function chooseDownloadDrive(game) {
  const backdrop = document.createElement("div");
  backdrop.className = "modal-backdrop";
  backdrop.innerHTML = `<div class="modal download-drive-modal"><div class="modal-head"><h2>Destination library drive</h2><button class="modal-close" id="download-drive-close" aria-label="Close">×</button></div>
    <p class="pane-note">Choose where SteamTron installs the game content.</p>
    <select id="download-drive-select" aria-label="Destination drive"><option>Loading drives…</option></select>
    <p class="download-drive-meta" id="download-drive-meta">Checking download size…</p>
    <div class="modal-actions"><button id="download-drive-cancel">Cancel</button><button class="primary" id="download-drive-start" disabled>Download</button></div></div>`;
  document.body.appendChild(backdrop);
  registerDialog(backdrop);
  const close = () => backdrop.remove();
  const select = backdrop.querySelector("#download-drive-select");
  const meta = backdrop.querySelector("#download-drive-meta");
  const startButton = backdrop.querySelector("#download-drive-start");
  backdrop.querySelector("#download-drive-close").onclick = close;
  backdrop.querySelector("#download-drive-cancel").onclick = close;
  let drives = [];
  try {
    const snapshot = await invoke("library_snapshot");
    drives = snapshot?.drives || [];
  } catch (error) {
    meta.textContent = String(error);
    meta.classList.add("error");
    return;
  }
  if (!drives.length) {
    select.innerHTML = '<option value="">No drives registered</option>';
    meta.textContent = "Add a drive in Settings → Storage first.";
    return;
  }
  const currentDrive = drives.find((drive) => {
    const root = game.installation?.root || "";
    return root === drive.root || root.startsWith(`${drive.root.replace(/\/+$/, "")}/`);
  });
  select.innerHTML = drives.map((drive) => {
    return `<option value="${escape(drive.id)}"${drive === (currentDrive || drives.find((entry) => entry.default) || drives[0]) ? " selected" : ""}>${escape(drive.label || drive.id)} · ${escape(drive.root)}</option>`;
  }).join("");
  startButton.disabled = false;
  meta.textContent = "Checking download size…";
  if (game.metadata_app_id) {
    void invoke("steam_download_size", { appid: game.metadata_app_id }).then((size) => {
      meta.textContent = size > 0 ? `Download size: ${formatBytes(size)}` : "Download size: unknown";
    }).catch(() => { meta.textContent = "Download size: unknown"; });
  } else {
    meta.textContent = "Download size: unknown";
  }
  void invoke("available_library_drives").catch(() => []).then((available) => {
    const freeByPath = new Map((available || []).map((drive) => [drive.path, drive]));
    for (const option of select.options) {
      const drive = drives.find((entry) => entry.id === option.value);
      const disk = drive ? freeByPath.get(drive.root) : null;
      if (!drive || !disk) continue;
      option.textContent = `${drive.label || drive.id} · ${drive.root} · ${formatBytes(disk.free)} free`;
    }
  });
  startButton.onclick = async () => {
    const driveId = select.value;
    if (!driveId) return;
    const stop = busy(startButton, "Preparing…");
    try {
      const snapshot = await invoke("prepare_managed_download", { gameId: game.id, driveId });
      managedLibrary = snapshot;
      managedDownloads.set(game.id, { gameId: game.id, status: "starting", percent: 0, completed: 0, total: 0, speed: 0, message: "Contacting depot sources…" });
      close();
      renderManagedMode();
      await invoke("start_managed_download", { gameId: game.id });
      watchManagedDownload(game.id);
      openDownloadManager(game);
    } catch (error) {
      stop();
      managedDownloads.delete(game.id);
      renderManagedMode();
      showToast(String(error), true);
    }
  };
}

function startManagedDownload(game) {
  void chooseDownloadDrive(game);
}

const downloadWatchTimers = new Map();

function stopDownloadWatch(gameId) {
  const timer = downloadWatchTimers.get(gameId);
  if (timer) clearInterval(timer);
  downloadWatchTimers.delete(gameId);
}

function watchManagedDownload(gameId) {
  if (downloadWatchTimers.has(gameId)) return;
  const timer = setInterval(async () => {
    let state = null;
    try { state = await invoke("managed_download_status", { gameId }); }
    catch {}
    if (state && !downloadTerminal(state)) {
      managedDownloads.set(gameId, state);
      if (appView === "game" && managedSelectedId === gameId) applyInlineDownload(gameId);
      return;
    }
    stopDownloadWatch(gameId);
    if (state) managedDownloads.set(gameId, state);
    else managedDownloads.delete(gameId);
    if (appView === "game" && managedSelectedId === gameId) renderManagedMode();
  }, 2000);
  downloadWatchTimers.set(gameId, timer);
}

function openDownloadManager(game) {
  if (document.querySelector("#download-manager")) return;
  const backdrop = document.createElement("div");
  backdrop.className = "modal-backdrop";
  backdrop.innerHTML = `<div class="modal download-modal" id="download-manager"><div class="modal-head"><h2>Downloading · ${escape(game.title)}</h2><button class="modal-close" aria-label="Close">×</button></div>
    <p class="download-stage" id="download-manager-stage">Preparing download…</p>
    <div class="download-progress-row">
      <div class="download-bar"><span id="download-manager-fill"></span></div>
      <strong class="operation-percent" id="download-manager-percent">—</strong>
      <div class="download-actions-inline" id="download-manager-actions">
        <button class="game-icon-button" id="download-manager-pause" title="Pause" aria-label="Pause"><svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M9 5v14M15 5v14"></path></svg></button>
        <button class="game-icon-button download-cancel-icon" id="download-manager-cancel" title="Cancel" aria-label="Cancel"><svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"></path></svg></button>
      </div>
    </div>
    <div class="download-stats">
      <div class="download-stat"><span>Downloaded</span><strong id="download-manager-completed">—</strong></div>
      <div class="download-stat"><span>Total size</span><strong id="download-manager-total">—</strong></div>
      <div class="download-stat"><span>Average speed</span><strong id="download-manager-speed">—</strong></div>
      <div class="download-stat"><span>ETA</span><strong id="download-manager-eta">—</strong></div>
      <div class="download-stat"><span>Time spent</span><strong id="download-manager-elapsed">—</strong></div>
    </div>
    <section class="download-next hidden" id="download-manager-next">
      <div class="download-next-head"><h3>Content groups</h3><span id="download-manager-next-count"></span></div>
      <div class="download-next-list" id="download-manager-depots-list"></div>
    </section>
    </div>`;
  document.body.appendChild(backdrop);
  registerDialog(backdrop, { title: `Downloading ${game.title}` });
  const fill = backdrop.querySelector("#download-manager-fill");
  const stage = backdrop.querySelector("#download-manager-stage");
  const percentNode = backdrop.querySelector("#download-manager-percent");
  const completedNode = backdrop.querySelector("#download-manager-completed");
  const totalNode = backdrop.querySelector("#download-manager-total");
  const speedNode = backdrop.querySelector("#download-manager-speed");
  const etaNode = backdrop.querySelector("#download-manager-eta");
  const elapsedNode = backdrop.querySelector("#download-manager-elapsed");
  const nextSection = backdrop.querySelector("#download-manager-next");
  const nextCount = backdrop.querySelector("#download-manager-next-count");
  const depotList = backdrop.querySelector("#download-manager-depots-list");
  const pauseButton = backdrop.querySelector("#download-manager-pause");
  const cancelButton = backdrop.querySelector("#download-manager-cancel");
  const actionsRow = backdrop.querySelector("#download-manager-actions");
  let timer = null;
  let closed = false;
  let terminal = false;
  const close = () => {
    if (closed) return;
    closed = true;
    if (timer) clearInterval(timer);
    timer = null;
    if (document.body.contains(backdrop)) backdrop.remove();
    if (appView === "game" && managedSelectedId === game.id) renderManagedMode();
  };
  const render = (state) => {
    if (!state) return;
    const percent = Math.max(0, Math.min(100, Number(state.percent) || 0));
    const total = Number(state.total) || 0;
    const completed = Number(state.completed) || 0;
    const speed = downloadSpeed(state);
    const eta = downloadEta(state);
    terminal = downloadTerminal(state);
    fill.style.width = `${terminal && state.status === "done" ? 100 : percent}%`;
    percentNode.textContent = total ? `${percent.toFixed(1)}%` : `${percent.toFixed(1)}%`;
    completedNode.textContent = completed ? formatBytes(completed) : "—";
    totalNode.textContent = total ? formatBytes(total) : "Calculating…";
    speedNode.textContent = speed > 0 ? `${formatBytes(speed)}/s` : "—";
    etaNode.textContent = eta || "—";
    elapsedNode.textContent = formatSeconds(state.elapsedSeconds || 0);
    const stalled = Number(state.stalledSeconds) || 0;
    const stalledNow = state.status === "downloading" && stalled >= 20;
    stage.textContent = stalledNow
      ? `No progress for ${stalled}s · ${state.message || "Waiting for the Steam CDN"}`
      : state.message || (state.status === "paused" ? "Download paused" : "Preparing download…");
    stage.classList.toggle("error", state.status === "error");
    stage.classList.toggle("warning", stalledNow);
    const depots = Array.isArray(state.depots) ? state.depots : [];
    nextSection.classList.toggle("hidden", depots.length === 0);
    if (depots.length) {
      const finished = depots.filter((depot) => depot.status === "done").length;
      nextCount.textContent = `${finished} / ${depots.length} complete`;
      depotList.innerHTML = depots.map((depot) => {
        const percent = Math.max(0, Math.min(100, Number(depot.percent) || 0));
        const label = depot.status === "done" ? "Done" : depot.status === "downloading" ? "Downloading" : "Waiting";
        const size = Number(depot.total) > 0
          ? `${formatBytes(Number(depot.completed) || 0)} / ${formatBytes(Number(depot.total))}`
          : Number(depot.completed) > 0 ? formatBytes(Number(depot.completed)) : "";
        return `<div class="download-depot-row ${escape(depot.status || "pending")}">
          <div class="download-depot-head"><span>Depot ${escape(depot.depotId)}</span><small>${label}</small></div>
          <div class="download-depot-track"><span style="width:${percent}%"></span></div>
          <div class="download-depot-meta"><span>${percent.toFixed(1)}%</span><span>${size}</span></div>
        </div>`;
      }).join("");
    }
    actionsRow.classList.toggle("hidden", terminal);
    if (!terminal) {
      const paused = state.status === "paused";
      const label = paused ? "Resume" : "Pause";
      pauseButton.innerHTML = paused
        ? '<svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M8 5l11 7-11 7z"></path></svg>'
        : '<svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M9 5v14M15 5v14"></path></svg>';
      pauseButton.title = label;
      pauseButton.setAttribute("aria-label", label);
      pauseButton.disabled = !["downloading", "paused"].includes(state.status);
      cancelButton.disabled = false;
    }
  };
  const settle = (state) => {
    stopDownloadWatch(game.id);
    if (timer) clearInterval(timer);
    timer = null;
    if (state) managedDownloads.set(game.id, state);
    else managedDownloads.delete(game.id);
    if (appView === "game" && managedSelectedId === game.id) renderManagedMode();
  };
  const refresh = async () => {
    if (closed) return;
    let state = null;
    try { state = await invoke("managed_download_status", { gameId: game.id }); }
    catch {}
    if (closed) return;
    if (!state) {
      render({ ...(managedDownloads.get(game.id) || {}), status: "error", message: "The download is no longer running." });
      settle(null);
      return;
    }
    managedDownloads.set(game.id, state);
    render(state);
    if (downloadTerminal(state)) {
      settle(state);
      if (state.status === "done") {
        managedLibrary = await invoke("library_snapshot").catch(() => managedLibrary);
        if (appView === "game" && managedSelectedId === game.id) renderManagedMode();
        showToast("Download complete");
      } else {
        showToast(state.message || (state.status === "cancelled" ? "Download cancelled" : "Download failed"), state.status === "error");
      }
    }
  };
  backdrop.querySelector(".modal-close").onclick = close;
  pauseButton.onclick = async () => {
    const state = managedDownloads.get(game.id);
    const stop = busy(pauseButton, state?.status === "paused" ? "Resuming…" : "Pausing…");
    try {
      await invoke(state?.status === "paused" ? "resume_managed_download" : "pause_managed_download", { gameId: game.id });
      stop();
      await refresh();
    } catch (error) { stop(); showToast(String(error), true); }
  };
  cancelButton.onclick = async () => {
    if (!(await confirmDialog({
      title: "Cancel download",
      message: `Cancel the download of ${game.title}? Partial game files are removed.`,
      confirmLabel: "Cancel download",
      cancelLabel: "Keep downloading",
      danger: true,
    }))) return;
    const stop = busy(cancelButton, "Cancelling…");
    try {
      await invoke("cancel_managed_download", { gameId: game.id });
      stop();
      await refresh();
    } catch (error) { stop(); showToast(String(error), true); }
  };
  render(managedDownloads.get(game.id));
  timer = setInterval(refresh, 800);
  void refresh();
  focusControl(pauseButton);
}


const OPERATION_LABELS = {
  backup: ["Backing up game files…", "Back up game files…"],
  verify: ["Verifying files integrity…", "Verify files integrity"],
  restore: ["Restoring game backup…", "Restore…"],
  "library-export": ["Exporting game backups…", "Export"],
};
const operationInlineTimers = new WeakMap();
const inlineOperations = new Set();
const cancelledInline = new Set();

function operationInlineNodes(operation, gameId = "") {
  return [...document.querySelectorAll(`[data-operation-inline="${operation}"]`)].filter((node) => {
    const scoped = node.dataset.operationGame || "";
    return !scoped || !gameId || scoped === gameId;
  });
}

function setOperationButton(operation, running) {
  const button = document.querySelector(`[data-operation-button="${operation}"]`);
  if (!button) return;
  const labels = OPERATION_LABELS[operation];
  if (!labels) return;
  button.textContent = running ? labels[0] : labels[1];
  button.dataset.operationRunning = running ? "1" : "0";
}

function setOperationInline(operation, gameId, message, status, percent) {
  for (const node of operationInlineNodes(operation, gameId)) {
    const timer = operationInlineTimers.get(node);
    if (timer) clearTimeout(timer);
    const track = node.querySelector(".download-inline-track");
    const text = node.querySelector(".operation-inline-text");
    node.classList.remove("hidden", "error");
    const bar = track?.querySelector("span");
    if (status === "done") {
      if (bar) bar.style.width = "100%";
      if (text) text.textContent = `100% · ${message || "Complete"}`;
      operationInlineTimers.set(node, setTimeout(() => node.classList.add("hidden"), 3500));
      setOperationButton(operation, false);
    } else if (status === "cancelled") {
      if (text) text.textContent = message || "Cancelled";
      operationInlineTimers.set(node, setTimeout(() => node.classList.add("hidden"), 3500));
      setOperationButton(operation, false);
    } else if (status === "paused") {
      if (text) text.textContent = message || "Paused";
      setOperationButton(operation, true);
    } else if (status === "error") {
      node.classList.add("error");
      if (text) text.textContent = message || "Failed";
      setOperationButton(operation, false);
    } else {
      const current = Math.max(0, Math.min(100, Number(percent) || 0));
      if (bar) bar.style.width = `${current}%`;
      if (text) text.textContent = `${current.toFixed(0)}% · ${message || "Starting…"}`;
      setOperationButton(operation, true);
    }
  }
}

function beginOperation(operation, gameId) {
  setOperationInline(operation, gameId, "Starting…", "running", 0);
  setOperationButton(operation, true);
}

function notifyTaskResult(operation, gameId, success, message) {
  void invoke("notify_task_result", { operation, gameId, success, message: String(message || "") }).catch(() => {});
}

function finishOperation(operation, gameId, message) {
  setOperationInline(operation, gameId, message || "Complete", "done");
  setOperationButton(operation, false);
  notifyTaskResult(operation, gameId, true, message || "Complete");
}

function failOperation(operation, gameId, message) {
  setOperationInline(operation, gameId, String(message), "error");
  setOperationButton(operation, false);
  notifyTaskResult(operation, gameId, false, String(message));
}

function cancelOperation(operation, gameId) {
  for (const node of operationInlineNodes(operation, gameId)) {
    node.classList.add("hidden");
    node.classList.remove("indeterminate");
  }
  setOperationButton(operation, false);
}

const activeOperations = new Map();

function ensureOperationManager(operation, gameId, options) {
  const key = `${operation}:${gameId || ""}`;
  const existing = activeOperations.get(key);
  if (existing && !existing.isTerminal()) {
    existing.show();
    return null;
  }
  const manager = openOperationManager(options);
  activeOperations.set(key, manager);
  return manager;
}

function formatSeconds(seconds) {
  const value = Math.max(0, Math.floor(Number(seconds) || 0));
  if (value >= 3600) return `${Math.floor(value / 3600)}h ${Math.floor((value % 3600) / 60)}m`;
  return `${Math.floor(value / 60)}m ${value % 60}s`;
}

function openOperationManager(options = {}) {
  const operation = options.operation || "operation";
  const title = options.title || "Working";
  const rich = operation === "backup" || operation === "restore";
  const backdrop = document.createElement("div");
  backdrop.className = "modal-backdrop";
  backdrop.innerHTML = `<div class="modal download-modal operation-modal" id="operation-manager"><div class="modal-head"><h2>${escape(title)}</h2><button class="modal-close" aria-label="Close">×</button></div>
    <div class="download-progress-row"><div class="download-bar operation-bar"><span></span></div><strong class="operation-percent" id="operation-percent">0%</strong>
      <div class="download-actions-inline" id="operation-actions">
        <button class="game-icon-button download-cancel-icon" id="operation-cancel" title="Cancel" aria-label="Cancel"><svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"></path></svg></button>
      </div>
    </div>
    <p class="download-stage operation-activity" id="operation-stage">Starting…</p>
    ${rich ? `<div class="download-stats operation-stats">
      <div class="download-stat"><span>Transferred</span><strong id="operation-completed">—</strong></div>
      <div class="download-stat"><span>Total size</span><strong id="operation-total">—</strong></div>
      <div class="download-stat"><span>Transfer speed</span><strong id="operation-speed">—</strong></div>
      <div class="download-stat"><span>ETA</span><strong id="operation-eta">—</strong></div>
      <div class="download-stat"><span>Time spent</span><strong id="operation-elapsed">00:00</strong></div>
    </div>` : ""}
    </div>`;
  document.body.appendChild(backdrop);
  registerDialog(backdrop, { title });
  const stage = backdrop.querySelector("#operation-stage");
  const fill = backdrop.querySelector(".operation-bar span");
  const percentNode = backdrop.querySelector("#operation-percent");
  const completedNode = backdrop.querySelector("#operation-completed");
  const totalNode = backdrop.querySelector("#operation-total");
  const speedNode = backdrop.querySelector("#operation-speed");
  const etaNode = backdrop.querySelector("#operation-eta");
  const elapsedNode = backdrop.querySelector("#operation-elapsed");
  const cancelButton = backdrop.querySelector("#operation-cancel");
  const actions = backdrop.querySelector("#operation-actions");
  const closeButton = backdrop.querySelector(".modal-close");
  let unlisten = null;
  let resolveReady;
  const ready = new Promise((resolve) => { resolveReady = resolve; });
  let closed = false;
  let terminal = false;
  let cancelled = false;
  let currentPercent = 0;
  let completed = 0;
  let total = 0;
  let speed = 0;
  let elapsed = 0;
  const startedAt = Date.now();
  const elapsedTimer = rich ? setInterval(() => {
    if (!terminal && elapsedNode && elapsed === 0) elapsedNode.textContent = formatSeconds((Date.now() - startedAt) / 1000);
  }, 1000) : null;
  const stopElapsedTimer = () => { if (elapsedTimer) clearInterval(elapsedTimer); };
  const close = () => {
    if (closed) return;
    if (terminal) {
      closed = true;
      stopElapsedTimer();
      if (unlisten) unlisten();
      unlisten = null;
      if (document.body.contains(backdrop)) backdrop.remove();
      syncDialogBlocking();
      if (appView === "game" && managedSelectedId) renderManagedMode();
    } else {
      backdrop.classList.add("hidden");
      syncDialogBlocking();
    }
  };
  const show = () => {
    if (closed) return;
    if (!document.body.contains(backdrop)) document.body.appendChild(backdrop);
    backdrop.classList.remove("hidden");
    registerDialog(backdrop, { title });
    syncDialogBlocking();
  };
  const setPercent = (value) => {
    if (typeof value === "number" && Number.isFinite(value)) {
      currentPercent = Math.max(0, Math.min(100, value));
    }
    fill.style.width = `${currentPercent}%`;
    percentNode.textContent = `${currentPercent.toFixed(0)}%`;
  };
  const setStat = (node, value) => { if (node) node.textContent = value; };
  const applyStats = (payload) => {
    if (typeof payload.completed === "number") completed = payload.completed;
    if (typeof payload.total === "number") total = payload.total;
    if (typeof payload.speed === "number") speed = payload.speed;
    if (typeof payload.elapsed === "number") elapsed = payload.elapsed;
    setStat(completedNode, completed > 0 ? formatBytes(completed) : "—");
    setStat(totalNode, total > 0 ? formatBytes(total) : "—");
    setStat(speedNode, speed > 0 ? `${formatBytes(speed)}/s` : "—");
    const eta = speed > 0 && total > completed ? (total - completed) / speed : 0;
    setStat(etaNode, eta > 0 ? formatSeconds(eta) : "—");
    setStat(elapsedNode, formatSeconds(elapsed));
  };
  const apply = (payload) => {
    if (!payload || payload.operation !== operation) return;
    if (payload.gameId && options.gameId && payload.gameId !== options.gameId) return;
    if (payload.status === "cancelled") {
      cancelled = true;
      terminal = true;
      stage.textContent = payload.message || "Cancelled";
      actions.classList.add("hidden");
    } else {
      stage.textContent = payload.message || "Working…";
      stage.classList.toggle("error", payload.status === "error");
      setPercent(payload.percent);
      applyStats(payload);
      if (payload.status === "done") complete(payload.message);
    }
    if (terminal) actions.classList.add("hidden");
  };
  const complete = (message) => {
    if (closed) return;
    terminal = true;
    stopElapsedTimer();
    setPercent(100);
    stage.textContent = message || "Complete";
    stage.classList.remove("error");
    actions.classList.add("hidden");
    if (backdrop.classList.contains("hidden")) close();
  };
  const fail = (message) => {
    if (closed) return;
    terminal = true;
    stopElapsedTimer();
    stage.textContent = String(message);
    stage.classList.add("error");
    actions.classList.add("hidden");
    if (backdrop.classList.contains("hidden")) close();
  };
  const discard = () => {
    terminal = true;
    cancelled = true;
    close();
  };
  closeButton.onclick = close;
  cancelButton.onclick = async () => {
    if (!(await confirmDialog({
      title: "Cancel operation",
      message: `Cancel ${operation}? This cannot be undone.`,
      confirmLabel: "Cancel operation",
      cancelLabel: "Keep running",
      danger: true,
    }))) return;
    try {
      await invoke("cancel_operation", { operation, gameId: options.gameId || "" });
      cancelled = true;
      terminal = true;
      stopElapsedTimer();
      stage.textContent = "Cancelled";
      actions.classList.add("hidden");
      setOperationInline(operation, options.gameId || "", "Cancelled", "cancelled", currentPercent);
    } catch (error) { showToast(String(error), true); }
  };
  setPercent(0);
  if (window.__TAURI__?.event?.listen) {
    void window.__TAURI__.event.listen("operation-progress", (event) => apply(event.payload || {})).then((stop) => {
      if (closed) stop();
      else unlisten = stop;
      resolveReady();
    });
  } else {
    resolveReady();
  }
  return { close, complete, fail, discard, ready, show, isTerminal: () => terminal, isCancelled: () => cancelled };
}

function applyDownloadEvent(state) {
  if (!state?.gameId) return;
  managedDownloads.set(state.gameId, state);
  if (!downloadTerminal(state)) watchManagedDownload(state.gameId);
  if (appView === "game" && managedSelectedId === state.gameId) {
    if (!document.querySelector("#managed-download-inline")) {
      renderManagedMode();
    } else {
      applyInlineDownload(state.gameId);
    }
  }
}


if (window.__TAURI__?.event?.listen) {
  window.__TAURI__.event.listen("fix-progress", (event) => {
    const button = document.querySelector(".game-settings-pane button.loading");
    if (button) button.textContent = String(event.payload || "Working…");
  });
  window.__TAURI__.event.listen("download-progress", (event) => {
    applyDownloadEvent(event.payload || {});
  });
  window.__TAURI__.event.listen("operation-progress", (event) => {
    const payload = event.payload || {};
    if (!payload.operation) return;
    setOperationInline(payload.operation, payload.gameId || "", payload.message || "Working…", payload.status || "running", payload.percent);
    if (["done", "cancelled", "error"].includes(payload.status)) {
      const key = `${payload.operation}:${payload.gameId || ""}`;
      inlineOperations.delete(key);
      if (payload.status !== "error") cancelledInline.delete(key);
    }
  });
  window.__TAURI__.event.listen("download-complete", async (event) => {
    const payload = event.payload || {};
    if (payload.gameId) stopDownloadWatch(payload.gameId);
    if (payload.cancelled) {
      if (payload.gameId) managedDownloads.delete(payload.gameId);
      if (appView === "game" && managedSelectedId === payload.gameId) renderManagedMode();
      showToast(payload.message || "Download cancelled");
      return;
    }
    notifyTaskResult("download", payload.gameId || "", Boolean(payload.success), payload.message || (payload.success ? "Download complete" : "Download failed"));
    if (payload.gameId) {
      const current = managedDownloads.get(payload.gameId);
      if (current) {
        current.status = payload.success ? "done" : "error";
        current.message = payload.message || current.message;
      }
    }
    if (payload.success) {
      managedLibrary = await invoke("library_snapshot").catch(() => managedLibrary);
      if (payload.gameId) managedDownloads.delete(payload.gameId);
      if (["home", "library", "game"].includes(appView)) renderManagedMode();
      showToast("Download complete");
    } else if (payload.gameId) {
      if (appView === "game" && managedSelectedId === payload.gameId) renderManagedMode();
    }
  });
}

async function hydrateGameDetail(game) {
  const appId = game.metadata_app_id;
  const seed = gameDetailSeed;
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
  if (seed?.id) chips.push({ text: `IGDB_ID: ${seed.id}`, copy: String(seed.id), label: "IGDB_ID" });
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
        if (igdb && !chips.some((chip) => chip.label === "IGDB_ID")) {
          chips.splice(1, 0, { text: `IGDB_ID: ${igdb}`, copy: String(igdb), label: "IGDB_ID" });
        }
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
    const [disk, download] = await Promise.all([
      invoke("managed_game_size", { gameId: game.id }).catch(() => 0),
      appId ? invoke("steam_download_size", { appid: appId }).catch(() => 0) : Promise.resolve(0),
    ]);
    if (sizeNode) {
      if (disk > 0) {
        sizeNode.textContent = `Size: ${formatBytes(disk)}`;
        sizeNode.title = "Installed size on disk";
      } else if (download > 0) {
        sizeNode.textContent = `Size: ${formatBytes(download)}`;
        sizeNode.title = "Download size via Steam depots";
      } else {
        sizeNode.textContent = "Size: —";
        sizeNode.title = "Steam depot size is unavailable";
      }
    }
  } catch (_error) {
    if (sizeNode) sizeNode.textContent = "Size: —";
  }
}

async function renderCompatibilityTab(pane, game, latestGame, refreshGame, renderTab) {
  const data = await invoke("proton_runners").catch(() => ({ runners: [], default: "" }));
  const runners = [...(data.runners || [])];
  const current = latestGame.launch?.proton || latestGame.installation?.proton || data.default || "";
  if (current && !runners.some((runner) => runner.path === current)) {
    runners.unshift({ name: current.split(/[\\/]/).pop(), path: current, source: "selected", exists: true });
  }
  const options = runners.map((runner) => {
    const installed = runner.source === "steam" || runner.source === "downloaded" || runner.source === "selected";
    const suffix = !runner.exists ? " — missing" : installed ? " — installed" : "";
    return `<option value="${escape(runner.path)}"${runner.path === current ? " selected" : ""}>${escape(runner.name + suffix)}</option>`;
  }).join("");
  const prefix = latestGame.installation?.prefix || "";
  const prefixMode = latestGame.launch?.prefix_mode === "steam" ? "steam" : "umu";
  const requiresSteam = Boolean(latestGame.launch?.requires_steam);
  pane.innerHTML = `<h3>Compatibility</h3>
    <label for="gs-proton-select">Proton</label>
    <select id="gs-proton-select">${options || '<option value="">No Proton runners installed</option>'}</select>
    <button class="primary pane-save" id="gs-proton-save">Save Proton</button>
    <h3>Current selection</h3>
    <div class="game-options-paths">
      ${row("Proton", current || "Not selected")}
      ${row("Prefix", `${prefix || "Not configured"}${latestGame.prefix_exists ? "" : " (not created yet)"}`)}
    </div>
    <label for="gs-prefix-select">Prefix source</label>
    <p class="pane-note">Choose where this game keeps its Wine prefix. Steam mode uses Steam compatdata for this game's App ID.</p>
    <select id="gs-prefix-select">
      <option value="steam"${prefixMode === "steam" ? " selected" : ""}${latestGame.metadata_app_id ? "" : " disabled"}>Steam prefix</option>
      <option value="umu"${prefixMode === "umu" ? " selected" : ""}>SteamTron prefix</option>
    </select>
    <button class="primary pane-save" id="gs-prefix-save">Save prefix source</button>
    <h3>Steam client</h3>
    <div class="game-options-toggle"><div><strong>Require Steam to be running</strong><p class="pane-note">Off by default. When enabled, Play requires the Steam client to be installed and running. If it is not running, Play stops with an error.</p></div><label class="switch"><input type="checkbox" id="gs-steam-required"${requiresSteam ? " checked" : ""}><span></span></label></div>
    <button class="primary pane-save" id="gs-steam-save">Save Steam requirement</button>`;

  pane.querySelector("#gs-proton-save").onclick = async (event) => {
    const stop = busy(event.target, "Saving…");
    try {
      await invoke("update_managed_launch_settings", { gameId: game.id, proton: pane.querySelector("#gs-proton-select").value });
      await refreshGame();
      stop();
      showToast("Proton runner saved");
      renderTab("compatibility");
    } catch (error) { stop(); showToast(String(error), true); }
  };
  pane.querySelector("#gs-prefix-save").onclick = async (event) => {
    const stop = busy(event.target, "Saving…");
    try {
      await invoke("update_managed_launch_settings", { gameId: game.id, prefixMode: pane.querySelector("#gs-prefix-select").value });
      await refreshGame();
      stop();
      showToast("Prefix source saved");
      renderTab("compatibility");
    } catch (error) { stop(); showToast(String(error), true); }
  };
  pane.querySelector("#gs-steam-save").onclick = async (event) => {
    const stop = busy(event.target, "Saving…");
    try {
      await invoke("update_managed_launch_settings", { gameId: game.id, requiresSteam: pane.querySelector("#gs-steam-required").checked });
      await refreshGame();
      stop();
      showToast("Steam requirement saved");
      renderTab("compatibility");
    } catch (error) { stop(); showToast(String(error), true); }
  };
}

async function renderCompatibilityTab(pane, game, refreshGame, renderTab) {
  const data = await invoke("proton_runners").catch(() => ({ runners: [], default: "" }));
  const runners = data.runners || [];
  const defaultRunner = data.default || "";
  const current = game.launch?.proton || game.installation?.proton || "";
  const effective = current || defaultRunner;
  const fileName = (path) => String(path || "").split(/[\\/]/).pop() || path;
  const options = [`<option value=""${current ? "" : " selected"}>Use global default${defaultRunner ? ` (${escape(fileName(defaultRunner))})` : ""}</option>`];
  const seen = new Set([""]);
  for (const runner of runners) {
    if (seen.has(runner.path)) continue;
    seen.add(runner.path);
    const label = `${runner.name}${runner.exists ? " — installed" : " — missing"}`;
    options.push(`<option value="${escape(runner.path)}"${runner.path === current ? " selected" : ""}>${escape(label)}</option>`);
  }
  if (current && !seen.has(current)) {
    options.push(`<option value="${escape(current)}" selected>${escape(fileName(current))} — selected</option>`);
  }
  const prefixMode = game.launch?.prefix_mode === "steam" ? "steam" : "umu";
  const prefix = game.installation?.prefix || "";
  const requiresSteam = Boolean(game.launch?.requires_steam);
  pane.innerHTML = `<h3>Compatibility</h3>
    <select id="gs-proton-select" aria-label="Proton runner">${options.join("")}</select>
    <button class="primary pane-save" id="gs-proton-save">Save Proton</button>
    <h3>Current selection</h3>
    <p class="pane-note">Proton: ${escape(effective || "Not selected")}</p>
    <p class="pane-note">Prefix: ${escape(prefix || "Not selected")}${prefix && !game.prefix_exists ? " (not created yet)" : ""}</p>
    <h3>Prefix source</h3>
    <p class="pane-note">Choose where this game keeps its Wine prefix. Steam mode uses Steam compatdata for this game's App ID.</p>
    <select id="gs-prefix-select" aria-label="Prefix source">
      <option value="steam"${prefixMode === "steam" ? " selected" : ""}${game.metadata_app_id ? "" : " disabled"}>Steam prefix</option>
      <option value="umu"${prefixMode === "umu" ? " selected" : ""}>SteamTron prefix</option>
    </select>
    <button class="primary pane-save" id="gs-prefix-save">Save prefix source</button>
    <h3>Steam client</h3>
    <div class="settings-row"><div class="row-text"><strong>Require Steam to be running</strong><span>Off by default. When enabled, Play requires the Steam client and uses Steam compatdata.</span></div><label class="switch"><input type="checkbox" id="gs-steam-required"${requiresSteam ? " checked" : ""}><span></span></label></div>
    <button class="primary pane-save" id="gs-steam-save">Save Steam requirement</button>`;

  pane.querySelector("#gs-proton-save").onclick = async (event) => {
    const stop = busy(event.target, "Saving…");
    try {
      await invoke("update_managed_launch_settings", {
        gameId: game.id,
        proton: pane.querySelector("#gs-proton-select").value,
      });
      await refreshGame();
      stop();
      showToast("Proton runner saved");
      renderTab("compatibility");
    } catch (error) { stop(); showToast(String(error), true); }
  };
  pane.querySelector("#gs-prefix-save").onclick = async (event) => {
    const stop = busy(event.target, "Saving…");
    try {
      await invoke("update_managed_launch_settings", {
        gameId: game.id,
        prefixMode: pane.querySelector("#gs-prefix-select").value,
      });
      await refreshGame();
      stop();
      showToast("Prefix source saved");
      renderTab("compatibility");
    } catch (error) { stop(); showToast(String(error), true); }
  };
  pane.querySelector("#gs-steam-save").onclick = async (event) => {
    const stop = busy(event.target, "Saving…");
    try {
      await invoke("update_managed_launch_settings", {
        gameId: game.id,
        requiresSteam: pane.querySelector("#gs-steam-required").checked,
      });
      await refreshGame();
      stop();
      showToast("Steam requirement saved");
      renderTab("compatibility");
    } catch (error) { stop(); showToast(String(error), true); }
  };
}

function openManagedGameOptions(game) {
  if (document.body.classList.contains("fullscreen-mode")) {
    showToast("Game settings are available in desktop mode.", true);
    return;
  }
  const backdrop = document.createElement("div");
  backdrop.className = "modal-backdrop";
  const installed = Boolean(game.installed) || game.installation?.mode === "linked";
  const tabs = [
    ["general", "General"],
    ["compatibility", "Compatibility"],
    ["environment", "Environment"],
    ["debug", "Debug"],
    ["link", "Game Link"],
  ];
  if (installed) {
    if (game.metadata_app_id) tabs.push(["dlc", "DLC"], ["updates", "Updates"]);
    tabs.push(["files", "Installed Files"], ["fixes", "Fixes"]);
  }
  backdrop.innerHTML = `<div class="modal game-settings-modal">
    <div class="game-settings-head"><h2>Game Settings · ${escape(game.title)}</h2><button class="modal-close" aria-label="Close">×</button></div>
    <div class="game-settings-body">
      <nav class="game-settings-nav">${tabs.map(([key, label], index) => `<button class="game-settings-tab${index === 0 ? " active" : ""}" data-game-tab="${key}">${label}</button>`).join("")}</nav>
      <section class="game-settings-pane" id="game-settings-pane"></section>
    </div>
  </div>`;
  document.body.appendChild(backdrop);
  registerDialog(backdrop);
  const pane = backdrop.querySelector("#game-settings-pane");
  let latestGame = game;
  const close = () => { backdrop.remove(); renderManagedMode(); };
  backdrop.querySelector(".modal-close").onclick = close;

  const refreshGame = async () => {
    try {
      managedLibrary = await invoke("library_snapshot");
      latestGame = managedLibrary?.games.find((entry) => entry.id === game.id) || latestGame;
    } catch (_error) { /* keep the last copy */ }
    return latestGame;
  };

  const row = (label, value) => `<div class="row"><div class="label">${escape(label)}</div><div class="value">${escape(value || "—")}</div></div>`;

  const renderTab = (key) => {
    for (const button of backdrop.querySelectorAll("[data-game-tab]")) {
      button.classList.toggle("active", button.dataset.gameTab === key);
    }
    pane.scrollTop = 0;
    if (key === "general") {
      const registration = latestGame.steam;
      const target = latestGame.launch_targets?.find((item) => item.primary) || latestGame.launch_targets?.[0];
      const draft = {
        path: latestGame.launch?.prelaunch || "",
        sha256: latestGame.launch?.prelaunch_sha256 || "",
      };
      pane.innerHTML = `<h3>General</h3><p class="pane-note">Library entry, paths and Steam registration for this game.</p>
        <div class="game-options-paths">
          ${row("Library entry", latestGame.id)}
          ${row("Steam App ID", latestGame.metadata_app_id ? String(latestGame.metadata_app_id) : "None")}
          ${row("Installation", latestGame.installation?.root)}
          ${row("Prefix", latestGame.installation?.prefix)}
          ${row("Manifest", latestGame.installation?.manifest)}
          ${row("Launch target", target?.executable || "Not configured")}
          ${row("Steam registration", registration ? `${registration.state} · ${registration.backend}` : "Not registered")}
        </div>
        <h3 class="game-link-section-title">Pre-launch fix script</h3>
        <p class="pane-note">Runs before every Play, on the host, as your user. The script must check whether its fix is already applied and exit without changes when it is. A non-zero exit blocks the launch. Editing the file blocks the next launch until you approve it again.</p>
        <label for="gs-prelaunch-path">Script</label>
        <div class="game-link-folder-row">
          <input id="gs-prelaunch-path" value="${escape(draft.path)}" placeholder="No pre-launch script" readonly>
          <button id="gs-prelaunch-choose">Browse…</button>
        </div>
        <div class="pane-actions"><button id="gs-prelaunch-clear">Clear</button></div>
        <p class="pane-note" id="gs-prelaunch-digest">${draft.sha256 ? `Approved SHA-256: ${escape(draft.sha256)}` : "No pre-launch script."}</p>
        <button class="primary pane-save" id="gs-general-save">Save general settings</button>`;
      const pathInput = pane.querySelector("#gs-prelaunch-path");
      const digest = pane.querySelector("#gs-prelaunch-digest");
      pane.querySelector("#gs-prelaunch-choose").onclick = async () => {
        try {
          const chosen = await invoke("choose_prelaunch_script");
          if (!chosen) return;
          draft.path = chosen.path;
          draft.sha256 = chosen.sha256;
          pathInput.value = chosen.path;
          digest.textContent = `Approved SHA-256: ${chosen.sha256}`;
        } catch (error) { showToast(String(error), true); }
      };
      pane.querySelector("#gs-prelaunch-clear").onclick = () => {
        draft.path = "";
        draft.sha256 = "";
        pathInput.value = "";
        digest.textContent = "No pre-launch script.";
      };
      pane.querySelector("#gs-general-save").onclick = async (event) => {
        const stop = busy(event.target, "Saving…");
        try {
          await invoke("set_managed_prelaunch", { gameId: game.id, path: draft.path || null, sha256: draft.sha256 || null });
          await refreshGame();
          stop();
          showToast(draft.path ? "Pre-launch script saved and approved" : "Pre-launch script cleared");
          renderTab("general");
        } catch (error) { stop(); showToast(String(error), true); }
      };
      return;
    }
    if (key === "compatibility") {
      void renderCompatibilityTab(pane, latestGame, refreshGame, renderTab);
      return;
    }
    if (key === "environment") {
      const entries = Object.entries(latestGame.launch?.environment || {});
      pane.innerHTML = `<h3>Environment</h3>
        <p class="pane-note">Pass key-value variables only to this game's Windows launch. SteamTron manages the runner, prefix, Steam identity, paths and diagnostic logging.</p>
        <div id="gs-env-rows"></div>
        <button class="pane-add" id="gs-env-add">Add variable</button>
        <button class="primary pane-save" id="gs-env-save">Save environment</button>`;
      const rows = pane.querySelector("#gs-env-rows");
      const addRow = (name = "", value = "") => {
        const row = document.createElement("div");
        row.className = "env-row";
        row.innerHTML = `<input class="env-key" placeholder="KEY" value="${escape(name)}" aria-label="Variable name"><input class="env-value" placeholder="VALUE" value="${escape(value)}" aria-label="Variable value"><button class="env-remove" aria-label="Remove variable">Remove</button>`;
        row.querySelector(".env-remove").onclick = () => row.remove();
        rows.appendChild(row);
        return row;
      };
      for (const [name, value] of entries) addRow(name, value);
      pane.querySelector("#gs-env-add").onclick = () => {
        const row = addRow();
        row.querySelector(".env-key").focus();
      };
      pane.querySelector("#gs-env-save").onclick = async (event) => {
        const map = {};
        for (const row of rows.querySelectorAll(".env-row")) {
          const key = row.querySelector(".env-key").value.trim();
          if (!key) continue;
          map[key] = row.querySelector(".env-value").value.trim();
        }
        const stop = busy(event.target, "Saving…");
        try {
          await invoke("update_managed_launch_settings", { gameId: game.id, environment: map });
          await refreshGame();
          stop();
          showToast("Launch environment saved");
          renderTab("environment");
        } catch (error) { stop(); showToast(String(error), true); }
      };
      return;
    }
    if (key === "debug") {
      pane.innerHTML = `<h3>Debug</h3><p class="pane-note">Latest Proton diagnostic log for this game. The viewer shows the newest 64 KiB.</p>
        <pre class="game-log" id="gs-log">Loading…</pre>
        <p class="pane-note" id="gs-log-path"></p>
        <div class="pane-actions"><button id="gs-log-copy">Copy</button><button class="danger" id="gs-log-clear">Clear</button></div>`;
      const logNode = pane.querySelector("#gs-log");
      const pathNode = pane.querySelector("#gs-log-path");
      void invoke("managed_game_log", { gameId: game.id }).then((log) => {
        logNode.textContent = log.content || "No log yet. Launch the game to create one.";
        pathNode.textContent = log.path || "";
      }).catch((error) => { logNode.textContent = String(error); });
      pane.querySelector("#gs-log-copy").onclick = () => { void copyToClipboard(logNode.textContent || "", "Log"); };
      pane.querySelector("#gs-log-clear").onclick = async () => {
        if (!(await confirmDialog({ title: "Clear logs", message: "Clear this game's previous diagnostic logs?", confirmLabel: "Clear", danger: true }))) return;
        try {
          const removed = await invoke("clear_managed_game_logs", { gameId: game.id });
          showToast(`Cleared ${removed} log${removed === 1 ? "" : "s"}`);
          renderTab("debug");
        } catch (error) { showToast(String(error), true); }
      };
      return;
    }
    if (key === "link") {
      renderManagedLink(pane, latestGame, refreshGame, renderTab);
      return;
    }
    if (key === "updates") {
      renderUpdatesPanel(pane, latestGame);
      return;
    }
    if (key === "dlc") {
      const renderDlcTab = async () => {
        pane.innerHTML = '<h3>Downloadable Content</h3><p class="pane-note">Loading DLC from Steam…</p>';
        let entries = [];
        try { entries = await invoke("managed_dlc", { gameId: game.id }); } catch (_error) { entries = []; }
        const rows = entries.map((entry) => `<tr>
          <td><input type="checkbox" data-dlc-id="${entry.app_id}"${entry.selected ? " checked" : ""}${entry.available ? "" : " disabled"} aria-label="Install ${escape(entry.name)}" title="${entry.available ? "Check to queue this DLC; uncheck to remove it." : "This DLC is not present in the game manifest."}"></td>
          <td>${escape(entry.name)}<span class="dlc-note" data-dlc-installed="${entry.app_id}"${entry.installed ? "" : " hidden"}> installed</span>${entry.available ? "" : ' <span class="dlc-note">not in manifest</span>'}</td>
          <td>${escape(entry.released || "—")}</td>
          <td data-dlc-size="${entry.app_id}">—</td>
        </tr>`).join("");
        pane.innerHTML = `<h3>Downloadable Content</h3>
          <p class="pane-note">Check DLC to queue it. Unchecking removes its installed files immediately. When a download is queued, Play becomes Update.</p>
          <table class="dlc-table"><thead><tr><th>Install</th><th>Name</th><th>Released</th><th>Size</th></tr></thead>
          <tbody>${rows || '<tr><td colspan="4">No DLC entries were reported for this game.</td></tr>'}</tbody></table>`;
        const sizeIds = entries.map((entry) => entry.app_id);
        if (sizeIds.length) {
          void invoke("managed_dlc_sizes", { gameId: game.id, appIds: sizeIds }).then((sizes) => {
            for (const cell of pane.querySelectorAll("[data-dlc-size]")) {
              const size = Number(sizes[cell.dataset.dlcSize]) || 0;
              cell.textContent = size > 0 ? formatBytes(size) : "—";
            }
          }).catch(() => {
            for (const cell of pane.querySelectorAll("[data-dlc-size]")) cell.textContent = "—";
          });
        }
        for (const input of pane.querySelectorAll("input[data-dlc-id]")) {
          input.onchange = async () => {
            input.disabled = true;
            try {
              const appId = Number(input.dataset.dlcId);
              await invoke("set_managed_dlc", { gameId: game.id, appId, enabled: input.checked });
              const note = pane.querySelector(`[data-dlc-installed="${appId}"]`);
              if (note) note.hidden = true;
              await updateManagedDlcAction(game);
              showToast(input.checked ? "DLC added to the update queue." : "DLC removed.");
              input.disabled = false;
            } catch (error) {
              input.checked = !input.checked;
              input.disabled = false;
              showToast(String(error), true);
            }
          };
        }
      };
      void renderDlcTab();
      return;
    }
    if (key === "files") {
      const renderFilesTab = async () => {
        pane.innerHTML = '<h3>Installed Files</h3><p class="pane-note">Calculating installation size…</p>';
        let size = 0;
        try { size = await invoke("managed_game_size", { gameId: game.id }); } catch (_error) { size = 0; }
        const name = game.installation?.root?.split(/[\\/]/).filter(Boolean).pop() || game.title;
        const linked = latestGame.installation?.mode === "linked";
        pane.innerHTML = `<h3>Installed Files</h3>
          <div class="files-card files-size-card"><strong>Size of installation: ${formatBytes(size)} (${escape(name)})</strong><button id="files-browse">Browse…</button></div>
          <div class="files-card"><p class="pane-note">Create a portable backup folder with content.tar, fixes.tar, depot manifests and appmanifest metadata.</p><button class="files-wide" id="files-backup" data-operation-button="backup">Back up game files…</button><div class="operation-inline hidden" data-operation-inline="backup" data-operation-game="${game.id}"><div class="download-inline-track"><span></span></div><div class="download-inline-meta"><span class="operation-inline-text">Starting…</span></div></div></div>
          <div class="files-card"><p class="pane-note">Verify this game's files are installed correctly. SteamTron checks the installation against its depot manifests and downloads only missing or damaged files; intact files are left untouched.</p><button class="primary files-wide" id="files-verify" data-operation-button="verify">Verify files integrity</button><div class="operation-inline hidden" data-operation-inline="verify" data-operation-game="${game.id}"><div class="operation-inline-progress"><div class="download-inline-track"><span></span></div><button class="game-icon-button download-cancel-icon" data-operation-cancel="verify" data-operation-game="${game.id}" title="Cancel verification" aria-label="Cancel verification"><svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"></path></svg></button></div><div class="download-inline-meta"><span class="operation-inline-text">Starting…</span></div></div><p class="files-status" id="files-status"></p></div>
          <div class="files-card"><p class="pane-note">Move this game's content to another registered drive.</p><button class="files-wide" id="files-move">Move install folder…</button></div>
          <div class="files-card"><p class="pane-note">${linked ? "Unlink removes this external game's library entry while keeping all files on disk. Uninstall is unavailable for external folders." : "Uninstall permanently deletes this game's installed files. The game stays in your library so you can download it again; use Remove from library on the game page to remove the entry."}</p><div class="files-pair"><button class="danger" id="files-unlink"${linked ? "" : " disabled title=\"Unlink is only available for external linked games.\""}>Unlink</button><button class="danger" id="files-uninstall"${linked ? " disabled title=\"External folders must be unlinked, not deleted.\"" : ""}>Uninstall</button></div></div>
          <div class="files-card"><p class="pane-note">Export or restore campaign saves and checkpoints from this game's prefix.</p><div class="files-pair"><button id="files-export-saves">Export saves…</button><button id="files-import-saves">Import saves…</button></div></div>
          <div class="files-card"><p class="pane-note">Restore the game's Lua manifest and every depot manifest from SteamTron's persistent bundle store. The API is used only when the bundle is missing.</p><button class="files-wide" id="files-regenerate">Generate Lua/manifests</button></div>`;

        const status = pane.querySelector("#files-status");
        const refreshGameAndTab = async () => {
          await refreshGame();
          await renderFilesTab();
        };
        const run = async (button, work, message, done) => {
          const stop = busy(button, "Working…");
          try {
            const result = await work();
            stop();
            showToast(typeof message === "function" ? message(result) : message);
            if (done) await done();
            else await refreshGameAndTab();
          } catch (error) {
            stop();
            showToast(String(error), true);
            if (status) { status.textContent = String(error); status.classList.add("error"); }
          }
        };

        pane.querySelector("#files-browse").onclick = async () => {
          try { await invoke("open_path", { path: latestGame.installation.root }); }
          catch (error) { showToast(String(error), true); }
        };
        pane.querySelector("#files-backup").onclick = async () => {
          const operation = ensureOperationManager("backup", game.id, {
            operation: "backup",
            title: `Backing up · ${latestGame.title}`,
            gameId: game.id,
            gameName: latestGame.title,
          });
          if (!operation) return;
          beginOperation("backup", game.id);
          await operation.ready;
          try {
            let result = await invoke("backup_managed_game", { gameId: game.id, destination: null, replace: false });
            if (!result) { cancelOperation("backup", game.id); operation.discard(); showToast("Backup cancelled."); return; }
            if (result.exists) {
              const overwrite = await confirmDialog({
                title: "Overwrite backup?",
                message: `A backup folder already exists at ${result.target}.\n\nOverwrite it with a new backup? The old backup is kept until the new one is ready.`,
                confirmLabel: "Overwrite backup",
                cancelLabel: "Keep old backup",
                danger: true,
              });
              if (!overwrite) {
                cancelOperation("backup", game.id);
                operation.discard();
                showToast("Backup cancelled; the old backup was kept.");
                return;
              }
              result = await invoke("backup_managed_game", { gameId: game.id, destination: result.parent, replace: true });
              if (!result) { cancelOperation("backup", game.id); operation.discard(); showToast("Backup cancelled."); return; }
            }
            finishOperation("backup", game.id, `Backup written to ${result.path}`);
            operation.complete(`Backup written to ${result.path}`);
            showToast(`Backup written to ${result.path}`);
          } catch (error) {
            if (operation.isCancelled()) return;
            failOperation("backup", game.id, String(error));
            operation.fail(String(error));
          }
        };
        const verifyKey = `verify:${game.id}`;
        pane.querySelector("#files-verify").onclick = async () => {
          if (inlineOperations.has(verifyKey)) return;
          inlineOperations.add(verifyKey);
          beginOperation("verify", game.id);
          status.textContent = "Verifying files against depot manifests…";
          status.classList.remove("error");
          try {
            await invoke("verify_managed_files", { gameId: game.id });
            finishOperation("verify", game.id, "Verification complete");
            status.textContent = "Verification complete. Missing or damaged files were repaired.";
            status.classList.remove("error");
            showToast("File integrity verified");
            const next = await invoke("managed_game_size", { gameId: game.id }).catch(() => size);
            pane.querySelector(".files-size-card strong").textContent = `Size of installation: ${formatBytes(next)} (${name})`;
          } catch (error) {
            if (cancelledInline.has(verifyKey)) return;
            failOperation("verify", game.id, String(error));
            status.textContent = String(error);
            status.classList.add("error");
          } finally {
            inlineOperations.delete(verifyKey);
            cancelledInline.delete(verifyKey);
          }
        };
        const inlineVerifyCancel = pane.querySelector('[data-operation-cancel="verify"]');
        if (inlineVerifyCancel) inlineVerifyCancel.onclick = async () => {
          if (!(await confirmDialog({ title: "Cancel verification", message: "Stop verifying this game? Files already checked stay as they are.", confirmLabel: "Cancel verification", cancelLabel: "Keep verifying", danger: true }))) return;
          cancelledInline.add(verifyKey);
          try { await invoke("cancel_operation", { operation: "verify", gameId: game.id }); }
          catch (error) { cancelledInline.delete(verifyKey); showToast(String(error), true); }
        };
        pane.querySelector("#files-move").onclick = async () => {
          const snapshot = await invoke("library_snapshot").catch(() => managedLibrary);
          const drives = snapshot?.drives || [];
          if (!drives.length) { showToast("No other drives are registered.", true); return; }
          const backdrop = document.createElement("div");
          backdrop.className = "modal-backdrop";
          backdrop.innerHTML = `<div class="modal"><div class="modal-head"><h2>Move install folder</h2><button class="modal-close" id="move-close">×</button></div><p class="pane-note">Choose a registered drive. Existing files are moved; no copy is left behind.</p><div class="drive-choices">${drives.map((drive) => `<label class="drive-choice"><input type="radio" name="move-drive" value="${escape(drive.id)}"><span class="drive-choice-copy"><strong>${escape(drive.label || drive.id)}</strong><small>${escape(drive.root)}</small></span></label>`).join("")}</div><div class="modal-actions"><button id="move-cancel">Cancel</button><button class="primary" id="move-confirm" disabled>Move</button></div></div>`;
          document.body.appendChild(backdrop);
          registerDialog(backdrop);
          const confirm = backdrop.querySelector("#move-confirm");
          backdrop.querySelector("#move-close").onclick = () => backdrop.remove();
          backdrop.querySelector("#move-cancel").onclick = () => backdrop.remove();
          for (const radio of backdrop.querySelectorAll("input[name=move-drive]")) {
            radio.onchange = () => { confirm.disabled = false; };
          }
          confirm.onclick = async () => {
            const drive = backdrop.querySelector("input[name=move-drive]:checked")?.value;
            if (!drive) return;
            const stop = busy(confirm, "Moving…");
            try {
              await invoke("move_managed_game", { gameId: game.id, driveId: drive });
              backdrop.remove();
              showToast("Install folder moved");
              await refreshGameAndTab();
            } catch (error) { stop(); showToast(String(error), true); }
          };
        };
        pane.querySelector("#files-unlink").onclick = async () => {
          if (!(await confirmDialog({ title: "Unlink game", message: "Unlink this game? The library entry and Steam registration are removed; all game files stay on disk.", confirmLabel: "Unlink", danger: true }))) return;
          try {
            await invoke("remove_managed_game", { gameId: game.id });
            close();
            setView("library");
            showToast("Game unlinked; files were kept");
          } catch (error) { showToast(String(error), true); }
        };
        pane.querySelector("#files-uninstall").onclick = async () => {
          if (!(await confirmDialog({
            title: "Uninstall game",
            message: `Uninstall ${latestGame.title}? The installed game files are permanently deleted. The game stays in your library so you can download it again.`,
            confirmLabel: "Uninstall",
            danger: true,
          }))) return;
          const stop = busy(pane.querySelector("#files-uninstall"), "Uninstalling…");
          try {
            await invoke("uninstall_managed_game", { gameId: game.id });
            await refreshGame();
            close();
            showToast("Game files removed; game kept in library");
          } catch (error) { stop(); showToast(String(error), true); }
        };
        pane.querySelector("#files-export-saves").onclick = (event) => run(event.currentTarget, async () => {
          const path = await invoke("export_managed_saves", { gameId: game.id });
          if (!path) throw new Error("Export cancelled.");
          return path;
        }, (path) => `Saves exported to ${path}`, async () => {});
        pane.querySelector("#files-import-saves").onclick = (event) => run(event.currentTarget, async () => {
          const result = await invoke("import_managed_saves", { gameId: game.id });
          if (!result) throw new Error("Import cancelled.");
          return result;
        }, (result) => `Imported ${result.files} save file${result.files === 1 ? "" : "s"}`, async () => {});
        pane.querySelector("#files-regenerate").onclick = (event) => run(event.currentTarget,
          () => invoke("regenerate_managed_manifests", { gameId: game.id }),
          (result) => `Generated Lua manifest and ${result.manifests} depot manifest${result.manifests === 1 ? "" : "s"}`);
      };
      void renderFilesTab();
      return;
    }
    const renderFixesTab = async () => {
      pane.innerHTML = '<h3>Fixes</h3><p class="pane-note">Loading fixes…</p>';
      let status;
      try {
        status = await invoke("managed_fixes_status", { gameId: game.id });
      } catch (error) {
        pane.innerHTML = `<h3>Fixes</h3><div class="status error">${escape(String(error))}</div>`;
        return;
      }
      const present = status.files > 0;
      pane.innerHTML = `<h3>Fixes</h3>
        <p class="pane-note">Import, export, or remove per-game fixes. Fixes are layered over the game only while SteamTron launches it; installed files and the Proton prefix are never changed.</p>
        <div class="fix-action-row"><button class="primary" id="fix-import">Import fixes ZIP…</button><button id="fix-export"${present ? "" : " disabled"}>Export fixes ZIP…</button></div>
        <section class="fix-section"><h4>Current fixes location</h4>
          <p class="fix-path">${status.path ? escape(status.path) : "No fixes folder yet."}</p>
          <p class="fix-meta">${present ? `${status.files} file${status.files === 1 ? "" : "s"} · ${formatBytes(status.bytes)}` : "No fixes installed."}</p>
          <div class="fix-action-row"><button id="fix-open"${present ? "" : " disabled"}>Open fixes folder</button><button id="fix-remove" class="danger"${present ? "" : " disabled"}>Remove fixes</button></div>
        </section>
        <section class="fix-section"><h4>GoldBerg emulator</h4>
          <p class="pane-note">Replaces this game's Steam library with the GoldBerg emulator and generates its interface list from the original library. The emulator release is downloaded once and shared by every game; the installed game files are never changed.</p>
          <p class="fix-meta">${status.goldberg ? "GoldBerg is applied to this fixes folder." : "GoldBerg is not applied. Assets are downloaded and verified on first use."}</p>
          <div class="fix-action-row"><button id="fix-goldberg-apply">Apply GoldBerg</button><button id="fix-goldberg-remove" class="danger"${status.goldberg ? "" : " disabled"}>Remove GoldBerg</button></div>
        </section>
        <section class="fix-section"><h4>Steamless</h4>
          <p class="pane-note">Copies each registered launcher into this fixes folder with its exact structure and removes SteamStub DRM from that copy using SteamTron's built-in unpacker. No Wine or external tool is required. The installed game files are never changed.</p>
          <p class="fix-meta">${status.steamless ? "Steamless is applied to this fixes folder." : "Steamless is not applied. The built-in unpacker runs locally."}</p>
          <div class="fix-action-row"><button id="fix-steamless-apply">Apply Steamless</button><button id="fix-steamless-remove" class="danger"${status.steamless ? "" : " disabled"}>Remove Steamless</button></div>
        </section>`;

      const refresh = async () => {
        await refreshGame();
        await renderFixesTab();
      };
      const run = async (button, work, message) => {
        const stop = busy(button, "Working…");
        try {
          const result = await work();
          stop();
          showToast(typeof message === "function" ? message(result) : message);
          await refresh();
        } catch (error) {
          stop();
          showToast(String(error), true);
        }
      };
      pane.querySelector("#fix-import").onclick = (event) => run(event.currentTarget, async () => {
        const result = await invoke("install_managed_fixes", { gameId: game.id });
        return result || { files: [] };
      }, (result) => `Installed ${result.files?.length || 0} fix file${result.files?.length === 1 ? "" : "s"}`);
      pane.querySelector("#fix-export").onclick = (event) => run(event.currentTarget, async () => {
        const path = await invoke("export_managed_fixes", { gameId: game.id });
        if (!path) throw new Error("Export cancelled.");
        return path;
      }, (path) => `Exported to ${path}`);
      pane.querySelector("#fix-open").onclick = async () => {
        if (!status.path) return;
        try { await invoke("open_path", { path: status.path }); }
        catch (error) { showToast(String(error), true); }
      };
      pane.querySelector("#fix-remove").onclick = async (event) => {
        if (!(await confirmDialog({ title: "Remove fixes", message: "Remove all fixes for this game? Files are deleted from the fixes folder; installed game files are unchanged.", confirmLabel: "Remove", danger: true }))) return;
        void run(event.currentTarget, () => invoke("remove_managed_fixes", { gameId: game.id }),
          (result) => `Removed ${result.files} fix file${result.files === 1 ? "" : "s"}`);
      };
      pane.querySelector("#fix-goldberg-apply").onclick = (event) => {
        showToast("Applying GoldBerg — first use downloads the emulator assets.");
        void run(event.currentTarget, () => invoke("apply_managed_goldberg", { gameId: game.id }), "GoldBerg applied");
      };
      pane.querySelector("#fix-goldberg-remove").onclick = (event) => run(event.currentTarget,
        () => invoke("remove_managed_goldberg", { gameId: game.id }), "GoldBerg removed");
      pane.querySelector("#fix-steamless-apply").onclick = (event) => {
        showToast("Applying Steamless…");
        void run(event.currentTarget, () => invoke("apply_managed_steamless", { gameId: game.id }),
          (result) => result.message || "Steamless applied");
      };
      pane.querySelector("#fix-steamless-remove").onclick = (event) => run(event.currentTarget,
        () => invoke("remove_managed_steamless", { gameId: game.id }), "Steamless removed");
    };
    void renderFixesTab();
  };

  for (const button of backdrop.querySelectorAll("[data-game-tab]")) {
    button.onclick = () => {
      focusControl(button);
      renderTab(button.dataset.gameTab);
    };
  }
  renderTab("general");
  focusControl(backdrop.querySelector(".game-settings-tab.active"));
}

function pad2(value) {
  return String(value).padStart(2, "0");
}

function renderManagedHome() {
  const games = managedLibrary.games;
  const recentIds = managedLibrary.recently_played || [];
  const recent = recentIds
    .map((id) => games.find((game) => game.id === id))
    .filter(Boolean);
  const dated = games
    .filter((game) => game.setup_completed_at)
    .sort((left, right) => right.setup_completed_at - left.setup_completed_at);
  const undated = games
    .filter((game) => !game.setup_completed_at && managedPlayable(game))
    .sort((left, right) => left.title.localeCompare(right.title));
  const ready = [...dated, ...undated];
  const choices = [...recent, ...ready.filter((game) => !recent.some((entry) => entry.id === game.id))];
  const remembered = routeSelection.home;
  const featured = (remembered && choices.find((game) => game.id === remembered.id)) || choices[0] || null;
  const featuredSection = recent.some((game) => game.id === featured?.id) ? "recent" : "setup";
  // Keep the remembered card when it still exists; otherwise Home starts on
  // its own first card.
  if (!featured) routeSelection.home = null;
  else if (!remembered || remembered.id !== featured.id || remembered.section !== featuredSection) {
    routeSelection.home = { id: featured.id, section: featuredSection };
  }
  const appId = featured?.metadata_app_id;
  const hero = appId ? heroUrl(appId) : "";
  const running = featured ? runningManaged.has(featured.id) : false;
  const recentIndex = Math.max(1, recent.findIndex((game) => game.id === featured?.id) + 1);
  const railCounter = recent.length ? `${pad2(recentIndex)} / ${pad2(recent.length)}` : "";
  detailEl.innerHTML = `<div class="page home-page">
    <section class="home-hero${hero ? "" : " no-artwork"}" id="home-hero"${hero ? ` style="background-image:url('${escape(hero)}')"` : ""}>
      <div class="home-hero-shade"></div>
      <div class="home-hero-content">
        <p class="eyebrow" id="home-eyebrow">${featuredSection === "recent" ? "Recently played" : "Recently ready to play"}</p>
        <h1 id="home-title">${escape(featured?.title || (games.length ? "Nothing ready yet" : "No games yet"))}</h1>
        <p class="home-meta" id="home-related">${featured ? escape(appId ? `Steam App ${appId}` : "Managed entry") + (managedReady(featured) ? " · Ready to play" : "") : games.length ? "Finish a download and setup to see it here." : "Add a game from the Store to get started."}</p>
        <p class="home-description hidden" id="home-description"></p>
        <div class="home-actions"><button class="${running ? "danger" : "primary"}" id="home-play"${featured ? "" : " disabled"}>${running ? "Stop" : "Play"}</button><button id="home-view"${featured ? "" : " disabled"}>View game</button></div>
      </div>
    </section>
    <section class="managed-rail" data-rail="recent">
      <div class="rail-head"><h2>Recently played</h2><span class="muted" id="home-counter">${railCounter}</span></div>
      ${recent.length ? `<div class="rail-scroll">${recent.map((game) => coverCard(game, "recent")).join("")}</div>` : '<p class="muted">Your recent plays will appear here after you start a game.</p>'}
    </section>
    <section class="managed-rail" data-rail="setup">
      <div class="rail-head"><h2>Recently ready to play</h2></div>
      <p class="muted">Your ready games, newest completed setups first.</p>
      ${ready.length ? `<div class="rail-scroll">${ready.map((game) => coverCard(game, "setup", "Ready to play")).join("")}</div>` : '<p class="muted">Completed setups will appear here. Saving metadata does not count as setup.</p>'}
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
  if (!managedShell()) return;
  document.body.classList.add("managed");
  if (!managedLibrary.games.some((game) => game.id === managedSelectedId)) {
    managedSelectedId = managedLibrary.games[0]?.id || null;
  }
  for (const view of ["home", "library"]) {
    const selection = routeSelection[view];
    if (selection && !managedLibrary.games.some((game) => game.id === selection.id)) {
      routeSelection[view] = null;
    }
  }
  if (!hasManagedLibrary()) {
    if (appView === "home") { renderManagedHome(); return; }
    if (appView === "game") appView = "library";
    renderManagedLibrary();
    return;
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

async function openManagedGame(id, origin = "library", seed = null) {
  managedSelectedId = id;
  gameDetailOrigin = origin;
  gameDetailSeed = seed;
  appView = "game";
  // Re-read installed/content state when opening the page so a newly added,
  // not-yet-downloaded game always shows Download instead of a stale Play.
  managedLibrary = await invoke("library_snapshot").catch(() => managedLibrary);
  renderManagedMode();
  // Land on Play/Download so the primary action is one keypress away.
  const action = firstContentControl("game");
  if (action) focusControl(action);
}

function setView(view, options = {}) {
  const focusMode = options.focus || "first";
  const returningToStore = view === "store" && appView === "store-game";
  appView = view;
  if (returningToStore) restoreStoreScroll = true;
  document.body.classList.toggle("store-open", view === "store");
  const navView = view === "game"
    ? gameDetailOrigin
    : view === "store-game" ? "store" : view;
  for (const name of ["home", "library", "store"]) {
    const button = document.querySelector(`#nav-${name}`);
    button.classList.toggle("active", navView === name);
    button.setAttribute("aria-current", navView === name ? "page" : "false");
  }
  if (view === "store") {
    if (!storeTried && !storeLoading) void openStore().then(() => applyPendingRouteFocus("store"));
    else renderStorePage();
    focusRouteOnEntry(navView, focusMode);
    return;
  }
  if (managedShell()) {
    renderManagedMode();
  } else if (view === "library" || view === "game") {
    renderGames();
    const game = games.find((item) => item.appid === selected);
    if (game) renderDetail(game, detailTab);
    else detailEl.innerHTML = `<div class="empty">No Steam games found.</div>`;
  } else {
    detailEl.innerHTML = `<div class="empty">Add a game to build your library.</div>`;
  }
  if (view === "home" || view === "library") focusRouteOnEntry(view, focusMode);
}

gamesEl.addEventListener("click", (event) => {
  const button = event.target.closest("[data-managed]");
  if (!button) return;
  rememberRouteFocus(button);
  openManagedGame(button.dataset.managed, cardRoute(button));
});

detailEl.addEventListener("click", (event) => {
  const button = event.target.closest("[data-managed]");
  if (!button) return;
  rememberRouteFocus(button);
  openManagedGame(button.dataset.managed, cardRoute(button));
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
  const view = appView === "game"
    ? gameDetailOrigin
    : appView === "store-game" ? "store" : appView;
  return document.querySelector(`.topbar #nav-${view}`) || document.querySelector(".topbar #nav-home");
}

function enterContentFromHeader() {
  if (appView === "home") {
    // Home order is top menu -> hero Play -> first card.
    const play = document.querySelector("#home-play:not([disabled]), #home-view:not([disabled])");
    if (play) { focusControl(play); return true; }
    const card = firstContentControl("home");
    if (card) { focusControl(card); return true; }
  }
  if (appView === "store") {
    const card = document.querySelector("#store-results .cover-card")
      || document.querySelector(".store-content .cover-card");
    if (card) { focusControl(card); return true; }
    if (storeLoading) {
      // Catalogue is still painting: keep the tab focused and let the first
      // card take over when results arrive, rather than dropping into the
      // filter sidebar.
      focusRouteOnEntry("store", "first");
      return true;
    }
    const retry = document.querySelector("#store-retry");
    if (retry) { focusControl(retry); return true; }
    return false;
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

function visibleModal() {
  const entry = dialogTop();
  if (entry) return entry.backdrop;
  return [...document.querySelectorAll(".modal-backdrop")]
    .find((node) => !node.classList.contains("hidden")) || null;
}

function navigationControls() {
  const settings = document.querySelector("#settings-page");
  const dialog = document.querySelector("#profile-dialog");
  const modal = visibleModal();
  const root = dialog && !dialog.classList.contains("hidden")
    ? dialog
    : modal
      ? modal
      : settings && !settings.classList.contains("hidden")
        ? settings
        : document;
  return [...root.querySelectorAll("button, input, select, textarea, a[href], [tabindex='0']")]
    .filter((node) => !node.disabled
      && node.getClientRects().length > 0
      && !node.closest(".hidden")
      && !node.closest("#auth-gate"));
}

// Remember the card that opened a detail page so Escape can put focus back on
// it, and give every route a sensible first stop (the first card, not the
// header) when it is entered.
const routeFocusMemory = { home: null, library: null, store: null };
let pendingRouteFocus = null;

function routeFocusDescriptor(node) {
  if (!node?.matches) return null;
  if (node.matches(".home-page .cover-card[data-managed]")) {
    return {
      view: "home",
      managedId: node.dataset.managed || "",
      rail: node.closest(".managed-rail")?.dataset.rail || "",
    };
  }
  if (node.matches(".library-page .cover-card[data-managed], .games .game-card[data-appid]")) {
    return {
      view: "library",
      managedId: node.dataset.managed || "",
      appid: node.dataset.appid || "",
      page: libraryPage,
    };
  }
  if (node.matches(".store-page [data-store-id]")) {
    return { view: "store", storeId: node.dataset.storeId || "", id: node.id || "", page: storePage };
  }
  return null;
}

function rememberRouteFocus(node) {
  const descriptor = routeFocusDescriptor(node);
  if (descriptor) routeFocusMemory[descriptor.view] = descriptor;
  return descriptor;
}

function cardRoute(node) {
  if (node.closest?.(".home-page")) return "home";
  if (node.closest?.(".store-page") || node.closest?.(".store-content")) return "store";
  return "library";
}

function routeFocusNode(memory) {
  if (!memory) return null;
  if (memory.view === "home" && memory.managedId) {
    const id = CSS.escape(memory.managedId);
    if (memory.rail) {
      const rail = document.querySelector(`.home-page .managed-rail[data-rail="${CSS.escape(memory.rail)}"]`);
      const card = rail?.querySelector(`.cover-card[data-managed="${id}"]`);
      if (card) return card;
    }
    return document.querySelector(`.home-page .cover-card[data-managed="${id}"]`);
  }
  if (memory.view === "library") {
    if (memory.managedId) {
      const card = document.querySelector(`.library-page .cover-card[data-managed="${CSS.escape(memory.managedId)}"]`);
      if (card) return card;
    }
    if (memory.appid) {
      const card = document.querySelector(`.games .game-card[data-appid="${CSS.escape(String(memory.appid))}"]`);
      if (card) return card;
    }
  }
  if (memory.view === "store") {
    if (memory.storeId) {
      const id = CSS.escape(memory.storeId);
      const card = document.querySelector(`.store-page #store-results [data-store-id="${id}"]`)
        || document.querySelector(`.store-page .store-content [data-store-id="${id}"]`)
        || document.querySelector(`.store-page [data-store-id="${id}"]`);
      if (card) return card;
    }
    if (memory.id) {
      const node = document.getElementById(memory.id);
      if (node && document.contains(node)) return node;
    }
  }
  return null;
}

function firstContentControl(view = appView) {
  if (view === "home") {
    return document.querySelector(".home-page .cover-card.active")
      || document.querySelector(".home-page .cover-card")
      || document.querySelector("#home-play");
  }
  if (view === "library") {
    return document.querySelector(".library-page .cover-card.active")
      || document.querySelector(".library-page .cover-card")
      || document.querySelector(".games .game-card.active")
      || document.querySelector(".games .game-card");
  }
  if (view === "store") {
    return document.querySelector("#store-results .cover-card")
      || document.querySelector(".store-content .cover-card")
      || document.querySelector("#store-search");
  }
  if (view === "game") {
    // Detail pages land on the primary action (Play/Download), not the header.
    return document.querySelector("#managed-play:not([disabled])")
      || document.querySelector("#managed-download:not([disabled])")
      || document.querySelector("#managed-stop:not([disabled])")
      || document.querySelector("#managed-back");
  }
  if (view === "store-game") {
    return document.querySelector("#store-add:not([disabled])")
      || document.querySelector("#store-back");
  }
  return null;
}

function focusFirstContent(view = appView) {
  if (visibleModal() || document.querySelector("#settings-page:not(.hidden)")) return false;
  const node = firstContentControl(view);
  if (!node) return false;
  focusControl(node);
  return true;
}

function focusRouteOnEntry(view, mode = "first") {
  pendingRouteFocus = { view, mode };
  applyPendingRouteFocus(view);
}

function applyPendingRouteFocus(view = appView) {
  if (!pendingRouteFocus || pendingRouteFocus.view !== view) return false;
  if (appView !== view) return false;
  if (visibleModal() || document.querySelector("#settings-page:not(.hidden)")) return false;
  // The store paints a loading skeleton first. Wait for cards instead of
  // parking focus on the search box.
  if (view === "store" && storeLoading && !document.querySelector("#store-results .cover-card")) return false;
  const node = pendingRouteFocus.mode === "restore"
    ? routeFocusNode(routeFocusMemory[view]) || firstContentControl(view)
    : firstContentControl(view);
  if (!node) return false;
  pendingRouteFocus = null;
  focusControl(node);
  return true;
}

let pageScrollFrame = null;
let pageScrollTarget = null;

function pageScroller() {
  return document.scrollingElement || document.documentElement;
}

function pageScrollTop() {
  const root = pageScroller();
  return root.scrollTop ?? window.scrollY ?? 0;
}

function cancelPageScroll() {
  if (pageScrollFrame !== null) {
    cancelAnimationFrame(pageScrollFrame);
    pageScrollFrame = null;
  }
  pageScrollTarget = null;
}

// One animation owner for the page keeps rapid focus moves from stacking
// native smooth-scroll requests, which is what makes repeated arrow presses
// look like stuttering.
function smoothScrollPageTo(target) {
  const root = pageScroller();
  const viewport = window.innerHeight || root.clientHeight || 0;
  const max = Math.max(0, (root.scrollHeight || document.body.scrollHeight || 0) - viewport);
  const destination = Math.max(0, Math.min(Math.round(target), max));
  const start = pageScrollTop();
  if (Math.abs(destination - start) < 1) {
    cancelPageScroll();
    root.scrollTop = destination;
    return;
  }
  if (pageScrollTarget === destination && pageScrollFrame !== null) return;
  cancelPageScroll();
  pageScrollTarget = destination;
  const duration = Math.min(520, Math.max(240, Math.abs(destination - start) * 0.35));
  const started = performance.now();
  let last = Math.round(start);
  const step = (now) => {
    const progress = Math.min(1, (now - started) / duration);
    const eased = 1 - Math.pow(1 - progress, 3);
    const next = Math.round(start + (destination - start) * eased);
    if (next !== last) {
      root.scrollTop = next;
      last = next;
    }
    if (progress < 1) {
      pageScrollFrame = requestAnimationFrame(step);
    } else {
      root.scrollTop = destination;
      pageScrollFrame = null;
      pageScrollTarget = null;
    }
  };
  pageScrollFrame = requestAnimationFrame(step);
}

window.addEventListener("wheel", cancelPageScroll, { passive: true });
window.addEventListener("touchmove", cancelPageScroll, { passive: true });

function scrollControlIntoView(node) {
  const margin = 28;
  // Home hero and the top menu belong to the same vertical stop: reveal the
  // whole hero instead of only the focused Play button, and keep it fully in
  // view while the sticky top menu owns focus.
  if (node.closest(".home-hero")) {
    smoothScrollPageTo(0);
    return;
  }
  if (node.closest(".topbar")) {
    if (appView === "home") smoothScrollPageTo(0);
    return;
  }
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
    smoothScrollPageTo(pageScrollTop() + rect.top - header - 16);
  } else if (rect.bottom > window.innerHeight - 16) {
    smoothScrollPageTo(pageScrollTop() + rect.bottom - (window.innerHeight - 16));
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
  rememberRouteFocus(node);
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
      // Selection follows focus within Library only.
      setLibrarySelection(node.dataset.managed);
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
    if (routeSelection.home?.id !== game.id) return;
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
  routeSelection.home = id ? { id, section } : null;
  for (const card of document.querySelectorAll(".home-page .cover-card")) {
    const railSection = card.closest(".managed-rail")?.dataset.rail;
    card.classList.toggle("active", card.dataset.managed === id && railSection === section);
  }
}

function setLibrarySelection(id) {
  routeSelection.library = id ? { id } : null;
  for (const card of document.querySelectorAll(".library-page .cover-card")) {
    card.classList.toggle("active", card.dataset.managed === id);
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
    if (focusFirstContent()) return true;
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
  if (current.closest(".home-page")) {
    // Home uses an explicit vertical chain: top menu -> hero Play -> cards.
    if (current.id === "home-play" || current.id === "home-view") {
      if (action === "up") {
        const nav = activeNavControl();
        if (nav) { focusControl(nav); return true; }
      }
      if (action === "down") {
        const card = firstContentControl("home");
        if (card) { focusControl(card); return true; }
      }
    }
    if (action === "up" && current.matches?.(".cover-card[data-managed]")) {
      const rail = current.closest(".managed-rail");
      const rails = [...document.querySelectorAll(".home-page .managed-rail")]
        .filter((node) => node.querySelector(".cover-card"));
      if (rail && rails[0] === rail) {
        const play = document.querySelector("#home-play:not([disabled]), #home-view:not([disabled])");
        if (play) { focusControl(play); return true; }
      }
    }
  }
  if (action === "next" || action === "previous") {
    const index = controls.indexOf(current);
    const step = action === "next" ? 1 : -1;
    focusControl(controls[(index + step + controls.length) % controls.length]);
    return true;
  }
  // Home rails wrap with modulo; focus moves across the last visible cards
  // and then loops back to the start of the same rail.
  if ((action === "left" || action === "right") && current.matches?.(".cover-card[data-managed]")) {
    const rail = current.closest(".rail-scroll");
    if (rail && rail.closest(".home-page")) {
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
  const currentRect = current.getBoundingClientRect();
  const bestIn = (nodes) => {
    let best = null;
    for (const node of nodes) {
      if (node === current) continue;
      const [cx, cy] = center(node);
      const dx = cx - x;
      const dy = cy - y;
      const horizontal = action === "left" || action === "right";
      if (horizontal) {
        // Left/Right stays on the visual row; a chip or pager above/below is
        // not a horizontal neighbour.
        const nodeRect = node.getBoundingClientRect();
        if (Math.abs(dy) > (currentRect.height + nodeRect.height) / 2) continue;
      }
      const along = horizontal ? (action === "right" ? dx : -dx) : (action === "down" ? dy : -dy);
      const across = horizontal ? Math.abs(dy) : Math.abs(dx);
      if (along <= 1) continue;
      const score = along + across * 3;
      if (!best || score < best.score) best = { score, node };
    }
    return best;
  };
  // Keep focus inside the current region (store sidebar vs catalogue).
  // Up/Down never cross regions; Left/Right is the explicit pane switch.
  const region = regionOf(current);
  const regional = controls.filter((node) => regionOf(node) === region);
  if (action === "left" || action === "right") {
    if (region === "sidebar") {
      // The sidebar is a vertical stack; Right is the way out.
      return action === "right" ? focusStoreContent() : false;
    }
    if (region === "content") {
      // Horizontal movement still walks the catalogue grid; only the left
      // edge leaves for the sidebar.
      const best = bestIn(regional);
      if (best) {
        focusControl(best.node);
        return true;
      }
      return action === "left" ? focusStoreSidebar() : false;
    }
    const best = bestIn(regional);
    if (best) {
      focusControl(best.node);
      return true;
    }
    return false;
  }
  const best = bestIn(regional);
  if (best) {
    focusControl(best.node);
    return true;
  }
  // At the top of a route body, Up reaches that route's own tab. Focus never
  // leaks sideways into the Store's other pane or a sibling tab.
  if (action === "up" && region !== "header") {
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
      if (closeTopDialog()) return;
      document.activeElement.blur();
      if (!document.body.classList.contains("settings-open")) {
        const nav = activeNavControl();
        if (nav) focusControl(nav);
      }
      return;
    }
    if (closeTopDialog()) return;
    if (profileDialogOpen()) { closeProfileDialog(); return; }
    if (document.body.classList.contains("settings-open")) { closeSettingsPage(); return; }
    if (appView === "store-game") { setView("store", { focus: "restore" }); return; }
    if (appView === "game") { setView(gameDetailOrigin, { focus: "restore" }); return; }
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
  const entry = dialogEntryFor(profileDialog);
  if (entry) cleanupDialog(entry);
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
  const entry = registerDialog(profileDialog, { title: "Account" });
  if (entry) raiseDialog(entry);
  profileDialog.classList.remove("hidden");
  syncDialogBlocking();
  focusControl(document.querySelector("#profile-logout"));
}

document.querySelector("#profile").onclick = () => { void openProfileDialog(); };
document.querySelector("#profile-close-icon").onclick = closeProfileDialog;
document.querySelector("#profile-logout").onclick = async () => {
  try {
    await invoke("profile_logout");
    closeProfileDialog();
    showAuthGate("Signed out. Sign in to continue.");
  } catch (error) {
    showToast(String(error), true);
  }
};

function applyFullscreenState(fullscreen) {
  document.body.classList.toggle("fullscreen-mode", fullscreen);
  for (const button of document.querySelectorAll("#managed-options")) {
    button.disabled = fullscreen;
    button.title = fullscreen ? "Game settings are available in desktop mode" : "Game options";
  }
  if (fullscreen) {
    const modal = document.querySelector(".game-settings-modal");
    const backdrop = modal?.closest(".modal-backdrop");
    if (backdrop) {
      const entry = dialogEntryFor(backdrop);
      if (entry) dismissDialog(entry);
      else backdrop.remove();
    }
  }
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
  registerDialog(backdrop);
  const close = () => backdrop.remove();
  backdrop.querySelector(".modal-close").onclick = close;
  backdrop.querySelector("#fullscreen-reboot").onclick = async () => {
    if (!(await confirmDialog({ title: "Reboot system", message: "Reboot the system? Save any work in other applications first.", confirmLabel: "Reboot", danger: true }))) return;
    try { await invoke("reboot_system"); } catch (error) { showToast(String(error), true); }
  };
  backdrop.querySelector("#fullscreen-shutdown").onclick = async () => {
    if (!(await confirmDialog({ title: "Shut down system", message: "Shut down the system? Save any work in other applications first.", confirmLabel: "Shut down", danger: true }))) return;
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
  syncAppInert();
  try {
    const auth = await invoke("profile_status");
    if (!auth.authenticated) {
      showAuthGate("Sign in to SteamTron to manage your Steam library.");
      return;
    }
    hideAuthGate();
    await load();
  } catch (error) {
    showAuthGate(`Could not check sign-in: ${error}`);
  }
}

const gateQuit = document.querySelector("#gate-quit");
const gateSpinner = document.querySelector("#gate-spinner");
let signInRunning = false;

async function runSignIn() {
  if (signInRunning) return;
  signInRunning = true;
  gateSpinner.classList.remove("hidden");
  gateSignin.disabled = true;
  gateStatus.textContent = "Opening your browser to sign in…";
  try {
    const auth = await invoke("profile_login");
    if (!auth.authenticated) {
      gateStatus.textContent = "Sign-in did not complete. Click Sign in to try again.";
      return;
    }
    hideAuthGate();
    await load();
  } catch (error) {
    gateStatus.textContent = String(error);
  } finally {
    signInRunning = false;
    gateSpinner.classList.add("hidden");
    gateSignin.disabled = false;
  }
}

gateSignin.onclick = () => { void runSignIn(); };
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

function showManagedLoading() {
  document.body.classList.add("managed");
  detailEl.innerHTML = `<div class="page loading-page">
    <header class="page-head"><h1>Loading your games…</h1><p>Preparing the home screen.</p></header>
    <div class="cover-grid">${Array.from({ length: 12 }, () => '<div class="cover-card skeleton"><span class="cover"></span><span class="cover-title">&nbsp;</span></div>').join("")}</div>
  </div>`;
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
    registerDialog(backdrop);
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
  registerDialog(backdrop);
  const statusNode = backdrop.querySelector("#launch-editor-status");
  const setStatus = (message, error = false) => { statusNode.textContent = message; statusNode.classList.toggle("error", error); };
  const close = () => backdrop.remove();
  backdrop.querySelector(".modal-close").onclick = close;
  backdrop.querySelector("#launch-cancel").onclick = close;
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
      if (!(await confirmDialog({ title: "Delete launch option", message: "Delete this launch option?", confirmLabel: "Delete", danger: true }))) return;
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
    if (!(await confirmDialog({ title: "Remove Steam library entry", message: `Remove ${game.name} from the Steam library?`, confirmLabel: "Remove", danger: true }))) return;
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
        <button class="danger" id="remove" ${hasFixes ? "" : "disabled"}>Remove fixes entry</button>
        <button id="launch-option" ${hasFixes ? "" : "disabled"}>${current?.launch_option_set ? "Clear launch option" : "Set launch option"}</button>
      </div>
    </div>
    <div class="status${hasFixes && !current.launch_option_set ? " error" : ""}">${current?.launch_option_error ? escape(current.launch_option_error) : hasFixes ? (current.launch_option_set ? "Fixes installed." : "The Steam launch option is not set. Click Set launch option, then fully restart Steam.") : "No fixes installed. Launch options are available independently."}</div>`;
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
    if (!(await confirmDialog({ title: "Add Windows Security exclusions", message: `Add scoped Microsoft Defender exclusions for ${game.name}? This does not disable antivirus. It requires Windows admin approval.`, confirmLabel: "Add" }))) return;
    const stop = busy(securityApply, "Adding…");
    try {
      await invoke("windows_security_apply", { appid: game.appid, includeGame: true });
      await loadSecurity();
      status("Windows Security exception added for this game.");
    } catch (error) { status(String(error), true); }
    finally { stop(); }
  };
  securityRemove.onclick = async () => {
    if (!(await confirmDialog({ title: "Remove Windows Security exclusions", message: `Remove the Windows Security exclusions for ${game.name}?`, confirmLabel: "Remove", danger: true }))) return;
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
  panel.querySelector("#launch-option").onclick = async () => {
    const button = panel.querySelector("#launch-option");
    if (!await ensureSteamClosed(current?.launch_option_set ? "clear the launch option" : "set the launch option")) return;
    const enabled = !current?.launch_option_set;
    const stop = busy(button, enabled ? "Setting…" : "Clearing…");
    try { const message = await invoke("apply_launch_option", { appid: game.appid, enabled }); await loadFixes(); markRestartRequired(`${game.name} launch option changed. Restart Steam to apply.`); stop(); renderDetail(game, "fixes"); status(message); }
    catch (error) { stop(); status(String(error), true); }
  };
  panel.querySelector("#remove").onclick = async () => {
    if (!(await confirmDialog({ title: "Remove fixes entry", message: "Remove this fixes entry and delete its fixes folder from disk?", confirmLabel: "Remove", danger: true }))) return;
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
  const appid = String(game.appid || game.metadata_app_id || "");
  const gameName = game.name || game.title;
  const load = async () => {
    const button = host.querySelector("#updates-refresh");
    const stop = busy(button, "Checking…");
    summary.classList.remove("error");
    summary.textContent = "Loading archived builds…";
    list.replaceChildren();
    try {
      data = await invoke("update_versions", { appid });
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
    try { data = await invoke("clear_update_pin", { appid }); selectedVersion = data.installed || data.latest || ""; markRestartRequired(`${gameName} update pin cleared. Restart Steam to apply.`); stop(); render(); }
    catch (error) { stop(); summary.textContent = String(error); summary.classList.add("error"); }
  };
  pin.onclick = async () => {
    if (!selectedVersion) return;
    const stop = busy(pin, "Pinning…");
    try { data = await invoke("update_pin", { appid, date: selectedVersion }); markRestartRequired(`${gameName} update pin changed. Restart Steam to apply.`); stop(); render(); }
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
  // Decide the shell before painting: a managed library must never flash the
  // legacy Steam skeleton.
  try { managedLibrary = await invoke("library_snapshot"); }
  catch (_error) { managedLibrary = null; }
  if (managedShell()) showManagedLoading();
  else if (!games.length) showLibraryLoading();
  await loadBackend();
  try {
    let legacyError = null;
    try {
      games = await invoke("list_games");
      await loadFixes();
      try {
        const added = await invoke("auto_launch_targets");
        if (added.length) await loadFixes();
      } catch (_error) { /* non-fatal */ }
    } catch (error) {
      // A managed library does not depend on Steam's install scan. Keep the
      // snapshot path usable while Steam is missing or still starting.
      legacyError = error;
      games = [];
    }
    try { await invoke("auto_pin_installed"); } catch (_error) { /* non-fatal */ }
    try { await invoke("reconcile_managed_installs"); } catch (_error) { /* non-fatal */ }
    try { await invoke("reconcile_registrations"); } catch (_error) { /* keep stored states */ }
    try { managedLibrary = await invoke("library_snapshot"); } catch (_error) { managedLibrary = null; }
    await refreshManagedRunning();
    if (managedShell()) {
      document.body.classList.add("managed");
      renderManagedMode();
      focusRouteOnEntry(appView, "first");
      void loadArtwork();
      setTimeout(() => { void warmStore(); }, 1200);
      return true;
    }
    if (legacyError) throw legacyError;
    document.body.classList.remove("managed");
    if (appView === "home") {
      void loadArtwork();
      setView("library");
      return true;
    }
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
    focusRouteOnEntry(appView, "first");
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
    if (!(await confirmDialog({ title: "Repair Steam integration", message: "Repair installs SLSsteam Moon into your existing Steam installation. Steam itself is not installed by this action. Start Steam manually afterward. Continue?", confirmLabel: "Repair" }))) return;
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

let settingsReturnFocus = null;

function toggleSettingsMaximize() {
  const page = settingsPage();
  if (!page) return;
  const maximized = page.classList.toggle("settings-maximized");
  const button = document.querySelector("#settings-maximize");
  if (button) {
    button.title = maximized ? "Restore down" : "Maximize";
    button.setAttribute("aria-label", `${maximized ? "Restore down" : "Maximize"} settings`);
  }
}

async function openSettingsPage() {
  const active = document.activeElement;
  settingsReturnFocus = active && active !== document.body && !settingsPage().contains(active)
    ? active
    : document.querySelector("#settings");
  document.body.classList.add("settings-open");
  document.querySelector("#settings-backdrop")?.classList.remove("hidden");
  settingsPage().classList.remove("hidden");
  syncAppInert();
  await renderSettingsPage();
  const activeTab = settingsPage().querySelector(".settings-tab.active");
  if (activeTab) focusControl(activeTab);
}

function closeSettingsPage() {
  stopProtonInstallPolling();
  const returnFocus = settingsReturnFocus;
  settingsReturnFocus = null;
  const page = settingsPage();
  if (page) {
    page.classList.add("hidden");
    page.classList.remove("settings-maximized");
  }
  document.querySelector("#settings-backdrop")?.classList.add("hidden");
  document.body.classList.remove("settings-open");
  syncAppInert();
  const maximize = document.querySelector("#settings-maximize");
  if (maximize) {
    maximize.title = "Maximize";
    maximize.setAttribute("aria-label", "Maximize settings");
  }
  if (navState.focused) navState.focused.classList.remove("nav-focus");
  navState.focused = null;
  if (returnFocus && document.contains(returnFocus) && !returnFocus.disabled
    && document.querySelector("#auth-gate").classList.contains("hidden")) {
    focusControl(returnFocus);
  }
}

async function refreshSettingsLibrary() {
  try { settingsLibrary = await invoke("library_snapshot"); }
  catch (_error) { settingsLibrary = null; }
  return settingsLibrary;
}

async function renderSettingsPage() {
  stopProtonInstallPolling();
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
  const prefix = settings.default_prefix_mode || "umu";
  const gamemode = settings.gamemode !== false;
  const email = profile?.email || (profile?.authenticated ? "Signed in" : "Signed out");
  const activity = diagnostics
    ? `${diagnostics.games} games · ${diagnostics.files} files · ${formatBytes(diagnostics.bytes)}`
    : "Unavailable";
  host.innerHTML = `
    <section class="settings-section"><h2>Display mode</h2><p>Default mode applies on the next app start. Enter fullscreen from the mode button; leave through fullscreen options or the tray.</p><div class="settings-card">
      ${settingsRow("Default launch mode", "Desktop or fullscreen when a game starts.", `<select id="setting-display-mode"><option value="desktop"${mode === "desktop" ? " selected" : ""}>Desktop</option><option value="fullscreen"${mode === "fullscreen" ? " selected" : ""}>Fullscreen</option></select>`)}
      ${settingsRow("Default prefix source", "Used for new games. Steam prefixes borrow Steam's compatdata; umu prefixes live inside the SteamTron library.", `<select id="setting-prefix-mode"><option value="umu"${prefix === "umu" ? " selected" : ""}>umu (SteamTron)</option><option value="steam"${prefix === "steam" ? " selected" : ""}>Steam compatdata</option></select>`)}
      ${settingsRow("Feral GameMode", "Automatically optimize CPU governor, GPU performance, and process priorities while gaming, then revert on exit.", `<label class="switch"><input type="checkbox" id="setting-gamemode"${gamemode ? " checked" : ""}><span></span></label>`)}
    </div></section>
    <section class="settings-section"><h2>Account</h2><p>Session used for manifests, library actions and Steam registration.</p><div class="settings-card">
      ${settingsRow(email, profile?.base_url || "SteamTron account", '<button id="setting-signout">Sign out</button>')}
    </div></section>
    <section class="settings-section"><h2>Maintenance</h2><p>Library backup, restore and desktop integration.</p><div class="settings-card">
      <div class="settings-row"><div class="row-text"><strong>Game metadata backups</strong><span>Exports one ZIP per game with the exact appmanifest and manifest bundle. Game content and fixes are not included.</span></div><div class="row-control settings-inline"><button id="setting-export-games" data-operation-button="library-export">Export</button><button id="setting-import-games">Import ZIPs…</button><button id="setting-import-folder">Import folder…</button></div></div>
      ${settingsRow("Desktop shortcut", "Add SteamTron to the application menu.", '<button id="setting-shortcut">Install</button>')}
      ${settingsRow("Steam client", "Restart Steam to apply library changes.", '<button id="setting-restart">Restart Steam</button>')}
    </div></section>
    <section class="settings-section"><h2>Steam diagnostics</h2><p>Validate Steam, fixes and the unlock backend.</p><div class="settings-card">
      ${settingsRow("Doctor", "Checks Steam, the Lua folder, fixes and the platform unlock backend.", '<button id="setting-doctor-run">Run checks</button>')}
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
  host.querySelector("#setting-prefix-mode").onchange = async (event) => {
    try {
      await invoke("update_library_settings", { defaultPrefixMode: event.target.value });
      showToast(`Default prefix source: ${event.target.value}`);
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
  host.querySelector("#setting-export-games").onclick = async () => {
    const operation = ensureOperationManager("library-export", "", {
      operation: "library-export",
      title: "Exporting game backups",
    });
    if (!operation) return;
    beginOperation("library-export", "");
    await operation.ready;
    try {
      const result = await invoke("export_game_backups", { destination: null });
      if (!result) { cancelOperation("library-export", ""); operation.discard(); showToast("Export cancelled."); return; }
      finishOperation("library-export", "", `Exported ${result.games} games`);
      operation.complete(`Exported ${result.games} game backups to ${result.folder}`);
      const generated = result.generated_lua ? `, generated ${result.generated_lua} Lua` : "";
      const missing = result.missing_manifests ? `, ${result.missing_manifests} manifests still missing` : "";
      showToast(`Exported ${result.games} games (${result.files} files${generated}${missing})`);
    } catch (error) {
      if (operation.isCancelled()) return;
      failOperation("library-export", "", String(error));
      operation.fail(String(error));
    }
  };
  const importMetadata = async (event, mode) => {
    let paths = [];
    try { paths = await invoke("pick_import_paths", { mode }); }
    catch (error) { showToast(String(error), true); return; }
    if (!paths?.length) return;
    const operation = ensureOperationManager("library-import", "", {
      operation: "library-import",
      title: "Importing game metadata",
    });
    if (!operation) return;
    beginOperation("library-import", "");
    await operation.ready;
    try {
      const result = await invoke("import_game_metadata", { paths });
      finishOperation("library-import", "", `Imported ${result.imported} games`);
      operation.complete(`Imported ${result.imported} games`);
      await load();
      const warnings = result.warnings?.length ? ` · ${result.warnings.length} warning${result.warnings.length === 1 ? "" : "s"}` : "";
      showToast(`Imported ${result.imported} games${result.skipped ? `, skipped ${result.skipped}` : ""}${warnings}`);
    } catch (error) {
      if (operation.isCancelled()) return;
      failOperation("library-import", "", String(error));
      operation.fail(String(error));
    }
  };
  host.querySelector("#setting-import-games").onclick = (event) => { event.stopPropagation(); void importMetadata(event, "files"); };
  host.querySelector("#setting-import-folder").onclick = (event) => { event.stopPropagation(); void importMetadata(event, "folder"); };
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
    if (!(await confirmDialog({ title: "Clear diagnostic logs", message: "Clear all diagnostic logs? Active logs are kept.", confirmLabel: "Clear", danger: true }))) return;
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
    const backdrop = document.createElement("div");
    backdrop.className = "modal-backdrop";
    backdrop.innerHTML = `<div class="modal doctor-modal">
      <div class="modal-head"><h2>Steam checks</h2><button class="modal-close" id="doctor-close" aria-label="Close">×</button></div>
      <p class="pane-note" id="doctor-status">Running checks…</p>
      <div class="doctor-results" id="doctor-results"><div class="settings-row"><div class="row-text"><span>Checking Steam, Lua, fixes and the unlock backend…</span></div></div></div>
      <div class="modal-actions"><button class="primary" id="doctor-repair">Repair Steam library</button><button id="doctor-refresh">Recheck</button><button id="doctor-dismiss">Close</button></div>
    </div>`;
    document.body.appendChild(backdrop);
    registerDialog(backdrop);
    const close = () => backdrop.remove();
    backdrop.querySelector("#doctor-close").onclick = close;
    backdrop.querySelector("#doctor-dismiss").onclick = close;

    const checkRepair = (check) => {
      if (check.id === "managed-appmanifest") return { label: "Repair appmanifests", command: "repair_library_entries" };
      if (check.id === "steam-process" || check.id === "steam-injection") return { label: "Start Steam", command: "restart_steam" };
      if (check.id === "unlock-backend" || check.id === "managed-ownership" || check.repair === "install-unlock-backend") {
        return { label: `Install ${check.title}`, command: "repair_steam_library", confirm: true };
      }
      if (check.id === "stplug-in") return { label: "Repair Steam library", command: "repair_steam_library", confirm: true };
      return null;
    };

    const runRepair = async (button, command) => {
      const stop = busy(button, "Repairing…");
      const status = backdrop.querySelector("#doctor-status");
      if (status) status.textContent = "Repairing… this can take up to a minute.";
      try {
        const result = await invoke(command);
        if (command === "repair_library_entries") {
          const games = Array.isArray(result) ? result.length : 0;
          showToast(`Repaired ${games} appmanifest${games === 1 ? "" : "s"}`);
        } else if (command === "restart_steam") {
          showToast(String(result));
        } else {
          showToast(String(result));
        }
        await runChecks();
      } catch (error) {
        stop();
        if (status) status.textContent = "Repair failed.";
        showToast(String(error), true);
      }
    };

    const runChecks = async () => {
      const status = backdrop.querySelector("#doctor-status");
      const results = backdrop.querySelector("#doctor-results");
      status.textContent = "Running checks…";
      results.innerHTML = '<div class="settings-row"><div class="row-text"><span>Checking Steam, Lua, fixes and the registration backend…</span></div></div>';
      try {
        const report = await invoke("doctor_report");
        const checks = report.checks || [];
        const failed = checks.some((check) => check.status !== "ok");
        status.textContent = failed ? "Some checks need attention." : "All checks passed.";
        results.innerHTML = checks.map((check) => {
          const repair = check.status === "ok" ? null : checkRepair(check);
          const action = repair
            ? `<button data-doctor-repair="${repair.command}" data-doctor-confirm="${repair.confirm ? "1" : ""}">${repair.label}</button>`
            : "";
          return `<div class="settings-row"><div class="row-text"><strong>${escape(check.title)}</strong><span>${escape(check.detail)}</span></div><div class="row-control">${action}${settingsStatusBadge(check.status)}</div></div>`;
        }).join("") || '<div class="settings-row"><div class="row-text"><span>No checks reported.</span></div></div>';
        for (const button of results.querySelectorAll("[data-doctor-repair]")) {
          button.onclick = async () => {
            if (button.dataset.doctorConfirm
              && !(await confirmDialog({ title: "Repair Steam components", message: "This repair can install or change SLSsteam Moon inside your existing Steam installation. Steam itself is not installed. Continue?", confirmLabel: "Repair" }))) return;
            void runRepair(button, button.dataset.doctorRepair);
          };
        }
      } catch (reportError) {
        status.textContent = "Checks could not run.";
        results.innerHTML = `<div class="settings-row"><div class="row-text"><span class="status error">${escape(String(reportError))}</span></div></div>`;
      }
    };

    backdrop.querySelector("#doctor-refresh").onclick = () => { void runChecks(); };
    backdrop.querySelector("#doctor-repair").onclick = async () => {
      if (!(await confirmDialog({ title: "Repair SteamTron integration", message: "Repair SteamTron's Steam integration? This can install SLSsteam Moon into your existing Steam installation; Steam itself is not installed. Start Steam manually afterward.", confirmLabel: "Repair" }))) return;
      void runRepair(backdrop.querySelector("#doctor-repair"), "repair_steam_library");
    };
    await runChecks();
    stop();
  };
}

async function showAddDriveDialog() {
  const backdrop = document.createElement("div");
  backdrop.className = "modal-backdrop";
  backdrop.innerHTML = `<div class="modal drive-modal">
    <div class="modal-head"><button id="drive-cancel">Cancel</button><h2>Add game drive</h2><button id="drive-add" class="primary" disabled>Add drive</button></div>
    <p class="pane-note">Choose a mounted partition.</p>
    <p class="pane-note">Registration keeps existing game locations unchanged. No partition, folder or game file is created.</p>
    <div id="drive-choices" class="drive-choices"><div class="status">Checking connected drives…</div></div>
    <div class="update-actions"><button id="drive-refresh">Refresh drives</button></div>
  </div>`;
  document.body.appendChild(backdrop);
  registerDialog(backdrop);
  const choices = backdrop.querySelector("#drive-choices");
  const add = backdrop.querySelector("#drive-add");
  let selected = null;
  const close = () => backdrop.remove();
  backdrop.querySelector("#drive-cancel").onclick = close;
  backdrop.querySelector(".modal-close")?.addEventListener("click", close);

  const load = async () => {
    selected = null;
    add.disabled = true;
    choices.innerHTML = '<div class="status">Checking connected drives…</div>';
    let drives = [];
    try { drives = await invoke("available_library_drives"); }
    catch (error) {
      choices.innerHTML = `<div class="status error">${escape(String(error))}</div>`;
      return;
    }
    if (!drives.length) {
      choices.innerHTML = '<div class="status">No mounted internal partitions found. Mount a supported internal partition, then refresh.</div>';
      return;
    }
    choices.innerHTML = drives.map((drive) => {
      const blocked = drive.registered || !drive.writable;
      const reason = drive.registered
        ? "Already registered — change roles on its Storage card."
        : !drive.writable
          ? "This drive is mounted read-only."
          : `${formatBytes(drive.free)} available of ${formatBytes(drive.total)} · Writable`;
      return `<label class="drive-choice${blocked ? " disabled" : ""}">
        <input type="radio" name="library-drive" value="${escape(drive.path)}"${blocked ? " disabled" : ""}>
        <span class="drive-choice-copy"><strong>${escape(drive.label)}</strong><small>${escape(drive.path)}</small><small>${escape(reason)}</small></span>
      </label>`;
    }).join("");
    for (const radio of choices.querySelectorAll("input[name=library-drive]")) {
      radio.onchange = () => {
        selected = radio.value;
        add.disabled = !selected;
      };
    }
    const first = choices.querySelector("input[name=library-drive]:not(:disabled)");
    if (first) focusControl(first);
  };

  add.onclick = async () => {
    if (!selected) return;
    const stop = busy(add, "Adding…");
    try {
      const result = await invoke("add_library_drive", { path: selected });
      managedLibrary = result;
      close();
      await renderSettingsPage();
      showToast("Drive added");
    } catch (error) {
      stop();
      showToast(String(error), true);
    }
  };
  backdrop.querySelector("#drive-refresh").onclick = () => { void load(); };
  await load();
}

async function renderStorageSettings(host) {
  const snapshot = await refreshSettingsLibrary();
  const drives = snapshot?.drives || [];
  const games = snapshot?.games || [];
  const libraryPath = snapshot?.path || "";
  const dataDir = libraryPath.replace(/\/[^/]+$/, "");
  const available = await invoke("available_library_drives").catch(() => []);
  const availableByPath = new Map(available.map((drive) => [drive.path, drive]));
  const gamesOn = (drive) => games.filter((game) => {
    const root = game.installation?.root || "";
    return root === drive.root || root.startsWith(`${drive.root.replace(/\/+$/, "")}/`);
  }).length;
  const rows = drives.map((drive) => {
    const disk = availableByPath.get(drive.root);
    const used = disk && disk.total ? Math.min(100, Math.max(0, 100 - (disk.free / disk.total) * 100)) : 0;
    const usage = disk
      ? `<div class="drive-usage"><span style="width:${used}%"></span></div><span>${formatBytes(disk.free)} available of ${formatBytes(disk.total)}</span>`
      : "";
    return `<div class="settings-row drive-row">
      <div class="row-text"><strong>${escape(drive.label || drive.id)}${drive.default ? " · Default" : ""}</strong><span>${escape(drive.root)} · ${gamesOn(drive)} game${gamesOn(drive) === 1 ? "" : "s"}</span>${usage}</div>
      <div class="row-control">${drive.default ? "" : `<button data-default-drive="${escape(drive.id)}">Make default</button>`}<button data-open-drive="${escape(drive.root)}">Open</button><button class="danger" data-remove-drive="${escape(drive.id)}">Remove</button></div>
    </div>`;
  }).join("");
  host.innerHTML = `
    <section class="settings-section"><h2>Your game drives</h2><p>Added drives are the only destinations for managed games. Game content lives under <code>SteamTronLibrary/steamtronapps</code>; registration never moves existing files.</p><div class="settings-card">
      <div class="settings-row"><div class="row-text"><strong>Registered game drives</strong><span>Select one of these mounted roots when adding or restoring a game.</span></div><div class="row-control"><button id="setting-refresh-drives">Refresh drives</button></div></div>
      ${rows || '<div class="settings-row"><div class="row-text"><span>No drives registered yet.</span></div></div>'}
      <div class="settings-row"><div class="row-text"><strong>Add drive</strong><span>Choose a mounted partition. No folder or game file is created.</span></div><div class="row-control"><button id="setting-add-drive">Add drive…</button></div></div>
    </div></section>
    <section class="settings-section"><h2>Game backups</h2><p>Restore a portable SteamTron game backup to a registered game or create its library entry automatically.</p><div class="settings-card">
      <div class="settings-row"><div class="row-text"><strong>Restore backup folder</strong><span>Choose a SteamTron backup folder; the game is added to your library automatically if needed.</span></div><div class="row-control"><button id="setting-restore-game" class="primary" data-operation-button="restore">Restore…</button></div></div>
      <div class="operation-inline hidden" data-operation-inline="restore"><div class="download-inline-track"><span></span></div><div class="download-inline-meta"><span class="operation-inline-text">Starting…</span></div></div>
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
  for (const button of host.querySelectorAll("[data-remove-drive]")) {
    button.onclick = async () => {
      if (!(await confirmDialog({ title: "Remove drive", message: "Remove this drive registration? Only the registration is removed. Games, saves and folders stay on the drive.", confirmLabel: "Remove", danger: true }))) return;
      try {
        await invoke("remove_library_drive", { id: button.dataset.removeDrive });
        await load();
        await renderSettingsPage();
        showToast("Drive registration removed");
      } catch (error) { showToast(String(error), true); }
    };
  }
  host.querySelector("#setting-restore-game").onclick = async () => {
    const operation = ensureOperationManager("restore", "", {
      operation: "restore",
      title: "Restoring game backup",
      gameName: "Choose a backup file",
    });
    if (!operation) return;
    beginOperation("restore", "");
    await operation.ready;
    try {
      const result = await invoke("restore_game_backup");
      if (!result) { cancelOperation("restore", ""); operation.discard(); showToast("Restore cancelled."); return; }
      finishOperation("restore", "", `Restored ${result.title || "game"}`);
      operation.complete(`Restored ${result.title || "game"} to ${result.root}`);
      managedLibrary = result.library;
      await load();
      await renderSettingsPage();
      showToast(`Restored ${result.title || "game"} to ${result.root}`);
    } catch (error) {
      if (operation.isCancelled()) return;
      failOperation("restore", "", String(error));
      operation.fail(String(error));
    }
  };
  host.querySelector("#setting-refresh-drives").onclick = () => { void renderSettingsPage(); };
  host.querySelector("#setting-add-drive").onclick = () => { void showAddDriveDialog(); };
  const openData = host.querySelector("#setting-open-data");
  if (openData) openData.onclick = async () => {
    try { await invoke("open_path", { path: dataDir }); }
    catch (error) { showToast(String(error), true); }
  };
}

let protonSource = "GE-Proton";
let protonInstallTimer = null;
let protonPollGraceUntil = 0;
const protonNoticeShown = new Set();
const protonSourceCache = new Map();

function stopProtonInstallPolling() {
  if (protonInstallTimer) {
    clearInterval(protonInstallTimer);
    protonInstallTimer = null;
  }
}

function protonInstallActive(state) {
  return ["starting", "downloading", "extracting", "registering"].includes(state.status);
}

function protonReleaseKey(release) {
  return `${release.family}|${release.tag}|${release.name}`;
}

function protonIcon(action, key, title, path) {
  return `<button class="proton-icon" data-proton-action="${action}" data-proton-key="${escape(key)}" title="${escape(title)}" aria-label="${escape(title)}"><svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">${path}</svg></button>`;
}

function protonProgressMarkup(state) {
  const percent = Math.max(0, Math.min(100, Number(state.percent) || 0));
  const key = protonReleaseKey(state);
  const paused = state.status === "paused";
  const actions = [];
  if (paused) {
    actions.push(protonIcon("resume", key, "Resume", '<path d="M8 5l11 7-11 7z"></path>'));
  } else if (state.pid) {
    actions.push(protonIcon("pause", key, "Pause", '<path d="M9 5v14M15 5v14"></path>'));
  }
  if (paused || state.pid) {
    actions.push(protonIcon("cancel", key, "Cancel", '<path d="M6 6l12 12M18 6L6 18"></path>'));
  }
  return `<div class="proton-progress"><div class="proton-progress-track"><span style="width:${percent}%"></span></div><div class="proton-progress-meta"><small>${percent.toFixed(0)}% · ${escape(state.message || state.status)}</small><div class="proton-progress-actions">${actions.join("")}</div></div></div>`;
}

async function renderProtonSettings(host) {
  const data = await invoke("proton_runners").catch(() => ({ runners: [], default: "" }));
  const runners = data.runners || [];
  const defaultPath = data.default || "";
  const options = runners.map((runner) =>
    `<option value="${escape(runner.path)}"${runner.path === defaultPath ? " selected" : ""}>${escape(runner.name)}</option>`
  ).join("");
  const sources = ["GE-Proton", "UMU-Proton", "Proton-CachyOS", "Valve-Proton"];
  host.innerHTML = `
    <section class="settings-section"><h2>Default Proton runner</h2><p>Used for new managed games unless a game overrides it.</p><div class="settings-card">
      <div class="settings-row"><div class="row-text"><strong>Default runner</strong><span>${escape(defaultPath || "Not selected")}</span></div><div class="row-control"><select id="setting-default-runner">${options || '<option value="">No runners installed</option>'}</select></div></div>
    </div></section>
    <section class="settings-section"><h2>Download Proton</h2><p>Official releases from each source. Downloads are verified before they are added as runners.</p>
      <div class="proton-source-head">
        <div class="proton-source-tabs">${sources.map((family) => `<button class="proton-source-tab${family === protonSource ? " active" : ""}" data-proton-source="${family}">${family}</button>`).join("")}</div>
        <button id="proton-source-refresh" class="secondary">Refresh</button>
      </div>
      <div class="settings-card" id="proton-source-list"><div class="settings-row"><div class="row-text"><span>Loading releases…</span></div></div></div>
    </section>`;

  const select = host.querySelector("#setting-default-runner");
  if (select) select.onchange = async () => {
    if (!select.value) return;
    try {
      await invoke("set_default_proton_runner", { path: select.value });
      await renderSettingsPage();
      showToast("Default Proton runner updated");
    } catch (error) { showToast(String(error), true); }
  };

  const list = host.querySelector("#proton-source-list");
  const renderReleases = (releases, installStates) => {
    if (!releases.length) {
      list.innerHTML = '<div class="settings-row"><div class="row-text"><span>No downloadable releases were found.</span></div></div>';
      return;
    }
    const stateFor = (release) => installStates.find((state) => protonReleaseKey(state) === protonReleaseKey(release));
    list.innerHTML = releases.map((release, index) => {
      let control = "";
      const state = stateFor(release);
      if (state && (protonInstallActive(state) || state.status === "paused")) {
        control = protonProgressMarkup(state);
      } else if (release.installed) {
        const actions = [];
        if (release.installed_path === defaultPath) {
          actions.push('<span class="settings-badge ok">default</span>');
        } else {
          actions.push(`<button data-proton-default="${escape(release.installed_path)}">Set as default</button>`);
        }
        if (release.removable) {
          actions.push(`<button class="danger" data-proton-remove="${escape(release.installed_path)}">Uninstall</button>`);
        }
        control = actions.join("");
      } else if (release.installable) {
        control = `<button data-proton-install="${index}">Install</button>`;
      }
      const detail = [release.name, release.size ? formatBytes(release.size) : "", release.installed ? "Installed" : "Available"].filter(Boolean).join(" · ");
      const key = protonReleaseKey(release);
      return `<div class="settings-row" data-proton-row="${escape(key)}"><div class="row-text"><strong>${escape(release.tag)}</strong><span>${escape(detail)}</span></div><div class="row-control">${control}</div></div>`;
    }).join("");
    for (const button of list.querySelectorAll("[data-proton-default]")) {
      button.onclick = async () => {
        try {
          await invoke("set_default_proton_runner", { path: button.dataset.protonDefault });
          await renderSettingsPage();
          showToast("Default Proton runner updated");
        } catch (error) { showToast(String(error), true); }
      };
    }
    for (const button of list.querySelectorAll("[data-proton-remove]")) {
      button.onclick = async () => {
        const release = releases.find((entry) => entry.installed_path === button.dataset.protonRemove);
        if (!release) return;
        if (!(await confirmDialog({ title: "Uninstall Proton release", message: `Uninstall ${release.tag}? The downloaded Proton files will be deleted.`, confirmLabel: "Uninstall", danger: true }))) return;
        const stop = busy(button, "Uninstalling…");
        try {
          await invoke("uninstall_proton_release", { path: button.dataset.protonRemove });
          protonSourceCache.clear();
          await renderSettingsPage();
          showToast(`Uninstalled ${release.tag}`);
        } catch (error) {
          stop();
          showToast(String(error), true);
        }
      };
    }
    for (const button of list.querySelectorAll("[data-proton-install]")) {
      button.onclick = async () => {
        const release = releases[Number(button.dataset.protonInstall)];
        if (!release) return;
        const row = button.closest("[data-proton-row]");
        const control = row?.querySelector(".row-control");
        if (control) {
          control.innerHTML = protonProgressMarkup({ percent: 0, status: "starting", message: "Starting…" });
        }
        try {
          await invoke("install_proton_release", { family: release.family, tag: release.tag, name: release.name });
          protonPollGraceUntil = Date.now() + 15000;
          ensurePolling();
          window.setTimeout(() => { void pollInstalls(); }, 400);
        } catch (error) {
          if (control) control.innerHTML = `<button data-proton-install="${index}">Install</button>`;
          showToast(String(error), true);
        }
      };
    }
  };
  const loadSource = async (family, { force = false } = {}) => {
    protonSource = family;
    for (const tab of host.querySelectorAll("[data-proton-source]")) {
      tab.classList.toggle("active", tab.dataset.protonSource === family);
    }
    let installStates = [];
    try { installStates = await invoke("proton_install_states"); } catch (_error) { installStates = []; }
    const cached = protonSourceCache.get(family);
    if (cached && !force) {
      const finished = installStates.some((state) => state.family === family
        && state.status === "done"
        && cached.some((release) => release.tag === state.tag && !release.installed));
      if (!finished) {
        renderReleases(cached, installStates);
        return;
      }
    }
    list.innerHTML = '<div class="settings-row"><div class="row-text"><span>Loading releases…</span></div></div>';
    try {
      const releases = await invoke("proton_source_releases", { family, force });
      protonSourceCache.set(family, releases);
      renderReleases(releases, installStates);
    } catch (error) {
      list.innerHTML = `<div class="settings-row"><div class="row-text"><span class="status error">${escape(String(error))}</span></div><div class="row-control"><button id="proton-source-retry">Retry</button></div></div>`;
      list.querySelector("#proton-source-retry").onclick = () => { void loadSource(family, { force: true }); };
    }
  };
  const pollInstalls = async () => {
    let states = [];
    try { states = await invoke("proton_install_states"); } catch (_error) { states = []; }
    let active = false;
    for (const state of states) {
      if (!protonInstallActive(state)) continue;
      active = true;
      const key = protonReleaseKey(state);
      const row = [...list.querySelectorAll("[data-proton-row]")].find((node) => node.dataset.protonRow === key);
      if (row) {
        const control = row.querySelector(".row-control");
        if (control) control.innerHTML = protonProgressMarkup(state);
      }
    }
    const failed = states.find((state) => state.status === "error"
      && Date.now() / 1000 - state.updated_at < 30
      && !protonNoticeShown.has(protonReleaseKey(state)));
    if (failed) {
      protonNoticeShown.add(protonReleaseKey(failed));
      showToast(failed.message || "Proton installation failed.", true);
    }
    if (active) {
      protonPollGraceUntil = 0;
    } else if (Date.now() < protonPollGraceUntil) {
      return;
    } else {
      stopProtonInstallPolling();
      protonSourceCache.delete(protonSource);
      await loadSource(protonSource);
    }
  };
  const ensurePolling = () => {
    if (protonInstallTimer) return;
    protonInstallTimer = setInterval(() => { void pollInstalls(); }, 1000);
  };
  list.addEventListener("click", async (event) => {
    const action = event.target.closest("[data-proton-action]");
    if (!action) return;
    const [family, tag, name] = action.dataset.protonKey.split("|");
    if (!family || !tag || !name) return;
    try {
      if (action.dataset.protonAction === "pause") {
        await invoke("pause_proton_install", { family, tag, name });
        showToast("Proton installation paused");
        await loadSource(protonSource);
      } else if (action.dataset.protonAction === "resume") {
        await invoke("resume_proton_install", { family, tag, name });
        showToast("Resuming Proton installation");
        protonPollGraceUntil = Date.now() + 15000;
        ensurePolling();
        void pollInstalls();
      } else if (action.dataset.protonAction === "cancel") {
        await invoke("cancel_proton_install", { family, tag, name });
        showToast("Proton installation cancelled");
        await loadSource(protonSource);
      }
    } catch (error) {
      showToast(String(error), true);
    }
  });
  const refresh = host.querySelector("#proton-source-refresh");
  if (refresh) refresh.onclick = () => {
    const stop = busy(refresh, "Refreshing…");
    void loadSource(protonSource, { force: true }).finally(stop);
  };
  for (const tab of host.querySelectorAll("[data-proton-source]")) {
    tab.onclick = () => {
      focusControl(tab);
      void loadSource(tab.dataset.protonSource);
    };
  }
  await loadSource(protonSource);
  if ((await invoke("proton_install_states").catch(() => [])).some(protonInstallActive)) {
    protonPollGraceUntil = Date.now() + 15000;
    ensurePolling();
    void pollInstalls();
  }
}

for (const tab of document.querySelectorAll("[data-settings-tab]")) {
  tab.onclick = () => {
    focusControl(tab);
    settingsTab = tab.dataset.settingsTab;
    void renderSettingsPage();
  };
}
document.querySelector("#settings-close").onclick = closeSettingsPage;
document.querySelector("#settings-maximize").onclick = toggleSettingsMaximize;

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
let storeGeneration = 0;
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
    openManagedGame(known.id, "store", base);
    return;
  }
  let item = base;
  // Paint from the list entry immediately. The full request can be slow when
  // the catalogue came from the cache and the API is unreachable.
  storeDetail = item;
  appView = "store-game";
  renderStoreDetail();
  try {
    const full = await invoke("store_game", { id: Number(id) });
    if (!full || !full.id) return;
    if (appView !== "store-game" || String(storeDetail?.id) !== String(id)) return;
    item = full;
    storeDetail = item;
    renderStoreDetail({ focus: false });
  } catch (_error) {
    // The list entry is enough for the detail page.
  }
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
  registerDialog(backdrop);
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
    progress.setStatus("Downloading the Lua manifest…");
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
    if (result.warning) showToast(result.warning);
    else showToast(`Added ${result.title} to your library`);
    openManagedGame(result.game_id, "store", item);
  } catch (error) {
    progress.fail(String(error));
    stop();
    showToast(String(error), true);
  }
}

function renderStoreDetail({ focus = true } = {}) {
  const item = storeDetail;
  if (!item) { setView("store"); return; }
  // A background refresh replaces the detail DOM; remember the focused
  // control so the user's focus survives the repaint.
  const previousFocus = !focus && detailEl.contains(document.activeElement)
    ? document.activeElement.id
      ? `#${CSS.escape(document.activeElement.id)}`
      : document.activeElement.dataset?.copy
        ? `[data-copy="${CSS.escape(document.activeElement.dataset.copy)}"]`
        : null
    : null;
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
  detailEl.querySelector("#store-back").onclick = () => setView("store", { focus: "restore" });
  renderGameChips(detailEl.querySelector("#store-chips"), chips);
  if (!item.steam_app_id) {
    void invoke("store_resolve_steam_cached", { igdbId: Number(item.id), title: item.name }).then((appid) => {
      if (!appid || appView !== "store-game" || storeDetail?.id !== item.id) return;
      storeDetail = { ...storeDetail, steam_app_id: appid };
      const managed = managedGameForStoreItem(storeDetail);
      if (managed) { openManagedGame(managed.id, "store", storeDetail); return; }
      renderStoreDetail({ focus: false });
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
  // Land on Add to library so a Store title can be added without hunting for
  // the button. Titles without a Steam build fall back to Back. Background
  // refreshes must not move the user's focus.
  if (focus) {
    const focusTarget = firstContentControl("store-game");
    if (focusTarget) focusControl(focusTarget);
  } else if (previousFocus) {
    const restored = detailEl.querySelector(previousFocus);
    if (restored && !restored.disabled) focusControl(restored);
  }
}

async function mapLimit(items, limit, worker) {
  const results = new Array(items.length);
  let index = 0;
  const runners = Array.from({ length: Math.min(limit, items.length) }, async () => {
    while (index < items.length) {
      const current = index;
      index += 1;
      results[current] = await worker(items[current]);
    }
  });
  await Promise.all(runners);
  return results;
}

const steamAvailability = new Map();

async function resolveSteamForItem(item) {
  if (item.steam_app_id) return item.steam_app_id;
  const key = String(item.id);
  if (steamAvailability.has(key)) return steamAvailability.get(key);
  try {
    const appid = await invoke("store_resolve_steam_cached", {
      igdbId: Number(item.id),
      title: item.name,
    });
    const value = appid || null;
    steamAvailability.set(key, value);
    return value;
  } catch (_error) {
    return undefined; // network failure: keep the item rather than emptying the page
  }
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
  storeGeneration += 1;
  storeBuffer = [];
  storeServerPage = 0;
  storeExhausted = false;
  storePage = 1;
  storeError = "";
  storeRefreshing = false;
}

// The Rust side caches each catalogue page and the genre list. Read those
// first so browsing the Store does not hit the API on every launch and keeps
// working when the API is unreachable.
async function loadStoreGenres() {
  if (storeGenres.length) return storeGenres;
  try {
    const cached = await invoke("store_cached_genres");
    if (Array.isArray(cached) && cached.length) {
      storeGenres = cached;
      return storeGenres;
    }
  } catch (_error) { /* fall through to the network */ }
  try { storeGenres = await invoke("store_genres"); }
  catch (_error) { storeGenres = []; }
  return storeGenres;
}

async function cachedStorePage(page) {
  try {
    const data = await invoke("store_cached_catalog", {
      query: storeQuery,
      genre: storeGenre,
      sort: storeSort,
      page,
    });
    return data && Array.isArray(data.items) ? data : null;
  } catch (_error) {
    return null;
  }
}

async function absorbStorePage(data, serverPage, generation) {
  const items = data.items || [];
  const existing = new Set(storeBuffer.map((entry) => entry.id));
  const candidates = items.filter((item) => isPcCatalogItem(item) && !existing.has(item.id));
  // Verify Steam availability (cached per IGDB id) and keep only titles that
  // genuinely exist on Steam; edition variants are matched.
  const checked = await mapLimit(candidates, 5, async (item) => {
    const steam = await resolveSteamForItem(item);
    if (steam === undefined) return item;
    if (!steam) return null;
    return { ...item, steam_app_id: steam };
  });
  if (generation !== storeGeneration) return false;
  storeServerPage = Math.max(storeServerPage, serverPage);
  for (const item of checked) {
    if (!item) continue;
    if (!storeBuffer.some((entry) => entry.id === item.id)) storeBuffer.push(item);
  }
  const totalPages = data.total_pages || 1;
  if (!items.length || storeServerPage >= totalPages) storeExhausted = true;
  return true;
}

async function ensureStorePage(page, render = true) {
  const generation = storeGeneration;
  storePage = Math.max(1, page);
  storeLoading = true;
  storeRefreshing = false;
  storeError = "";
  if (render) renderStorePage();
  const target = storePage * STORE_PAGE_SIZE;
  let guard = 0;

  await loadStoreGenres();
  if (generation !== storeGeneration) return;

  // Paint cached server pages first, with no API request for them.
  while (storeBuffer.length < target && !storeExhausted && guard < 20) {
    guard += 1;
    const nextServerPage = storeServerPage + 1;
    const cached = await cachedStorePage(nextServerPage);
    if (generation !== storeGeneration) return;
    if (!cached) break;
    await absorbStorePage(cached, nextServerPage, generation);
    if (generation !== storeGeneration) return;
  }
  if (storeBuffer.length) {
    storeLoading = false;
    storeRefreshing = true;
    if (render || appView === "store") renderStorePage();
  }

  // Only pages that are not cached are fetched from the network.
  try {
    while (storeBuffer.length < target && !storeExhausted && guard < 20) {
      guard += 1;
      const nextServerPage = storeServerPage + 1;
      const data = await invoke("store_catalog", {
        query: storeQuery,
        genre: storeGenre,
        sort: storeSort,
        page: nextServerPage,
      });
      if (generation !== storeGeneration) return;
      await absorbStorePage(data, nextServerPage, generation);
      if (generation !== storeGeneration) return;
    }
  } catch (error) {
    // A cached catalogue stays usable when the API is offline.
    if (generation === storeGeneration && !storeBuffer.length) storeError = String(error);
  }
  storeLoading = false;
  storeRefreshing = false;
  storeTried = true;
  // Warm-up fetches often finish after the user opened the Store; always
  // repaint then so the screen never stays on "Loading…".
  if (render || appView === "store") renderStorePage();
}

async function openStore() {
  // If an earlier warm-up failed without cache, retry when the user opens the
  // Store instead of leaving the error on screen.
  if (!storeBuffer.length) {
    resetStorePaging();
    await ensureStorePage(1);
  } else if (storeError) {
    await ensureStorePage(storePage);
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
  const knownPages = storeExhausted ? Math.max(1, Math.ceil(storeBuffer.length / STORE_PAGE_SIZE)) : Infinity;
  const windowStart = Math.max(1, storePage - 2);
  const pageNumbers = [];
  for (let page = windowStart; page < windowStart + 5 && page <= knownPages; page += 1) {
    pageNumbers.push(page);
  }
  const count = storeLoading
    ? "Loading…"
    : storeError
      ? "Store unavailable"
      : storeRefreshing
        ? "Saved catalogue · updating…"
        : `${storeBuffer.length} PC titles`;
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
          ${pageNumbers.map((page) => `<button class="pager-number${page === storePage ? " active" : ""}" data-page-number="${page}">${page}</button>`).join("")}
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
  for (const button of detailEl.querySelectorAll("[data-page-number]")) {
    button.onclick = () => {
      const next = Number(button.dataset.pageNumber);
      if (Number.isInteger(next) && next !== storePage) void ensureStorePage(next);
    };
  }
  for (const card of detailEl.querySelectorAll("[data-store-id]")) {
    card.onclick = () => {
      rememberRouteFocus(card);
      openStoreItem(card.dataset.storeId);
    };
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
  applyPendingRouteFocus("store");
}

function showAddGame() {
  const backdrop = document.createElement("div");
  backdrop.className = "modal-backdrop";
  backdrop.innerHTML = `<div class="modal"><div class="modal-head"><h2>Add game</h2><button class="modal-close" id="add-close" aria-label="Close">×</button></div><p class="muted">Search the Steam catalogue by game name, App ID, or Steam URL. Select the exact result before adding.</p><label for="add-search">Search</label><div class="search-row"><input id="add-search" placeholder="Game name, 3468650, or Steam URL"><button id="add-search-button">Search</button></div><div class="search-results"></div><div class="selected-game" style="display:none"></div><div class="modal-status"></div><div class="security-option" id="add-security-option" hidden><label><input type="checkbox" id="add-security"> Add a Windows Security exception before fixes</label><div class="muted">Adds scoped Microsoft Defender exclusions for SteamTron's fix, upper-layer, and mounted-overlay paths, plus the game folder. It does not disable antivirus. Requires Windows admin approval.</div></div><div class="modal-actions" id="add-actions" hidden><button id="add-import" disabled>Import ZIP/Lua for selected game</button><button id="add-submit" class="primary" disabled>Add selected game</button></div></div>`;
  document.body.appendChild(backdrop);
  registerDialog(backdrop);
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

document.querySelector("#nav-library").onclick = () => setView("library", { focus: "restore" });
document.querySelector("#nav-home").onclick = () => setView("home", { focus: "restore" });
document.querySelector("#nav-store").onclick = () => setView("store", { focus: "restore" });
window.addEventListener("focus", async () => {
  if (!managedShell()) return;
  const active = document.activeElement;
  const detailHadFocus = detailEl.contains(active);
  await refreshManagedRunning();
  if (appView === "library" || appView === "home" || appView === "game") {
    const focusLost = !active || active === document.body || !document.contains(active);
    const focusUnchanged = document.activeElement === active;
    renderManagedMode();
    if (focusUnchanged && (detailHadFocus || focusLost)
      && !visibleModal()
      && !document.querySelector("#settings-page:not(.hidden)")) {
      focusRouteOnEntry(appView, "restore");
    }
  }
});

let runningPollBusy = false;
setInterval(async () => {
  if (!managedShell() || runningPollBusy) return;
  runningPollBusy = true;
  try {
    const next = new Set(await invoke("running_managed"));
    const now = Date.now();
    for (const [id, until] of runningGrace) {
      if (next.has(id) || until <= now) runningGrace.delete(id);
      else next.add(id);
    }
    const changed = next.size !== runningManaged.size
      || [...next].some((id) => !runningManaged.has(id));
    if (changed) {
      runningManaged = next;
      if (!document.body.classList.contains("settings-open")
        && ["home", "library", "game"].includes(appView)) {
        renderManagedMode();
      }
    }
  } catch (_error) {
    // Keep the last observed state.
  } finally {
    runningPollBusy = false;
  }
}, 2000);
document.querySelector("#settings").onclick = () => {
  if (document.body.classList.contains("fullscreen-mode")) openFullscreenOptions();
  else void openSettingsPage();
};
document.querySelector("#refresh").onclick = refreshLibrary;
searchEl.oninput = () => { if (hasManagedLibrary()) renderManagedList(); else renderGames(); };
window.addEventListener("error", (event) => {
  console.error(event.error || event.message);
  showToast(`UI error: ${event.message}`, true);
});
window.addEventListener("unhandledrejection", (event) => {
  console.error(event.reason);
  showToast(`UI error: ${event.reason}`, true);
});
boot();
