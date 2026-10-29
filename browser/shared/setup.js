"use strict";
const api = globalThis.browser || globalThis.chrome;
let timer;
let attempts = 0;
let checking = false;
async function connect() {
  if (checking) return;
  checking = true;
  clearTimeout(timer);
  const retry = document.getElementById("retry");
  retry.disabled = true;
  let ok = false;
  try { ok = (await api.runtime.sendMessage({type: "ping"}))?.ok === true; } catch { /* Desktop not ready yet. */ }
  document.getElementById("title").textContent = ok ? "Rivet is ready" : "Waiting for the desktop app";
  document.getElementById("connection").textContent = ok
    ? "Your browser is connected. Return to Rivet to download files. No restart is needed."
    : "The extension is installed. Open Rivet and this page will finish connecting automatically.";
  document.getElementById("help").hidden = ok;
  document.getElementById("close").hidden = !ok;
  retry.hidden = ok;
  retry.disabled = false;
  checking = false;
  if (!ok && ++attempts < 30) timer = setTimeout(connect, 2000);
}
document.getElementById("retry").addEventListener("click", () => { attempts = 0; connect(); });
document.getElementById("close").addEventListener("click", async () => {
  const tab = await api.tabs.getCurrent();
  if (tab?.id !== undefined) await api.tabs.remove(tab.id);
});
connect();
