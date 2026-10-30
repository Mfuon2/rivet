/* Isolated content script, injected only on explicitly enabled sites. */
(() => {
  "use strict";
  const api = globalThis.browser || globalThis.chrome;
  const KEY = "__rivetMediaWidget";
  if (globalThis[KEY]) { globalThis[KEY].refresh(); return; }
  document.querySelectorAll("[data-rivet-media-widget]").forEach(stale => stale.remove());
  const widgets = new Map();
  const dismissed = new WeakSet();
  const protectedMedia = new WeakSet();
  let enabled = false;
  let fontsRequested = false;
  let frame = 0;
  let observer;

  function directUrl(value) {
    try {
      const url = new URL(value, location.href);
      return ["http:", "https:"].includes(url.protocol) && !url.username && !url.password ? url.href : null;
    } catch { return null; }
  }
  const playlist = url => /\.(m3u8|mpd)(?:[?#]|$)/i.test(url);
  function position() {
    frame = 0;
    for (const [media, widget] of widgets) {
      if (!media.isConnected) { widget.resize.disconnect(); widget.host.remove(); widgets.delete(media); continue; }
      const rect = media.getBoundingClientRect();
      const visible = rect.width >= 100 && rect.height >= 24 && rect.bottom > 0 && rect.top < innerHeight && rect.right > 0 && rect.left < innerWidth && getComputedStyle(media).visibility !== "hidden";
      widget.host.style.setProperty("display", visible ? "block" : "none", "important");
      widget.host.style.setProperty("top", `${Math.max(4, rect.top + 8)}px`, "important");
      widget.host.style.setProperty("left", `${Math.max(4, Math.min(innerWidth - 275, rect.right - 270))}px`, "important");
      const parent = document.fullscreenElement || document.documentElement;
      if (widget.host.parentNode !== parent) parent.append(widget.host);
    }
  }
  function schedule() { if (!frame) frame = requestAnimationFrame(position); }
  async function choose(media, panel, status) {
    panel.replaceChildren();
    if (media.mediaKeys || protectedMedia.has(media)) { status.textContent = "Protected media is not supported."; return; }
    const page=new URL(location.href);
    const youtube=["youtube.com","www.youtube.com","m.youtube.com","music.youtube.com"].includes(page.hostname) && (page.pathname==="/watch" || /^\/(shorts|embed|live)\//.test(page.pathname));
    if(youtube) {
      const reply=await api.runtime.sendMessage({type:"widget-status"});
      if(!reply?.enabled){teardown();return;}
      const button=document.createElement("button");button.type="button";button.textContent="Download video with audio · MP4";
      button.addEventListener("click",async event=>{
        if(!event.isTrusted)return;event.stopPropagation();button.disabled=true;
        status.textContent="Starting the video download in Rivet…";
        try {
          const result=await api.runtime.sendMessage({type:"download-page"});
          status.textContent=result?.ok ? "Download requested. Rivet opens automatically; follow progress in the desktop app." : result?.error||"Update and open Rivet to enable video downloads.";
        } catch {status.textContent="Update Rivet and reload this extension to enable video downloads.";}
        finally {button.disabled=false;}
      });
      panel.append(button);status.textContent="Rivet resolves the public video, downloads video and audio, and merges a complete MP4. Protected or sign-in-only videos are not supported.";
      return;
    }
    const source = directUrl(media.currentSrc || media.src);
    const reply = await api.runtime.sendMessage({type: "widget-status"});
    if (!reply?.enabled) { teardown(); return; }
    const candidates = source ? [{url: source, kind: playlist(source) ? "playlist" : "media file"}] : (reply.items || []);
    if (!candidates.length) {
      status.textContent = "No direct file link found. Blob players and protected streams may not expose a downloadable file.";
      return;
    }
    status.textContent = source ? "Download this file in Rivet." : "Choose a detected link. It may belong to another player in this frame.";
    for (const item of candidates.slice(0, 50)) {
      const url = directUrl(item.url);
      if (!url) continue;
      const row = document.createElement("button");
      row.type = "button";
      const parsed = new URL(url);
      const isPlaylist = item.kind === "playlist" || playlist(url);
      row.textContent = `${isPlaylist ? "Save playlist file" : "Download with Rivet"} · ${parsed.hostname}${parsed.pathname}`;
      row.title = isPlaylist ? "Saves the playlist text only; complete HLS/DASH assembly is not supported." : "Starts this file in the desktop app.";
      row.addEventListener("click", async event => {
        if (!event.isTrusted) return;
        event.stopPropagation();
        row.disabled = true;
        status.textContent = "Sending to Rivet…";
        try {
          const response = await api.runtime.sendMessage({type: "download-file", url});
          status.textContent = response?.ok ? "Download requested. Rivet opens automatically; follow progress in the desktop app." : response?.error || "Open Rivet and check the extension connection.";
        } catch { status.textContent = "Rivet is unavailable. Check the desktop helper connection."; }
        finally { row.disabled = false; }
      });
      panel.append(row);
    }
  }
  function attach(media) {
    if (!enabled || widgets.has(media) || dismissed.has(media) || widgets.size >= 20) return;
    const host = document.createElement("div");
    host.dataset.rivetMediaWidget = "";
    host.style.cssText = "all:initial;position:fixed!important;z-index:2147483647!important;width:266px!important;font:12px \"Segoe UI\",Poppins,sans-serif!important;color:#20334b!important;";
    if (!fontsRequested) {
      fontsRequested = true;
      fetch(api.runtime.getURL("user-font.css")).then(response => response.text()).then(css => {
        if (css.includes("segoeui.woff")) {
          const face = new FontFace("Segoe UI", `url("${api.runtime.getURL("segoeui.woff")}")`, {weight:"400"});
          face.load().then(loaded => document.fonts.add(loaded)).catch(() => {});
        }
      }).catch(() => {});
      for (const [file, weight] of [["Poppins-Regular.ttf", "400"], ["Poppins-SemiBold.ttf", "600"]]) {
        const font = new FontFace("Poppins", `url("${api.runtime.getURL(file)}")`, {weight});
        font.load().then(loaded => document.fonts.add(loaded)).catch(() => {});
      }
    }
    const shadow = host.attachShadow({mode: "closed"});
    const style = document.createElement("style");
    style.textContent = `:host{font:12px "Segoe UI",Poppins,sans-serif;color:#20334b}*{box-sizing:border-box}.bar{display:flex;justify-content:flex-end;gap:3px}button{font:600 12px "Segoe UI",Poppins,sans-serif;border:1px solid #b9d0e5;border-radius:5px;background:#f4f8fd;color:#174b78;padding:7px 10px;cursor:pointer;box-shadow:0 2px 7px #0003}button:focus-visible{outline:2px solid #1687d0;outline-offset:2px}button:disabled{opacity:.6}.panel{margin-top:5px;background:#f7faff;border:1px solid #b9d0e5;border-radius:7px;padding:10px;box-shadow:0 3px 12px #0004}.panel[hidden]{display:none}.links{max-height:180px;overflow:auto}.links button{display:block;width:100%;margin-bottom:5px;overflow-wrap:anywhere;font-weight:400;text-align:left;box-shadow:none}.status{margin:6px 0 0;line-height:1.5;color:#455c73}.close{padding:7px 9px}`;
    const bar = document.createElement("div"); bar.className = "bar";
    const trigger = document.createElement("button"); trigger.type = "button"; trigger.textContent = "↓ Download with Rivet";
    trigger.setAttribute("aria-expanded", "false");
    const close = document.createElement("button"); close.type = "button"; close.className = "close"; close.textContent = "×"; close.title = "Hide Rivet for this player"; close.setAttribute("aria-label", close.title);
    const box = document.createElement("div"); box.className = "panel"; box.hidden = true;
    const links = document.createElement("div"); links.className = "links";
    const status = document.createElement("p"); status.className = "status"; status.setAttribute("role", "status");
    box.append(links, status);
    trigger.addEventListener("click", async event => {
      if (!event.isTrusted) return;
      event.stopPropagation();
      box.hidden = !box.hidden;
      trigger.setAttribute("aria-expanded", String(!box.hidden));
      if (!box.hidden) try { await choose(media, links, status); } catch { status.textContent = "Reload this page after updating Rivet."; }
    });
    close.addEventListener("click", event => { if (!event.isTrusted) return; event.stopPropagation(); dismissed.add(media); widgets.get(media)?.resize.disconnect(); host.remove(); widgets.delete(media); });
    bar.append(trigger, close); shadow.append(style, bar, box);
    const resize = new ResizeObserver(schedule);
    widgets.set(media, {host, resize}); document.documentElement.append(host); schedule();
    resize.observe(media);
  }
  function onPlay(event) { if (event.target instanceof HTMLMediaElement) attach(event.target); }
  function scan() {
    for (const media of document.querySelectorAll("video,audio")) if (!media.paused && !media.ended) attach(media);
    schedule();
  }
  function teardown() {
    enabled = false;
    for (const widget of widgets.values()) { widget.resize.disconnect(); widget.host.remove(); }
    widgets.clear(); observer?.disconnect(); observer = null;
    if (frame) cancelAnimationFrame(frame); frame = 0;
  }
  async function refresh() {
    let reply;
    try { reply = await api.runtime.sendMessage({type: "widget-status"}); } catch { teardown(); return; }
    if (!reply?.enabled) { teardown(); return; }
    enabled = true;
    if (!observer) { observer = new MutationObserver(scan); observer.observe(document.documentElement, {childList: true, subtree: true}); }
    scan();
  }
  document.addEventListener("play", onPlay, true);
  document.addEventListener("encrypted", event => { if (event.target instanceof HTMLMediaElement) protectedMedia.add(event.target); }, true);
  document.addEventListener("scroll", schedule, true);
  window.addEventListener("resize", schedule);
  document.addEventListener("fullscreenchange", schedule);
  api.runtime.onMessage.addListener(message => { if (message?.type === "rivet-widget-refresh") refresh(); });
  window.addEventListener("pagehide", teardown);
  window.addEventListener("pageshow", refresh);
  globalThis[KEY] = {refresh};
  refresh();
})();
