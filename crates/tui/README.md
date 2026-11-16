# crabdash-tui

[Crabdash](https://github.com/th0jensen/crabdash)'s dashboard in the terminal: Docker
containers, disks, system services and system activity for the machine it runs on,
read with the same code as the desktop app's local machine.

## Install

Build this PR from its checkout (Rust **1.96+**, Zig **0.17.1**):

```sh
git submodule update --init --recursive
cargo run --locked -p crabdash-tui -- system
# Or install the checkout's executable:
cargo install --locked --path crates/tui
```

On Linux, building needs OpenSSL headers (`libssl-dev` or `openssl-devel`) and `pkg-config`.
Bless is linked in on Unix; no separate `bls` installation is needed. The GUI and
Bless share the vendored Ghostty build. Windows uses the built-in log viewer.

**crates.io release gate:** the pinned `bless-pager` git library and its updated
Ghostty bindings need compatible registry releases before `cargo install crabdash-tui`
can deliver this version. `publish = true` alone does not make git dependencies
publishable. Merging does not publish; the version-tag workflow checks this gate
before uploading any Crabdash crate and also needs `CARGO_REGISTRY_TOKEN`.

## Use

```sh
crabdash-tui [docker|disks|services|system]
```

The borderless dashboard fills its Rex pane. Tap the top tabs to switch sections,
the search field to type, and chips to filter or sort. Tap a container or service
to expand its metadata and action buttons in place; tap again to collapse it.
Wide panes put actions beside metadata. Swipe/wheel to scroll without activating rows.

System reflows meters beside or above processes as the viewport changes. It shows
per-core CPU, memory, swap, GPU telemetry where available, CPU history and I/O.
Short panes preserve process rows and usable actions rather than fixed-height charts.

Keys: **←/→** or **1**–**4** switch sections; **/** searches; **f/F** cycles filters or
sort order; **j/k** or **↑/↓** selects; **Tab/Enter** expands; **s/r/p/l** starts/stops,
restarts, pauses/resumes or opens logs. **Esc** clears, collapses or closes; **q** quits.
While editing a search, **Enter/Esc** leaves the field and **Ctrl+U** clears it.
Each section keeps its own search. Container/service actions keep their selected
identity across refreshes.

On Unix, logs open in [Bless](https://github.com/th0jensen/bless): ANSI-aware,
unwrapped lines with horizontal panning, search and mouse controls for small panes.
Quit the pager to return to the dashboard. Empty logs or pager errors stay in the
built-in viewer; it also supports search and scrolling.

## Rex

```sh
crabdash-tui rex install     # crabdash-tui rex uninstall removes it
```

This adds a plugin to Rex's config on this machine, so every Rex app connected to it
offers Crabdash in its command palette, including Rex for iOS:

- **Crabdash: Open Dashboard** opens a tab with a pane for each section.
- **Crabdash: Docker**, **Disks**, **Services** and **System** open one section beside
  the current pane.

Rex carries the connection, so each pane reads the machine its session runs on. A
pane for a session on another host runs that host's `crabdash-tui`, installing it with
`cargo install` first when it is missing. Upgrading `crabdash-tui` updates the plugin
the next time it runs.
Until the registry release gate is cleared, install the checkout's binary on each
target host; automatic registry bootstrap cannot install this revision yet.

Each pane titles itself "Crabdash · *Section*" and reports its status with the
[Program Status Protocol](https://www.superlogical.com/rex/docs/build/program-status)
(OSC 7501, `app=crabdash`), so Rex shows a working indicator and message on the tab
while a container or service action runs.
