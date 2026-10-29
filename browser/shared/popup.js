"use strict";
const api = globalThis.browser || globalThis.chrome;
const status = document.getElementById("status");
document.getElementById("setup").addEventListener("click", () => api.tabs.create({url: api.runtime.getURL("setup.html")}));
let tab;

async function send(url) {
  status.textContent = "Adding to your local queue…";
  try {
    const response = await api.runtime.sendMessage({type: "capture", url});
    status.textContent = response?.ok ? "Queued. Open Rivet to review and start it." : response?.error || "Handoff failed.";
  } catch { status.textContent = "Cannot reach the extension worker. Reload the extension."; }
}

async function refresh() {
  const list = document.getElementById("media");
  list.replaceChildren();
  const response = await api.runtime.sendMessage({type: "list", tabId: tab?.id});
  for (const item of response?.items || []) {
    const row = document.createElement("div"); row.className = "media";
    const label = document.createElement("p");
    try { const url = new URL(item.url); label.textContent = `${item.kind}: ${url.hostname}${url.pathname}`; }
    catch { continue; }
    const button = document.createElement("button"); button.textContent = item.kind === "playlist" ? "Save playlist file" : "Queue media file";
    button.addEventListener("click", () => send(item.url));
    row.append(label, button); list.append(row);
  }
  if (!list.childElementCount) { const p = document.createElement("p"); p.textContent = "No media detected in this worker session."; list.append(p); }
}

document.getElementById("capture").addEventListener("submit", event => { event.preventDefault(); send(document.getElementById("url").value); });
document.getElementById("refresh").addEventListener("click", () => refresh().catch(() => { status.textContent = "Could not refresh media."; }));
document.getElementById("enable").addEventListener("click", async () => {
  try {
    const origin = new URL(tab.url);
    if (!["http:", "https:"].includes(origin.protocol)) throw new Error();
    const granted = await api.permissions.request({origins: [`${origin.origin}/*`]});
    const response = granted && await api.runtime.sendMessage({type: "enable", origin: origin.origin});
    status.textContent = response?.ok ? "Enabled for this site. Reload the page and start playback." : "Permission was not granted or discovery could not be enabled.";
  } catch { status.textContent = "Site capture is available on HTTP/HTTPS pages only."; }
});
document.getElementById("disable").addEventListener("click", async () => {
  try {
    const origin = new URL(tab.url).origin;
    await api.permissions.remove({origins: [`${origin}/*`]});
    await api.runtime.sendMessage({type: "disable", origin});
    await api.runtime.sendMessage({type: "clear", tabId: tab.id});
    status.textContent = "Site permission removed and detected links cleared.";
    await refresh();
  } catch { status.textContent = "Could not change site permission."; }
});
api.tabs.query({active: true, currentWindow: true}).then(tabs => { tab = tabs[0]; return refresh(); }).catch(() => { status.textContent = "Could not inspect the current tab."; });

// An explicit, queue-free native-host probe. Never forwards a page URL.
document.getElementById("check").addEventListener("click", async () => {
  const button = document.getElementById("check");
  const connection = document.getElementById("connection-status");
  button.disabled = true;
  connection.textContent = "Checking desktop helper…";
  try {
    const reply = await api.runtime.sendMessage({type: "ping"});
    connection.textContent = reply?.ok ? "Connected. Links can be queued in Rivet." : reply?.error || "No response from Rivet.";
  } catch { connection.textContent = "Could not reach the extension worker. Reload this extension."; }
  finally { button.disabled = false; }
});
