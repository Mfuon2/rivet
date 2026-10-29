import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parents[1]))
import publish
import release


class PublishTests(unittest.TestCase):
    def test_version_allocation_avoids_existing_tags_and_honors_source_series(self):
        self.assertEqual(publish.next_version("0.1.0", ["v0.1.0", "v0.1.2"]), "0.1.3")
        self.assertEqual(publish.next_version("0.2.0", ["v0.1.9"]), "0.2.0")
        self.assertEqual(publish.next_version("1.0.0", []), "1.0.0")
        for value in ["1.2.3;echo unsafe", "-1.2.3", "0.1.65536", "refs/heads/main"]:
            with self.assertRaises(ValueError): publish.version_tuple(value)

    def test_stamp_changes_only_workspace_and_extension_versions(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "Cargo.toml").write_text('[workspace.package]\nversion = "0.1.0"\nlicense = "MIT"\n')
            blocks = ['[[package]]\nname = "' + name + '"\nversion = "0.1.0"\n' for name in ("rivet-core", "rivet-cli", "rivet-desktop")]
            (root / "Cargo.lock").write_text('version = 4\n' + ''.join(blocks) + '[[package]]\nname = "external"\nversion = "9.8.7"\n')
            for browser in ("chromium", "firefox"):
                folder = root / "browser" / browser
                folder.mkdir(parents=True)
                (folder / "manifest.json").write_text(json.dumps({"version": "0.1.0", "key": "identity-stays-the-same"}))
            publish.stamp("0.1.5", root)
            self.assertIn('version = "0.1.5"', (root / "Cargo.toml").read_text())
            self.assertIn('version = "9.8.7"', (root / "Cargo.lock").read_text())
            self.assertEqual((root / "Cargo.lock").read_text().count('version = "0.1.5"'), 3)
            self.assertEqual(json.loads((root / "browser/chromium/manifest.json").read_text())["key"], "identity-stays-the-same")
            for name in ("Cargo.toml", "Cargo.lock", "browser/chromium/manifest.json", "browser/firefox/manifest.json"):
                self.assertNotIn(b"\r\n", (root / name).read_bytes(), "Release stamping must produce identical bytes on Windows and Unix")

    def test_existing_published_commit_is_skipped_on_rerun(self):
        commit = "a" * 40
        releases = [[{"target_commitish": commit, "tag_name": "v0.1.7", "draft": False}]]
        with tempfile.TemporaryDirectory() as temporary, patch.object(publish, "gh", return_value=json.dumps(releases)), patch.dict(os.environ, {"GITHUB_OUTPUT": str(Path(temporary) / "outputs")}):
            with contextlib.redirect_stdout(io.StringIO()): publish.plan("owner/repo", commit)
            text = (Path(temporary) / "outputs").read_text()
            self.assertIn("version=0.1.7", text)
            self.assertIn("already=true", text)

    def test_collect_requires_every_platform_and_rejects_conflicting_duplicates(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "input/platform"
            source.mkdir(parents=True)
            names = ["windows-x86_64-setup.exe", "windows-x86_64.zip", "linux-x86_64.deb", "linux-x86_64.tar.gz", "macos-aarch64.pkg", "macos-aarch64.zip", "extension-chromium.zip", "extension-firefox.zip", "chrome-store.zip"]
            for name in names:
                (source / ("rivet-0.1.1-" + name)).write_bytes(b"test artifact")
            release.checksums(source)
            publish.collect(root / "input", root / "complete", "0.1.1")
            self.assertEqual(release.verify_hashes(root / "complete"), 9)
            (source / "rivet-0.1.1-windows-x86_64-setup.exe").unlink()
            release.checksums(source)
            with self.assertRaises(ValueError): publish.collect(root / "input", root / "missing", "0.1.1")
            duplicate = root / "input/other"
            duplicate.mkdir()
            (duplicate / "rivet-0.1.1-extension-chromium.zip").write_bytes(b"different bytes")
            with self.assertRaises(ValueError): publish.collect(root / "input", root / "conflict", "0.1.1")

    def test_signing_requires_configured_environment_secrets(self):
        with patch.dict(os.environ, {}, clear=True):
            with self.assertRaises(ValueError): publish.sign_from_environment(Path("unused"))

    @unittest.skipUnless(shutil.which("minisign"), "Install Minisign to test encrypted CI signing")
    def test_encrypted_environment_signing_uses_the_pinned_public_key(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            private, public = root / "test.key", root / "test.pub"
            password = "temporary-test-password-not-a-production-secret"
            subprocess.run([release.minisign(), "-G", "-s", private, "-p", public], input=password + "\n" + password + "\n", text=True, capture_output=True, check=True)
            packages = root / "packages"
            packages.mkdir()
            (packages / "rivet-test.zip").write_bytes(b"fixture")
            release.checksums(packages)
            with patch.dict(os.environ, {"MINISIGN_SECRET_KEY": private.read_text(), "MINISIGN_PASSWORD": password}), patch.object(publish.release, "PUBLIC_KEY", public):
                publish.sign_from_environment(packages)
            self.assertEqual((packages / "minisign.pub").read_bytes(), public.read_bytes())
            release.verify(packages, public)


if __name__ == "__main__": unittest.main()
