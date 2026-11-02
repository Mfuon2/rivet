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
api.tabs.query({active: true, currentWindow: true}).then(tabs => { tab = tabs[0]; document.getElementById("download-page").hidden = !/^https:\/\/(www\.|m\.|music\.)?youtube\.com\/(watch|shorts\/|live\/)/.test(tab?.url||""); return refresh(); }).catch(() => { status.textContent = "Could not inspect the current tab."; });

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

document.getElementById("download-page").addEventListener("click",async()=>{
  status.textContent="Starting the video download…";
  try {const reply=await api.runtime.sendMessage({type:"download-page",url:tab?.url});status.textContent=reply?.ok?"Download requested. Rivet opens automatically.":reply?.error||"Update and open Rivet.";}
  catch {status.textContent="Update the Rivet desktop app and reload this extension.";}
});

let syncTimer;
async function syncDownloads() {
  try {
    const state=await api.runtime.sendMessage({type:"sync-status"});
    document.getElementById("connection-status").textContent=state?.connected ? `Connected${state.version?" · Rivet "+state.version:""}` : "Desktop not connected. Choose Finish setup to install or repair Rivet.";
    document.getElementById("sync-status").textContent=!state?.connected?"Waiting for the desktop connection.":state.liveUnavailable?"Update or repair Rivet to enable live download status.":state.running?"Synced with your desktop app.":"Desktop helper connected. Rivet opens when you request a download.";
    const list=document.getElementById("desktop-jobs");list.replaceChildren();
    for(const job of state?.jobs||[]) {
      const row=document.createElement("div");row.className="media";
      const name=document.createElement("p");name.textContent=job.name;
      const status=document.createElement("small");status.textContent=`${job.status} · ${(job.bytes/1048576).toFixed(1)} MiB${job.total>0?" / "+(job.total/1048576).toFixed(1)+" MiB":""}`;
      row.append(name,status);
      if(job.total>0){const progress=document.createElement("progress");progress.setAttribute("aria-label",job.name+" download progress");progress.max=1;progress.value=Math.min(1,job.bytes/job.total);row.append(progress);}
      list.append(row);
    }
    if(state?.connected && !list.childElementCount){const note=document.createElement("p");note.textContent="Downloads you send from this profile will appear here.";list.append(note);}
  } catch {document.getElementById("sync-status").textContent="Could not sync. Retry the desktop connection.";}
  syncTimer=setTimeout(syncDownloads,2500);
}
document.getElementById("open-desktop").addEventListener("click",()=>api.runtime.sendMessage({type:"open-desktop"}).then(result=>{if(!result?.ok)status.textContent="Open Finish setup to install or reconnect Rivet.";}).catch(()=>{status.textContent="Open Finish setup to install or reconnect Rivet.";}));
window.addEventListener("pagehide",()=>clearTimeout(syncTimer));
syncDownloads();
