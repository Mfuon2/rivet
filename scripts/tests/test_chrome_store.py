import contextlib
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile

sys.path.insert(0, str(Path(__file__).parents[1]))
import chrome_store

CREDS = {"CWS_PUBLISHER_ID": "publisher-123", "CWS_CLIENT_ID": "test-client", "CWS_CLIENT_SECRET": "secret-fixture", "CWS_REFRESH_TOKEN": "refresh-fixture"}


class StoreTests(unittest.TestCase):
    def package(self, directory, **extra):
        path = Path(directory) / "rivet-0.1.8-chrome-store.zip"
        with zipfile.ZipFile(path, "w") as archive:
            archive.writestr("manifest.json", json.dumps(dict(version="0.1.8", manifest_version=3, **extra)))
        return path

    def test_missing_credentials_reports_setup_and_partial_configuration_fails(self):
        with patch.dict(os.environ, {}, clear=True), contextlib.redirect_stdout(io.StringIO()) as output:
            self.assertFalse(chrome_store.configured())
            self.assertIn("submission was skipped", output.getvalue())
        with patch.dict(os.environ, {"CWS_CLIENT_ID": "fixture"}, clear=True):
            with self.assertRaisesRegex(ValueError, "Incomplete publisher setup"):
                chrome_store.configured()

    def test_store_manifest_rejects_dev_key_wrong_version_and_format(self):
        with tempfile.TemporaryDirectory() as directory:
            package = self.package(directory, key="dev-public-key")
            with self.assertRaises(ValueError): chrome_store.validate_package(package, "0.1.8")
            package = self.package(directory)
            with self.assertRaises(ValueError): chrome_store.validate_package(package, "0.1.9")
            chrome_store.validate_package(package, "0.1.8")

    def test_fixed_endpoint_does_not_allow_credentials_redirect(self):
        with self.assertRaises(ValueError): chrome_store.request("https://attacker.example/token", token="test")
        redirect = chrome_store.NoRedirect()
        self.assertIsNone(redirect.redirect_request(None, None, 302, "", {}, "https://attacker.example"))

    def test_submit_uploads_verified_version_and_requests_review_without_bypass(self):
        calls = []
        replies = [{"access_token": "token-fixture"}, {}, {"uploadState": "IN_PROGRESS"}, {"lastAsyncUploadState": "SUCCEEDED"}, {"state": "PENDING_REVIEW"}]
        def api(url, **kwargs):
            calls.append((url, kwargs))
            return replies.pop(0)
        with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, CREDS, clear=True), contextlib.redirect_stdout(io.StringIO()) as output:
            chrome_store.submit(self.package(directory), "0.1.8", api=api, pause=lambda _: None)
            self.assertIn("PENDING_REVIEW", output.getvalue())
            self.assertNotIn("token-fixture", output.getvalue())
        self.assertTrue(calls[2][0].endswith("/items/oloipikeghldaokdiglloelbpfheeeik:upload?uploadType=media"))
        body = json.loads(calls[-1][1]["data"])
        self.assertEqual(body, {"publishType": "DEFAULT_PUBLISH", "skipReview": False, "blockOnWarnings": True})

    def test_existing_review_is_idempotent_and_different_review_is_preserved(self):
        for pending_version, blocked in [("0.1.8", False), ("0.1.7", True)]:
            replies = [{"access_token": "fixture"}, {"submittedItemRevisionStatus": {"state": "PENDING_REVIEW", "distributionChannels": [{"crxVersion": pending_version}]}}]
            calls = []
            def api(url, **kwargs):
                calls.append(url)
                return replies.pop(0)
            with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, CREDS, clear=True), contextlib.redirect_stdout(io.StringIO()):
                if blocked:
                    with self.assertRaisesRegex(ValueError, "was preserved"):
                        chrome_store.submit(self.package(directory), "0.1.8", api=api)
                else:
                    chrome_store.submit(self.package(directory), "0.1.8", api=api)
            self.assertEqual(len(calls), 2)

    def test_failed_or_wrong_version_upload_never_submits(self):
        for upload in [{"uploadState": "FAILED"}, {"uploadState": "SUCCEEDED", "crxVersion": "0.1.9"}]:
            replies = [{"access_token": "fixture"}, {}, upload]
            with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, CREDS, clear=True):
                with self.assertRaises(ValueError):
                    chrome_store.submit(self.package(directory), "0.1.8", api=lambda *args, **kwargs: replies.pop(0))
            self.assertFalse(replies)

    def test_download_refuses_preview_before_downloading(self):
        metadata = json.dumps({"draft": False, "prerelease": True})
        with tempfile.TemporaryDirectory() as directory, patch.object(chrome_store, "gh", return_value=metadata) as gh:
            with self.assertRaisesRegex(ValueError, "stable releases"):
                chrome_store.download("Mfuon2/rivet", "v0.1.8", Path(directory))
            self.assertEqual(gh.call_count, 1)


if __name__ == "__main__": unittest.main()
