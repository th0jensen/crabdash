# libghostty-vt-sys

Raw FFI bindings for libghostty-vt.

- Fetches and builds `libghostty-vt.a` from ghostty sources via Zig by default.
- Exposes checked-in generated bindings in `src/bindings.rs`.
- Static linking is the baseline rather than a Cargo feature. Enable the
  additive `link-dynamic` feature to link the shared library instead.
- Set `GHOSTTY_SOURCE_DIR` to force the build to use a local Ghostty checkout.
  The build copies its tracked files and nonignored local additions, including
  working-tree edits, into `OUT_DIR/ghostty-build-src` before applying Crabdash's
  checked-in resize and mouse patches. It accepts sources with either patch
  already applied and never patches the original checkout.
  Ignored caches, Zig build outputs, and original Git metadata are excluded.
  The staged checkout has its own Git metadata, so Cargo cache restoration does
  not depend on the source checkout's former location. Unchanged staged sources
  and the separate Zig cache/install directories are reused; source or patch
  changes refresh the staged copy. Staging needs no network access.
- Set `GHOSTTY_ZIG_SYSTEM_DIR` to force Zig package resolution through a
  pre-fetched `zig build --system` directory. This is intended for Nix and other
  sandboxed package managers that cannot fetch during build scripts.
- Vendored builds target Zig's portable `baseline` CPU by default. Set
  `LIBGHOSTTY_VT_SYS_CPU` to `native`, a named CPU model such as `x86_64_v3`, or
  another Zig CPU expression to optimize for known deployment hardware.
- Set `LIBGHOSTTY_VT_SYS_OPTIMIZE` to `Debug`, `ReleaseSafe`, `ReleaseFast`, or
  `ReleaseSmall` to override the Zig optimize mode used by vendored builds.
- If the `pkg-config` feature is enabled, the build will use an installed
  `libghostty-vt` found through `pkg-config` only when `GHOSTTY_SOURCE_DIR` is
  unset. With the default static link mode, it probes Ghostty's
  `libghostty-vt-static` pkg-config module instead.
- libghostty-vt is pre-1.0, so these bindings do not guarantee compatibility
  with arbitrary installed C API revisions.

The staging and patch checks can be run without Zig or a Cargo native build:

```sh
rustc --edition 2024 --test vendor/libghostty-vt-sys/patch.rs -o /tmp/crabdash-patch-tests
/tmp/crabdash-patch-tests
```
