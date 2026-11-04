const invoke = (command, args = {}) => window.__TAURI__.core.invoke(command, args);
const gamesEl = document.querySelector("#games");
const detailEl = document.querySelector("#detail");
const searchEl = document.querySelector("#search");
let games = [];
let fixes = new Map();
let selected = null;

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

function status(message, error = false) {
  const node = detailEl.querySelector(".status");
  if (!node) return;
  node.textContent = message;
  node.classList.toggle("error", error);
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

async function boot() {
  try {
    const auth = await invoke("profile_status");
    if (!auth.authenticated) { showAuthGate("Sign in to continue."); return; }
    hideAuthGate();
    await load();
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
  const node = document.querySelector("#toast");
  node.textContent = message;
  node.classList.toggle("error", error);
  node.classList.add("show");
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => node.classList.remove("show"), 2600);
}

function renderGames() {
  const query = searchEl.value.trim().toLowerCase();
  gamesEl.replaceChildren();
  for (const game of games.filter((item) => !query || `${item.name} ${item.appid}`.toLowerCase().includes(query))) {
    const button = document.createElement("button");
    button.className = `game${selected === game.appid ? " active" : ""}`;
    const build = game.build_id && game.build_id !== "0"
      ? `build ${escape(game.build_id)}`
      : game.version ? `version ${escape(game.version)}` : "version unknown";
    const state = game.installed ? "" : '<span class="badge warn">Not installed</span>';
    button.innerHTML = `<span><strong>${escape(game.name)}</strong><span>${escape(game.appid)} · ${build}</span></span>${state}${game.managed ? '<span class="badge">Managed</span>' : ""}`;
    button.addEventListener("click", () => { selected = game.appid; renderGames(); renderDetail(game); });
    gamesEl.appendChild(button);
  }
}

function renderDetail(game, tab = "general") {
  const current = fixes.get(game.appid);
  const tabs = [
    ["general", "General"],
    ["fixes", "Fixes"],
    ["updates", "Updates"],
  ];
  detailEl.innerHTML = `<h2>${escape(game.name)}</h2><p class="muted">Steam App ${escape(game.appid)}</p><div class="game-tabs">${tabs.map(([key, label]) => `<button class="game-tab${key === tab ? " active" : ""}" data-tab="${key}" ${key === "updates" && !game.managed ? "disabled" : ""}>${label}</button>`).join("")}</div><div class="game-panel"></div>`;
  const panel = detailEl.querySelector(".game-panel");
  for (const button of detailEl.querySelectorAll(".game-tab")) button.onclick = () => renderDetail(game, button.dataset.tab);
  if (tab === "updates" && game.managed) { renderUpdatesPanel(panel, game); return; }
  if (tab === "fixes") { renderFixesPanel(panel, game, current); return; }
  renderGeneralPanel(panel, game);
}

function renderGeneralPanel(panel, game) {
  panel.innerHTML = `
    <div class="row"><div class="label">Game directory</div><div class="value">${escape(game.common)}</div></div>
    <div class="row"><div class="label">Library status</div><div class="value">${game.managed ? "Managed through SteamTron" : "Managed by Steam"} · ${game.installed ? "content installed" : "content not installed"}</div></div>
    <div class="actions"><button class="danger" id="remove-library" ${game.managed ? "" : "disabled"}>Remove from library</button></div>
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
      showToast(result.steam_restart_required
        ? `Removed ${game.name} · restart Steam from Settings to apply`
        : `Removed ${game.name} · no library files were present`);
    } catch (error) { stop(); status(String(error), true); }
  };
}

function renderFixesPanel(panel, game, current) {
  panel.innerHTML = `
    <div class="fixes-section">
      <div class="actions">
        <button class="primary" id="install">Import fixes ZIP</button>
        <button id="export" ${current ? "" : "disabled"}>Export fixes ZIP</button>
      </div>
    </div>
    <div class="fixes-separator"></div>
    <div class="fixes-section">
      <div class="row"><div class="label">Current fixes location</div><div class="value">${current ? escape(current.fixes) : "No fixes installed"}</div></div>
      <div class="actions"><button id="use-folder">Use existing fixes folder</button></div>
    </div>
    <div class="fixes-separator"></div>
    <div class="fixes-section">
      <div class="row"><div class="label">Steam launch option</div><div class="value">${current?.launch_option_set ? "Set" : "Not set"}</div></div>
      <div class="actions">
        <button id="toggle" ${current ? "" : "disabled"}>${current?.enabled ? "Disable fixes" : "Enable fixes"}</button>
        <button class="danger" id="remove" ${current ? "" : "disabled"}>Remove fixes entry</button>
        <button id="launch-option" ${current ? "" : "disabled"}>${current?.launch_option_set ? "Clear launch option" : "Set launch option"}</button>
      </div>
    </div>
    <div class="status${current && current.enabled && !current.launch_option_set ? " error" : ""}">${current?.launch_option_error ? escape(current.launch_option_error) : current ? (current.enabled && !current.launch_option_set ? "Fixes are enabled, but the Steam launch option is not set. Click Set launch option, then fully restart Steam." : current.enabled ? "Fixes enabled." : "Fixes installed but disabled.") : "Import a ZIP or choose an existing fixes folder."}</div>`;
  panel.querySelector("#install").onclick = async () => {
    const button = panel.querySelector("#install");
    const stop = busy(button, "Installing…");
    try {
      status("Choose a fixes ZIP…");
      const result = await invoke("install_fixes", { appid: game.appid });
      if (result) {
        fixes.set(game.appid, result);
        await loadFixes();
        stop();
        renderDetail(game, "fixes");
        renderGames();
        status(result.launch_option_error
          ? `Fixes installed, but the Steam launch option failed: ${result.launch_option_error}`
          : "Fixes installed and Steam launch option set. Restart Steam before launching.", !!result.launch_option_error);
      } else { stop(); status("Install cancelled."); }
    } catch (error) { stop(); status(String(error), true); }
  };
  panel.querySelector("#use-folder").onclick = async () => {
    const button = panel.querySelector("#use-folder");
    const stop = busy(button, "Opening…");
    try {
      const result = await invoke("use_fixes_folder", { appid: game.appid });
      if (!result) { stop(); status("Folder selection cancelled."); return; }
      fixes.set(game.appid, result);
      await loadFixes();
      stop();
      renderDetail(game, "fixes");
      renderGames();
      status(result.launch_option_error
        ? `Fixes folder set, but the Steam launch option failed: ${result.launch_option_error}`
        : "Fixes folder set and Steam launch option applied.", !!result.launch_option_error);
    } catch (error) { stop(); status(String(error), true); }
  };
  panel.querySelector("#export").onclick = async () => {
    const button = panel.querySelector("#export");
    const stop = busy(button, "Exporting…");
    try { status("Preparing fixes ZIP…"); const result = await invoke("export_fixes", { appid: game.appid, title: game.name }); status(result ? `Exported ${result}` : "Export cancelled."); }
    catch (error) { status(String(error), true); }
    finally { stop(); }
  };
  panel.querySelector("#toggle").onclick = async () => {
    const button = panel.querySelector("#toggle");
    const stop = busy(button, current.enabled ? "Disabling…" : "Enabling…");
    try {
      const message = await invoke("set_enabled", { appid: game.appid, enabled: !current.enabled });
      await loadFixes();
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
    const enabled = !current?.launch_option_set;
    const stop = busy(button, enabled ? "Setting…" : "Clearing…");
    try { const message = await invoke("apply_launch_option", { appid: game.appid, enabled }); await loadFixes(); stop(); renderDetail(game, "fixes"); status(message); }
    catch (error) { stop(); status(String(error), true); }
  };
  panel.querySelector("#remove").onclick = async () => {
    const button = panel.querySelector("#remove");
    const stop = busy(button, "Removing fixes…");
    try { await invoke("remove_fixes", { appid: game.appid }); fixes.delete(game.appid); await loadFixes(); stop(); renderDetail(game, "fixes"); renderGames(); status("Fixes entry removed."); }
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
    try { data = await invoke("clear_update_pin", { appid: game.appid }); selectedVersion = data.installed || data.latest || ""; stop(); render(); }
    catch (error) { stop(); summary.textContent = String(error); summary.classList.add("error"); }
  };
  pin.onclick = async () => {
    if (!selectedVersion) return;
    const stop = busy(pin, "Pinning…");
    try { data = await invoke("update_pin", { appid: game.appid, date: selectedVersion }); stop(); render(); }
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
  await loadBackend();
  try {
    games = await invoke("list_games");
    await loadFixes();
    renderGames();
    if (selected) {
      const game = games.find((item) => item.appid === selected);
      if (game) renderDetail(game);
    }
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
  try { showToast(await invoke("restart_steam")); }
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
    if (!window.confirm("Repair will install SLSsteam and restart Steam. Continue?")) return;
    const button = host.querySelector("#doctor-repair");
    const stop = busy(button, "Repairing…");
    try {
      const message = await invoke("repair_steam_library");
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
  detailEl.innerHTML = `<div class="doctor-head"><div><h2>Settings</h2><p class="muted">Account and Steam diagnostics.</p></div><div class="update-actions"><button id="settings-shortcut">Install desktop shortcut</button><button id="settings-restart">Restart Steam</button></div></div><div class="settings-tabs"><button class="settings-tab active" id="tab-profile">Profile</button><button class="settings-tab" id="tab-doctor">Doctor</button></div><div id="settings-content"><div class="settings-message status"></div><div id="settings-panel"></div></div>`;
  const panel = detailEl.querySelector("#settings-panel");
  const profileTab = detailEl.querySelector("#tab-profile");
  const doctorTab = detailEl.querySelector("#tab-doctor");
  const settingsRestart = detailEl.querySelector("#settings-restart");
  settingsRestart.onclick = () => restartSteam(settingsRestart);
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

function showAddGame() {
  const backdrop = document.createElement("div");
  backdrop.className = "modal-backdrop";
  backdrop.innerHTML = `<div class="modal"><div class="modal-head"><h2>Add game</h2><button class="modal-close" id="add-close" aria-label="Close">×</button></div><p class="muted">Search the Steam catalogue by game name, App ID, or Steam URL. Select the exact result before adding.</p><label for="add-search">Search</label><div class="search-row"><input id="add-search" placeholder="Game name, 3468650, or Steam URL"><button id="add-search-button">Search</button></div><div class="search-results"></div><div class="selected-game" style="display:none"></div><div class="modal-status"></div><div class="modal-actions" id="add-actions" hidden><button id="add-import" disabled>Import ZIP/Lua for selected game</button><button id="add-submit" class="primary" disabled>Add selected game</button></div></div>`;
  document.body.appendChild(backdrop);
  const queryInput = backdrop.querySelector("#add-search");
  const searchButton = backdrop.querySelector("#add-search-button");
  const resultsNode = backdrop.querySelector(".search-results");
  const selectedNode = backdrop.querySelector(".selected-game");
  const statusNode = backdrop.querySelector(".modal-status");
  const submit = backdrop.querySelector("#add-submit");
  const importButton = backdrop.querySelector("#add-import");
  const actionRow = backdrop.querySelector("#add-actions");
  let selected = null;
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
    try { const result = await invoke("add_game", { appidOrUrl: selected.appid, title: selected.name }); stop(); await finish(result); }
    catch (error) { stop(); setStatus(String(error), true); submit.disabled = false; importButton.disabled = false; }
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

document.querySelector("#add-game").onclick = showAddGame;
document.querySelector("#settings").onclick = renderSettings;
document.querySelector("#refresh").onclick = refreshLibrary;
searchEl.oninput = renderGames;
boot();
