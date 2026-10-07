# GPUI 0.2.2 local patch

Source: the published `gpui` 0.2.2 crate from crates.io (Zed Industries).
Upstream code remains under the included Apache 2.0 license.

`src/elements/text.rs`: keep the original `TextRun` ranges immutable and clone
runs for each measured-layout invocation. GPUI may first measure an ellipsized
label narrowly, then remeasure its original text at a wider flex width. Reusing
the truncated ranges can split a Unicode character (for example byte 7 in
`Linux · SSH`) and abort in macOS CoreText shaping. The patch applies to every
platform and preserves Unicode and ellipsis behavior.

Two macOS CoreText regression tests cover narrow-to-wide and untruncated
remeasurement, multiple styles, middots, accented Latin, combining marks, CJK,
and emoji. Run from the workspace root:

```sh
cargo --config 'profile.release.build-override.strip="none"' test --manifest-path vendor/gpui/Cargo.toml --lib unicode_remeasurement --features runtime_shaders --target-dir target --release -- --test-threads=1
```

`runtime_shaders` is optional when the Apple Metal compiler is installed.

The strip override matches the workspace profile and preserves Rust proc-macro
metadata when running this crate standalone with the macOS linker.

`src/platform/mac/platform.rs`: register the Services submenu (`NSMenu`) with
`NSApplication`, rather than its containing `NSMenuItem`. The Objective-C
bindings use an untyped `id`, so Rust cannot reject the incorrect object type.
Crabdash's native app menu exercises this registration path. Apple's
[servicesMenu documentation](https://developer.apple.com/documentation/appkit/nsapplication/servicesmenu)
specifies an `NSMenu` value. This correction was reviewed against the source and
public API contract; macOS compilation and runtime remain unverified.
