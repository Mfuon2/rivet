/* Rivet: optional isolated media controls; no cookies, analytics, or remote code. */
"use strict";
const api = globalThis.browser || globalThis.chrome;
const HOST = "org.rivet.downloads";
const mediaByTab = new Map();
const MAX_PER_TAB = 50;
const WIDGET_ID = "rivet-media-widget";
let widgetUpdate = Promise.resolve();
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


function youtubePage(value) {
  try {
    const url=new URL(value);
    if (!httpUrl(value) || url.protocol !== "https:" || url.port) return null;
    let id;
    if(url.hostname==="youtu.be") id=url.pathname.split("/")[1];
    else if(["youtube.com","www.youtube.com","m.youtube.com","music.youtube.com"].includes(url.hostname)) {
      id=url.pathname==="/watch" ? url.searchParams.get("v") : /^\/(?:shorts|embed|live)\/([A-Za-z0-9_-]{11})$/.exec(url.pathname)?.[1];
    }
    return /^[A-Za-z0-9_-]{11}$/.test(id||"") ? `https://www.youtube.com/watch?v=${id}` : null;
  } catch {return null;}
}
async function immediateDownload(url, media=false) {
  if(!httpUrl(url)) throw Error("Invalid download address");
  const reply=await api.runtime.sendNativeMessage(HOST,{type:media?"media":"download",url});
  if(!reply?.ok)throw Error(reply?.error||"Update and open the complete Rivet desktop application.");
  return reply;
}

function announceConnection() {
  api.runtime.sendNativeMessage(HOST, {type: "ping"}).catch(() => {});
}
function startConnectionChecks() {
  announceConnection();
  // A lightweight local heartbeat lets an open desktop setup detect installation,
  // re-enabling and helper repairs without asking the user to press Check connection.
  Promise.resolve(api.alarms.create("rivet-connection", {periodInMinutes: 1})).catch(() => {});
}
// Registered only for explicitly enabled origins with current host permission.
function updateWidgetScripts() {
  widgetUpdate = widgetUpdate.catch(() => {}).then(async () => {
    await sitesReady;
    const matches = [];
    for (const origin of enabledSites) {
      if (httpUrl(origin) && await api.permissions.contains({origins: [`${origin}/*`]})) matches.push(`${origin}/*`);
    }
    const existing = await api.scripting.getRegisteredContentScripts({ids: [WIDGET_ID]});
    if (existing.length) await api.scripting.unregisterContentScripts({ids: [WIDGET_ID]});
    if (matches.length) await api.scripting.registerContentScripts([{
      id: WIDGET_ID, matches, js: ["media-widget.js"], runAt: "document_idle", allFrames: true,
    }]);
    // Update already-open pages too. Restricted browser pages simply reject injection.
    const tabs = await api.tabs.query({});
    await Promise.all(tabs.map(async tab => {
      if (tab.id === undefined) return;
      try { await api.tabs.sendMessage(tab.id, {type: "rivet-widget-refresh"}); } catch { /* No existing widget. */ }
      if (httpUrl(tab.url) && enabledSites.has(new URL(tab.url).origin)) {
        try { await api.scripting.executeScript({target: {tabId: tab.id, allFrames: true}, files: ["media-widget.js"]}); } catch { /* Browser-restricted page/frame. */ }
      }
    }));
  });
  return widgetUpdate;
}
async function widgetAllowed(sender) {
  await sitesReady;
  if (!sender.tab || !httpUrl(sender.url)) return false;
  const origin = new URL(sender.url).origin;
  return enabledSites.has(origin) && await api.permissions.contains({origins: [`${origin}/*`]});
}
api.runtime.onStartup.addListener(startConnectionChecks);
api.alarms.onAlarm.addListener(alarm => { if (alarm.name === "rivet-connection") announceConnection(); });
api.runtime.onInstalled.addListener(details => {
  startConnectionChecks();
  if (details.reason === "install") api.tabs.create({url: api.runtime.getURL("setup.html")}).catch(() => {});
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
    if (items.size < MAX_PER_TAB || items.has(details.url)) items.set(details.url, {url: details.url, kind, frameId: details.frameId || 0});
  }).catch(() => {});
}, {urls: ["http://*/*", "https://*/*"]}, ["responseHeaders"]);

api.tabs.onRemoved.addListener(tabId => mediaByTab.delete(tabId));
api.tabs.onUpdated.addListener((tabId, change) => { if (change.status === "loading") mediaByTab.delete(tabId); });
api.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if(message?.type==="download-page" || message?.type==="download-file") {
    (async()=>{
      if(sender.tab && !await widgetAllowed(sender))throw Error("Enable this site before downloading");
      let url;
      if(message.type==="download-page") {
        // Content scripts cannot substitute a different page or resolve arbitrary sites.
        url=youtubePage(sender.tab ? sender.url : message.url);
        if(!url)throw Error("This page is not a supported public YouTube video");
      } else {url=message.url;}
      return immediateDownload(url,message.type==="download-page");
    })().then(sendResponse,()=>sendResponse({ok:false,error:"Could not start the download. Update Rivet, open the desktop app and check this site's access."}));
    return true;
  }
  if (message?.type === "widget-status") {
    widgetAllowed(sender).then(enabled => sendResponse({enabled, items: enabled
      ? [...(mediaByTab.get(sender.tab.id)?.values() || [])].filter(item => item.frameId === (sender.frameId || 0)) : []})).catch(() => sendResponse({enabled: false, items: []}));
    return true;
  }
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
      await updateWidgetScripts();
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
    (async () => {
      if (sender.tab && !await widgetAllowed(sender)) throw new Error("Site not enabled");
      return capture(message.url);
    })().then(reply => sendResponse(reply), () => sendResponse({ok: false, error: "Could not queue the link. Check site access and Rivet's native helper."}));
    return true;
  }
  if (message?.type === "clear") {
    mediaByTab.delete(message.tabId);
    sendResponse({ok: true});
  }
  return false;
});
api.permissions.onRemoved.addListener(() => { updateWidgetScripts().catch(() => {}); });
// Restore the connection alarm if the browser discarded it while disabled or
// after a restart. This also connects immediately when no alarm exists yet.
Promise.resolve(api.alarms.get("rivet-connection")).then(alarm => { if (!alarm) startConnectionChecks(); }).catch(() => {});
sitesReady.then(updateWidgetScripts).catch(() => {});
