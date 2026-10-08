Based on libghostty-vt-sys 0.2.2 from https://github.com/uzaaft/libghostty-rs,
licensed MIT OR Apache-2.0. Original source and generated bindings are retained;
the Apache-2.0 license text is included in LICENSE-APACHE.

The build script copies the tracked GHOSTTY_SOURCE_DIR files into a private
OUT_DIR staging checkout before invoking Zig. It applies the versioned
script/patches/ghostty-resize-bottom.patch and ghostty-mouse-tracking.patch there;
the original source checkout and pinned Ghostty submodule remain unchanged.
An OS file lock serializes staging, and a source/patch fingerprint reuses valid
snapshots while rebuilding incomplete or altered ones. Incompatible patches
fail before building native code.

The opt-in primary-screen resize extension anchors existing content at the
bottom. The mouse extension retains native tracking state when encoding options
and geometry are unchanged, avoiding duplicate motion reports.
