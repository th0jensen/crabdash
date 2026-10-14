#!/usr/bin/env python3
"""Select an installed Xcode with the macOS 26+ SDK required by native Glass."""
from pathlib import Path
import re
import subprocess
import sys


def is_full_xcode(developer):
    developer = Path(developer).resolve()
    return developer.parent.parent.suffix == ".app" and (developer / "usr/bin/xcodebuild").is_file()


def select(candidates, sdk_version, supports_xcodebuild=is_full_xcode):
    supported = []
    for developer in candidates:
        if not supports_xcodebuild(developer):
            continue
        try:
            value = sdk_version(developer).strip()
            if re.fullmatch(r"\d+(?:\.\d+)*", value):
                version = tuple(map(int, value.split(".")))
                if version >= (26,):
                    supported.append((version, str(developer)))
        except (OSError, subprocess.SubprocessError):
            continue
    if not supported:
        raise RuntimeError("An installed Xcode with macOS SDK 26 or newer is required")
    return max(supported)[1]


def main():
    candidates = [path / "Contents/Developer" for path in Path("/Applications").glob("Xcode*.app")]
    active = subprocess.run(["xcode-select", "-p"], capture_output=True, text=True, check=True).stdout.strip()
    candidates.append(Path(active))

    def sdk_version(developer):
        import os
        return subprocess.run(
            ["xcrun", "--sdk", "macosx", "--show-sdk-version"],
            env={**os.environ, "DEVELOPER_DIR": str(developer)},
            capture_output=True, text=True, check=True, timeout=30,
        ).stdout

    print(select(candidates, sdk_version))


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError, subprocess.SubprocessError) as error:
        print(error, file=sys.stderr)
        sys.exit(1)
