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
deferred popup prepaint. Per-control presentation uses the more specific
`native_control_presentation` API described below. Native controls use AppKit
tooltips so help does not replace their glass with a GPUI fallback.

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
original direct child of the window content view keeps synchronous callbacks
until it has been embedded. Once embedded, deferred callbacks also cover the
transition back to a direct child, which can happen during the same app update.

`src/platform/mac/window.rs`: make the renderer's `NSTextInputClient` accept
first responder and restore AppKit focus on a left click, before processing its
input context and outside the window-state lock. A native sidebar can otherwise
retain keyboard focus after GPUI focuses a dashboard filter, so typing goes to
the sidebar and produces the system beep. Native siblings retain keyboard focus
until the user clicks the GPUI view. Crabdash also restores this responder after
installing its native split-view host.


`src/window.rs`: expose macOS-only paint-phase `native_control_presentation`
for a marker inserted after each native leaf's own children. The presentation
API keeps original native materials dimmed outside explicitly registered modal
dialog bounds, with input disabled by the native host. It hides views intersecting
the dialog, content clipping, tooltips or other later blocking layers. The dialog
marker and its paired full-window backplate live in frame hitboxes, preserving
stacking and bounds during cached prepaint replay. Modal controls remain visible
because their markers follow the dialog marker.
`paint_native_control_fallback` records the existing GPUI listeners with zero
paint opacity, avoiding an opaque duplicate beneath real AppKit leaf content.
`dispatch_native_control_click` sends a validated native activation to those
listeners as a left click, preserving held keyboard modifiers, without exposing
GPUI's private dispatch-result type.
The controls use NSGlassEffectView.contentView and a native glass container; see
[Apple's AppKit guidance](https://developer.apple.com/videos/play/wwdc2025/310/).

`src/elements/div.rs`: expose `Interactivity::clear_tooltip` so native controls
can use their AppKit help without also creating a GPUI tooltip layer. Calling it
before prepaint also clears any pending tooltip from the shared appearance.

`src/elements/div.rs`, `src/window.rs`: non-hoverable tooltip visibility follows
the source element's current hitbox ID through persistent state. After root and
deferred prepaint, the visibility callback uses the next frame's hit-test hover
prefix, accounting for moved geometry, content masks and blocking overlays.
Cached prepaint replay preserves the IDs. Removing a tooltip builder also clears
its source ID and cancels any delayed show or active tooltip. Hoverable tooltips
retain their existing bounds-based transition into their own view. Two pure
frame hit-test tests cover changed IDs and geometry, clipping, both blocking
behaviors, missing sources and replay. The frame drawing order is unchanged.

`src/window/tooltip.rs`, `src/window.rs`, `src/elements/div.rs`: tooltip lifetimes
use real renderer pointer presence and an epoch, independent of the public
macOS active-window hover policy. Cursor departure, hover(false) and focus loss
invalidate visible and delayed tooltips; repeated exit signals are idempotent.
Show timers and cached prepaint callbacks cannot claim another lifetime or
render a fresh pending show. Reentry schedules the normal full delay. Hoverable
tooltips retain their source-to-tooltip transition and hide delay inside the
renderer, while whole-renderer departure invalidates both phases. Four pure
policy tests cover departure, rapid reentry, focus loss, inactive popup hover,
hoverable bounds and stale cached callbacks. Existing hit-test regressions remain.
Nine headless tests exercise the real tasks and visibility callbacks, including
full fresh show delays, same-epoch cached requests, source removal, source redraw
and occlusion without mouse motion, and hoverable hide cancellation and expiry.
Source hover predicates follow the current persisted hitbox ID in the rendered
hit test, so an unrelated redraw does not cancel a legitimate delayed show.
The test window records hover transitions; Linux Wayland and X11 also store real
hover state before publishing it so the platform getter remains authoritative.

`src/platform/mac/window.rs`: every renderer kind owns an Entered/Exited,
ActiveAlways, InVisibleRect tracking area; only PopUp also requests MouseMoved.
Genuine native movement synchronizes presence against the renderer's visible
rectangle. Tracking changes publish outside the native lock through ordered,
weak-state foreground tasks, with an explicit removed-window guard. A declared
FIFO queue retains every enter/exit transition and exit input. Before a genuine
pointer callback, the queue is flushed synchronously outside the native lock;
mouse motion is neither fabricated nor deferred past subsequent pointer input.
Callbacks cannot restore themselves after window removal. Two queue tests cover
ordering and removal, and an options test covers all three kinds. Native AppKit event
delivery and renderer reparenting still require macOS runtime verification.

`src/platform/linux/text_system.rs` and its declared `emoji_fallback.rs` module:
retain Cosmic Text's installed font database, locale, generic families and script
fallbacks. On Linux only, replace the common Noto Color Emoji fallback with an
installed scalable Noto Emoji face when every installed color face is proven to
contain only COLRv1 paint graphs without COLRv0 or bitmap alternatives. The
pinned Swash 0.2.10 rasterizer cannot draw those graphs; Fedora ships this format.
The unsupported family is also excluded from Cosmic Text's catch-all fallback
stage in that case. Supported color faces, mixed installations, absent outline
fonts and unknown/truncated headers or invalid references retain the normal policy. Primary font
selection and the font database remain unchanged, and inspection occurs once at
text-system creation. FreeBSD retains the default policy.

Four pure `emoji_fallback` tests cover supported color/bitmap alternatives,
truncated/invalid tables, unavailable or mixed replacements, preservation of
other common fallbacks and locale-sensitive script delegation. The optional
Linux `unicode-script` dependency names the exact `Fallback` trait argument type
already used by Cosmic Text; both locked dependency graphs retain version 0.5.8.

`build.rs`, declared `build/windows.rs` and the `embed-resource` build dependency:
dispatch Windows build work using Cargo's target OS on every host. The manifest
remains feature-gated and only Windows targets invoke its resource compiler.
The existing debug/runtime and release/embedded shader selection is unchanged.
Release shader compilation keeps native FXC discovery (`GPUI_FXC_PATH`, the first
existing `where.exe` result, then the Windows SDK path), and accepts an explicit
`GPUI_VKD3D_COMPILER` host-executable file path for a compiler supporting HLSL
input and `dxbc-tpf` output. Each of the sixteen vertex/fragment entry points,
including `color_text_raster.hlsl`, uses its existing Shader Model 4.1 profile.
Configured compiler paths are resolved before invocation; vkd3d runs in the HLSL
source directory so its default include callback finds `alpha_correction.hlsl`.
Compiler failures include entry-point diagnostics; invalid or truncated DXBC and
DXIL-only output is rejected. Binary `.cso` files are embedded through generated
Rust `include_bytes!` constants, with no FXC C-header parsing. Both compiler
environment variables, all three HLSL inputs and the build module are tracked.
Three build-helper tests cover command arguments, binary-container rejection and
multiline FXC discovery. Actual shader compilation and cross-host resource
embedding require host tools and remain build/runtime verification tasks.
