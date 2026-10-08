#!/usr/bin/env python3
"""Audit and package an already-built native Crabdash release. Standard library only."""
import argparse
import hashlib
import json
from pathlib import Path
import plistlib
import re
import shutil
import struct
import subprocess
import sys
import tarfile
import tempfile
import zipfile


ROOT = Path(__file__).resolve().parents[2]
TARGETS = {
    "aarch64-apple-darwin": ("macos", "arm64"),
    "x86_64-apple-darwin": ("macos", "x86_64"),
    "aarch64-unknown-linux-gnu": ("linux", "aarch64"),
    "x86_64-unknown-linux-gnu": ("linux", "x86_64"),
    "x86_64-pc-windows-msvc": ("windows", "AMD64"),
}
ASSETS = {
    "aarch64-apple-darwin": "crabdash-macOS-applesilicon.dmg",
    "x86_64-apple-darwin": "crabdash-macOS-intel.dmg",
    "aarch64-unknown-linux-gnu": "crabdash-Linux-aarch64.tar.gz",
    "x86_64-unknown-linux-gnu": "crabdash-Linux-x86_64.tar.gz",
    "x86_64-pc-windows-msvc": "crabdash-Windows-x86_64.zip",
}
NOTICES = {
    "crates/app/assets/fonts/ibm-plex-sans/OFL.txt": "IBM-Plex-Sans-OFL.txt",
    "crates/app/assets/fonts/ibm-plex-sans/README.md": "IBM-Plex-Sans-README.md",
    "vendor/ghostty/vendor/nerd-fonts/LICENSE": "Nerd-Fonts-LICENSE.txt",
    "vendor/ghostty/src/font/res/OFL.txt": "Ghostty-Font-OFL.txt",
    "vendor/ghostty/LICENSE": "Ghostty-LICENSE.txt",
    "vendor/gpui/LICENSE-APACHE": "GPUI-LICENSE-APACHE.txt",
    "vendor/ksni/UNLICENSE": "KSNI-UNLICENSE.txt",
    "vendor/portable-pty/LICENSE.md": "Portable-PTY-LICENSE.txt",
    "vendor/libghostty-vt/Cargo.toml": "Libghostty-VT-Package.txt",
    "crates/app/assets/machine-logos/LICENSE": "Machine-Logos-LICENSE.txt",
    "crates/app/assets/machine-logos/README.md": "Machine-Logos-README.md",
    "crates/app/assets/brands/README.md": "Docker-Logo-Notice.txt",
}
WINDOWS_SYSTEM_DLLS = set("""
advapi32 bcrypt cfgmgr32 comctl32 comdlg32 crypt32 cryptnet d2d1 d3d11 dcomp
dwmapi dwrite dxgi gdi32 gdiplus icuuc imm32 iphlpapi kernel32 kernelbase mpr msimg32 msvcrt
ncrypt netapi32 normaliz ntdll ole32 oleaut32 pdh powrprof propsys psapi rpcrt4
secur32 setupapi shell32 shcore shlwapi user32 userenv usp10 uxtheme version
winhttp wininet winmm winspool ws2_32 wtsapi32 ucrtbase
""".split())


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def run(*arguments):
    try:
        result = subprocess.run(arguments, check=True, text=True, capture_output=True, timeout=180)
    except subprocess.CalledProcessError as error:
        raise RuntimeError(f"{arguments[0]} failed: {error.stderr.strip() or error.stdout.strip()}") from error
    return result.stdout.strip()


def digest(path):
    with path.open("rb") as source:
        checksum = hashlib.sha256()
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            checksum.update(chunk)
        return checksum.hexdigest()


def copy(source, destination):
    require(source.is_file(), f"Required input missing: {source}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, destination)


def notices(destination):
    for source, name in NOTICES.items():
        copy(ROOT / source, destination / "licenses" / name)
    for name in ("LICENSE", "THIRD-PARTY-NOTICES.txt", "NATIVE-THIRD-PARTY-NOTICES.txt"):
        copy(ROOT / name, destination / name)
    for name in ("THIRD-PARTY-NOTICES.md", "THIRD-PARTY-NOTICES"):
        source = ROOT / name
        if source.exists():
            copy(source, destination / name)


def audit_macos(binary, architecture):
    require(sys.platform == "darwin", "macOS packaging requires a native macOS host")
    description = run("file", "-b", str(binary))
    require("Mach-O" in description, f"Expected Mach-O executable: {description}")
    require(run("lipo", "-archs", str(binary)).split() == [architecture], "Mach-O architecture differs from target")
    dependencies = run("otool", "-L", str(binary)).splitlines()[1:]
    for line in dependencies:
        library = line.strip().split(" (", 1)[0]
        require(library.startswith(("/usr/lib/", "/System/Library/")), f"Non-system macOS dylib dependency: {library}")
    minimums = []
    for command in run("otool", "-l", str(binary)).split("Load command ")[1:]:
        if re.search(r"\bcmd LC_BUILD_VERSION\b", command):
            match = re.search(r"\bminos\s+([\d.]+)", command)
        elif re.search(r"\bcmd LC_VERSION_MIN_MACOSX\b", command):
            match = re.search(r"\bversion\s+([\d.]+)", command)
        else:
            continue
        require(match is not None, "Mach-O deployment version missing")
        version = tuple(map(int, match.group(1).split(".")))
        require(version + (0,) * (3 - len(version)) <= (13, 0, 0), f"Mach-O requires macOS newer than 13.0: {match.group(1)}")
        minimums.append(match.group(1))
    require(minimums, "Mach-O macOS deployment version not found")
    return {"file": description, "dependencies": dependencies, "minimum_macos": minimums}


def audit_linux(binary, architecture):
    require(sys.platform.startswith("linux"), "Linux packaging requires a native Linux host")
    with binary.open("rb") as source:
        header = source.read(20)
    require(len(header) == 20 and header[:6] == b"\x7fELF\x02\x01", "Expected little-endian ELF64 executable")
    machine = struct.unpack_from("<H", header, 18)[0]
    require(machine == {"x86_64": 62, "aarch64": 183}[architecture], "ELF architecture differs from target")
    description = run("file", "-b", str(binary))
    versions = run("readelf", "--version-info", str(binary))
    glibc = {tuple(map(int, value.split("."))) for value in re.findall(r"GLIBC_(\d+\.\d+(?:\.\d+)?)", versions)}
    require(glibc, "No glibc version requirements found; expected a GNU/Linux executable")
    require(max(glibc) <= (2, 35), f"glibc requirement exceeds Ubuntu 22.04 baseline: {max(glibc)}")
    dependencies = run("ldd", str(binary))
    require("not found" not in dependencies and "not a dynamic executable" not in dependencies,
            f"Unresolved Linux runtime dependencies: {dependencies}")
    require(not re.search(r"\blib(?:ssl|crypto)\.so", dependencies), "OpenSSL must be linked statically for Linux release packages")
    return {"file": description, "glibc_max": ".".join(map(str, max(glibc))), "dependencies": dependencies.splitlines()}


def audit_windows(binary):
    data = binary.read_bytes()
    require(len(data) >= 64 and data[:2] == b"MZ", "Expected Windows PE executable")
    offset = struct.unpack_from("<I", data, 60)[0]
    require(offset + 24 <= len(data) and data[offset:offset + 4] == b"PE\0\0", "Invalid PE header")
    machine, sections = struct.unpack_from("<HH", data, offset + 4)
    optional_size = struct.unpack_from("<H", data, offset + 20)[0]
    optional = offset + 24
    require(machine == 0x8664, "PE architecture differs from AMD64 target")
    require(optional_size >= 136 and optional + optional_size <= len(data), "Truncated PE optional header")
    require(struct.unpack_from("<H", data, optional)[0] == 0x20B, "Expected PE32+ executable")
    require(struct.unpack_from("<I", data, optional + 108)[0] >= 3, "PE resource directory missing")
    resource_rva, resource_size = struct.unpack_from("<II", data, optional + 128)
    require(resource_rva and resource_size, "Windows icon/version resources missing")
    table = optional + optional_size
    require(table + sections * 40 <= len(data), "Truncated PE section table")
    def raw_offset(address, length):
        for index in range(sections):
            section = table + index * 40
            virtual_size, rva, raw_size, raw = struct.unpack_from("<IIII", data, section + 8)
            if rva <= address < rva + virtual_size:
                start = raw + address - rva
                require(address - rva + length <= raw_size and start + length <= len(data), "PE directory is not backed by valid section data")
                return start
        raise RuntimeError("PE directory references an absent section")

    resources = raw_offset(resource_rva, resource_size)
    require(resource_size >= 16, "Truncated PE resource directory")
    named, identifiers = struct.unpack_from("<HH", data, resources + 12)
    require(16 + (named + identifiers) * 8 <= resource_size, "Truncated PE resource entries")
    resource_types = set()
    for index in range(named + identifiers):
        identifier = struct.unpack_from("<I", data, resources + 16 + index * 8)[0]
        if not identifier & 0x80000000:
            resource_types.add(identifier)
    require({3, 14, 24}.issubset(resource_types), "Windows icon, group-icon, or DPI manifest resources missing")

    imports = []
    import_rva, import_size = struct.unpack_from("<II", data, optional + 120)
    if import_rva:
        directory = raw_offset(import_rva, import_size)
        terminated = False
        for index in range(min(import_size // 20, 4096)):
            descriptor = struct.unpack_from("<IIIII", data, directory + index * 20)
            if not any(descriptor):
                terminated = True
                break
            start = raw_offset(descriptor[3], 1)
            end = data.find(b"\0", start, min(start + 256, len(data)))
            require(end >= start, "Invalid PE import name")
            name = data[start:end].decode("ascii").lower()
            require(name.endswith(".dll") and (name[:-4] in WINDOWS_SYSTEM_DLLS or name.startswith(("api-ms-win-", "ext-ms-win-"))), f"Non-system Windows DLL dependency: {name}")
            imports.append(name)
        require(terminated, "Unterminated PE import directory")
    # A delay import could hide a dependency from the regular import table.
    if struct.unpack_from("<I", data, optional + 108)[0] >= 14:
        require(optional_size >= 224, "Truncated PE delay-import directory")
        delay_rva, delay_size = struct.unpack_from("<II", data, optional + 216)
        require(not delay_rva and not delay_size, "Delay imports require an explicit dependency audit")
    return {"file": "PE32+ AMD64", "resource_bytes": resource_size, "resource_types": sorted(resource_types), "dependencies": imports}


def package_macos(binary, version, stage, artifact):
    app = stage / "Crabdash.app"
    contents = app / "Contents"
    executable = contents / "MacOS" / "crabdash"
    copy(binary, executable)
    executable.chmod(0o755)
    resources = contents / "Resources"
    for name in ("AppIcon.icns", "Assets.car"):
        copy(ROOT / "assets/icons" / name, resources / name)
    notices(resources)
    info = {
        "CFBundleName": "Crabdash", "CFBundleDisplayName": "Crabdash",
        "CFBundleIdentifier": "com.thojensen.crabdash", "CFBundleExecutable": "crabdash",
        "CFBundlePackageType": "APPL", "CFBundleShortVersionString": version,
        "CFBundleVersion": version, "CFBundleIconFile": "AppIcon.icns", "CFBundleIconName": "AppIcon",
        "LSMinimumSystemVersion": "13.0", "NSHighResolutionCapable": True,
        "NSPrincipalClass": "NSApplication", "NSSupportsAutomaticGraphicsSwitching": True,
    }
    with (contents / "Info.plist").open("wb") as output:
        plistlib.dump(info, output)
    run("codesign", "--force", "--deep", "--sign", "-", "--timestamp=none", str(app))
    run("codesign", "--verify", "--deep", "--strict", str(app))
    (stage / "Applications").symlink_to("/Applications", target_is_directory=True)
    (stage / "INSTALL.txt").write_text("Drag Crabdash.app to Applications. Requires macOS 13 or later.\nThis build is ad-hoc signed; it is not Developer ID signed or notarized.\n", encoding="utf-8")
    run("hdiutil", "create", "-volname", "Crabdash", "-srcfolder", str(stage), "-ov", "-format", "UDZO", str(artifact))
    run("hdiutil", "verify", str(artifact))
    return executable


def package_linux(binary, stage, artifact):
    executable = stage / "crabdash"
    copy(binary, executable)
    executable.chmod(0o755)
    copy(ROOT / "assets/icons/AppIcon.png", stage / "crabdash.png")
    notices(stage)
    (stage / "crabdash.desktop").write_text("[Desktop Entry]\nType=Application\nName=Crabdash\nComment=System dashboard\nExec=crabdash\nIcon=crabdash\nTerminal=false\nCategories=System;Monitor;\n", encoding="utf-8")
    (stage / "INSTALL.txt").write_text("Requires glibc 2.35 or newer (Ubuntu 22.04 LTS baseline), a graphical desktop, and working Vulkan drivers.\nRun ./crabdash. Use ldd ./crabdash to identify missing runtime libraries.\nUbuntu runtime packages include libasound2, libfontconfig1, libglib2.0-0, libva2, libwayland-client0, libxcb1, libxkbcommon0, libxkbcommon-x11-0, libzstd1, and libvulkan1.\nTo install locally, place crabdash in ~/.local/bin, crabdash.desktop in ~/.local/share/applications, and crabdash.png in ~/.local/share/icons/hicolor/1024x1024/apps. Create these directories if needed and ensure ~/.local/bin is on PATH.\n", encoding="utf-8")
    with tarfile.open(artifact, "w:gz") as archive:
        archive.add(stage, arcname=stage.name)
    return executable


def package_windows(binary, stage, artifact):
    executable = stage / "crabdash.exe"
    copy(binary, executable)
    notices(stage)
    (stage / "INSTALL.txt").write_text("Extract the entire archive and run crabdash.exe on 64-bit Windows.\nRequires a working DirectX graphics driver. The C/C++ runtime is linked statically; no separate Visual C++ Redistributable is required.\nThis portable package is not Authenticode signed.\n", encoding="utf-8")
    with zipfile.ZipFile(artifact, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(stage.rglob("*")):
            if path.is_file():
                archive.write(path, path.relative_to(stage.parent))
    return executable


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True, choices=TARGETS)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--version", required=True)
    parser.add_argument("--output", type=Path, default=Path("dist"))
    args = parser.parse_args()
    require(re.fullmatch(r"\d+\.\d+\.\d+", args.version), "Version must be numeric major.minor.patch")
    binary = args.binary.resolve()
    require(binary.is_file(), f"Binary missing: {binary}")
    platform, architecture = TARGETS[args.target]
    audit = {"macos": lambda: audit_macos(binary, architecture), "linux": lambda: audit_linux(binary, architecture), "windows": lambda: audit_windows(binary)}[platform]()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    name = f"crabdash-v{args.version}-{args.target}"
    artifact = output / ASSETS[args.target]
    with tempfile.TemporaryDirectory(prefix=".package-", dir=output) as temporary:
        stage = Path(temporary) / name
        stage.mkdir()
        packaged = package_macos(binary, args.version, stage, artifact) if platform == "macos" else {"linux": package_linux, "windows": package_windows}[platform](binary, stage, artifact)
        receipt = {"version": args.version, "target": args.target, "platform": platform, "architecture": architecture,
                   "binary_sha256": digest(binary), "packaged_binary_sha256": digest(packaged),
                   "artifact": artifact.name, "asset_sha256": digest(artifact), "audit": audit}
    (output / (args.target + ".json")).write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(receipt, indent=2))


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError, subprocess.SubprocessError, struct.error, UnicodeError) as error:
        print(f"Packaging failed: {error}", file=sys.stderr)
        sys.exit(1)
