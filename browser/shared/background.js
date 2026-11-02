/* Rivet: optional isolated media controls; no cookies, analytics, or remote code. */
"use strict";
const api = globalThis.browser || globalThis.chrome;
if (!globalThis.RivetInstaller && typeof importScripts === "function") importScripts("installer.js");
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
  return rememberJob(reply);
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
  return rememberJob(reply);
}

function announceConnection() { probeNative().catch(() => {}); }
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
  if (details.reason === "install") api.storage.local.set({onboardingAuto:true}).then(()=>api.tabs.create({url: api.runtime.getURL("setup.html")})).catch(()=>{});
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
const trustedPage = sender => typeof sender?.url === "string" && sender.url.startsWith(api.runtime.getURL(""));
let connection = {connected:false,checkedAt:0,reason:"unchecked",jobs:[]};
let syncFlight;
let installerFlight;
let releaseCache;
let releaseCachedAt=0;
let tracking=Promise.resolve();
function rememberJob(reply) {
  if(!reply?.ok || !/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(reply.id||""))return Promise.resolve(reply);
  tracking=tracking.catch(()=>{}).then(async()=>{
    const value=await api.storage.local.get("rivetJobs");
    await api.storage.local.set({rivetJobs:[reply.id,...(value.rivetJobs||[]).filter(id=>id!==reply.id)].slice(0,40)});
  });
  return tracking.then(()=>reply,()=>reply);
}
async function probeNative() {
  try {
    const reply=await api.runtime.sendNativeMessage(HOST,{type:"ping"});
    if(reply?.ok!==true || reply.id!==null)throw Error("legacy helper");
    connection={...connection,connected:true,checkedAt:Date.now(),reason:null};
    await api.storage.local.set({onboardingAuto:false});
    return {ok:true};
  } catch(error) {
    const missing=/native messaging host not found|no such native application|native application.*not found/i.test(String(error?.message||error));
    connection={connected:false,checkedAt:Date.now(),reason:missing?"missing":"unavailable",jobs:[]};
    return {ok:false,reason:connection.reason,error:"Cannot reach Rivet. Open the desktop app or repair its browser connection."};
  }
}
async function syncDesktop() {
  if(syncFlight)return syncFlight;
  syncFlight=(async()=>{
    if(!(await probeNative()).ok)return connection;
    const stored=await api.storage.local.get("rivetJobs");
    const ids=(stored.rivetJobs||[]).filter(id=>/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(id)).slice(0,40);
    try {
      const reply=await api.runtime.sendNativeMessage(HOST,{type:"sync",ids});
      if(reply?.ok!==true || reply.protocol!==1 || !Array.isArray(reply.jobs))throw Error("Unsupported sync");
      const allowed=new Set(ids);
      connection={connected:true,checkedAt:Date.now(),reason:null,version:String(reply.desktop_version||"").slice(0,32),running:reply.desktop_running===true,jobs:reply.jobs.filter(job=>allowed.has(job.id)).slice(0,40).map(job=>({id:job.id,name:String(job.name||"Download").slice(0,180),status:String(job.status||"unknown").slice(0,24),bytes:Math.max(0,Number(job.bytes)||0),total:Math.max(0,Number(job.total)||0)}))};
    } catch { connection={...connection,liveUnavailable:true,jobs:[]}; }
    return connection;
  })().finally(()=>{syncFlight=null;});
  return syncFlight;
}
async function installerStatus(target,kind) {
  if(!releaseCache || Date.now()-releaseCachedAt>5*60*1000) {
    releaseCache=await RivetInstaller.releases(fetch);releaseCachedAt=Date.now();
  }
  let hints={};
  try {hints=await navigator.userAgentData?.getHighEntropyValues(["architecture","bitness"])||{};} catch { /* Runtime platform remains available. */ }
  const detected=RivetInstaller.platform(await api.runtime.getPlatformInfo(),hints);
  const selected=target||detected;
  const packageInfo=RivetInstaller.asset(releaseCache,selected,kind);
  const stored=await api.storage.local.get(["rivetInstaller","onboardingAuto"]);
  let download=stored.rivetInstaller;
  if(download?.id!==undefined) {
    const items=await api.downloads.search({id:download.id});
    const item=items[0];
    if(item && item.url===download.url && (!item.byExtensionId || item.byExtensionId===api.runtime.id)) {
      download={...download,state:item.state,bytes:item.bytesReceived,total:item.totalBytes,danger:item.danger};
    } else { download={...download,state:"missing"}; }
  }
  return {ok:true,detected,selected,package:packageInfo,download,automatic:stored.onboardingAuto===true};
}
async function downloadInstaller(message) {
  if(installerFlight)return installerFlight;
  installerFlight=(async()=>{
    // Recheck before starting; a desktop installed while metadata loaded wins over downloading.
    if((await probeNative()).ok)return {ok:true,connected:true};
    const state=await installerStatus(message.target,message.kind);
    if(message.automatic && (!state.automatic || connection.reason!=="missing"))return {ok:false,error:"Open Rivet or choose Download to repair the connection."};
    const item=state.package;
    if(!item)return {ok:false,error:"No installer is published for this processor. Choose another supported platform or open GitHub releases."};
    if(state.download?.url===item.url && ["in_progress","complete"].includes(state.download.state) && !message.again)return {ok:true,download:state.download};
    const id=await api.downloads.download({url:item.url,filename:item.name,saveAs:false,conflictAction:"uniquify"});
    const download={...item,id,state:"in_progress",bytes:0,total:item.size};
    await api.storage.local.set({rivetInstaller:download,onboardingAuto:false});
    return {ok:true,download};
  })().finally(()=>{installerFlight=null;});
  return installerFlight;
}

api.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (["sync-status","installer-status","installer-download","installer-show","open-desktop"].includes(message?.type)) {
    if (!trustedPage(sender)) {sendResponse({ok:false,error:"This action is available only in Rivet's own pages."});return false;}
    (async()=>{
      if(message.type==="sync-status")return {ok:true,...await syncDesktop()};
      if(message.type==="installer-status")return installerStatus(message.target,message.kind);
      if(message.type==="installer-download")return downloadInstaller(message);
      if(message.type==="open-desktop")return api.runtime.sendNativeMessage(HOST,{type:"open"});
      const state=await api.storage.local.get("rivetInstaller");
      if(state.rivetInstaller?.id!==undefined) {
        const items=await api.downloads.search({id:state.rivetInstaller.id});
        if(!items[0] || items[0].url!==state.rivetInstaller.url || (items[0].byExtensionId && items[0].byExtensionId!==api.runtime.id))throw Error("Installer is no longer available");
        await api.downloads.show(state.rivetInstaller.id);
      }
      return {ok:true};
    })().then(sendResponse,()=>sendResponse({ok:false,error:"Setup could not finish this request. Check your connection and retry, or use the official releases link."}));
    return true;
  }

  if(message?.type==="download-page" || message?.type==="download-file") {
    (async()=>{
      if(sender.tab && !trustedPage(sender) && !await widgetAllowed(sender))throw Error("Enable this site before downloading");
      let url;
      if(message.type==="download-page") {
        // Content scripts cannot substitute a different page or resolve arbitrary sites.
        url=youtubePage(sender.tab && !trustedPage(sender) ? sender.url : message.url);
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
    probeNative().then(sendResponse);return true;
  }
  if (message?.type === "list") {
    sendResponse({items: [...(mediaByTab.get(message.tabId)?.values() || [])]});
    return false;
  }
  if (message?.type === "capture") {
    (async () => {
      if (sender.tab && !trustedPage(sender) && !await widgetAllowed(sender)) throw new Error("Site not enabled");
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
