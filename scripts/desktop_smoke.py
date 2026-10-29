"""Launch only Rivet with an isolated database; optionally capture its own macOS window."""
import argparse
import json
import os
from pathlib import Path
import platform
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser()
parser.add_argument("--screenshot", action="store_true")
parser.add_argument("--startup", action="store_true", help="Save a separate first-launch setup preview")
parser.add_argument("--seed-demo", action="store_true", help="Queue synthetic example.test links; never download them")
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
binary = root / "target" / "debug" / ("rivet-desktop.exe" if os.name == "nt" else "rivet-desktop")
with tempfile.TemporaryDirectory(prefix="rivet-ui-smoke-") as temp:
    env = dict(os.environ, RIVET_DATABASE=str(Path(temp) / "ui.sqlite3"), RIVET_SETUP_ROOT=str(Path(temp) / "browser-home"))
    fixture_count = 0
    if args.seed_demo:
        cli = binary.with_name("rivet.exe" if os.name == "nt" else "rivet")
        examples = ["linux-desktop.iso", "project-source.zip", "release-notes.pdf", "conference-talk.mp4", "ambient-session.flac", "rivet-setup.exe", "research-data.csv", "design-assets.7z", "podcast-episode.mp3", "tutorial-series.webm", "deployment-guide.pdf", "linux-package.deb"]
        for index in range(36):
            name = f"{index+1:02d}-{examples[index % len(examples)]}"
            subprocess.run([str(cli), "add", "https://example.test/releases/"+name, "-o", str(Path(temp)/name)], env=env, capture_output=True, check=True, timeout=10)
            fixture_count += 1
    process = subprocess.Popen([str(binary)], cwd=root, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    result = {"platform": platform.platform(), "isolated_database": True, "synthetic_queued_entries": fixture_count, "fixture_downloads_started": False}
    try:
        time.sleep(3)
        result["startup_alive_after_3_seconds"] = process.poll() is None
        if args.screenshot and process.poll() is None and platform.system() == "Darwin":
            source = '''import CoreGraphics; import Foundation
let target = Int(CommandLine.arguments.last!)!
if let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] {
  for window in windows where window[kCGWindowOwnerPID as String] as? Int == target {
    if let n = window[kCGWindowNumber as String] as? Int { print(n) }
  }
}'''
            windows = subprocess.run(["swift", "-e", source, str(process.pid)], capture_output=True, text=True, timeout=25)
            ids = [line for line in windows.stdout.splitlines() if line.isdigit()]
            result["visible_owned_window"] = bool(ids)
            if ids:
                capture = subprocess.run(["screencapture", "-x", "-l", ids[0], str(root / ("docs/startup-preview.png" if args.startup else "docs/desktop-preview.png"))], capture_output=True, text=True, timeout=10)
                result["screenshot_saved"] = capture.returncode == 0
                if capture.returncode:
                    result["screenshot_error"] = capture.stderr.strip()
    finally:
        if process.poll() is None:
            process.terminate()
        try:
            stdout, stderr = process.communicate(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            stdout, stderr = process.communicate()
    result["stderr"] = stderr.decode(errors="replace")[-2500:]
    (root / "docs/desktop-smoke.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
    assert result["startup_alive_after_3_seconds"], "desktop startup failed"
