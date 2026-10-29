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
