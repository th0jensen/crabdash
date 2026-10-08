# Crabdash portable-pty patch

This directory vendors the published `portable-pty` **0.9.0** crate from
[crates.io](https://crates.io/crates/portable-pty/0.9.0). The original crate archive
SHA-256 is `b4a596a2b3d2752d94f51fac2d4a96737b8705dddd311a32b9af47211f08671e`.
Its published VCS metadata identifies the upstream
[wezterm commit f8921727a11b9f8b073e8c24821d72fd41283500](https://github.com/wezterm/wezterm/tree/f8921727a11b9f8b073e8c24821d72fd41283500/pty),
with `path_in_vcs = "pty"`.

The source, examples, normalized `Cargo.toml`, original manifest and MIT license
are retained. Registry cache metadata is omitted. A standalone `Cargo.lock` pins
the crate's runtime and upstream development dependencies for reproducible native
regression tests. The workspace pins this directory through `[patch.crates-io]`
without changing the version or dependency requirements.

The patch is limited to the Windows backend:

- `src/win/psuedocon.rs`: use `CreatePseudoConsole` flags **0**, documented as
  standard creation. Crabdash creates a fresh GUI terminal and has no parent
  console cursor to inherit. Remove the unused inherited-cursor and extension
  flag constants. Application-generated VT cursor queries still reach Ghostty;
  the transport does not intercept queries or fabricate cursor responses.
- `src/win/conpty.rs`: declare the owned readable/writable endpoints before the
  pseudoconsole so Rust closes unclaimed pipes before `ClosePseudoConsole` on
  setup failure. During normal sessions, Crabdash's independent cloned reader
  stays alive and drains output while the native master closes. Add a Windows
  native regression for unclaimed and failed-spawn teardown with a bounded
  completion check and no cursor-response thread.
- `src/win/mod.rs`: treat a nonzero `TerminateProcess` result as success in both
  the child and cloned killer paths, retrieving the OS error only on zero.
  Return the child kill result rather than swallowing it.

The Unix backend and public API are unchanged. Existing upstream `unwrap` calls
outside these changes are retained; the patch adds none.

Reference contracts:

- [CreatePseudoConsole flags and inherited-cursor obligations](https://learn.microsoft.com/en-us/windows/console/createpseudoconsole)
- [Creating a Pseudoconsole Session: independent I/O and teardown](https://learn.microsoft.com/en-us/windows/console/creating-a-pseudoconsole-session)
- [TerminateProcess return value](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-terminateprocess)

Crabdash's independent reader, FIFO writer, serialized native-control worker and
child waiter are implemented separately in
`crates/machines/src/terminal/local/windows.rs`. Host-side fake-PTY tests exercise
that real coordinator, including blocked resize/write, receiver drop and normal
child exit before output EOF. These tests and cross-compilation do not establish
native Windows runtime behavior; the native early-close regression still needs
a Windows runner.

The crate remains excluded from the Crabdash workspace. Run its native regression
through the standalone manifest so Cargo includes the upstream development
dependencies and uses this directory's lockfile:

```powershell
cargo test --locked --manifest-path vendor/portable-pty/Cargo.toml --target x86_64-pc-windows-msvc unclaimed_or_failed_session_closes_without_cursor_reply -- --test-threads=1
```

Regenerate this standalone lockfile with
`cargo generate-lockfile --manifest-path vendor/portable-pty/Cargo.toml` when
intentionally updating its pinned test dependencies. This does not replace the
root workspace lockfile.
