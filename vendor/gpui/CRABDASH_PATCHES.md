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
specifies an `NSMenu` value. The native app-menu registration path was verified
by compiling and launching Crabdash on macOS 27.2.

`src/window.rs`: expose `has_native_overlay_occluder` for embedded native controls.
The read-only query is intended for the Element paint phase, after tooltip and
deferred popup prepaint. Crabdash hides AppKit button overlays and paints their
GPUI fallbacks whenever GPUI displays a prompt, tooltip, or deferred overlay.

`src/platform/mac/window.rs` and `mac/view_coordinates.rs`: measure the drawable
viewport from GPUI's renderer NSView, and convert mouse, scroll, file-drop and IME
coordinates through that view. This supports hosting the renderer in the detail
pane of a native split-view controller without assuming it fills the NSWindow.
Screen-space IME rectangles use public view/window conversion methods. Outer
window bounds, native delegates, traffic-light placement and display-link policy
remain unchanged. Two pure geometry tests cover nonzero view bounds, both view
orientations, outside hit-test points and caret rectangle conversion.

`src/window.rs`: expose macOS-only `refresh_native_viewport` for synchronizing
the cached viewport immediately after a native host's initial layout, before
the first frame. Normal renderer-view resize callbacks continue to update it.
Hosts must retain the GPUI NSView across reparenting, keep its layer and the
existing NSWindow delegate, and lay it out before calling this helper.
Embedded renderer resize notifications are coalesced onto the foreground
executor: native Auto Layout may resize its NSView during an existing GPUI app
update, where a synchronous resize callback would reborrow that app. The
original direct child of the window content view keeps synchronous callbacks.
