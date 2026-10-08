#!/usr/bin/env python3
"""Fail publication unless every supported executable was packaged."""
import hashlib
import json
import sys
from pathlib import Path

directory, tag = Path(sys.argv[1]), sys.argv[2]
expected = {
    "crabdash-macOS-applesilicon.dmg": "aarch64-apple-darwin",
    "crabdash-macOS-intel.dmg": "x86_64-apple-darwin",
    "crabdash-Linux-x86_64.tar.gz": "x86_64-unknown-linux-gnu",
    "crabdash-Linux-aarch64.tar.gz": "aarch64-unknown-linux-gnu",
    "crabdash-Windows-x86_64.zip": "x86_64-pc-windows-msvc",
}
for name, target in expected.items():
    asset = directory / name
    receipt = directory / f"{target}.json"
    if not asset.is_file() or asset.stat().st_size < 100_000 or not receipt.is_file():
        raise SystemExit(f"Missing or invalid release asset: {name}")
    audit = json.loads(receipt.read_text())
    if audit["version"] != tag.removeprefix("v") or audit["target"] != target:
        raise SystemExit(f"Wrong release version/target: {receipt.name}")
    if audit["asset_sha256"] != hashlib.sha256(asset.read_bytes()).hexdigest():
        raise SystemExit(f"Release asset hash differs from build receipt: {name}")
files = sorted(path for path in directory.iterdir() if path.is_file() and path.name != "SHA256SUMS")
(directory / "SHA256SUMS").write_text("".join(
    f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n" for path in files
))
print(f"Verified {len(expected)} platform packages and generated SHA256SUMS")
