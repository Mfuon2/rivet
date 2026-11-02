"use strict";
const api=globalThis.browser||globalThis.chrome;
const $=id=>document.getElementById(id);
let timer,checking=false,attempts=0,automaticAttempted=false,latest;
const packageKind=()=>$("package").value;
async function packages() {
  const result=await api.runtime.sendMessage({type:"installer-status",target:$("platform").value,kind:packageKind()});
  if(!result?.ok)throw Error(result?.error||"Could not load the release");
  latest=result;
  if(!$("platform").value && result.detected)$("platform").value=result.detected;
  const linux=(result.selected||"").startsWith("linux-");
  $("package").hidden=$("linux-label").hidden=!linux;
  $("platform-status").textContent=result.package ? `Latest release: Rivet ${result.package.version} · ${result.package.name}` : "No package is available for this platform. Choose the correct processor or use GitHub releases.";
  $("download").disabled=!result.package;
  $("install-instructions").textContent=result.package?.kind===".dmg" ? "Open the downloaded DMG, drag Rivet into Applications, then open Rivet from Applications. This page will connect automatically." : linux && packageKind()!=="deb" ? "Extract the downloaded portable archive, open its bin folder, then launch rivet-desktop. This page will connect automatically." : "Run the downloaded installer, approve operating-system prompts, then open Rivet once. This page will connect automatically.";
  const download=result.download;
  if(download) {
    const running=download.state==="in_progress";
    $("download-progress").hidden=!running;
    $("download-progress").value=download.total>0?Math.min(1,download.bytes/download.total):0;
    $("download-status").textContent=download.state==="complete" ? download.kind===".dmg"?`${download.name} is downloaded. Open it, drag Rivet into Applications, then launch Rivet.`:`${download.name} is downloaded. Run it and open Rivet.` : running ? `Downloading ${download.name}… ${download.total>0?Math.round(download.bytes/download.total*100)+"%":""}` : "The installer download was interrupted or removed. Choose Download to try again.";
    if(download.danger && !["safe","accepted","allowlistedByPolicy"].includes(download.danger))$("download-status").textContent+=" Review any download warning in your browser. Rivet does not bypass browser safety checks.";
    $("show-installer").hidden=download.state!=="complete";
    $("download").textContent=download.state==="complete"?"Download again":"Download latest Rivet";
    if(running)$("download").disabled=true;
  }
  return result;
}
async function download(automatic=false) {
  $("download").disabled=true;
  try {
    const result=await api.runtime.sendMessage({type:"installer-download",target:$("platform").value,kind:packageKind(),automatic,again:!automatic && latest?.download?.state==="complete"});
    if(!result?.ok)throw Error(result?.error||"Download did not start");
    await packages();
  } catch(error) {$("download-status").textContent=error.message;$("download").disabled=false;}
}
async function connect() {
  if(checking)return;checking=true;clearTimeout(timer);
  try {
    const state=await api.runtime.sendMessage({type:"sync-status"});
    const ok=state?.connected===true;
    $("title").textContent=ok?"Rivet is ready":"Let’s connect your desktop";
    $("connection").textContent=ok?`Connected${state.version?" to Rivet "+state.version:""}. Downloads and status now stay in sync locally. No restart is needed.`:state?.reason==="missing"?"The desktop helper was not found. Install and open Rivet to complete setup.":"Rivet is not connected yet. If it is installed, open it or use Browsers to repair the connection; otherwise download it below.";
    $("install").hidden=ok;
    $("close").hidden=$("open-desktop").hidden=!ok;
    $("retry").hidden=ok;
    $("step-desktop").classList.toggle("complete",ok);
    $("step-connect").classList.toggle("complete",ok);
    if(!ok) {
      try {
        const release=await packages();
        if(++attempts>=2 && release.automatic && state?.reason==="missing" && !automaticAttempted && release.package) {automaticAttempted=true;await download(true);}
      } catch(error) {$("platform-status").textContent=error.message;}
    }
  } catch {$("connection").textContent="Could not check Rivet. Retry when the browser is ready.";}
  finally {checking=false;timer=setTimeout(connect,document.hidden?30000:attempts<15?2000:5000);}
}
$("retry").addEventListener("click",connect);
$("download").addEventListener("click",()=>download(false));
$("show-installer").addEventListener("click",()=>api.runtime.sendMessage({type:"installer-show"}));
$("platform").addEventListener("change",()=>packages().catch(error=>$("platform-status").textContent=error.message));
$("package").addEventListener("change",()=>packages().catch(error=>$("platform-status").textContent=error.message));
$("open-desktop").addEventListener("click",()=>api.runtime.sendMessage({type:"open-desktop"}).then(result=>{if(!result?.ok)$("connection").textContent="Open Rivet from your applications menu.";}));
$("close").addEventListener("click",async()=>{const tab=await api.tabs.getCurrent();if(tab?.id!==undefined)await api.tabs.remove(tab.id);});
window.addEventListener("pagehide",()=>clearTimeout(timer));
connect();
