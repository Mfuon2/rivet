import importlib.util
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("release", Path(__file__).parents[1] / "release.py")
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)

class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="rivet-release-test-")
        self.addCleanup(self.temp.cleanup)
        self.folder = Path(self.temp.name)
        self.artifact = self.folder / "rivet-0.1.0-test.zip"
        self.artifact.write_bytes(b"release fixture, not an executable")

    def test_checksums_cover_packages_and_detect_changed_bytes(self):
        release.checksums(self.folder)
        self.assertEqual(release.verify_hashes(self.folder), 1)
        self.artifact.write_bytes(b"tampered")
        with self.assertRaises(ValueError): release.verify_hashes(self.folder)

    def test_manifest_rejects_traversal_duplicates_and_unlisted_packages(self):
        manifest = release.checksums(self.folder)
        original = manifest.read_text()
        for data in [original + original, "0" * 64 + "  ../outside.zip\n", "0" * 64 + "  /outside.zip\n"]:
            manifest.write_text(data)
            with self.assertRaises(ValueError): release.verify_hashes(self.folder)
        manifest.write_text(original)
        (self.folder / "extra.zip").write_bytes(b"unlisted")
        with self.assertRaises(ValueError): release.verify_hashes(self.folder)

    @unittest.skipIf(__import__("os").name == "nt", "Symlink creation may require Windows privileges")
    def test_symlinks_are_not_release_packages(self):
        (self.folder / "linked.zip").symlink_to(self.artifact)
        with self.assertRaises(ValueError): release.checksums(self.folder)

    def test_rehashing_invalidates_old_signature(self):
        signature = self.folder / "SHA256SUMS.minisig"
        signature.write_text("old signature")
        release.checksums(self.folder)
        self.assertFalse(signature.exists())

    def test_personal_fonts_and_stale_files_never_enter_extension_releases(self):
        import zipfile
        output = release.ROOT / "dist/chromium"
        output.mkdir(parents=True, exist_ok=True)
        (output / "segoeui.woff").write_bytes(b"private font sentinel, not a font")
        (output / "unrelated.txt").write_text("stale generated output")
        subprocess.run([__import__("sys").executable, str(release.ROOT / "scripts/package_browser.py"), "--chrome-store"], check=True, stdout=subprocess.DEVNULL)
        for name in ["rivet-chromium.zip", "rivet-chrome-store.zip"]:
            with zipfile.ZipFile(release.ROOT / "dist" / name) as archive:
                self.assertNotIn("segoeui.woff", archive.namelist())
                self.assertNotIn("unrelated.txt", archive.namelist())
                self.assertNotIn("src:url", archive.read("user-font.css").decode())

    def test_extension_packages_are_reproducible(self):
        command = [__import__("sys").executable, str(release.ROOT / "scripts/package_browser.py")]
        subprocess.run(command, check=True, stdout=subprocess.DEVNULL)
        first = [(release.ROOT / f"dist/rivet-{browser}.zip").read_bytes() for browser in ("chromium", "firefox")]
        subprocess.run(command, check=True, stdout=subprocess.DEVNULL)
        second = [(release.ROOT / f"dist/rivet-{browser}.zip").read_bytes() for browser in ("chromium", "firefox")]
        self.assertEqual(first, second)
        import zipfile
        for browser in ("chromium", "firefox"):
            with zipfile.ZipFile(release.ROOT / f"dist/rivet-{browser}.zip") as archive:
                self.assertEqual(archive.namelist(), sorted(archive.namelist()), "Archive names must use platform-independent lexical ordering")

    def test_chrome_store_upload_omits_development_key_and_keeps_assets(self):
        import json
        import zipfile
        subprocess.run([__import__("sys").executable, str(release.ROOT / "scripts/package_browser.py"), "--chrome-store"], check=True, stdout=subprocess.DEVNULL)
        with zipfile.ZipFile(release.ROOT / "dist/rivet-chrome-store.zip") as archive:
            manifest = json.loads(archive.read("manifest.json"))
            self.assertEqual(archive.namelist(), sorted(archive.namelist()), "Store archive ordering must match on Windows and POSIX")
            self.assertNotIn("key", manifest)
            self.assertNotIn("update_url", manifest)
            self.assertEqual(manifest["manifest_version"], 3)
            self.assertEqual(manifest["version"], release.version())
            for icon in manifest["icons"].values():
                self.assertIn(icon, archive.namelist())
            for font in ["Poppins-Regular.ttf", "Poppins-SemiBold.ttf", "POPPINS-LICENSE.txt"]:
                self.assertIn(font, archive.namelist())
            self.assertIn("background.js", archive.namelist())
            self.assertIn("popup.html", archive.namelist())
        development = json.loads((release.ROOT / "dist/chromium/manifest.json").read_text())
        self.assertIn("key", development, "Store packaging must not change development identity")

    @unittest.skipUnless(shutil.which("minisign"), "Install Minisign for real cryptographic verification tests")
    def test_real_signature_verification_rejects_tampering_and_wrong_keys(self):
        private, public = self.folder / "temporary.key", self.folder / "temporary.pub"
        # Only disposable test keys are unencrypted. They never enter source or published artifacts.
        subprocess.run([release.minisign(), "-G", "-W", "-p", public, "-s", private], check=True, stdout=subprocess.DEVNULL)
        release.checksums(self.folder)
        release.sign(self.folder, private, public)
        other_private, other_public = self.folder / "other.key", self.folder / "other.pub"
        subprocess.run([release.minisign(), "-G", "-W", "-p", other_public, "-s", other_private], check=True, stdout=subprocess.DEVNULL)
        with self.assertRaises(subprocess.CalledProcessError): release.verify(self.folder, other_public)
        self.artifact.write_bytes(b"changed after signing")
        with self.assertRaises(ValueError): release.verify(self.folder, public)
        (self.folder / "SHA256SUMS").write_text("0" * 64 + "  rivet-0.1.0-test.zip\n")
        with self.assertRaises(subprocess.CalledProcessError): release.verify(self.folder, public)

if __name__ == "__main__": unittest.main()
