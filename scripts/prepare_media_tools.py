#!/usr/bin/env python3
"""Prepare checksum-pinned media tools. FFmpeg is built locally from unmodified LGPL source."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
OPTIONS = ["--disable-debug", "--disable-doc", "--disable-network", "--disable-autodetect", "--disable-everything", "--enable-ffmpeg", "--enable-ffprobe", "--enable-protocol=file", "--enable-demuxer=mov,matroska,aac,mp3", "--enable-muxer=mp4,matroska", "--enable-parser=aac,h264,opus,vp9,av1", "--enable-decoder=h264,aac,opus,vp9,av1", "--enable-bsf=aac_adtstoasc,extract_extradata", "--disable-shared", "--enable-static", "--disable-x86asm"]


def fetch(spec, directory):
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / spec["sha256"]
    if not path.exists():
        temporary = path.with_suffix(".partial")
        with urllib.request.urlopen(urllib.request.Request(spec["url"], headers={"User-Agent": "Rivet-release-builder"}), timeout=120) as source, temporary.open("wb") as target:
            shutil.copyfileobj(source, target)
        temporary.replace(path)
    if hashlib.sha256(path.read_bytes()).hexdigest() != spec["sha256"]:
        raise ValueError("Media dependency checksum mismatch: " + spec["url"])
    return path


def windows_build_environment():
    vswhere = Path(os.environ["ProgramFiles(x86)"]) / "Microsoft Visual Studio/Installer/vswhere.exe"
    install = subprocess.check_output([str(vswhere), "-latest", "-products", "*", "-requires", "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "-property", "installationPath"], text=True).strip()
    vcvars = Path(install) / "VC/Auxiliary/Build/vcvars64.bat"
    values = subprocess.check_output(["cmd.exe", "/d", "/s", "/c", f'""{vcvars}" >nul && set"'], text=True)
    env = os.environ.copy()
    for line in values.splitlines():
        key, sep, value = line.partition("=")
        if sep and key:
            # Windows environment names are case insensitive; avoid both PATH and Path.
            for existing in list(env):
                if existing.lower() == key.lower():
                    del env[existing]
            env[key] = value
    env["MSYS_NO_PATHCONV"] = "1"
    return env


def prepare(output):
    lock = json.loads((ROOT / "packaging/media-tools.lock.json").read_text())
    target = platform.system() + "-" + platform.machine()
    specs = lock["platforms"].get(target)
    if specs is None:
        raise ValueError("No pinned media tools for " + target)
    cache = ROOT / "target/media-cache"
    output.mkdir(parents=True, exist_ok=True)
    suffix = ".exe" if os.name == "nt" else ""
    for name, spec in specs.items():
        if name == "yt-dlp-notices":
            continue
        executable = output / (name + suffix)
        shutil.copyfile(fetch(spec, cache), executable)
        executable.chmod(0o755)
    archive = fetch(lock["ffmpeg_source"], cache)
    source = cache / ("ffmpeg-" + lock["ffmpeg_version"])
    if not source.exists():
        with tarfile.open(archive) as tar:
            tar.extractall(cache, filter="data")
    env = os.environ.copy()
    options = OPTIONS.copy()
    if os.name == "nt":
        env = windows_build_environment()
        options += ["--toolchain=msvc", "--cc=cl.exe", "--ld=link.exe"]
        bash = str(Path(os.environ.get("ProgramFiles", "C:/Program Files")) / "Git/bin/bash.exe")
    else:
        bash = "bash"
    subprocess.run([bash, "./configure", *options], cwd=source, env=env, check=True)
    subprocess.run([bash, "-c", "export PATH=/usr/bin:$PATH; make -j " + str(min(os.cpu_count() or 2, 8))], cwd=source, env=env, check=True)
    for name in ["ffmpeg", "ffprobe"]:
        dest = output / (name + suffix)
        shutil.copy2(source / (name + suffix), dest)
        dest.chmod(0o755)
    notices = output / "licenses"
    notices.mkdir(exist_ok=True)
    shutil.copy2(archive, notices / "ffmpeg-9.0.2.tar.xz")
    shutil.copy2(source / "COPYING.LGPLv2.1", notices / "FFMPEG-LGPLv2.1.txt")
    shutil.copy2(ROOT / "scripts/prepare_media_tools.py", notices / "prepare_media_tools.py")
    shutil.copy2(ROOT / "packaging/media-tools.lock.json", notices / "media-tools.lock.json")
    # Preserve yt-dlp's complete notices and upstream sources beside its standalone binary.
    yt_source = fetch(lock["yt_dlp_source"], cache)
    shutil.copy2(yt_source, notices / "yt-dlp-2026.10.26.tar.gz")
    with tarfile.open(yt_source) as tar:
        for member in tar.getmembers():
            name = Path(member.name).name
            if member.isfile() and name in {"LICENSE", "THIRD_PARTY_LICENSES.txt"}:
                (notices / ("YT-DLP-" + name)).write_bytes(tar.extractfile(member).read())
    with zipfile.ZipFile(fetch(specs["yt-dlp-notices"], cache)) as bundled:
        notice_names = [name for name in bundled.namelist() if name.endswith("THIRD_PARTY_LICENSES.txt")]
        if len(notice_names) != 1:
            raise ValueError("Upstream binary third-party notices missing")
        (notices / "YT-DLP-THIRD_PARTY_LICENSES.txt").write_bytes(bundled.read(notice_names[0]))
    request = urllib.request.Request("https://raw.githubusercontent.com/quickjs-ng/quickjs/v0.17.0/LICENSE", headers={"User-Agent": "Rivet-release-builder"})
    # License text is not executable. The runtime itself is pinned by SHA-256 above.
    with urllib.request.urlopen(request, timeout=60) as source_license:
        (notices / "QUICKJS-LICENSE.txt").write_bytes(source_license.read())
    (notices / "NOTICE.txt").write_text("Rivet invokes separately distributed media programs as subprocesses. Rivet source remains MIT.\nFFmpeg 9.0.2: LGPL-2.1-or-later, unmodified source and build script included. No GPL/nonfree configuration.\nyt-dlp 2026.10.26 standalone binaries: GPL-3.0-or-later combined distribution; source and third-party notices included.\nQuickJS-NG 0.17.0: MIT; https://github.com/quickjs-ng/quickjs/tree/v0.17.0\n" + "FFmpeg configuration: " + " ".join(options) + "\n", encoding="utf-8")
    subprocess.run([output / ("yt-dlp" + suffix), "--ignore-config", "--version"], check=True)
    subprocess.run([output / ("qjs" + suffix), "-e", 'print("RIVET_JS_OK")'], check=True)
    for name in ["ffmpeg", "ffprobe"]:
        subprocess.run([output / (name + suffix), "-version"], check=True)
    # Exercise real muxing, including both tracks, without contacting an external media service in CI.
    merged = cache / ("mux-test" + suffix + ".mp4")
    merged.unlink(missing_ok=True)
    subprocess.run([output / ("ffmpeg" + suffix), "-v", "error", "-i", ROOT / "scripts/fixtures/video.mp4", "-i", ROOT / "scripts/fixtures/audio.m4a", "-map", "0:v", "-map", "1:a", "-c", "copy", merged], check=True)
    tracks = json.loads(subprocess.check_output([output / ("ffprobe" + suffix), "-v", "error", "-show_entries", "stream=codec_type", "-of", "json", merged]))
    if {stream["codec_type"] for stream in tracks["streams"]} != {"video", "audio"}:
        raise ValueError("Bundled media merger did not preserve both tracks")
    merged.unlink()
    print("Prepared pinned media components:", output)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "target/media-tools")
    prepare(parser.parse_args().output.resolve())
