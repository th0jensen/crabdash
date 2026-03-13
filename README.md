# Crabdash

> [!WARNING]
> This project is fast moving and features may break without notice. It will stabilize once it hits v0.1.1

![Screenshot of the app (v0.1.0)](assets/screenshot.png)

Crabdash is a native desktop dashboard for managing machines and services (like homelabs).

It provides a single interface for inspecting and controlling:

- local system services
- Docker containers
- disks and mounts
- remote Linux machines over SSH

The goal is to replace scattered terminal commands with a focused control panel while still allowing quick fallbacks to the terminal when needed.

Crabdash is built as a native application using **Rust and [GPUI](https://www.gpui.rs/)**.

## Features (Milestone v0.1.1)

- [x] System overview (hostname, OS version, architecture)
- [x] Docker container management
- [ ] System service management (`systemd`)
- [ ] Disk and mount inspection
- [ ] Remote machine support via SSH
- [ ] Quick command execution and logs

## Run

```bash
cargo run
```
