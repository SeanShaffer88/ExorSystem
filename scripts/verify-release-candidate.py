#!/usr/bin/env python3
"""Verify a downloaded candidate before promotion. Does not tag or publish anything."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import sys
import tomllib


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


def verify(directory: Path, version: str, commit: str, run_id: str) -> None:
    prefix = f"ExorSystem-v{version}-x64"
    names = {f"{prefix}-Setup.exe", f"{prefix}.msi", f"{prefix}-Portable.zip"}
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


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--run-id", required=True)
    args = parser.parse_args()
    try:
        if not re.fullmatch(r"[0-9a-f]{40}", args.commit) or not re.fullmatch(r"[1-9][0-9]*", args.run_id):
            raise ValueError("expected a full commit SHA and numeric Actions run ID")
        version = stable_version(Path(__file__).resolve().parents[1])
        verify(args.directory, version, args.commit, args.run_id)
    except (OSError, ValueError, KeyError, TypeError) as e:
        print(f"Candidate verification failed: {e}", file=sys.stderr)
        return 1
    print(version)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
