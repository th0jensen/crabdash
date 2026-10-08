Based on libghostty-vt 0.2.2 from https://github.com/uzaaft/libghostty-rs,
under its MIT OR Apache-2.0 license.

Crabdash builds the bundled Ghostty tip source through GHOSTTY_SOURCE_DIR.
Its C API differs from the crates.io wrapper at terminal creation and mode
queries. This local patch uses the dimension arguments of terminal_new,
sets the scrollback line limit through terminal_set, and accesses modes
through the ModeConfig terminal_get/terminal_set queries. Other wrapper code
is unchanged apart from safe getters for the existing grid pixel dimensions.
The selection wrapper also exposes a safe borrowed snapshot getter for the
existing active-screen selection query. This lets terminal Copy availability
recognize selected whitespace and offscreen content without formatting or
retaining untracked refs. Keep these definitions aligned with vendor/ghostty/include.
Terminal regressions in crates/app/src/features/terminal/state.rs exercise the ABI,
scrollback, resize, mode queries, and rendering.
