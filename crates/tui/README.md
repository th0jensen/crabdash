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

**Tab** or **1**–**4** switches sections; **j**/**k** select. In Docker and Services,
**s** starts or stops, **r** restarts and **l** shows logs; **p** pauses a container and
**a** shows only active services. **q** quits. Tabs, rows and key hints also take
taps, so it works from touch clients.

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
