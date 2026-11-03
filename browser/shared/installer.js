/* Bundled platform/release selection. No remote code or arbitrary download addresses. */
(function (root) {
  "use strict";
  const REPO = "https://github.com/Mfuon2/rivet";
  const API = "https://api.github.com/repos/Mfuon2/rivet/releases?per_page=100";
  function version(value) {
    const match = /^v?(\d{1,5})\.(\d{1,5})\.(\d{1,5})$/.exec(value || "");
    return match ? match.slice(1).map(Number) : null;
  }
  function platform(info, hints = {}) {
    const os = ({mac:"macos",win:"windows",linux:"linux"})[info?.os];
    let arch = ({"x86-64":"x86_64","x86_64":"x86_64",arm64:"aarch64",aarch64:"aarch64"})[info?.arch];
    if (hints.architecture === "arm" && hints.bitness === "64") arch = "aarch64";
    if (hints.architecture === "x86" && hints.bitness === "64" && !arch) arch = "x86_64";
    return os && arch ? `${os}-${arch}` : null;
  }
  function latest(releases) {
    if (!Array.isArray(releases)) throw Error("Invalid release metadata");
    return releases.filter(r => !r.draft && /^v/.test(r.tag_name) && version(r.tag_name))
      .sort((a,b) => { const av=version(a.tag_name),bv=version(b.tag_name);return bv[0]-av[0]||bv[1]-av[1]||bv[2]-av[2]; })[0] || null;
  }
  function asset(release, target, kind) {
    if (!/^(macos|windows|linux)-(aarch64|x86_64)$/.test(target || "") || !version(release?.tag_name)) return null;
    const value=release.tag_name.slice(1);
    const extension=target.startsWith("macos-") ? ((release.assets||[]).some(a=>a.name===`rivet-${value}-${target}.dmg`)?".dmg":".pkg") : target.startsWith("windows-") ? "-setup.exe" : kind==="deb" ? ".deb" : ".tar.gz";
    const name=`rivet-${value}-${target}${extension}`;
    const matches=(release.assets || []).filter(a => a.name===name);
    if(matches.length!==1)return null;
    const item=matches[0];
    const url=`${REPO}/releases/download/${release.tag_name}/${name}`;
    if(item.browser_download_url!==url || !Number.isSafeInteger(item.size) || item.size<1 || item.size>1024*1024*1024)throw Error("Invalid release package metadata");
    return {name,url,size:item.size,version:value,target,kind:extension,sha256:/^sha256:[a-f0-9]{64}$/.test(item.digest||"")?item.digest.slice(7):null};
  }
  async function releases(fetcher) {
    const response=await fetcher(API,{credentials:"omit",redirect:"error",cache:"no-store",signal:AbortSignal.timeout(15000)});
    if(!response.ok)throw Error("GitHub is unavailable or its request limit was reached. Use the releases link or retry shortly.");
    const text=await response.text();
    if(text.length>2*1024*1024)throw Error("Release metadata exceeds limits");
    const result=latest(JSON.parse(text));
    if(!result)throw Error("No published Rivet release was found");
    return result;
  }
  const exports={REPO,API,version,platform,latest,asset,releases};
  root.RivetInstaller=exports;
  if(typeof module!=="undefined")module.exports=exports;
})(globalThis);
