"""Static release-evidence tests. The fake program bytes are never executed."""

import hashlib
import io
import json
from pathlib import Path
import runpy
import struct
import subprocess
import sys
import tempfile
import unittest
import warnings
import zipfile


SCRIPT = Path(__file__).resolve().parents[1] / "verify-published-release.py"
VERIFIER = runpy.run_path(str(SCRIPT))
TAG = "v0.3.0"
COMMIT = "b34188983fdfe2d5e502380ce02873131d388c8e"
RUN = "44415431944"
PREFIX = f"ExorSystem-{TAG}-x64"
SETUP, MSI, PORTABLE = (f"{PREFIX}{s}" for s in ("-Setup.exe", ".msi", "-Portable.zip"))
URL = "https://github.com/SeanShaffer88/ExorSystem"


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def pe(machine: int = 0x8664) -> bytes:
    result = bytearray(70)
    result[:2] = b"MZ"
    struct.pack_into("<I", result, 0x3C, 64)
    result[64:68] = b"PE\0\0"
    struct.pack_into("<H", result, 68, machine)
    return bytes(result)


def portable(extra: str | None = None, machine: int = 0x8664) -> bytes:
    data = io.BytesIO()
    with zipfile.ZipFile(data, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("ExorSystem.exe", pe(machine))
        for name in ("README.md", "LICENSE", "CHANGELOG.md", "CREDITS.md"):
            archive.writestr(name, "fixture\n")
        if extra is not None:
            with warnings.catch_warnings():
                warnings.simplefilter("ignore", UserWarning)
                archive.writestr(extra, "extra fixture entry\n")
    return data.getvalue()


class PortableEntriesTests(unittest.TestCase):
    def test_command_line_is_expected_only_after_v0_3_0(self):
        entries = VERIFIER["portable_entries"]
        self.assertNotIn("exor.exe", entries("v0.3.0"))
        self.assertNotIn("exor.exe", entries("v0.2.0"))
        self.assertIn("exor.exe", entries("v0.3.1"))
        self.assertIn("exor.exe", entries("v1.0.0"))

    def test_later_release_needs_an_x64_command_line(self):
        def archive(cli):
            data = io.BytesIO()
            with zipfile.ZipFile(data, "w") as z:
                z.writestr("ExorSystem.exe", pe())
                for name in ("README.md", "LICENSE", "CHANGELOG.md", "CREDITS.md"):
                    z.writestr(name, "fixture\n")
                if cli is not None:
                    z.writestr("exor.exe", cli)
            folder = tempfile.TemporaryDirectory()
            self.addCleanup(folder.cleanup)
            path = Path(folder.name) / "p.zip"
            path.write_bytes(data.getvalue())
            return path

        inspect, entries = VERIFIER["inspect_portable"], VERIFIER["portable_entries"]
        later = entries("v0.4.0")
        self.assertEqual(inspect(archive(pe()), later)["command_line"]["sha256"], sha(pe()))
        with self.assertRaisesRegex(ValueError, "portable ZIP entries"):
            inspect(archive(None), later)
        with self.assertRaisesRegex(ValueError, "exor.exe is not the expected x64"):
            inspect(archive(pe(0x014C)), later)
        # v0.3.0 still verifies without it, and treats it as an unexpected extra entry
        self.assertNotIn("command_line", inspect(archive(None), entries("v0.3.0")))
        with self.assertRaisesRegex(ValueError, "portable ZIP entries"):
            inspect(archive(pe()), entries("v0.3.0"))


class PublishedReleaseTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.packages = self.root / "packages"
        self.packages.mkdir()
        self.release_info = self.root / "release.json"
        self.data = {SETUP: pe(0x014C), MSI: bytes.fromhex("d0cf11e0a1b11ae1") + b"fixture", PORTABLE: portable()}
        self.write_release()

    def write_release(self, candidate_run: str = RUN):
        """Emit small fixtures in the shape of our Release workflow's metadata."""
        assets = self.data.copy()
        assets["SHA256SUMS.txt"] = "".join(f"{sha(data)}  {name}\n" for name, data in sorted(self.data.items())).encode()
        assets["PACKAGE-CHECKS.json"] = json.dumps({
            "kind": "static-package-checks-only", "app_version": TAG[1:], "commit": COMMIT, "workflow_run": candidate_run,
            "artifacts": [{"name": name, "bytes": len(data), "sha256": sha(data)} for name, data in self.data.items()],
        }).encode()
        for name, data in assets.items():
            (self.packages / name).write_bytes(data)
        self.metadata = {
            "tag_name": TAG, "html_url": f"{URL}/releases/tag/{TAG}", "target_commitish": COMMIT,
            "draft": False, "prerelease": False,
            "body": f"# Release fixture\n\n## Verification\n\n- Source commit: `{COMMIT}`\n- [Windows candidate package run]({URL}/actions/runs/{RUN})\n",
            "assets": [
                {"name": name, "state": "uploaded", "size": len(data), "digest": f"sha256:{sha(data)}"}
                for name, data in assets.items()
            ],
        }
        self.save_metadata()

    def save_metadata(self):
        self.release_info.write_text(json.dumps(self.metadata), encoding="utf-8")

    def verify(self):
        return VERIFIER["verify_published"](self.packages, self.release_info, TAG)

    def command(self, path: Path):
        return subprocess.run([
            sys.executable, "-B", str(SCRIPT), str(self.packages), "--release-info", str(self.release_info),
            "--tag", TAG, "--output", str(path),
        ], capture_output=True, text=True, check=False)

    def test_exact_asset_set_passes_and_fingerprints_only_portable_executable(self):
        report = self.verify()
        self.assertEqual(report["release"], {"tag": TAG, "commit": COMMIT, "candidate_run_id": RUN, "url": self.metadata["html_url"]})
        self.assertEqual(report["portable_binary"]["sha256"], sha(pe()))
        self.assertEqual({item["name"] for item in report["packages"].values()}, {SETUP, MSI, PORTABLE})
        self.assertEqual(len(report["evidence_files"]), 2)
        self.assertEqual(report["static_findings"]["nsis_launcher_pe_machine"], "0x014c")
        self.assertTrue(report["static_findings"]["candidate_report_and_checksums_match"])
        self.assertFalse((self.packages / "ExorSystem.exe").exists())

    def test_size_preserving_modified_package_is_rejected(self):
        path = self.packages / SETUP
        path.write_bytes(b"X" * path.stat().st_size)
        with self.assertRaisesRegex(ValueError, "published SHA-256 mismatch"):
            self.verify()

    def test_local_evidence_cannot_be_rewritten_to_hide_modified_package(self):
        original_release = self.release_info.read_bytes()
        self.data[MSI] = self.data[MSI][:-1] + b"!"
        self.write_release()
        self.release_info.write_bytes(original_release)
        with self.assertRaisesRegex(ValueError, "published SHA-256 mismatch"):
            self.verify()

    def test_consistent_asset_hashes_do_not_excuse_wrong_candidate(self):
        self.write_release(candidate_run="44415431945")
        with self.assertRaisesRegex(ValueError, "requested run, version and commit"):
            self.verify()

    def test_extra_local_file_or_missing_package_fails(self):
        extra = self.packages / "other.exe"
        extra.write_bytes(b"unlisted")
        with self.assertRaisesRegex(ValueError, "only the five release assets"):
            self.verify()
        extra.unlink()
        (self.packages / MSI).unlink()
        with self.assertRaisesRegex(ValueError, "release assets are missing"):
            self.verify()

    def test_non_official_metadata_and_duplicate_assets_fail(self):
        self.metadata["html_url"] = "https://github.com/Elsewhere/ExorSystem/releases/tag/v0.3.0"
        self.save_metadata()
        with self.assertRaisesRegex(ValueError, "requested published stable release"):
            self.verify()
        self.metadata["html_url"] = f"{URL}/releases/tag/{TAG}"
        self.metadata["assets"][-1] = self.metadata["assets"][0]
        self.save_metadata()
        with self.assertRaisesRegex(ValueError, "duplicate published asset"):
            self.verify()

    def test_incomplete_or_mismatched_metadata_is_not_verified(self):
        self.metadata["assets"][0]["digest"] = None
        self.save_metadata()
        with self.assertRaisesRegex(ValueError, "published SHA-256 is missing"):
            self.verify()
        self.write_release()
        self.metadata["target_commitish"] = "a" * 40
        self.save_metadata()
        with self.assertRaisesRegex(ValueError, "same source commit"):
            self.verify()

    def test_suspicious_zip_shapes_and_32_bit_desktop_fail(self):
        for extra in ("../hidden.exe", "ExorSystem.exe"):
            with self.subTest(extra=extra):
                self.data[PORTABLE] = portable(extra)
                self.write_release()
                with self.assertRaisesRegex(ValueError, "portable ZIP entries"):
                    self.verify()
        self.data[PORTABLE] = portable(machine=0x014C)
        self.write_release()
        with self.assertRaisesRegex(ValueError, "expected x64 PE"):
            self.verify()

    def test_valid_release_digests_do_not_hide_broken_package_headers(self):
        for name, failure in ((SETUP, "missing PE DOS header"), (MSI, "compound-file header")):
            with self.subTest(name=name):
                original = self.data[name]
                self.data[name] = b"0" * len(original)
                self.write_release()
                with self.assertRaisesRegex(ValueError, failure):
                    self.verify()
                self.data[name] = original

    def test_invalid_zip_records_final_failure_even_with_matching_digests(self):
        self.data[PORTABLE] = b"not a ZIP"
        self.write_release()
        report = self.root / "corrupt-zip.json"
        self.assertEqual(self.command(report).returncode, 1)
        self.assertEqual(json.loads(report.read_text())["status"], "Fail")

    def test_linked_package_is_not_accepted(self):
        path = self.packages / MSI
        target = self.root / MSI
        path.rename(target)
        try:
            path.symlink_to(target)
        except OSError as exc:
            self.skipTest(f"symlinks unavailable on this runner: {type(exc).__name__}")
        with self.assertRaisesRegex(ValueError, "regular, non-redirected file"):
            self.verify()

    def test_cli_outputs_non_overwriting_evidence_and_never_claims_launch(self):
        report = self.root / "report.json"
        result = self.command(report)
        self.assertEqual(result.returncode, 0, result.stderr)
        record = json.loads(report.read_text())
        self.assertEqual(record["status"], "Pass")
        self.assertIn("Native desktop launch, profile creation and shutdown", record["not_tested"])
        first = report.read_bytes()
        self.assertNotEqual(self.command(report).returncode, 0)
        self.assertEqual(report.read_bytes(), first)
        inside = self.packages / "report.json"
        self.assertNotEqual(self.command(inside).returncode, 0)
        self.assertFalse(inside.exists())

    def test_cli_failed_verification_records_failure(self):
        (self.packages / SETUP).write_bytes(b"not the release installer")
        report = self.root / "failure.json"
        self.assertEqual(self.command(report).returncode, 1)
        record = json.loads(report.read_text())
        self.assertEqual(record["status"], "Fail")
        self.assertNotIn("packages", record)
        self.assertIn("size mismatch", record["error"])


if __name__ == "__main__":
    unittest.main()
