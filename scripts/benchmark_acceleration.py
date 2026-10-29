"""Measure Rivet against a controlled HTTP range server, verifying every output.

The fixture throttles each response independently. Results demonstrate concurrency
under that model; they are not an Internet speed claim or a comparison with IDM.
"""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import socket
import statistics
import subprocess
import tempfile
import threading
import time

parser = argparse.ArgumentParser()
parser.add_argument("--trials", type=int, default=3)
parser.add_argument("--binary-dir", default="target/release")
parser.add_argument("--report", default="docs/acceleration-benchmark.json")
args = parser.parse_args()
if not 1 <= args.trials <= 10:
    parser.error("trials must be 1–10")
root = Path(__file__).resolve().parents[1]
binary = root / args.binary_dir / ("rivet.exe" if os.name == "nt" else "rivet")
data = bytes(range(256)) * (32 * 1024)
digest = hashlib.sha256(data).hexdigest()

class RangeServer(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    def log_message(self, *_args):
        pass
    def do_GET(self):
        self.connection.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        header = self.headers.get("Range")
        start, end = (0, len(data)-1)
        if header:
            lo, hi = header.removeprefix("bytes=").split("-")
            start, end = int(lo), int(hi)
        self.send_response(206 if header else 200)
        self.send_header("Content-Length", end-start+1)
        self.send_header("ETag", '"rivet-benchmark-v1"')
        if header:
            self.send_header("Content-Range", f"bytes {start}-{end}/{len(data)}")
        self.end_headers()
        try:
            for pos in range(start, end+1, 64*1024):
                if end > start:
                    time.sleep(0.008)
                self.wfile.write(data[pos:min(end+1, pos+64*1024)])
                self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            pass

server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), RangeServer)
threading.Thread(target=server.serve_forever, daemon=True).start()
measurements = []
try:
    with tempfile.TemporaryDirectory(prefix="rivet-benchmark-") as temp:
        for connections in (1, 4, 8):
            trials = []
            for trial in range(args.trials):
                output = Path(temp) / f"c{connections}-t{trial}.bin"
                start = time.perf_counter()
                command = [str(binary), "get", f"http://127.0.0.1:{server.server_port}/fixture", "-o", str(output), "-c", str(connections), "--sha256", digest]
                subprocess.run(command, capture_output=True, check=True, timeout=30)
                elapsed = time.perf_counter()-start
                assert hashlib.sha256(output.read_bytes()).hexdigest() == digest
                trials.append(elapsed)
            median = statistics.median(trials)
            measurements.append({"connections": connections, "median_seconds": median, "median_mib_per_second": len(data)/1048576/median, "trial_seconds": trials})
finally:
    server.shutdown()
    server.server_close()
baseline = measurements[0]["median_seconds"]
for row in measurements:
    row["speedup_vs_single_connection"] = baseline / row["median_seconds"]
result = {"fixture_bytes": len(data), "fixture_sha256": digest, "per_response_throttle": "64 KiB per 8 ms", "measurements": measurements,
          "scope": "Local controlled per-response throttling. Not an Internet speed guarantee.",
          "strategy": "Bounded dynamic scheduling over preplanned ranges; pooled HTTP connections. Mid-response segment splitting remains planned."}
(root / args.report).write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps(result, indent=2))
