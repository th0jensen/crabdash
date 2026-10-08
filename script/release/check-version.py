#!/usr/bin/env python3
"""Keep Cargo metadata, lockfile and release tags in agreement."""
import re
import sys
import tomllib
from pathlib import Path

root = Path(__file__).resolve().parents[2]
version = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"]["version"]
lock = tomllib.loads((root / "Cargo.lock").read_text())
for package in lock["package"]:
    if package["name"] in {"app", "crabdash", "machines", "services", "utils"}:
        if package["version"] != version:
            raise SystemExit(f"{package['name']} lockfile version differs from {version}")
if len(sys.argv) > 1 and sys.argv[1].startswith("v"):
    tag = sys.argv[1]
    if not re.fullmatch(r"v\d+\.\d+\.\d+(?:-[A-Za-z0-9.-]+)?", tag) or tag != f"v{version}":
        raise SystemExit(f"Release tag {tag!r} differs from Cargo version {version}")
print(version)
