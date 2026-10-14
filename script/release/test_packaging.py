import importlib.util
from pathlib import Path
import struct
import subprocess
import tempfile
import unittest


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


package = load("release_package", "package.py")
sdk = load("select_macos_sdk", "select-macos-sdk.py")


class NativePatchInputs(unittest.TestCase):
    def test_extension_patches_keep_lf_when_git_autocrlf_is_enabled(self):
        root = Path(__file__).resolve().parents[2]
        patches = sorted((root / "script/patches").glob("*.patch"))
        self.assertTrue(patches)
        for patch in patches:
            with self.subTest(patch=patch.name):
                self.assertNotIn(b"\r", patch.read_bytes())
                relative = patch.relative_to(root).as_posix()
                result = subprocess.run(
                    ["git", "-c", "core.autocrlf=true", "check-attr", "text", "eol", "--", relative],
                    cwd=root, capture_output=True, text=True, check=True,
                ).stdout.splitlines()
                self.assertIn(f"{relative}: text: set", result)
                self.assertIn(f"{relative}: eol: lf", result)


def version_blob(version=(0, 1, 5, 0)):
    key = "VS_VERSION_INFO\0".encode("utf-16-le")
    blob = bytearray(struct.pack("<HHH", 0, 52, 0) + key)
    blob.extend(bytes(-len(blob) % 4))
    major, minor, patch, revision = version
    ms, ls = major << 16 | minor, patch << 16 | revision
    blob.extend(struct.pack("<13I", 0xFEEF04BD, 0x10000, ms, ls, ms, ls, 0x3F, 0, 0x40004, 1, 0, 0, 0))
    struct.pack_into("<H", blob, 0, len(blob))
    return blob


def executable(resource_types=(3, 14, 16, 24), version=(0, 1, 5, 0)):
    # A minimal PE fixture with one .rsrc section and type/name/language
    # directories. The audit never executes the fixture.
    blob = version_blob(version)
    data = bytearray(0x400 + len(blob))
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 60, 0x80)
    data[0x80:0x84] = b"PE\0\0"
    struct.pack_into("<HH", data, 0x84, 0x8664, 1)
    struct.pack_into("<H", data, 0x94, 240)
    optional = 0x98
    struct.pack_into("<H", data, optional, 0x20B)
    struct.pack_into("<I", data, optional + 108, 16)
    struct.pack_into("<II", data, optional + 128, 0x1000, len(data) - 0x200)
    table = optional + 240
    struct.pack_into("<IIII", data, table + 8, len(data) - 0x200, 0x1000, len(data) - 0x200, 0x200)
    struct.pack_into("<HH", data, 0x200 + 12, 0, len(resource_types))
    for index, kind in enumerate(resource_types):
        struct.pack_into("<II", data, 0x210 + index * 8, kind, 0x80000000 | 0x60)
    struct.pack_into("<HH", data, 0x260 + 12, 0, 1)
    struct.pack_into("<II", data, 0x270, 1, 0x80000000 | 0x80)
    struct.pack_into("<HH", data, 0x280 + 12, 0, 1)
    struct.pack_into("<II", data, 0x290, 1033, 0xA0)
    struct.pack_into("<IIII", data, 0x2A0, 0x1200, len(blob), 0, 0)
    data[0x400:] = blob
    return data


class WindowsReleaseAudit(unittest.TestCase):
    def audit(self, data):
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "crabdash.exe"
            binary.write_bytes(data)
            return package.audit_windows(binary, "0.1.5")

    def test_version_is_reported_from_executable(self):
        result = self.audit(executable())
        self.assertEqual(result["executable_version"], ["0.1.5.0"])
        self.assertIn(16, result["resource_types"])

    def test_missing_version_resource_is_rejected(self):
        with self.assertRaisesRegex(RuntimeError, "version"):
            self.audit(executable(resource_types=(3, 14, 24)))

    def test_stale_executable_version_is_rejected(self):
        with self.assertRaisesRegex(RuntimeError, "differs from release"):
            self.audit(executable(version=(0, 5, 0, 0)))

    def test_truncated_or_invalid_fixed_version_is_rejected(self):
        for blob in (b"", version_blob()[:30], bytes(len(version_blob()))):
            with self.assertRaises(RuntimeError):
                package.audit_windows_version(blob, "0.1.5")


class MacOSSDKSelection(unittest.TestCase):
    def test_ignores_old_default_and_selects_newest_supported_sdk(self):
        versions = {"default": "15.4", "Xcode_26.3": "26.2\n", "Xcode_next": "27.0"}
        self.assertEqual(sdk.select(versions, versions.__getitem__, lambda _: True), "Xcode_next")

    def test_requires_macOS_26_sdk(self):
        for versions in ({"default": "15.4"}, {"broken": "invalid"}, {}):
            with self.assertRaisesRegex(RuntimeError, "SDK 26"):
                sdk.select(versions, versions.__getitem__, lambda _: True)

    def test_command_line_tools_cannot_win_an_equal_sdk_tie(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            xcode = root / "Applications/Xcode.app/Contents/Developer"
            clt = root / "Library/Developer/CommandLineTools"
            for developer in (xcode, clt):
                tool = developer / "usr/bin/xcodebuild"
                tool.parent.mkdir(parents=True)
                tool.touch()
            versions = {xcode: "26.2", clt: "26.2"}
            self.assertEqual(sdk.select(versions, versions.__getitem__), str(xcode))
            with self.assertRaisesRegex(RuntimeError, "SDK 26"):
                sdk.select([clt], versions.__getitem__)


if __name__ == "__main__":
    unittest.main()
