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

function status(message, error = false) {
  const node = detailEl.querySelector(".status");
  if (!node) return;
  node.textContent = message;
  node.classList.toggle("error", error);
}

function renderGames() {
  const query = searchEl.value.trim().toLowerCase();
  gamesEl.replaceChildren();
  for (const game of games.filter((item) => !query || `${item.name} ${item.appid}`.toLowerCase().includes(query))) {
    const button = document.createElement("button");
    button.className = `game${selected === game.appid ? " active" : ""}`;
    button.innerHTML = `<span><strong>${escape(game.name)}</strong><span>${escape(game.appid)} · build ${escape(game.build_id || "unknown")}</span></span>${game.managed ? '<span class="badge">Managed</span>' : ""}`;
    button.addEventListener("click", () => { selected = game.appid; renderGames(); renderDetail(game); });
    gamesEl.appendChild(button);
  }
}

function renderDetail(game) {
  const current = fixes.get(game.appid);
  detailEl.innerHTML = `
    <h2>${escape(game.name)}</h2>
    <p class="muted">Steam App ${escape(game.appid)}</p>
    <div class="row"><div class="label">Game directory</div><div class="value">${escape(game.common)}</div></div>
    <div class="row"><div class="label">Fixes</div><div class="value">${current ? escape(current.fixes) : "No fixes installed"}</div></div>
    <div class="actions">
      <button class="primary" id="install">Install fixes ZIP</button>
      <button id="export" ${current ? "" : "disabled"}>Export fixes ZIP</button>
      <button id="toggle" ${current ? "" : "disabled"}>${current?.enabled ? "Disable fixes" : "Enable fixes"}</button>
      <button class="danger" id="remove" ${current ? "" : "disabled"}>Remove entry</button>
    </div>
    <div class="status">${current ? (current.enabled ? "Fixes enabled." : "Fixes installed but disabled.") : "Install a ZIP to enable the fixes overlay."}</div>`;
  detailEl.querySelector("#install").onclick = async () => {
    try { status("Choose a fixes ZIP…"); const result = await invoke("install_fixes", { appid: game.appid }); if (result) { fixes.set(game.appid, result); await loadFixes(); renderDetail(game); renderGames(); status("Fixes installed."); } else status("Install cancelled."); }
    catch (error) { status(String(error), true); }
  };
  detailEl.querySelector("#export").onclick = async () => {
    try { status("Preparing fixes ZIP…"); const result = await invoke("export_fixes", { appid: game.appid }); status(result ? `Exported ${result}` : "Export cancelled."); }
    catch (error) { status(String(error), true); }
  };
  detailEl.querySelector("#toggle").onclick = async () => {
    try { await invoke("set_enabled", { appid: game.appid, enabled: !current.enabled }); await loadFixes(); renderDetail(game); status(current.enabled ? "Fixes disabled." : "Fixes enabled."); }
    catch (error) { status(String(error), true); }
  };
  detailEl.querySelector("#remove").onclick = async () => {
    try { await invoke("remove_fixes", { appid: game.appid }); fixes.delete(game.appid); await loadFixes(); renderDetail(game); renderGames(); status("Fixes entry removed."); }
    catch (error) { status(String(error), true); }
  };
}

async function loadBackend() {
  const node = document.querySelector("#backend");
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
}

async function load() {
  loadBackend();
  try { games = await invoke("list_games"); await loadFixes(); renderGames(); if (selected) { const game = games.find((item) => item.appid === selected); if (game) renderDetail(game); } }
  catch (error) { detailEl.innerHTML = `<div class="status error">${escape(error)}</div>`; }
}

async function loadDoctor(target = null) {
  const host = target || detailEl;
  host.innerHTML = `<div class="doctor-head"><div><h2>Doctor</h2><p class="muted">Validate Steam and fixes requirements.</p></div><button id="doctor-refresh">Recheck</button></div><div class="doctor-list"><div class="status">Running checks…</div></div>`;
  try {
    const report = await invoke("doctor_report");
    const list = host.querySelector(".doctor-list");
    list.replaceChildren();
    for (const item of report.checks) {
      const row = document.createElement("div");
      row.className = `doctor-item ${item.status}`;
      row.innerHTML = `<div class="doctor-title">${escape(item.title)}</div><div class="doctor-detail">${escape(item.detail)}</div>`;
      if (item.repair) {
        const actions = document.createElement("div");
        actions.className = "doctor-actions";
        const button = document.createElement("button");
        button.textContent = item.repair === "install-unlock-backend" ? "Repair Steam library support" : "Repair";
        button.onclick = async () => {
          try { button.disabled = true; const result = await invoke("repair_backend"); renderSettingsMessage(result); await loadDoctor(host); }
          catch (error) { button.disabled = false; renderSettingsMessage(String(error), true); }
        };
        actions.appendChild(button);
        row.appendChild(actions);
      }
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
      try { await invoke("profile_login"); await renderProfile(host); }
      catch (error) { host.innerHTML = `<div class="status error">${escape(error)}</div>`; }
    };
    if (signout) signout.onclick = async () => {
      try { await invoke("profile_logout"); await renderProfile(host); }
      catch (error) { host.innerHTML = `<div class="status error">${escape(error)}</div>`; }
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
  detailEl.innerHTML = `<div class="doctor-head"><div><h2>Settings</h2><p class="muted">Account and Steam diagnostics.</p></div></div><div class="settings-tabs"><button class="settings-tab active" id="tab-profile">Profile</button><button class="settings-tab" id="tab-doctor">Doctor</button></div><div id="settings-content"><div class="settings-message status"></div><div id="settings-panel"></div></div>`;
  const panel = detailEl.querySelector("#settings-panel");
  const profileTab = detailEl.querySelector("#tab-profile");
  const doctorTab = detailEl.querySelector("#tab-doctor");
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
  backdrop.innerHTML = `<div class="modal"><h2>Add game</h2><p class="muted">Add a Steam library entry from the manifest API or a local ZIP.</p><label for="add-id">App ID or Steam store URL</label><input id="add-id" placeholder="3468650 or https://store.steampowered.com/app/3468650"><label for="add-title">Title (optional)</label><input id="add-title" placeholder="Game title"><div class="modal-status"></div><div class="modal-actions"><button id="add-cancel">Cancel</button><button id="add-import">Import ZIP</button><button id="add-submit" class="primary">Add from SteamTron</button></div></div>`;
  document.body.appendChild(backdrop);
  const idInput = backdrop.querySelector("#add-id");
  const titleInput = backdrop.querySelector("#add-title");
  const statusNode = backdrop.querySelector(".modal-status");
  const submit = backdrop.querySelector("#add-submit");
  const importButton = backdrop.querySelector("#add-import");
  const close = () => backdrop.remove();
  const setStatus = (message, error = false) => { statusNode.textContent = message; statusNode.classList.toggle("error", error); };
  backdrop.querySelector("#add-cancel").onclick = close;
  backdrop.addEventListener("click", (event) => { if (event.target === backdrop) close(); });
  idInput.focus();
  const finish = async (result) => {
    if (!result) return;
    setStatus(`Added ${result.title}. Restart Steam to see it in the library.`);
    await load();
    if (result.appmanifest) setStatus(`Added ${result.title}. Appmanifest: ${result.appmanifest}`);
  };
  submit.onclick = async () => {
    submit.disabled = true; importButton.disabled = true; setStatus("Downloading manifest bundle…");
    try { await finish(await invoke("add_game", { appidOrUrl: idInput.value, title: titleInput.value })); }
    catch (error) { setStatus(String(error), true); submit.disabled = false; importButton.disabled = false; }
  };
  importButton.onclick = async () => {
    submit.disabled = true; importButton.disabled = true; setStatus("Choose a Lua manifest ZIP…");
    try { await finish(await invoke("import_game_zip", { appidOrUrl: idInput.value, title: titleInput.value })); }
    catch (error) { setStatus(String(error), true); submit.disabled = false; importButton.disabled = false; }
  };
}

document.querySelector("#add-game").onclick = showAddGame;
document.querySelector("#settings").onclick = renderSettings;
document.querySelector("#refresh").onclick = load;
searchEl.oninput = renderGames;
load();
