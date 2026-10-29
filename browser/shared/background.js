/* Original Rivet source. No page scripts, cookie collection, analytics, or network service. */
"use strict";
const api = globalThis.browser || globalThis.chrome;
const HOST = "org.rivet.downloads";
const mediaByTab = new Map();
const MAX_PER_TAB = 50;
let enabledSites = new Set();
const sitesReady = api.storage.local.get("enabledSites").then(value => {
  enabledSites = new Set(Array.isArray(value.enabledSites) ? value.enabledSites : []);
});

function httpUrl(value) {
  try {
    const url = new URL(value);
    return ["https:", "http:"].includes(url.protocol) && !url.username && !url.password;
  } catch { return false; }
}

function mediaKind(details) {
  const type = (details.responseHeaders || []).find(h => h.name.toLowerCase() === "content-type")?.value?.split(";", 1)[0].toLowerCase() || "";
  if (/mpegurl|dash\+xml/.test(type) || /\.(m3u8|mpd)(\?|$)/i.test(details.url)) return "playlist";
  if (/^(video|audio)\//.test(type) || /\.(mp4|webm|m4a|mp3|ogg|mov)(\?|$)/i.test(details.url)) return "media file";
  return null;
}

async function capture(url) {
  if (!httpUrl(url)) throw new Error("Only HTTP/HTTPS links without embedded credentials are accepted.");
  const reply = await api.runtime.sendNativeMessage(HOST, {url});
  if (!reply?.ok) throw new Error(reply?.error || "Rivet did not accept the download.");
  return reply;
}

function announceConnection() {
  api.runtime.sendNativeMessage(HOST, {type: "ping"}).catch(() => {});
}
api.runtime.onStartup.addListener(announceConnection);
api.runtime.onInstalled.addListener(() => {
  announceConnection();
  api.contextMenus.removeAll().then(() => {
    api.contextMenus.create({id: "rivet-link", title: "Send link to Rivet", contexts: ["link"]});
    api.contextMenus.create({id: "rivet-media", title: "Send media file to Rivet", contexts: ["video", "audio"]});
  }).catch(console.error);
});

api.contextMenus.onClicked.addListener(info => {
  if (!["rivet-link", "rivet-media"].includes(info.menuItemId)) return;
  capture(info.linkUrl || info.srcUrl).catch(error => {
    // Log only our fixed error summary; native browser errors can include environment paths.
    console.error("Rivet handoff failed. Open the extension popup to check native-host setup.");
    void error;
  });
});

api.webRequest.onHeadersReceived.addListener(details => {
  if (details.tabId < 0 || !httpUrl(details.url)) return;
  const kind = mediaKind(details);
  if (!kind) return;
  // activeTab can grant temporary host access too. Require our explicit site toggle as well.
  sitesReady.then(() => {
    if (!enabledSites.has(new URL(details.url).origin)) return;
    let items = mediaByTab.get(details.tabId);
    if (!items) mediaByTab.set(details.tabId, items = new Map());
    if (items.size < MAX_PER_TAB || items.has(details.url)) items.set(details.url, {url: details.url, kind});
  }).catch(() => {});
}, {urls: ["http://*/*", "https://*/*"]}, ["responseHeaders"]);

api.tabs.onRemoved.addListener(tabId => mediaByTab.delete(tabId));
api.tabs.onUpdated.addListener((tabId, change) => { if (change.status === "loading") mediaByTab.delete(tabId); });
api.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message?.type === "enable" || message?.type === "disable") {
    (async () => {
      await sitesReady;
      if (!httpUrl(message.origin)) throw new Error("Invalid site origin");
      const origin = new URL(message.origin).origin;
      if (message.type === "enable") {
        if (!await api.permissions.contains({origins: [`${origin}/*`]})) throw new Error("Site access has not been granted");
        enabledSites.add(origin);
      } else {
        enabledSites.delete(origin);
        for (const items of mediaByTab.values()) {
          for (const url of items.keys()) if (new URL(url).origin === origin) items.delete(url);
        }
      }
      await api.storage.local.set({enabledSites: [...enabledSites]});
      sendResponse({ok: true});
    })().catch(() => sendResponse({ok: false, error: "Could not update site discovery."}));
    return true;
  }
  if (message?.type === "ping") {
    api.runtime.sendNativeMessage(HOST, {type: "ping"}).then(reply => {
      sendResponse(reply?.ok === true && reply.id === null
        ? {ok: true}
        : {ok: false, error: "Update the Rivet native helper, then check again."});
    }, () => sendResponse({ok: false, error: "Cannot reach Rivet. Check the helper path, native-host registration and this extension's ID."}));
    return true;
  }
  if (message?.type === "list") {
    sendResponse({items: [...(mediaByTab.get(message.tabId)?.values() || [])]});
    return false;
  }
  if (message?.type === "capture") {
    capture(message.url).then(reply => sendResponse(reply), () => sendResponse({ok: false, error: "Could not reach Rivet. Check native-host registration and try again."}));
    return true;
  }
  if (message?.type === "clear") {
    mediaByTab.delete(message.tabId);
    sendResponse({ok: true});
  }
  return false;
});
