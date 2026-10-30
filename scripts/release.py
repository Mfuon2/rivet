#!/usr/bin/env python3
"""Build release packages and authenticate their checksums with an offline Minisign key."""
import argparse
import getpass
import hashlib
import hmac
import json
import os
from pathlib import Path
import platform
import plistlib
import re
import secrets
import shlex
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
PUBLIC_KEY = ROOT / "release/minisign.pub"
CHECKSUMS = "SHA256SUMS"
ARTIFACT = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]*\.(?:zip|tar\.gz|pkg|exe|deb)\Z")


def run(args, **kwargs):
    subprocess.run([str(arg) for arg in args], check=True, **kwargs)


def version():
    return tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def artifacts(directory):
    found = []
    for path in directory.iterdir():
        if ARTIFACT.fullmatch(path.name):
            if path.is_symlink() or not path.is_file():
                raise ValueError(f"Artifact must be a regular file: {path.name}")
            found.append(path)
    if not found:
        raise ValueError("No release packages found")
    return sorted(found)


def checksums(directory):
    directory = directory.resolve()
    files = artifacts(directory)
    target = directory / CHECKSUMS
    if target.is_symlink():
        raise ValueError("Checksum manifest must not be a symlink")
    target.write_text("".join(f"{digest(path)}  {path.name}\n" for path in files), encoding="ascii")
    # A changed manifest must never keep a signature for a previous set of bytes.
    target.with_name(CHECKSUMS + ".minisig").unlink(missing_ok=True)
    return target


def verify_hashes(directory):
    directory = directory.resolve()
    manifest = directory / CHECKSUMS
    if manifest.is_symlink() or manifest.stat().st_size > 1024 * 1024:
        raise ValueError("Invalid checksum manifest")
    names = set()
    for line in manifest.read_text(encoding="ascii").splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  ([A-Za-z0-9][A-Za-z0-9._-]*)", line)
        if not match or not ARTIFACT.fullmatch(match[2]) or match[2] in names:
            raise ValueError("Malformed, duplicate or unsafe checksum entry")
        expected, name = match.groups()
        names.add(name)
        path = directory / name
        if path.is_symlink() or not path.is_file() or digest(path) != expected:
            raise ValueError(f"Checksum mismatch or missing artifact: {name}")
    actual = {path.name for path in artifacts(directory)}
    if names != actual:
        raise ValueError("Checksum manifest does not cover exactly the release packages")
    return len(names)


def minisign():
    executable = shutil.which("minisign")
    if not executable:
        raise ValueError("Install Minisign first: https://jedisct1.github.io/minisign/")
    return executable


KEYCHAIN_SERVICE = "org.rivet.downloads.minisign"


def keychain_password(create=False):
    if platform.system() != "Darwin":
        raise ValueError("Keychain signing is available on macOS only")
    account = getpass.getuser()
    result = subprocess.run(["security", "find-generic-password", "-a", account, "-s", KEYCHAIN_SERVICE, "-w"], capture_output=True, text=True)
    if result.returncode == 0:
        if create:
            raise ValueError("A signing password already exists in Keychain; refusing to replace it")
        return result.stdout.rstrip("\n")
    if not create:
        raise ValueError("Signing password is not available in this user's Keychain")
    password = secrets.token_urlsafe(48)
    # security's interactive mode keeps the password out of argv/process listings.
    command = "add-generic-password -a {} -s {} -w {}\n".format(shlex.quote(account), shlex.quote(KEYCHAIN_SERVICE), shlex.quote(password))
    stored = subprocess.run(["security", "-i"], input=command, capture_output=True, text=True)
    if stored.returncode != 0:
        raise ValueError("Could not store the signing password in Keychain")
    retrieved = subprocess.run(["security", "find-generic-password", "-a", account, "-s", KEYCHAIN_SERVICE, "-w"], capture_output=True, text=True)
    if retrieved.returncode != 0 or not hmac.compare_digest(retrieved.stdout.rstrip("\n"), password):
        raise ValueError("Could not verify Keychain storage; no private key generated")
    return password


def keygen(key_dir, use_keychain=False):
    key_dir = key_dir.expanduser().resolve()
    if key_dir.is_relative_to(ROOT):
        raise ValueError("Private signing keys must live outside the repository")
    private = key_dir / "minisign.key"
    if private.exists() or PUBLIC_KEY.exists():
        raise ValueError("A key already exists; refusing to replace the release identity")
    if not use_keychain and not sys.stdin.isatty():
        raise ValueError("Run keygen in your own terminal to enter a signing-key password securely")
    key_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
    if os.name != "nt":
        key_dir.chmod(0o700)
    command = [minisign(), "-G", "-p", PUBLIC_KEY, "-s", private]
    if use_keychain:
        password = keychain_password(create=True)
        result = subprocess.run(command, input=f"{password}\n{password}\n", text=True, capture_output=True)
        if result.returncode != 0:
            raise ValueError("Minisign key generation failed; the password remains protected in Keychain")
    else:
        run(command)
    if os.name != "nt":
        private.chmod(0o600)
    print(f"Public key: {PUBLIC_KEY}\nEncrypted private key: {private}\nBack up the private key and password separately. Commit only the public key.")


def sign(directory, key, pubkey, use_keychain=False, password_env=None):
    directory, key, pubkey = directory.resolve(), key.expanduser().resolve(), pubkey.resolve()
    if key.is_relative_to(ROOT):
        raise ValueError("Keep private signing keys outside the repository")
    if not key.is_file() or not pubkey.is_file():
        raise ValueError("Signing key/public key missing. Run keygen first.")
    verify_hashes(directory)
    manifest = directory / CHECKSUMS
    command = [minisign(), "-S", "-s", key, "-m", manifest, "-t", f"Rivet {version()} release checksums"]
    if use_keychain or password_env:
        password = keychain_password() if use_keychain else os.environ.get(password_env)
        if not password:
            raise ValueError("Signing password is missing")
        result = subprocess.run(command, input=password + "\n", capture_output=True, text=True)
        if result.returncode != 0:
            raise ValueError("Minisign signing failed; the encrypted private key was not exposed")
    else:
        run(command)
    verify(directory, pubkey)


def verify(directory, pubkey):
    directory = directory.resolve()
    # Authenticate first. The caller must select a public key obtained from a trusted source.
    run([minisign(), "-V", "-p", pubkey.resolve(), "-m", directory / CHECKSUMS])
    count = verify_hashes(directory)
    print(f"Signature valid; all {count} release packages match.")


def zip_tree(source, destination):
    with zipfile.ZipFile(destination, "w", zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(source.rglob("*")):
            if path.is_file():
                archive.write(path, path.relative_to(source.parent))


def mac_bundle(stage, binaries, app_version):
    app = stage / "Rivet.app"
    executable_dir = app / "Contents/MacOS"
    executable_dir.mkdir(parents=True)
    for name in ("rivet-desktop", "rivet-native-host", "rivet"):
        shutil.copy2(binaries / name, executable_dir / name)
    info = {"CFBundleIdentifier": "org.rivet.downloads", "CFBundleName": "Rivet", "CFBundleDisplayName": "Rivet", "CFBundleExecutable": "rivet-desktop", "CFBundlePackageType": "APPL", "CFBundleShortVersionString": app_version, "CFBundleVersion": app_version, "NSHighResolutionCapable": True, "CFBundleIconFile": "Rivet.icns"}
    (app / "Contents/Info.plist").write_bytes(plistlib.dumps(info))
    resources = app / "Contents/Resources"
    resources.mkdir()
    shutil.copy2(ROOT / "LICENSE", resources / "LICENSE")
    shutil.copy2(ROOT / "assets/branding/Rivet.icns", resources / "Rivet.icns")
    shutil.copy2(ROOT / "crates/rivet-desktop/assets/lucide/LICENSE", resources / "LUCIDE-LICENSE")
    shutil.copy2(ROOT / "assets/fonts/OFL.txt", resources / "POPPINS-LICENSE.txt")
    # Ad-hoc signing is free and supports local integrity checks. It is NOT Developer ID/notarization.
    run(["codesign", "--force", "--deep", "--sign", "-", app])
    return app


def linux_deb(stage, output, base, app_version, machine):
    package = stage / "deb-root"
    app_dir = package / "opt/rivet"
    shutil.copytree(stage / base / "bin", app_dir)
    applications = package / "usr/share/applications"
    applications.mkdir(parents=True)
    shutil.copy2(ROOT / "packaging/rivet.desktop", applications / "org.rivet.downloads.desktop")
    icons = package / "usr/share/icons/hicolor/256x256/apps"
    icons.mkdir(parents=True)
    shutil.copy2(ROOT / "assets/branding/icons/icon-256.png", icons / "org.rivet.downloads.png")
    licenses = package / "usr/share/doc/rivet"
    licenses.mkdir(parents=True)
    shutil.copy2(ROOT / "LICENSE", licenses / "copyright")
    shutil.copy2(ROOT / "crates/rivet-desktop/assets/lucide/LICENSE", licenses / "LUCIDE-LICENSE")
    shutil.copy2(ROOT / "assets/fonts/OFL.txt", licenses / "POPPINS-LICENSE.txt")
    control = package / "DEBIAN"
    control.mkdir()
    architecture = {"x86_64": "amd64", "aarch64": "arm64"}[machine]
    (control / "control").write_text(f"Package: rivet\nVersion: {app_version}\nSection: net\nPriority: optional\nArchitecture: {architecture}\nMaintainer: Rivet contributors\nDepends: libc6, libgcc-s1, libstdc++6, libx11-6, libxkbcommon0, libwayland-client0, libegl1, libgl1\nDescription: Open source desktop download manager\n HTTP/HTTPS downloads, a durable queue and browser handoff.\n")
    run(["dpkg-deb", "--build", "--root-owner-group", package, output / f"{base}.deb"])


def package(output, installer=False):
    app_version = version()
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    system = {"Darwin": "macos", "Windows": "windows", "Linux": "linux"}.get(platform.system())
    machine = {"AMD64": "x86_64", "arm64": "aarch64"}.get(platform.machine(), platform.machine())
    if system is None or machine not in {"x86_64", "aarch64"}:
        raise ValueError("Unsupported packaging OS/architecture")
    base = f"rivet-{app_version}-{system}-{machine}"
    binaries = ROOT / "target/release"
    suffix = ".exe" if system == "windows" else ""
    for name in ("rivet", "rivet-native-host", "rivet-desktop"):
        if not (binaries / (name + suffix)).is_file():
            raise ValueError("Build all release executables first: cargo build --release --workspace --locked")
    run([sys.executable, ROOT / "scripts/package_browser.py", "--chrome-store"])
    with tempfile.TemporaryDirectory(prefix="rivet-package-") as temporary:
        stage = Path(temporary)
        bundle = stage / base
        bundle.mkdir()
        for name in ("README.md", "LICENSE", "SECURITY.md"):
            shutil.copy2(ROOT / name, bundle / name)
        shutil.copy2(ROOT / "release/VERIFYING.md", bundle / "VERIFYING.md")
        shutil.copytree(ROOT / "assets/branding", bundle / "branding")
        shutil.copy2(ROOT / "crates/rivet-desktop/assets/lucide/LICENSE", bundle / "LUCIDE-LICENSE")
        shutil.copy2(ROOT / "assets/fonts/OFL.txt", bundle / "POPPINS-LICENSE.txt")
        docs = bundle / "docs"
        docs.mkdir()
        for name in ("BROWSER.md", "CAPABILITIES.md", "ROADMAP.md"):
            shutil.copy2(ROOT / "docs" / name, docs / name)
        for browser in ("chromium", "firefox"):
            shutil.copytree(ROOT / "dist" / browser, bundle / "browser" / browser)
        if system == "macos":
            app = mac_bundle(stage, binaries, app_version)
            shutil.copytree(app, bundle / "Rivet.app")
            zip_tree(bundle, output / f"{base}.zip")
            if installer:
                run(["pkgbuild", "--component", app, "--install-location", "/Applications", "--identifier", "org.rivet.downloads", "--version", app_version, output / f"{base}.pkg"])
        else:
            bindir = bundle / "bin"
            bindir.mkdir()
            for name in ("rivet", "rivet-native-host", "rivet-desktop"):
                shutil.copy2(binaries / (name + suffix), bindir / (name + suffix))
            if system == "windows":
                zip_tree(bundle, output / f"{base}.zip")
                if installer:
                    compiler = shutil.which("ISCC") or str(Path(os.environ.get("ProgramFiles(x86)", "C:/Program Files (x86)")) / "Inno Setup 6/ISCC.exe")
                    run([compiler, f"/DAppVersion={app_version}", f"/DSourceDir={bundle}", f"/DReleaseDir={output}", f"/DOutputName={base}-setup", ROOT / "packaging/windows.iss"])
            else:
                with tarfile.open(output / f"{base}.tar.gz", "w:gz") as archive:
                    archive.add(bundle, arcname=base)
                if installer:
                    linux_deb(stage, output, base, app_version, machine)
    for browser in ("chromium", "firefox"):
        shutil.copy2(ROOT / "dist" / f"rivet-{browser}.zip", output / f"rivet-{app_version}-extension-{browser}.zip")
    shutil.copy2(ROOT / "dist/rivet-chrome-store.zip", output / f"rivet-{app_version}-chrome-store.zip")
    checksums(output)
    print(f"Packages and checksums: {output}\nNot authenticated until signed; no platform publisher signing was performed.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    p = commands.add_parser("package")
    p.add_argument("--output", type=Path, default=ROOT / "dist/releases" / version())
    p.add_argument("--installer", action="store_true", help="Also create a native unsigned installer (requires OS packaging tools)")
    p = commands.add_parser("checksums")
    p.add_argument("directory", type=Path)
    p = commands.add_parser("keygen")
    p.add_argument("--key-dir", type=Path, default=Path.home() / ".config/rivet-signing")
    p.add_argument("--keychain", action="store_true", help="Generate a strong password and keep it in macOS Keychain")
    p = commands.add_parser("sign")
    p.add_argument("directory", type=Path)
    p.add_argument("--key", type=Path, default=Path.home() / ".config/rivet-signing/minisign.key")
    passwords = p.add_mutually_exclusive_group()
    passwords.add_argument("--keychain", action="store_true", help="Use the password protected by macOS Keychain")
    passwords.add_argument("--password-env", help="Read a signing password from this environment variable without logging it")
    p.add_argument("--pubkey", type=Path, default=PUBLIC_KEY)
    p = commands.add_parser("verify")
    p.add_argument("directory", type=Path)
    p.add_argument("--pubkey", type=Path, default=PUBLIC_KEY)
    args = parser.parse_args()
    try:
        if args.command == "package": package(args.output, args.installer)
        elif args.command == "checksums": checksums(args.directory)
        elif args.command == "keygen": keygen(args.key_dir, args.keychain)
        elif args.command == "sign": sign(args.directory, args.key, args.pubkey, args.keychain, args.password_env)
        elif args.command == "verify": verify(args.directory, args.pubkey)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Release operation failed: {error}\n")


if __name__ == "__main__":
    main()
