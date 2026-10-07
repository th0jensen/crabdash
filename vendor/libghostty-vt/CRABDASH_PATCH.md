Based on libghostty-vt 0.2.2 from https://github.com/uzaaft/libghostty-rs,
under its MIT OR Apache-2.0 license.

Crabdash builds the bundled Ghostty tip source through GHOSTTY_SOURCE_DIR.
Its C API differs from the crates.io wrapper at terminal creation and mode
queries. This local patch uses the dimension arguments of terminal_new,
sets the scrollback line limit through terminal_set, and accesses modes
through the ModeConfig terminal_get/terminal_set queries. Other wrapper code
is unchanged. Keep these definitions aligned with vendor/ghostty/include.
Terminal regressions in crates/app/src/content/terminal.rs exercise the ABI,
scrollback, resize, mode queries, and rendering.
