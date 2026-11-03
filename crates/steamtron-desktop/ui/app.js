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

document.querySelector("#refresh").onclick = load;
searchEl.oninput = renderGames;
load();
