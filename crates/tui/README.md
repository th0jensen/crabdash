# crabdash-tui

[Crabdash](https://github.com/th0jensen/crabdash)'s dashboard in the terminal: Docker
containers, disks, system services and system activity for the machine it runs on,
read with the same code as the desktop app's local machine.

## Install

```sh
cargo install --locked crabdash-tui
```

On Linux, building needs OpenSSL headers (`libssl-dev` or `openssl-devel`) and `pkg-config`.

## Use

```sh
crabdash-tui [docker|disks|services|system]
```

The frame fills the pane and is built for touch, like ramp: tap a section to switch,
tap a container or service for a sheet of its actions (start or stop, restart, pause,
logs), tap **×** to close it, and swipe to scroll. In Services, tapping the count shows
only active services. Keys do the same: **Tab** or **1**–**4** switch sections, **j**/**k**
select, **Enter** opens actions, **s**/**r**/**p**/**l** act on the selection, **a** shows
only active services, and **Esc** closes or quits.

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
