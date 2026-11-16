Based on libghostty-vt-sys 0.2.1 at uzaaft/libghostty-rs revision
483ea3d5247b214060da58ef6037a95f30ae89fb,
licensed MIT OR Apache-2.0. Original source and generated bindings are retained;
the Apache-2.0 license text is included in LICENSE-APACHE.

The temporary Ghostty override is 5ebe501a63425b13d9952e8c54682c046386e5af,
shared with Bless. Builds require Zig 0.17.1. The bindings pin 108cacb upstream;
the public VT headers and src/terminal/c differ only in version.h at this
override. Revalidate the interface and native tests before advancing it.

The build script copies the tracked GHOSTTY_SOURCE_DIR files into a private
OUT_DIR staging checkout before invoking Zig. It applies the versioned
script/patches/ghostty-resize-bottom.patch and ghostty-mouse-tracking.patch there;
the original source checkout and pinned Ghostty submodule remain unchanged.
An OS file lock serializes staging, and a source/patch fingerprint reuses valid
snapshots while rebuilding incomplete or altered ones. Incompatible patches
fail before building native code.

Zig's compilation and package caches stay outside the fingerprinted source
snapshot (`--cache-dir` and Zig 0.17's `--pkg-dir`). Immutable system-package
builds point both `--system` and `--pkg-dir` at GHOSTTY_ZIG_SYSTEM_DIR and redirect
the global cache with ZIG_GLOBAL_CACHE_DIR. The ZIG override,
baseline CPU policy, and Windows exact static-archive linking are preserved.
Release CI continues to build Windows consumers with the static CRT.

The opt-in primary-screen resize extension anchors existing content at the
bottom. The mouse extension retains native tracking state when encoding options
and geometry are unchanged, avoiding duplicate motion reports.
