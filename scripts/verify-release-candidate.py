#!/usr/bin/env python3
"""Verify a downloaded candidate before promotion. Does not tag or publish anything."""

import argparse
from datetime import datetime
import hashlib
import json
from pathlib import Path
import re
import sys
import tomllib
from urllib.parse import urlsplit


PACKAGE_FORMATS = {"-Setup.exe": "NSIS", ".msi": "MSI", "-Portable.zip": "Portable ZIP"}
CLIENTS = {"windows-10": "Windows 10 x64", "windows-11": "Windows 11 x64"}
REQUIRED_CHECKS = (
    "install_or_extract",
    "launch_and_exit",
    "upgrade_profile_and_rollback",
    "removal",
    "unelevated_profile_and_uac",
    "native_webview2_and_ipc",
    "watch_and_inspector",
    "cleanup_and_restore",
    "accessibility_and_dpi",
)
MAX_SIGN_OFF_BYTES = 64 * 1024


def packages(version: str) -> dict[str, str]:
    return {f"ExorSystem-v{version}-x64{suffix}": label for suffix, label in PACKAGE_FORMATS.items()}


def stable_version(root: Path) -> str:
    with (root / "Cargo.toml").open("rb") as f:
        version = tomllib.load(f)["workspace"]["package"]["version"]
    gui = root / "gui"
    with (gui / "src-tauri/Cargo.toml").open("rb") as f:
        desktop = tomllib.load(f)["package"]["version"]
    config = json.loads((gui / "src-tauri/tauri.conf.json").read_text(encoding="utf-8"))
    frontend = json.loads((gui / "package.json").read_text(encoding="utf-8"))
    if not re.fullmatch(r"(?:0|[1-9][0-9]*)(?:\.(?:0|[1-9][0-9]*)){2}", version):
        raise ValueError("expected a stable x.y.z version")
    if any(other != version for other in (desktop, config["version"], frontend["version"])):
        raise ValueError("workspace, desktop, Tauri and frontend versions must match")
    return version


def verify(directory: Path, version: str, commit: str, run_id: str) -> dict[str, str]:
    names = set(packages(version))
    expected = names | {"PACKAGE-CHECKS.json", "SHA256SUMS.txt"}
    files = {entry.name: entry for entry in directory.iterdir()}
    if files.keys() != expected or any(not p.is_file() or p.is_symlink() for p in files.values()):
        raise ValueError("expected only the three versioned packages and two evidence files")

    sums = {}
    for line in files["SHA256SUMS.txt"].read_text(encoding="ascii").splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  (.+)", line)
        if not match or match[2] not in names or match[2] in sums:
            raise ValueError("unexpected, duplicate, or invalid SHA-256 entry")
        sums[match[2]] = match[1]
    if sums.keys() != names:
        raise ValueError("SHA256SUMS.txt must cover exactly the three packages")

    meta = json.loads(files["PACKAGE-CHECKS.json"].read_text(encoding="utf-8-sig"))
    if (
        meta["kind"] != "static-package-checks-only"
        or meta["commit"] != commit
        or meta["workflow_run"] != run_id
        or meta["app_version"] != version
    ):
        raise ValueError("package evidence does not match the requested run, version and commit")
    rows = meta["artifacts"]
    recorded = {row["name"]: row for row in rows}
    if len(rows) != len(names) or recorded.keys() != names:
        raise ValueError("package evidence must cover exactly the three packages")
    for name in sorted(names):
        with files[name].open("rb") as f:
            actual = hashlib.file_digest(f, "sha256").hexdigest()
        if actual != sums[name] or actual != recorded[name]["sha256"]:
            raise ValueError(f"SHA-256 mismatch for {name}")
        if files[name].stat().st_size != recorded[name]["bytes"]:
            raise ValueError(f"size mismatch for {name}")
    return sums


def unique_object(pairs: list[tuple]) -> dict:
    obj = dict(pairs)
    if len(obj) != len(pairs):
        raise ValueError("duplicate field in client sign-off JSON")
    return obj


def fields(obj: object, keys: set[str], context: str) -> dict:
    if not isinstance(obj, dict) or obj.keys() != keys:
        raise ValueError(f"invalid fields in {context}")
    return obj


def plain_https_url(value: object) -> bool:
    if not isinstance(value, str) or not re.fullmatch(r"https://[^\s<>\\]+", value) or len(value) > 512:
        return False
    url = urlsplit(value)
    # Evidence links must not contain credentials or temporary access / query tokens.
    return bool(url.hostname and url.path and not url.query and url.username is None and url.password is None)


def verify_clients(path: Path, version: str, commit: str, run_id: str, sums: dict[str, str]) -> str:
    """Validate a reviewed client attestation, not the runner's actual OS or actions."""
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"missing reviewed Windows 10/11 sign-off: {path}")
    with path.open("rb") as f:
        raw = f.read(MAX_SIGN_OFF_BYTES + 1)
    if len(raw) > MAX_SIGN_OFF_BYTES:
        raise ValueError("client sign-off exceeds the size limit")
    record = fields(json.loads(raw.decode("utf-8-sig"), object_pairs_hook=unique_object), {
        "schema_version", "kind", "app_version", "commit", "candidate_run", "reviewed_by", "runs",
    }, "client sign-off")
    if type(record["schema_version"]) is not int or record["schema_version"] != 1 or record["kind"] != "native-windows-client-signoff":
        raise ValueError("expected a version 1 native Windows client sign-off, not static or launch-only evidence")
    if (record["app_version"], record["commit"], record["candidate_run"]) != (version, commit, run_id):
        raise ValueError("client sign-off is for a different version, source commit, or candidate run")
    handle = r"[A-Za-z0-9](?:[A-Za-z0-9-]{0,37}[A-Za-z0-9])?"
    if not isinstance(record["reviewed_by"], str) or not re.fullmatch(handle, record["reviewed_by"]):
        raise ValueError("client sign-off must name its reviewer")

    formats = packages(version)
    expected = {(os, package) for os in CLIENTS for package in formats}
    runs = record["runs"]
    if not isinstance(runs, list) or len(runs) != len(expected):
        raise ValueError("client sign-off must cover all six OS/package combinations")
    verified = {}
    for row in runs:
        row = fields(row, {"os", "build", "arch", "product_type", "package", "sha256", "webview2", "tester",
                           "tested_at_utc", "evidence_url", "checks"}, "client run")
        os, package = row["os"], row["package"]
        key = (os, package)
        if key not in expected or key in verified:
            raise ValueError("unexpected or duplicate OS/package combination in client sign-off")
        label = f"{CLIENTS[os]} / {formats[package]}"
        if row["sha256"] != sums[package]:
            raise ValueError(f"client package SHA-256 does not match the candidate: {label}")
        if row["arch"] != "x64" or type(row["product_type"]) is not int or row["product_type"] != 1:
            raise ValueError(f"expected an x64 Windows workstation, not a Server run: {label}")
        build = row["build"]
        if not isinstance(build, str) or not re.fullmatch(r"[1-9][0-9]{4,5}\.[0-9]{1,6}", build):
            raise ValueError(f"missing or invalid client build and revision: {label}")
        major = int(build.split(".", 1)[0])
        if (os == "windows-10" and not 10240 <= major <= 19045) or (os == "windows-11" and major < 22000):
            raise ValueError(f"build does not match the claimed client OS: {label}")
        if not isinstance(row["webview2"], str) or not re.fullmatch(r"[1-9][0-9]{0,5}(?:\.[0-9]{1,6}){3}", row["webview2"]):
            raise ValueError(f"missing or invalid observed WebView2 version: {label}")
        if not isinstance(row["tester"], str) or not re.fullmatch(handle, row["tester"]) or not plain_https_url(row["evidence_url"]):
            raise ValueError(f"client run needs a tester and a redacted HTTPS evidence link: {label}")
        when = row["tested_at_utc"]
        if not isinstance(when, str) or not re.fullmatch(r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z", when):
            raise ValueError(f"expected a UTC test timestamp: {label}")
        datetime.fromisoformat(when)
        checks = fields(row["checks"], set(REQUIRED_CHECKS), f"client checks for {label}")
        for check in REQUIRED_CHECKS:
            if checks[check] != "pass":
                raise ValueError(f"client check {check} must pass: {label}")
        verified[key] = row

    lines = ["### Native Windows 10/11 client sign-off", "",
             "Reviewed operator attestations for these exact packages. CI and package inspection are separate checks.", "",
             "| Client | Build | Package | WebView2 | Required checks |",
             "|---|---|---|---|---|"]
    for os, label in CLIENTS.items():
        for package, format_name in formats.items():
            row = verified[(os, package)]
            lines.append(f"| {label} | {row['build']} | {format_name} | {row['webview2']} | {len(REQUIRED_CHECKS)}/{len(REQUIRED_CHECKS)} passed |")
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--source-root", type=Path, default=Path(__file__).resolve().parents[1],
                        help="checkout of the candidate source; defaults to this script's repository")
    parser.add_argument("--client-evidence", type=Path, help="reviewed native sign-off (required for promotion)")
    parser.add_argument("--client-matrix-out", type=Path, help="create a new Markdown summary after client validation")
    args = parser.parse_args()
    if args.client_matrix_out is not None and args.client_evidence is None:
        parser.error("--client-matrix-out requires --client-evidence")
    try:
        if not re.fullmatch(r"[0-9a-f]{40}", args.commit) or not re.fullmatch(r"[1-9][0-9]*", args.run_id):
            raise ValueError("expected a full commit SHA and numeric Actions run ID")
        version = stable_version(args.source_root)
        sums = verify(args.directory, version, args.commit, args.run_id)
        if args.client_evidence is not None:
            matrix = verify_clients(args.client_evidence, version, args.commit, args.run_id, sums)
            if args.client_matrix_out is not None:
                with args.client_matrix_out.open("x", encoding="utf-8", newline="\n") as f:
                    f.write(matrix)
    except (OSError, ValueError, KeyError, TypeError, RecursionError) as e:
        print(f"Candidate verification failed: {e}", file=sys.stderr)
        return 1
    print(version)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
