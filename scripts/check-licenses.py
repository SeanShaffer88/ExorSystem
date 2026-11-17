#!/usr/bin/env python3
"""Fail if any dependency is not under a free/open-source license.

ExorSystem must stay free and open source. Checks Rust crates (both workspaces) and the
npm lockfile. Allowed: permissive licenses plus MPL-2.0 (file-level copyleft, compatible).
Run from the repo root: python3 scripts/check-licenses.py
"""
import json
import re
import subprocess
import sys

ALLOWED = {
    "MIT", "MIT-0", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "BSD-2-Clause", "BSD-3-Clause",
    "ISC", "Zlib", "Unlicense", "CC0-1.0", "0BSD", "Unicode-3.0", "Unicode-DFS-2016", "MPL-2.0",
    "BlueOak-1.0.0", "Python-2.0",
}


def ok(expr: str) -> bool:
    """An SPDX expression passes if every AND-part has at least one allowed OR-choice."""
    expr = expr.replace("/", " OR ").replace("(", " ").replace(")", " ")
    for part in re.split(r"\s+AND\s+", expr):
        choices = [c.strip() for c in re.split(r"\s+OR\s+", part) if c.strip()]
        # keep "X WITH Y" together
        if not any(c in ALLOWED for c in choices):
            return False
    return True


bad = []
total = 0
for manifest in ("Cargo.toml", "gui/src-tauri/Cargo.toml"):
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--manifest-path", manifest],
        check=True, capture_output=True, text=True,
    ).stdout
    for p in json.loads(out)["packages"]:
        if p["source"] is None:
            continue  # our own crates
        total += 1
        lic = p.get("license") or "NONE"
        if not ok(lic):
            bad.append((f"cargo:{p['name']}@{p['version']}", lic))

lock = json.load(open("gui/package-lock.json"))
for path, meta in lock.get("packages", {}).items():
    if not path:
        continue
    total += 1
    lic = meta.get("license") or "NONE"
    if not isinstance(lic, str) or not ok(lic):
        bad.append((f"npm:{path}", str(lic)))

for name, lic in bad:
    print(f"NOT ALLOWED: {name}: {lic}")
if bad:
    sys.exit(1)
print(f"{total} dependencies, all free/open-source licenses")
