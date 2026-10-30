"""Build original Rivet extension directories and ZIPs without installing them."""
import argparse
from pathlib import Path
import base64
import hashlib
import json
import shutil
import zipfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--chrome-store", action="store_true", help="Also create a Chrome Web Store upload ZIP without development-only manifest fields")
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
# Stable IDs let first launch register the native host without asking users to copy IDs.
distribution = json.loads((root / "browser/distribution.json").read_text())
chromium = json.loads((root / "browser/chromium/manifest.json").read_text())
key = base64.b64decode(chromium["key"], validate=True)
identity = "".join(chr(ord("a") + int(n, 16)) for n in hashlib.sha256(key).hexdigest()[:32])
assert identity == distribution["chromium_extension_id"], "Chromium public key and distribution ID must match"
firefox = json.loads((root / "browser/firefox/manifest.json").read_text())
assert firefox["browser_specific_settings"]["gecko"]["id"] == distribution["firefox_extension_id"]
for browser in ("chromium", "firefox"):
    output = root / "dist" / browser
    if output.exists():
        shutil.rmtree(output)
    output.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(root / "browser" / browser / "manifest.json", output / "manifest.json")
    for source in (root / "browser" / "shared").iterdir():
        if source.is_file():
            shutil.copyfile(source, output / source.name)
    for name, source in [("Poppins-Regular.ttf", "Poppins-Regular.ttf"), ("Poppins-SemiBold.ttf", "Poppins-SemiBold.ttf"), ("POPPINS-LICENSE.txt", "OFL.txt")]:
        shutil.copy2(root / "assets/fonts" / source, output / name)
    icons = output / "icons"
    icons.mkdir(exist_ok=True)
    for size in (16, 32, 48, 64, 128):
        shutil.copy2(root / "assets/branding/icons" / f"icon-{size}.png", icons / f"icon-{size}.png")
    json.loads((output / "manifest.json").read_text())
    # Fixed metadata and stored entries make extension ZIPs identical across OS builds.
    with zipfile.ZipFile(root / "dist" / f"rivet-{browser}.zip", "w") as archive:
        for source in sorted(output.rglob("*"), key=lambda path: path.relative_to(output).as_posix()):
            if source.is_file():
                entry = zipfile.ZipInfo(source.relative_to(output).as_posix(), (1980, 1, 1, 0, 0, 0))
                entry.create_system = 3
                entry.external_attr = 0o100644 << 16
                archive.writestr(entry, source.read_bytes())
    print(output)

if args.chrome_store:
    output = root / "dist/chrome-store"
    if output.exists():
        shutil.rmtree(output)
    output.mkdir(parents=True, exist_ok=True)
    store_manifest = dict(chromium)
    store_manifest.pop("key", None)
    store_manifest.pop("update_url", None)
    # Use an allowlisted source directory so no local files or development metadata leak in.
    with zipfile.ZipFile(root / "dist/rivet-chrome-store.zip", "w") as archive:
        for source in sorted((root / "dist/chromium").rglob("*"), key=lambda path: path.relative_to(root / "dist/chromium").as_posix()):
            if not source.is_file():
                continue
            name = source.relative_to(root / "dist/chromium").as_posix()
            data = (json.dumps(store_manifest, indent=2) + "\n").encode() if name == "manifest.json" else source.read_bytes()
            entry = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
            entry.create_system = 3
            entry.external_attr = 0o100644 << 16
            archive.writestr(entry, data)
    print(root / "dist/rivet-chrome-store.zip")
