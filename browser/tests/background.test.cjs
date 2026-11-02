const {test} = require("node:test");
const assert = require("node:assert/strict");
const vm = require("node:vm");
const fs = require("node:fs");
const path = require("node:path");

function fixture({alarmExists = true,fetcher} = {}) {
  const events = {};
  const sent = [];
  const registered = [];
  const opened = [];
  const alarms = [];
  const storage = {};
  const installerDownloads = [];
  const downloaded = new Map();
  const event = name => ({addListener: callback => { events[name] = callback; }});
  const api = {
    runtime: {id:"rivet",getPlatformInfo:async()=>({os:"mac",arch:"arm64"}),onStartup: event("startup"), onInstalled: event("installed"), onMessage: event("message"), getURL: name => "chrome-extension://rivet/" + name, sendNativeMessage: async (host, payload) => { sent.push({host, payload}); return {ok: true, id: "queued-job"}; }},
    downloads: {download:async options=>{installerDownloads.push(options);const id=installerDownloads.length;downloaded.set(id,{id,url:options.url,state:"in_progress",bytesReceived:0,totalBytes:100,byExtensionId:"rivet"});return id;},search:async({id})=>downloaded.has(id)?[downloaded.get(id)]:[],show:async()=>{}},
    alarms: {get: async name => alarmExists ? {name} : undefined, create: async (name, options) => { alarms.push({name,options}); }, onAlarm: event("alarm")},
    storage: {local: {get: async () => ({...storage}), set: async value => {Object.assign(storage,value);}}},
    permissions: {contains: async () => true, onRemoved: event("permissionsRemoved")},
    scripting: {
      getRegisteredContentScripts: async () => registered,
      unregisterContentScripts: async () => { registered.length = 0; },
      registerContentScripts: async scripts => { registered.push(...scripts); },
      executeScript: async () => {},
    },
    contextMenus: {removeAll: async () => {}, create: () => {}, onClicked: event("click")},
    webRequest: {onHeadersReceived: event("headers")},
    tabs: {onRemoved: event("removed"), onUpdated: event("updated"), query: async () => [], sendMessage: async () => {}, create: async item => { opened.push(item); }},
  };
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, "../shared/background.js"), "utf8"), {browser: api, URL, console,RivetInstaller:require("../shared/installer.js"),fetch:fetcher||async function(){throw Error("Network not configured for test");}});
  const message = (payload, sender = {}) => new Promise(resolve => events.message(payload, sender, resolve));
  return {events, sent, api, message, registered, opened, alarms,storage,installerDownloads,downloaded};
}
const flush = () => new Promise(resolve => setImmediate(resolve));

test("media discovery requires explicit enable even when browser grants activeTab access", async () => {
  const f = fixture();
  const details = {tabId: 7, url: "https://media.example/video.mp4?token=private", responseHeaders: [{name: "Content-Type", value: "video/mp4"}]};
  f.events.headers(details); await flush();
  assert.equal((await f.message({type: "list", tabId: 7})).items.length, 0);
  assert.equal((await f.message({type: "enable", origin: "https://media.example"})).ok, true);
  f.events.headers(details); await flush();
  const items = (await f.message({type: "list", tabId: 7})).items;
  assert.equal(items.length, 1);
  assert.equal(items[0].url, details.url);
  assert.equal(f.sent.length, 0, "discovery must never initiate a download");
  assert.equal((await f.message({type: "capture", url: details.url})).ok, true);
  assert.equal(f.sent[0].host, "org.rivet.downloads");
  assert.deepEqual(Object.keys(f.sent[0].payload), ["url"], "no cookies or page data in native payload");
  await f.message({type: "disable", origin: "https://media.example"});
  assert.equal((await f.message({type: "list", tabId: 7})).items.length, 0);
});

test("unsafe URLs cannot reach the native host", async () => {
  const f = fixture();
  assert.equal((await f.message({type: "capture", url: "file:///etc/passwd"})).ok, false);
  assert.equal((await f.message({type: "capture", url: "https://user:secret@example.com/file"})).ok, false);
  assert.equal(f.sent.length, 0);
});

test("site toggle requires browser permission and direct-media detection is bounded", async () => {
  const f = fixture();
  f.api.permissions.contains = async () => false;
  assert.equal((await f.message({type: "enable", origin: "https://media.example"})).ok, false);
  f.api.permissions.contains = async () => true;
  await f.message({type: "enable", origin: "https://media.example"});
  for (let n=0; n<80; n++) f.events.headers({tabId: 1, url: `https://media.example/${n}.mp4`, responseHeaders: []});
  await flush();
  assert.equal((await f.message({type: "list", tabId: 1})).items.length, 50);
  f.events.removed(1);
  assert.equal((await f.message({type: "list", tabId: 1})).items.length, 0);
});


test("connection probe sends no URL and handles absent or outdated hosts", async () => {
  const f = fixture();
  f.api.runtime.sendNativeMessage = async (host, payload) => {
    f.sent.push({host, payload}); return {ok: true, id: null};
  };
  assert.equal((await f.message({type: "ping"})).ok, true);
  assert.equal(f.sent[0].host, "org.rivet.downloads");
  assert.equal(JSON.stringify(f.sent[0].payload), '{"type":"ping"}');
  f.api.runtime.sendNativeMessage = async () => ({ok: true, id: "unexpected-job"});
  assert.equal((await f.message({type: "ping"})).ok, false);
  f.api.runtime.sendNativeMessage = async () => { throw new Error("private local path"); };
  const failed = await f.message({type: "ping"});
  assert.equal(failed.ok, false);
  assert.ok(!failed.error.includes("private local path"));
});


test("browser startup announces only a queue-free native connection", async () => {
  const f = fixture();
  f.events.startup(); await flush();
  assert.equal(f.sent.length, 1);
  assert.equal(JSON.stringify(f.sent[0].payload), '{"type":"ping"}');
  assert.equal((await f.message({type: "list", tabId: 7})).items.length, 0);
  assert.equal(f.alarms[0].options.periodInMinutes, 1);
});

test("first install opens automatic connection setup, updates do not open another tab", async () => {
  const f = fixture();
  f.events.installed({reason: "install"}); await flush();
  assert.equal(f.opened.length, 1);
  assert.equal(f.opened[0].url, "chrome-extension://rivet/setup.html");
  f.events.installed({reason: "update"}); await flush();
  assert.equal(f.opened.length, 1);
  f.events.alarm({name: "unrelated"}); await flush();
  const before = f.sent.length;
  f.events.alarm({name: "rivet-connection"}); await flush();
  assert.equal(f.sent.length, before + 1);
  assert.equal(JSON.stringify(f.sent.at(-1).payload), '{"type":"ping"}');
});

test("a discarded connection alarm is restored without opening an install tab", async () => {
  const f = fixture({alarmExists: false}); await flush();
  assert.equal(f.alarms.length, 1);
  assert.equal(f.alarms[0].name, "rivet-connection");
  assert.equal(f.sent.length, 1);
  assert.equal(JSON.stringify(f.sent[0].payload), '{"type":"ping"}');
  assert.equal(f.opened.length, 0);
});

test("widgets register only on enabled sites and disappear on disable", async () => {
  const f = fixture(); await flush();
  assert.equal(f.registered.length, 0);
  await f.message({type: "enable", origin: "https://media.example"});
  assert.equal(f.registered.length, 1);
  assert.equal(JSON.stringify(f.registered[0].matches), '["https://media.example/*"]');
  assert.equal(JSON.stringify(f.registered[0].js), '["media-widget.js"]');
  const sender = {tab: {id: 7}, frameId: 0, url: "https://media.example/watch"};
  assert.equal((await f.message({type: "widget-status"}, sender)).enabled, true);
  assert.equal((await f.message({type: "widget-status"}, {...sender, url: "https://other.example/"})).enabled, false);
  await f.message({type: "disable", origin: "https://media.example"});
  assert.equal(f.registered.length, 0);
  assert.equal((await f.message({type: "widget-status"}, sender)).enabled, false);
  assert.equal((await f.message({type: "capture", url: "https://media.example/video.mp4"}, sender)).ok, false);
  assert.equal(f.sent.length, 0);
});

test("widget choices stay inside their frame and revoked permission stops handoff", async () => {
  const f = fixture();
  await f.message({type: "enable", origin: "https://media.example"});
  const sender = {tab: {id: 7}, frameId: 2, url: "https://media.example/watch"};
  for (const frameId of [0,2]) f.events.headers({tabId: 7, frameId, url: `https://media.example/${frameId}.mp4`, responseHeaders: []});
  await flush();
  const reply = await f.message({type: "widget-status"}, sender);
  assert.equal(reply.items.length, 1);
  assert.equal(reply.items[0].url, "https://media.example/2.mp4");
  assert.equal((await f.message({type: "capture", url: reply.items[0].url}, sender)).ok, true);
  f.api.permissions.contains = async () => false;
  assert.equal((await f.message({type: "widget-status"}, sender)).enabled, false);
  assert.equal((await f.message({type: "capture", url: reply.items[0].url}, sender)).ok, false);
  assert.equal(f.sent.length, 1, "revoked permission cannot queue another URL");
});

test("YouTube download binds to the enabled sender page and strips playlists", async () => {
  const f=fixture();
  const sender={tab:{id:7},frameId:0,url:"https://www.youtube.com/watch?v=aBbaea3I5bo&list=unwanted"};
  assert.equal((await f.message({type:"download-page"},sender)).ok,false);
  await f.message({type:"enable",origin:"https://www.youtube.com"});
  assert.equal((await f.message({type:"download-page",url:"https://www.youtube.com/watch?v=abcdefghijk"},sender)).ok,true);
  assert.equal(JSON.stringify(f.sent[0].payload),JSON.stringify({type:"media",url:"https://www.youtube.com/watch?v=aBbaea3I5bo"}));
  f.api.permissions.contains=async()=>false;
  assert.equal((await f.message({type:"download-page"},sender)).ok,false);
  assert.equal(f.sent.length,1);
});

test("explicit direct download starts a job; ordinary capture remains queue-only", async () => {
  const f=fixture();
  assert.equal((await f.message({type:"download-file",url:"https://media.example/video.mp4"})).ok,true);
  assert.equal(f.sent[0].payload.type,"download");
  assert.equal((await f.message({type:"capture",url:"https://media.example/video.mp4"})).ok,true);
  assert.equal(f.sent[1].payload.type,undefined);
  for(const url of ["https://youtube.com.evil.test/watch?v=aBbaea3I5bo","https://www.youtube.com/playlist?list=123","file:///video.mp4"]) {
    assert.equal((await f.message({type:"download-page",url})).ok,false);
  }
  assert.equal(f.sent.length,2);
});

const Installer=require('../shared/installer.js');
const setupSender={url:'chrome-extension://rivet/setup.html',tab:{id:19}};
function release(value='0.1.8',target='macos-aarch64') {
 const name=`rivet-${value}-${target}${target.startsWith('macos')?'.dmg':target.startsWith('windows')?'-setup.exe':'.tar.gz'}`;
 return {tag_name:'v'+value,draft:false,prerelease:true,assets:[{name,size:100,browser_download_url:`https://github.com/Mfuon2/rivet/releases/download/v${value}/${name}`}]};
}
const fetchRelease=async()=>({ok:true,text:async()=>JSON.stringify([release()])});
test('platform selection never substitutes an incompatible architecture',()=>{
 assert.equal(Installer.platform({os:'mac',arch:'arm64'}),'macos-aarch64');
 assert.equal(Installer.platform({os:'win',arch:'x86-64'}),'windows-x86_64');
 assert.equal(Installer.platform({os:'linux',arch:'x86-64'}),'linux-x86_64');
 assert.equal(Installer.platform({os:'cros',arch:'x86-64'}),null);
 assert.equal(Installer.asset(release(),'macos-x86_64'),null);
 assert.equal(Installer.latest([release('0.1.9'),release('0.1.10')]).tag_name,'v0.1.10');
 const bad=release();bad.assets[0].browser_download_url='https://malicious.example/installer.pkg';assert.throws(()=>Installer.asset(bad,'macos-aarch64'));
});
test('fresh install downloads the matching latest installer once and resumes its state',async()=>{
 const f=fixture({fetcher:fetchRelease});f.api.runtime.sendNativeMessage=async()=>{throw Error('Specified native messaging host not found.');};
 f.events.installed({reason:'install'});await flush();
 const state=await f.message({type:'installer-status'},setupSender);assert.equal(state.automatic,true);assert.equal(state.detected,'macos-aarch64');
 const [one,two]=await Promise.all([f.message({type:'installer-download',automatic:true},setupSender),f.message({type:'installer-download',automatic:true},setupSender)]);
 assert.equal(one.ok,true);assert.equal(two.ok,true);assert.equal(f.installerDownloads.length,1);
 assert.equal(f.installerDownloads[0].filename,'rivet-0.1.8-macos-aarch64.dmg');
 assert.equal((await f.message({type:'installer-status'},setupSender)).download.state,'in_progress');
 f.downloaded.get(1).state='complete';
 assert.equal((await f.message({type:'installer-download'},setupSender)).ok,true);assert.equal(f.installerDownloads.length,1);
 assert.equal((await f.message({type:'installer-download',again:true},setupSender)).ok,true);assert.equal(f.installerDownloads.length,2);
});
test('existing desktop suppresses automatic downloads and broken helpers require an explicit repair download',async()=>{
 const f=fixture({fetcher:fetchRelease});f.storage.onboardingAuto=true;
 f.api.runtime.sendNativeMessage=async()=>({ok:true,id:null});
 assert.equal((await f.message({type:'installer-download',automatic:true},setupSender)).connected,true);assert.equal(f.installerDownloads.length,0);
 f.storage.onboardingAuto=true;f.api.runtime.sendNativeMessage=async()=>{throw Error('Native host has exited');};
 assert.equal((await f.message({type:'installer-download',automatic:true},setupSender)).ok,false);assert.equal(f.installerDownloads.length,0);
 assert.equal((await f.message({type:'installer-download'},setupSender)).ok,true);
});
test('web content cannot request installer downloads or read desktop history',async()=>{
 const f=fixture({fetcher:fetchRelease});const sender={tab:{id:7},url:'https://www.youtube.com/watch?v=aBbaea3I5bo'};
 for(const type of ['installer-status','installer-download','installer-show','sync-status','open-desktop'])assert.equal((await f.message({type},sender)).ok,false);
 assert.equal(f.installerDownloads.length,0);assert.equal(f.sent.length,0);
});
test('sync sends only locally tracked IDs and exposes no source URLs or paths',async()=>{
 const f=fixture();const id='30eae737-812a-4a4c-b749-912998407121';
 f.api.runtime.sendNativeMessage=async(host,payload)=>{
  f.sent.push({host,payload});
  if(payload.type==='ping')return {ok:true,id:null};
  if(payload.type==='sync')return {ok:true,protocol:1,desktop_version:'0.1.8',desktop_running:true,jobs:[{id,name:'My video.mp4',status:'running',bytes:10,total:100,url:'private',path:'/private/folder'},{id:'untracked',name:'Private file',status:'complete'}]};
  return {ok:true,id};
 };
 await f.message({type:'capture',url:'https://example.com/file.mp4'});
 const state=await f.message({type:'sync-status',ids:['untracked']},setupSender);
 assert.equal(state.connected,true);assert.equal(state.jobs.length,1);assert.equal(state.jobs[0].name,'My video.mp4');
 assert.equal(JSON.stringify(f.sent.find(s=>s.payload.type==='sync').payload.ids),JSON.stringify([id]));
 assert.equal(state.jobs[0].url,undefined);assert.equal(state.jobs[0].path,undefined);
});
test('older helper stays connected without causing installer loops',async()=>{
 const f=fixture();f.api.runtime.sendNativeMessage=async(host,payload)=>{if(payload.type==='ping')return {ok:true,id:null};throw Error('Old host');};
 const state=await f.message({type:'sync-status'},setupSender);assert.equal(state.connected,true);assert.equal(state.liveUnavailable,true);assert.equal(f.installerDownloads.length,0);
});
test('Rivet UI opened as a tab can queue links without granting a website access',async()=>{
 const f=fixture();
 assert.equal((await f.message({type:'capture',url:'https://example.com/file.mp4'},{url:'chrome-extension://rivet/popup.html',tab:{id:19}})).ok,true);
 assert.equal((await f.message({type:'capture',url:'https://example.com/file.mp4'},{url:'https://example.com/',tab:{id:20}})).ok,false);
 assert.equal(f.sent.length,1);
});
test('tracking storage failure does not turn a successful queue operation into a duplicate retry',async()=>{
 const f=fixture();f.api.runtime.sendNativeMessage=async()=>({ok:true,id:'30eae737-812a-4a4c-b749-912998407121'});
 f.api.storage.local.set=async()=>{throw Error('Disk full');};
 assert.equal((await f.message({type:'capture',url:'https://example.com/file.mp4'})).ok,true);
});

test('Mac setup prefers DMG and supports older PKG-only releases',()=>{
 const current=release();assert.equal(Installer.asset(current,'macos-aarch64').kind,'.dmg');
 const legacy=release();legacy.assets[0].name=legacy.assets[0].name.replace('.dmg','.pkg');legacy.assets[0].browser_download_url=legacy.assets[0].browser_download_url.replace('.dmg','.pkg');
 assert.equal(Installer.asset(legacy,'macos-aarch64').kind,'.pkg');
 current.assets.push(legacy.assets[0]);assert.equal(Installer.asset(current,'macos-aarch64').kind,'.dmg');
});
