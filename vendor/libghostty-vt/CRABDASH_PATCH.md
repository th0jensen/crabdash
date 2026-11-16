Based on libghostty-vt 0.2.1 at uzaaft/libghostty-rs revision
483ea3d5247b214060da58ef6037a95f30ae89fb, under its MIT OR Apache-2.0 license.
The upstream version is 0.2.1 even though the previous local copy was 0.2.2.
The temporary native-source override is Ghostty
5ebe501a63425b13d9952e8c54682c046386e5af, shared with Bless, using Zig 0.17.1.
The bindings pin 108cacb17ed8990840cd14abfce43d8ed8503ca1 upstream; the public
VT headers and src/terminal/c differ only in version.h at this override.
Revalidate the interface and native tests before advancing either revision.

The safe crate uses its adjacent sys crate by path, including when Bless's
Git-source dependency is overridden by this copy. Upstream now supplies the
constructor, scrollback/mode queries, pixel dimensions, and borrowed selection
getter that previously required Crabdash compatibility patches. Its sources
and tests are otherwise retained unchanged.

The opt-in `Terminal::resize_bottom` wrapper calls the bundled
`ghostty_terminal_resize_bottom` extension. Its primary-screen resize reveals
history above existing content and prepends blank rows when history is exhausted.
Alternate-screen behavior and the original resize API remain unchanged. The
flag is threaded through Terminal/Screen/PageList; tracked content pins remain
attached to their rows, and reflow preserves the cursor’s trailing blank cells.

Terminal regressions in crates/app/src/features/terminal exercise the ABI,
scrollback, resize, mouse tracking, selection, and rendering.
