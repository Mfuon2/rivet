"""End-to-end checks of Rivet binaries against an isolated local fixture.

Runs Rivet and an isolated local HTTP fixture server.
"""
import argparse
import functools
import hashlib
import http.server
import json
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import threading

parser = argparse.ArgumentParser()
parser.add_argument("--binary-dir", default="target/debug")
parser.add_argument("--report")
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
binary_dir = (root / args.binary_dir).resolve()
suffix = ".exe" if os.name == "nt" else ""
rivet = binary_dir / ("rivet" + suffix)
host = binary_dir / ("rivet-native-host" + suffix)

class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *_args):
        pass

with tempfile.TemporaryDirectory(prefix="rivet-smoke-") as temp:
    temp = Path(temp)
    data = bytes(range(251)) * 1024
    (temp / "fixture.bin").write_bytes(data)
    digest = hashlib.sha256(data).hexdigest()
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(QuietHandler, directory=str(temp)))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        url = f"http://127.0.0.1:{server.server_port}/fixture.bin"
        env = dict(os.environ, RIVET_DATABASE=str(temp / "queue.sqlite3"), RIVET_DOWNLOAD_DIR=str(temp / "browser-downloads"))

        def cli(*command):
            return subprocess.run([str(rivet), *command], env=env, capture_output=True, text=True, check=True, timeout=20).stdout

        done = json.loads(cli("get", url, "-o", str(temp / "immediate.bin"), "--sha256", digest))
        assert done["sha256"] == digest and (temp / "immediate.bin").read_bytes() == data
        job = cli("add", url, "-o", str(temp / "queued.bin")).strip()
        assert json.loads(cli("list"))[0]["status"] == "queued"
        cli("run", "--once")
        jobs = json.loads(cli("list"))
        assert jobs[0]["id"] == job and jobs[0]["status"] == "complete"
        assert (temp / "queued.bin").read_bytes() == data

        payload = json.dumps({"url": url}).encode()
        reply = subprocess.run([str(host)], input=struct.pack("<I", len(payload)) + payload, env=env, capture_output=True, check=True, timeout=10).stdout
        size = struct.unpack("<I", reply[:4])[0]
        assert size == len(reply)-4 and json.loads(reply[4:])["ok"]
        browser_job = next(j for j in json.loads(cli("list")) if j["status"] == "queued")
        assert Path(browser_job["request"]["destination"]).parent == temp / "browser-downloads"
        assert not Path(browser_job["request"]["destination"]).exists()
        manifest = json.loads(cli("host-manifest", "--executable", str(host), "--extension-id", "a" * 32))
        # Windows canonical paths may include the extended-length prefix; compare file identity.
        assert Path(manifest["path"]).samefile(host) and manifest["name"] == "org.rivet.downloads"
        result = {"cli_transfer": "passed", "durable_queue_worker": "passed", "native_host_framing_and_queue_only": "passed", "host_manifest": "passed", "fixture_sha256": digest, "all_network_traffic": "local HTTP fixture"}
    finally:
        server.shutdown()
        server.server_close()

if args.report:
    (root / args.report).write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps(result, indent=2))
