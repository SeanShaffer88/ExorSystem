"""Release policy fixtures only. They do not represent native Windows runs."""

import hashlib
import json
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "verify-release-candidate.py"
POLICY = runpy.run_path(str(SCRIPT))
VERSION = POLICY["stable_version"](SCRIPT.parents[1])
COMMIT = "a" * 40
RUN = "123456"


class CandidateClientSignoffTests(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        self.dist = self.root / "candidate"
        self.dist.mkdir()
        self.evidence = self.root / "client-signoff.json"
        self.matrix = self.root / "matrix.md"
        artifacts = []
        for package in POLICY["packages"](VERSION):
            data = f"test fixture bytes for {package}\n".encode()
            (self.dist / package).write_bytes(data)
            artifacts.append({"name": package, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()})
        (self.dist / "SHA256SUMS.txt").write_text("".join(f"{a['sha256']}  {a['name']}\n" for a in artifacts), encoding="ascii")
        (self.dist / "PACKAGE-CHECKS.json").write_text(json.dumps({
            "kind": "static-package-checks-only", "app_version": VERSION, "commit": COMMIT, "workflow_run": RUN,
            "artifacts": artifacts,
        }), encoding="utf-8")
        self.record = {
            "schema_version": 1, "kind": "native-windows-client-signoff", "app_version": VERSION,
            "commit": COMMIT, "candidate_run": RUN, "reviewed_by": "fixture-reviewer",
            "runs": [
                {"os": os, "build": build, "arch": "x64", "product_type": 1,
                 "package": a["name"], "sha256": a["sha256"], "webview2": "144.0.3719.92",
                 "tester": "fixture-tester", "tested_at_utc": "2026-01-06T12:00:00Z",
                 "evidence_url": "https://example.test/sanitized-fixture-log",
                 "checks": dict.fromkeys(POLICY["REQUIRED_CHECKS"], "pass")}
                for os, build in [("windows-10", "19045.6000"), ("windows-11", "26100.5000")]
                for a in artifacts
            ],
        }

    def command(self, signoff=True):
        args = [sys.executable, "-B", str(SCRIPT), str(self.dist), "--commit", COMMIT, "--run-id", RUN]
        if signoff:
            args += ["--client-evidence", str(self.evidence), "--client-matrix-out", str(self.matrix)]
        return subprocess.run(args, text=True, capture_output=True, check=False)

    def save(self):
        self.evidence.write_text(json.dumps(self.record), encoding="utf-8")

    def refusal(self, message):
        result = self.command()
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(message, result.stderr)
        self.assertEqual(result.stdout, "")
        self.assertFalse(self.matrix.exists())

    def test_candidate_build_can_still_validate_static_bytes_without_signoff(self):
        result = self.command(signoff=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), VERSION)
        self.assertFalse(self.matrix.exists())

    def test_complete_exact_candidate_produces_six_public_matrix_rows(self):
        self.save()
        result = self.command()
        self.assertEqual(result.returncode, 0, result.stderr)
        matrix = self.matrix.read_text()
        self.assertEqual(matrix.count("9/9 passed"), 6)
        self.assertEqual(matrix.count("Windows 10 x64"), 3)
        self.assertEqual(matrix.count("Windows 11 x64"), 3)
        self.assertNotIn("example.test", matrix)
        self.assertNotEqual(self.command().returncode, 0, "refuse to overwrite the matrix")

    def test_promotion_rejects_missing_and_launch_only_evidence(self):
        self.refusal("missing reviewed Windows 10/11 sign-off")
        self.record["kind"] = "observed-launch-and-exit-only"
        self.save()
        self.refusal("not static or launch-only evidence")

    def test_rejects_cross_candidate_evidence_and_changed_bytes(self):
        self.record["candidate_run"] = "123457"
        self.save()
        self.refusal("different version, source commit, or candidate run")
        self.record["candidate_run"] = RUN
        self.record["runs"][0]["sha256"] = "f" * 64
        self.save()
        self.refusal("client package SHA-256 does not match")
        (self.dist / self.record["runs"][0]["package"]).write_bytes(b"changed bytes")
        self.refusal("SHA-256 mismatch")

    def test_every_os_format_pair_required_exactly_once(self):
        last = self.record["runs"].pop()
        self.save()
        self.refusal("all six OS/package combinations")
        self.record["runs"].append(self.record["runs"][0])
        self.save()
        self.refusal("duplicate OS/package")
        self.record["runs"][-1] = last

    def test_server_wrong_architecture_and_wrong_build_do_not_count(self):
        row = self.record["runs"][0]
        row["product_type"] = 3
        self.save()
        self.refusal("not a Server run")
        row["product_type"] = 1
        row["arch"] = "arm64"
        self.save()
        self.refusal("expected an x64 Windows workstation")
        row["arch"] = "x64"
        row["build"] = "26100.5000"
        self.save()
        self.refusal("build does not match")

    def test_fail_blocked_skipped_and_missing_checks_stop_promotion(self):
        checks = self.record["runs"][0]["checks"]
        for status in ["fail", "blocked", "not_run", "skipped", True]:
            with self.subTest(status=status):
                checks["accessibility_and_dpi"] = status
                self.save()
                self.refusal("client check accessibility_and_dpi must pass")
        del checks["accessibility_and_dpi"]
        self.save()
        self.refusal("invalid fields in client checks")

    def test_rejects_duplicate_json_fields_and_token_links(self):
        self.record["runs"][0]["evidence_url"] = "https://example.test/log?token=value"
        self.save()
        self.refusal("redacted HTTPS evidence link")
        self.evidence.write_text('{"schema_version":1,"schema_version":1}', encoding="utf-8")
        self.refusal("duplicate field")
