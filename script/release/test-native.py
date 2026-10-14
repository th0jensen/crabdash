#!/usr/bin/env python3
"""Run standalone build-script tests omitted by cargo test --workspace --lib."""
from pathlib import Path
import os
import subprocess
import tempfile

root = Path(__file__).resolve().parents[2]
with tempfile.TemporaryDirectory(prefix="crabdash-build-tests-") as temporary:
    for source in ("vendor/libghostty-vt-sys/patch.rs", "crates/crabdash/windows_resource.rs"):
        binary = Path(temporary) / (Path(source).stem + (".exe" if os.name == "nt" else ""))
        subprocess.run(["rustc", "--edition=2024", "--test", str(root / source), "-o", str(binary)], check=True)
        subprocess.run([str(binary), "--test-threads=1"], check=True, cwd=root)
