#!/usr/bin/env python3
"""Version, inspect, collect, sign and publish a complete Rivet release."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import tarfile
import plistlib
import time
import tomllib
import zipfile

import release

VERSION = re.compile(r"^v?(\d+)\.(\d+)\.(\d+)$")


def gh(*args):
    result = subprocess.run(["gh", *map(str, args)], check=True, capture_output=True, text=True)
    return result.stdout.strip()


def version_tuple(value):
    match = VERSION.fullmatch(value)
    if not match:
        raise ValueError("Expected a numeric major.minor.patch version")
    numbers = tuple(map(int, match.groups()))
    if any(number > 65535 for number in numbers):
        raise ValueError("Version component exceeds the Windows resource limit")
    return numbers


def next_version(base, tags):
    candidate = version_tuple(base)
    existing = {version_tuple(tag) for tag in tags if VERSION.fullmatch(tag)}
    while candidate in existing:
        candidate = (candidate[0], candidate[1], candidate[2] + 1)
    # Continue the source version's series after the latest allocated version, including drafts.
    series = [value[2] for value in existing if value[:2] == candidate[:2]]
    if series:
        candidate = (candidate[0], candidate[1], max(candidate[2], max(series) + 1))
    value = ".".join(map(str, candidate))
    version_tuple(value)
    return value


def plan(repo, commit):
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("A full source commit is required")
    releases = json.loads(gh("api", "--paginate", f"repos/{repo}/releases?per_page=100", "--slurp"))
    items = [item for page in releases for item in page]
    matching = [item for item in items if item["target_commitish"] == commit and VERSION.fullmatch(item["tag_name"])]
    if matching:
        item = matching[0]
        result = {"version": item["tag_name"].removeprefix("v"), "tag": item["tag_name"], "already": str(not item["draft"]).lower()}
    else:
        tags = json.loads(gh("api", "--paginate", f"repos/{repo}/git/matching-refs/tags/v", "--slurp"))
        allocated = [item["ref"].removeprefix("refs/tags/") for page in tags for item in page]
        value = next_version(release.version(), allocated + [item["tag_name"] for item in items])
        result = {"version": value, "tag": "v" + value, "already": "false"}
    output = os.environ.get("GITHUB_OUTPUT")
    if output:
        with open(output, "a", encoding="utf-8") as stream:
            stream.write("".join(f"{key}={value}\n" for key, value in result.items()))
    print(json.dumps(result))


def stamp(value, root=release.ROOT):
    version_tuple(value)
    cargo = root / "Cargo.toml"
    lines = cargo.read_text().splitlines(keepends=True)
    inside = False
    changed = False
    for index, line in enumerate(lines):
        if line.startswith("["):
            inside = line.strip() == "[workspace.package]"
        if inside and line.startswith("version = "):
            lines[index] = f'version = "{value}"\n'
            changed = True
    if not changed:
        raise ValueError("Workspace package version not found")
    cargo.write_text("".join(lines), encoding="utf-8", newline="\n")
    lock = root / "Cargo.lock"
    blocks = lock.read_text().split("[[package]]")
    found = set()
    for index, block in enumerate(blocks):
        name = re.search(r'^name = "([^"]+)"$', block, re.M)
        if name and name[1] in {"rivet", "rivet-cli", "rivet-core", "rivet-desktop"}:
            blocks[index] = re.sub(r'^version = "[^"]+"$', f'version = "{value}"', block, count=1, flags=re.M)
            found.add(name[1])
    if not {"rivet-cli", "rivet-core", "rivet-desktop"}.issubset(found):
        raise ValueError("Workspace packages not found in lockfile")
    lock.write_text("[[package]]".join(blocks), encoding="utf-8", newline="\n")
    for browser in ("chromium", "firefox"):
        path = root / "browser" / browser / "manifest.json"
        manifest = json.loads(path.read_text())
        manifest["version"] = value
        # Text-mode defaults translate LF to CRLF on Windows, changing ZIP hashes.
        path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8", newline="\n")
    print("Stamped application, lockfile and browser versions:", value)


def inspect(directory, install=False):
    files = release.artifacts(directory)
    release.verify_hashes(directory)
    with tempfile.TemporaryDirectory(prefix="rivet-inspect-") as temporary:
        folder = Path(temporary)
        for path in files:
            if path.name.endswith(".zip"):
                with zipfile.ZipFile(path) as archive:
                    if archive.testzip() is not None:
                        raise ValueError("Corrupted ZIP: " + path.name)
                    names = archive.namelist()
                    if "-extension-" in path.name or "-chrome-store" in path.name:
                        manifest = json.loads(archive.read("manifest.json"))
                        if manifest["version"] != release.version():
                            raise ValueError("Extension version mismatch")
                        if "-chrome-store" in path.name and "key" in manifest:
                            raise ValueError("Store upload contains development key")
                        if not all(name in names for name in manifest["icons"].values()):
                            raise ValueError("Extension icons missing")
                    else:
                        needed = ("rivet.exe", "rivet-native-host.exe", "rivet-desktop.exe") if "-windows-" in path.name else ("rivet", "rivet-native-host", "rivet-desktop")
                        if not all(any(name.endswith("/" + binary) for name in names) for binary in needed):
                            raise ValueError("App archive is missing executables")
                        media_suffix = ".exe" if "-windows-" in path.name else ""
                        for tool in ("yt-dlp", "ffmpeg", "ffprobe", "qjs"):
                            if not any(name.endswith("/media/" + tool + media_suffix) for name in names):
                                raise ValueError("App archive is missing bundled media tool: " + tool)
                        if not any(name.endswith(("/media/licenses/NOTICE.txt", "/Resources/media-licenses/NOTICE.txt")) for name in names):
                            raise ValueError("Media dependency notices missing")
                        if "-macos-" in path.name:
                            app_folder = folder / "mac-app"
                            app_folder.mkdir(exist_ok=True)
                            for entry in archive.infolist():
                                if Path(entry.filename).is_absolute() or ".." in Path(entry.filename).parts:
                                    raise ValueError("Unsafe app archive path")
                                archive.extract(entry, app_folder)
                                extracted = app_folder / entry.filename
                                if not entry.is_dir():
                                    extracted.chmod((entry.external_attr >> 16) & 0o777 or 0o644)
                            app = next(app_folder.glob("*/Rivet.app"))
                            info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
                            if info["CFBundleShortVersionString"] != release.version() or info.get("CFBundleIconFile") != "Rivet.icns":
                                raise ValueError("macOS bundle version/icon mismatch")
                            release.run(["codesign", "--verify", "--deep", "--strict", app])
                            release.run([app / "Contents/MacOS/rivet", "--version"], timeout=20)
                            for tool, args in [("yt-dlp", ["--ignore-config", "--version"]), ("ffmpeg", ["-version"]), ("ffprobe", ["-version"]), ("qjs", ["-e", 'print("RIVET_JS_OK")'])]:
                                release.run([app / "Contents/MacOS/media" / tool, *args], timeout=30)
            elif path.name.endswith(".tar.gz"):
                with tarfile.open(path, "r:gz") as archive:
                    names = archive.getnames()
                    for binary in ("rivet", "rivet-native-host", "rivet-desktop"):
                        name = next((name for name in names if name.endswith("/bin/" + binary)), None)
                        if not name or archive.extractfile(name).read(4) != b"\x7fELF":
                            raise ValueError("Linux archive executable missing/invalid")
                    for tool in ("yt-dlp", "ffmpeg", "ffprobe", "qjs"):
                        if not any(name.endswith("/bin/media/" + tool) for name in names):
                            raise ValueError("Linux archive media tool missing: " + tool)
            elif path.suffix == ".exe":
                if path.read_bytes()[:2] != b"MZ":
                    raise ValueError("Invalid Windows installer")
                if install:
                    if os.environ.get("GITHUB_ACTIONS") != "true" or os.name != "nt":
                        raise ValueError("Installer execution is restricted to disposable CI runners")
                    destination = folder / "installed"
                    release.run([path.resolve(), "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", f"/DIR={destination}"], timeout=120)
                    release.run([destination / "bin/rivet.exe", "--version"], timeout=20)
                    if not (destination / "bin/rivet-native-host.exe").is_file():
                        raise ValueError("Installer did not include the native helper")
                    for tool, args in [("yt-dlp", ["--ignore-config", "--version"]), ("ffmpeg", ["-version"]), ("ffprobe", ["-version"]), ("qjs", ["-e", 'print("RIVET_JS_OK")'])]:
                        release.run([destination / "bin/media" / (tool + ".exe"), *args], timeout=30)
                    release.run([destination / "unins000.exe", "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"], timeout=120)
            elif path.suffix == ".deb":
                destination = folder / "deb"
                release.run(["dpkg-deb", "--extract", path, destination])
                release.run([destination / "opt/rivet/rivet", "--version"], timeout=20)
                for tool, args in [("yt-dlp", ["--ignore-config", "--version"]), ("ffmpeg", ["-version"]), ("ffprobe", ["-version"]), ("qjs", ["-e", 'print("RIVET_JS_OK")'])]:
                    release.run([destination / "opt/rivet/media" / tool, *args], timeout=30)
                if not (destination / "usr/share/applications/org.rivet.downloads.desktop").is_file():
                    raise ValueError("Linux launcher missing")
                if not (destination / "usr/share/icons/hicolor/256x256/apps/org.rivet.downloads.png").is_file():
                    raise ValueError("Linux application icon missing")
            elif path.suffix == ".pkg":
                listing = subprocess.check_output(["pkgutil", "--payload-files", path], text=True)
                for name in ("rivet-desktop", "rivet-native-host", "rivet"):
                    if f"Rivet.app/Contents/MacOS/{name}" not in listing:
                        raise ValueError("macOS installer executable missing")
                for tool in ("yt-dlp", "ffmpeg", "ffprobe", "qjs"):
                    if f"Rivet.app/Contents/MacOS/media/{tool}" not in listing:
                        raise ValueError("macOS installer media tool missing")
                if "Rivet.app/Contents/Resources/Rivet.icns" not in listing:
                    raise ValueError("macOS application icon missing")
    print("Native package and browser payload checks passed")


def collect(source, output, value):
    version_tuple(value)
    output.mkdir(parents=True, exist_ok=True)
    for manifest in source.rglob(release.CHECKSUMS):
        release.verify_hashes(manifest.parent)
    for path in sorted(source.rglob("*")):
        if not release.ARTIFACT.fullmatch(path.name):
            continue
        if path.is_symlink() or not path.is_file() or not path.name.startswith(f"rivet-{value}-"):
            raise ValueError("Unexpected release input")
        target = output / path.name
        if target.exists():
            if release.digest(path) != release.digest(target):
                raise ValueError("Conflicting cross-platform artifact: " + path.name)
        else:
            shutil.copy2(path, target)
    names = {p.name for p in release.artifacts(output)}
    required = {f"rivet-{value}-windows-x86_64-setup.exe", f"rivet-{value}-windows-x86_64.zip", f"rivet-{value}-linux-x86_64.deb", f"rivet-{value}-linux-x86_64.tar.gz", f"rivet-{value}-extension-chromium.zip", f"rivet-{value}-extension-firefox.zip", f"rivet-{value}-chrome-store.zip"}
    if not required.issubset(names):
        raise ValueError("Missing required artifacts: " + ", ".join(sorted(required - names)))
    mac = [name for name in names if re.fullmatch(rf"rivet-{re.escape(value)}-macos-(?:aarch64|x86_64)\.pkg", name)]
    if not mac or any(name.removesuffix(".pkg") + ".zip" not in names for name in mac):
        raise ValueError("macOS installer/archive pair missing")
    release.checksums(output)
    print("Collected complete Windows/macOS/Linux release:", len(names), "packages")


def sign_from_environment(directory):
    secret = os.environ.get("MINISIGN_SECRET_KEY")
    password = os.environ.get("MINISIGN_PASSWORD")
    if not secret or not password:
        raise ValueError("Protected release signing secrets are not configured")
    with tempfile.TemporaryDirectory(prefix="rivet-signing-") as temporary:
        key = Path(temporary) / "minisign.key"
        key.write_text(secret)
        key.chmod(0o600)
        release.sign(directory, key, release.PUBLIC_KEY, password_env="MINISIGN_PASSWORD")
    shutil.copy2(release.PUBLIC_KEY, directory / "minisign.pub")


def publish(repo, commit, value, directory):
    version_tuple(value)
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("Invalid source commit")
    release.verify(directory, release.PUBLIC_KEY)
    tag = "v" + value
    listing = subprocess.run(["gh", "release", "view", tag, "--repo", repo, "--json", "isDraft,targetCommitish"], capture_output=True, text=True)
    if listing.returncode == 0:
        existing = json.loads(listing.stdout)
        if existing["targetCommitish"] != commit:
            raise ValueError("Release tag belongs to a different source revision")
        if not existing["isDraft"]:
            raise ValueError("Refusing to overwrite a published release")
    notes = f"""Rivet {value} Preview

Automatically built, tested and published from source commit `{commit}` after `main` was updated.

### Downloads

- Windows x64: setup EXE and ZIP.
- macOS: PKG installer and app ZIP for the architecture in the filename.
- Linux x64: DEB and tarball, built on Ubuntu 24.04.
- Browser extensions: Chromium/Firefox development ZIPs and a separate Chrome Web Store upload ZIP.

### Verify the download

Packages are authenticated by Minisign-signed SHA-256 checksums. Obtain the trusted public key from `release/minisign.pub` in the source repository, then verify `SHA256SUMS.minisig` before checking the package hashes. The release includes `SHA256SUMS`, `SHA256SUMS.minisig` and `minisign.pub`.

### Current limitations

This is a preview. Windows Authenticode and Apple Developer ID/notarization are not configured; platform installation warnings can appear. Browser-store review/approval is separate, and developer loading is required until a production store identity is configured. Safari, authenticated capture, automatic takeover and HLS/DASH assembly are not implemented.

Build version `{value}` is stamped into the workspace and browser manifests by the workflow; the source branch does not need a manual version commit. See `release/README.md` for reproduction and pipeline details.
"""
    with tempfile.NamedTemporaryFile("w", suffix=".md", encoding="utf-8") as temporary:
        temporary.write(notes)
        temporary.flush()
        if listing.returncode != 0:
            gh("release", "create", tag, "--repo", repo, "--draft", "--prerelease", "--target", commit, "--title", f"Rivet {value} Preview", "--notes-file", temporary.name)
        else:
            gh("release", "edit", tag, "--repo", repo, "--notes-file", temporary.name)
    files = release.artifacts(directory) + [directory / name for name in ("SHA256SUMS", "SHA256SUMS.minisig", "minisign.pub")]
    gh("release", "upload", tag, "--repo", repo, "--clobber", *files)
    with tempfile.TemporaryDirectory(prefix="rivet-upload-verify-") as temporary:
        gh("release", "download", tag, "--repo", repo, "--dir", temporary)
        release.verify(Path(temporary), release.PUBLIC_KEY)
    gh("release", "edit", tag, "--repo", repo, "--draft=false", "--prerelease")
    with tempfile.TemporaryDirectory(prefix="rivet-public-verify-") as temporary:
        gh("release", "download", tag, "--repo", repo, "--dir", temporary)
        release.verify(Path(temporary), release.PUBLIC_KEY)
    url = f"https://github.com/{repo}/releases/tag/{tag}"
    print("Published and verified:", url)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as stream:
            stream.write(f"## Published [{tag}]({url})\n\nWindows, macOS, Linux and browser packages uploaded. Downloaded signatures and package hashes verified before and after publication.\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    p = commands.add_parser("plan"); p.add_argument("--repo", required=True); p.add_argument("--commit", required=True)
    p = commands.add_parser("stamp"); p.add_argument("version")
    p = commands.add_parser("inspect"); p.add_argument("directory", type=Path); p.add_argument("--install", action="store_true")
    p = commands.add_parser("collect"); p.add_argument("source", type=Path); p.add_argument("output", type=Path); p.add_argument("--version", required=True)
    p = commands.add_parser("sign"); p.add_argument("directory", type=Path)
    p = commands.add_parser("publish"); p.add_argument("directory", type=Path); p.add_argument("--repo", required=True); p.add_argument("--commit", required=True); p.add_argument("--version", required=True)
    args = parser.parse_args()
    try:
        if args.command == "plan": plan(args.repo, args.commit)
        elif args.command == "stamp": stamp(args.version)
        elif args.command == "inspect": inspect(args.directory, args.install)
        elif args.command == "collect": collect(args.source, args.output, args.version)
        elif args.command == "sign": sign_from_environment(args.directory)
        elif args.command == "publish": publish(args.repo, args.commit, args.version, args.directory)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Publication failed: {error}\n")


if __name__ == "__main__":
    main()
