#!/usr/bin/env python3
"""Verify GitHub's uploaded bytes before making the release public."""
import hashlib
import json
import sys
from pathlib import Path

directory, metadata = Path(sys.argv[1]), Path(sys.argv[2])
release = json.loads(metadata.read_text())
remote = {asset["name"]: asset for asset in release["assets"]}
local = {path.name: path for path in directory.iterdir() if path.is_file()}
if set(remote) != set(local):
    raise SystemExit(f"Uploaded asset set differs: missing={set(local)-set(remote)}, extra={set(remote)-set(local)}")
for name, path in local.items():
    asset = remote[name]
    expected = "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()
    if asset["state"] != "uploaded" or asset["size"] != path.stat().st_size or asset.get("digest") != expected:
        raise SystemExit(f"GitHub asset does not match local bytes: {name}")
print(f"Verified {len(local)} uploaded release assets")
