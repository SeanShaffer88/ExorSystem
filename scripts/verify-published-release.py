#!/usr/bin/env python3
"""Check a downloaded stable ExorSystem release against trusted GitHub release metadata.

This static check never installs, launches, rebuilds, or publishes an artifact. Get the release
JSON directly from the official repository, not from the same untrusted mirror as the packages.
"""

import argparse
from datetime import datetime, timezone
import hashlib
import io
import json
from pathlib import Path
import re
import runpy
import stat
import struct
import sys
import zipfile
import zlib


REPO = "SeanShaffer88/ExorSystem"
REPO_URL = f"https://github.com/{REPO}"
TAG = re.compile(r"v(?:0|[1-9][0-9]*)(?:\.(?:0|[1-9][0-9]*)){2}")
FORMATS = {"nsis": "-Setup.exe", "msi": ".msi", "portable": "-Portable.zip"}
EVIDENCE_FILES = ("PACKAGE-CHECKS.json", "SHA256SUMS.txt")
PORTABLE_ENTRIES = {"ExorSystem.exe", "README.md", "LICENSE", "CHANGELOG.md", "CREDITS.md"}
CLI_ENTRY = "exor.exe"
# Releases after v0.3.0 also ship the read-only command line in the portable ZIP.
LAST_WITHOUT_CLI = (0, 3, 0)
MAX_METADATA = 2 * 1024 * 1024
MAX_PACKAGE = 256 * 1024 * 1024
MAX_PORTABLE_BYTES = 128 * 1024 * 1024
NOT_TESTED = [
    "Installer installation, upgrade, removal, shortcuts and UAC",
    "Native desktop launch, profile creation and shutdown",
    "WebView2 rendering and native IPC",
    "Interactive Watch, History, cleanup, restore, and recovery",
    "Accessibility and mixed-DPI behavior",
]


def file_info(path: Path) -> int:
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode) or getattr(info, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT:
        raise ValueError("expected a regular, non-redirected file")
    return info.st_size


def read_json(path: Path) -> dict:
    if file_info(path) > MAX_METADATA:
        raise ValueError("metadata exceeds the read limit")
    with path.open("rb") as source:
        data = source.read(MAX_METADATA + 1)
    if len(data) > MAX_METADATA:
        raise ValueError("metadata exceeds the read limit")
    result = json.loads(data.decode("utf-8-sig"))
    if not isinstance(result, dict):
        raise ValueError("metadata must be a JSON object")
    return result


def release_identity(metadata: dict, tag: str) -> dict:
    if not TAG.fullmatch(tag):
        raise ValueError("use an exact stable release tag, such as v0.3.0")
    url = f"{REPO_URL}/releases/tag/{tag}"
    if (
        metadata.get("tag_name") != tag
        or metadata.get("html_url") != url
        or metadata.get("draft") is not False
        or metadata.get("prerelease") is not False
    ):
        raise ValueError("metadata does not describe the requested published stable release")
    commit = metadata.get("target_commitish")
    if not isinstance(commit, str) or not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("the published metadata must identify an exact source commit")
    body = metadata.get("body")
    if not isinstance(body, str) or f"- Source commit: `{commit}`" not in body.splitlines():
        raise ValueError("the release notes do not identify the same source commit")
    candidate = re.compile(rf"- \[Windows candidate package run\]\({re.escape(REPO_URL)}/actions/runs/([1-9][0-9]*)\)")
    matches = [match[1] for line in body.splitlines() if (match := candidate.fullmatch(line))]
    if len(matches) != 1:
        raise ValueError("expected one Windows candidate run link in the release notes")
    return {"tag": tag, "commit": commit, "candidate_run_id": matches[0], "url": url}


def published_assets(metadata: dict, expected: set[str]) -> dict[str, dict]:
    assets = metadata.get("assets")
    if not isinstance(assets, list) or len(assets) != len(expected):
        raise ValueError("the release must attach three packages and both evidence files")
    selected = {}
    for row in assets:
        if not isinstance(row, dict):
            raise ValueError("invalid asset metadata")
        name = row.get("name")
        if not isinstance(name, str) or name not in expected or name in selected:
            raise ValueError("unexpected or duplicate published asset name")
        size, digest = row.get("size"), row.get("digest")
        limit = MAX_METADATA if name in EVIDENCE_FILES else MAX_PACKAGE
        if type(size) is not int or not 0 < size <= limit or row.get("state") != "uploaded":
            raise ValueError(f"unexpected published size or state for {name}")
        if not isinstance(digest, str) or not re.fullmatch(r"sha256:[0-9a-f]{64}", digest):
            raise ValueError(f"published SHA-256 is missing or invalid for {name}")
        selected[name] = {"name": name, "bytes": size, "sha256": digest[7:]}
    return selected


def verify_asset(path: Path, expected: dict) -> None:
    remaining = expected["bytes"]
    if file_info(path) != remaining:
        raise ValueError(f"published size mismatch for {path.name}")
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while remaining:
            block = source.read(min(remaining, 1024 * 1024))
            if not block:
                raise ValueError(f"asset ended early: {path.name}")
            remaining -= len(block)
            digest.update(block)
        if source.read(1):
            raise ValueError(f"asset grew while checking {path.name}")
    if digest.hexdigest() != expected["sha256"]:
        raise ValueError(f"published SHA-256 mismatch for {path.name}")


def pe_machine(source, length: int) -> int:
    dos = source.read(64)
    if len(dos) != 64 or dos[:2] != b"MZ":
        raise ValueError("missing PE DOS header")
    offset = struct.unpack_from("<I", dos, 0x3C)[0]
    if not 64 <= offset <= length - 6:
        raise ValueError("invalid PE header offset")
    source.seek(offset)
    header = source.read(6)
    if len(header) != 6 or header[:4] != b"PE\0\0":
        raise ValueError("missing PE signature")
    return struct.unpack_from("<H", header, 4)[0]


def portable_entries(tag: str) -> set:
    """The exact portable ZIP contents for a release tag."""
    version = tuple(int(part) for part in tag[1:].split("."))
    return PORTABLE_ENTRIES | ({CLI_ENTRY} if version > LAST_WITHOUT_CLI else set())


def x64_entry(archive: zipfile.ZipFile, name: str) -> bytes:
    entry = archive.getinfo(name)
    with archive.open(entry) as source:
        binary = source.read(entry.file_size + 1)
    if len(binary) != entry.file_size or pe_machine(io.BytesIO(binary), len(binary)) != 0x8664:
        raise ValueError(f"the portable {name} is not the expected x64 PE")
    return binary


def inspect_portable(path: Path, expected: set = PORTABLE_ENTRIES) -> dict:
    with zipfile.ZipFile(path) as archive:
        entries = archive.infolist()
        if len(entries) != len(expected) or {i.filename for i in entries} != expected:
            raise ValueError("unexpected or duplicate portable ZIP entries")
        if any(
            i.is_dir() or i.flag_bits & 1 or stat.S_IFMT(i.external_attr >> 16) not in (0, stat.S_IFREG)
            for i in entries
        ):
            raise ValueError("portable ZIP entries must be regular, unencrypted files")
        if sum(i.file_size for i in entries) > MAX_PORTABLE_BYTES:
            raise ValueError("portable ZIP exceeds the uncompressed byte limit")
        binary = x64_entry(archive, "ExorSystem.exe")
        cli = x64_entry(archive, CLI_ENTRY) if CLI_ENTRY in expected else None
    report = {"name": "ExorSystem.exe", "bytes": len(binary), "sha256": hashlib.sha256(binary).hexdigest()}
    if cli is not None:
        report["command_line"] = {"name": CLI_ENTRY, "bytes": len(cli), "sha256": hashlib.sha256(cli).hexdigest()}
    return report


def verify_published(directory: Path, release_json: Path, tag: str) -> dict:
    metadata = read_json(release_json)
    release = release_identity(metadata, tag)
    names = {fmt: f"ExorSystem-{tag}-x64{suffix}" for fmt, suffix in FORMATS.items()}
    assets = published_assets(metadata, set(names.values()) | set(EVIDENCE_FILES))
    info = directory.lstat()
    if not stat.S_ISDIR(info.st_mode) or getattr(info, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT:
        raise ValueError("the package directory must be a plain directory")
    found = set()
    for path in directory.iterdir():
        if path.name not in assets:
            raise ValueError("the package directory must contain only the five release assets")
        verify_asset(path, assets[path.name])
        found.add(path.name)
    if found != assets.keys():
        raise ValueError("one or more release assets are missing")
    # Keep the promotion verifier as the source of truth for the candidate report and checksums.
    verifier = runpy.run_path(str(Path(__file__).with_name("verify-release-candidate.py")))
    verifier["verify"](directory, tag[1:], release["commit"], release["candidate_run_id"])
    with (directory / names["nsis"]).open("rb") as source:
        machine = pe_machine(source, assets[names["nsis"]]["bytes"])
        if machine not in (0x014C, 0x8664):
            raise ValueError("expected an x86 or x64 NSIS PE launcher")
    with (directory / names["msi"]).open("rb") as source:
        if source.read(8) != bytes.fromhex("d0cf11e0a1b11ae1"):
            raise ValueError("MSI is missing its compound-file header")
    return {
        "release": release,
        "packages": {fmt: assets[name] for fmt, name in names.items()},
        "evidence_files": [assets[name] for name in EVIDENCE_FILES],
        "static_findings": {
            "candidate_report_and_checksums_match": True,
            "nsis_launcher_pe_machine": f"0x{machine:04x}",
            "msi_header": "compound-file",
            "portable_entries": sorted(portable_entries(tag)),
            "portable_pe_machine": "0x8664",
        },
        "portable_binary": inspect_portable(directory / names["portable"], portable_entries(tag)),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path, help="directory containing only the five release assets")
    parser.add_argument("--release-info", type=Path, required=True, help="trusted GitHub API release JSON")
    parser.add_argument("--tag", required=True)
    parser.add_argument("--output", type=Path, required=True, help="new JSON evidence file outside the package directory")
    args = parser.parse_args()
    report = {
        "schema": 1,
        "kind": "published-windows-package-checks-only",
        "checked_at_utc": datetime.now(timezone.utc).isoformat(),
        "requested_tag": args.tag,
        "not_tested": NOT_TESTED,
    }
    try:
        if args.output.resolve().is_relative_to(args.directory.resolve()):
            raise ValueError("write evidence outside the package directory")
        with args.output.open("x", encoding="utf-8") as output:
            try:
                report.update(verify_published(args.directory, args.release_info, args.tag), status="Pass")
            except (ValueError, KeyError, TypeError, RuntimeError, OSError, EOFError, zipfile.BadZipFile, zlib.error) as exc:
                # I/O exceptions can include local account paths. Keep those out of shareable evidence.
                detail = f"could not read an input file ({type(exc).__name__})" if isinstance(exc, OSError) else str(exc)
                report.update(status="Fail", error=detail)
            json.dump(report, output, indent=2)
            output.write("\n")
    except (OSError, ValueError) as exc:
        print(f"Evidence was not written: {exc}", file=sys.stderr)
        return 2
    if report["status"] != "Pass":
        print(f"Published package verification failed: {report['error']}", file=sys.stderr)
        return 1
    print(f"Verified the five {args.tag} release assets. No installer or desktop was run.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
