"""Build original Rivet extension directories and ZIPs without installing them."""
from pathlib import Path
import base64
import hashlib
import json
import shutil
import zipfile

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
    output.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(root / "browser" / browser / "manifest.json", output / "manifest.json")
    for source in (root / "browser" / "shared").iterdir():
        if source.is_file():
            shutil.copyfile(source, output / source.name)
    json.loads((output / "manifest.json").read_text())
    with zipfile.ZipFile(root / "dist" / f"rivet-{browser}.zip", "w", zipfile.ZIP_DEFLATED) as archive:
        for source in sorted(output.iterdir()):
            archive.write(source, source.name)
    print(output)
