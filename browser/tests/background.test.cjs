const {test} = require("node:test");
const assert = require("node:assert/strict");
const vm = require("node:vm");
const fs = require("node:fs");
const path = require("node:path");

function fixture({alarmExists = true} = {}) {
  const events = {};
  const sent = [];
  const registered = [];
  const opened = [];
  const alarms = [];
  const event = name => ({addListener: callback => { events[name] = callback; }});
  const api = {
    runtime: {onStartup: event("startup"), onInstalled: event("installed"), onMessage: event("message"), getURL: name => "chrome-extension://rivet/" + name, sendNativeMessage: async (host, payload) => { sent.push({host, payload}); return {ok: true, id: "queued-job"}; }},
    alarms: {get: async name => alarmExists ? {name} : undefined, create: async (name, options) => { alarms.push({name,options}); }, onAlarm: event("alarm")},
    storage: {local: {get: async () => ({}), set: async () => {}}},
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
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, "../shared/background.js"), "utf8"), {browser: api, URL, console});
  const message = (payload, sender = {}) => new Promise(resolve => events.message(payload, sender, resolve));
  return {events, sent, api, message, registered, opened, alarms};
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
