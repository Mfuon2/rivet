import hashlib
import http.server
import importlib.util
from pathlib import Path
import tempfile
import threading
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("media_tools", Path(__file__).parents[1] / "prepare_media_tools.py")
media = importlib.util.module_from_spec(spec)
spec.loader.exec_module(media)

class MediaDownloadTests(unittest.TestCase):
    def test_transient_download_retries_and_cached_bytes_are_still_authenticated(self):
        payload = b"owned dependency test bytes"
        attempts = []
        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *args): pass
            def do_GET(self):
                attempts.append(1)
                if len(attempts) == 1:
                    self.send_error(503)
                else:
                    self.send_response(200)
                    self.send_header("Content-Length", str(len(payload)))
                    self.end_headers()
                    self.wfile.write(payload)
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with tempfile.TemporaryDirectory() as directory, patch.object(media.time, "sleep"):
                request = {"url": f"http://127.0.0.1:{server.server_port}/fixture", "sha256": hashlib.sha256(payload).hexdigest()}
                path = media.fetch(request, Path(directory))
                self.assertEqual(path.read_bytes(), payload)
                self.assertEqual(len(attempts), 2)
                path.write_bytes(b"changed cache")
                with self.assertRaises(ValueError): media.fetch(request, Path(directory))
        finally:
            server.shutdown()
            server.server_close()
