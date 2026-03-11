# Crabdash

Crabdash is a native desktop dashboard for managing machines and services.

It provides a single interface for inspecting and controlling:

- local system services
- Docker containers
- disks and mounts
- remote Linux machines over SSH

The goal is to replace scattered terminal commands with a focused control panel while still allowing quick fallbacks to the terminal when needed.

Crabdash is built as a native application using **Rust and [GPUI](https://www.gpui.rs/)**.

## Features (planned)

- System overview (hostname, OS version, architecture)
- Docker container management
- System service management (`systemd`)
- Disk and mount inspection
- Remote machine support via SSH
- Quick command execution and logs

The application is designed around **machines** and **resources** (services, containers, disks), with actions such as start, stop, and restart exposed directly in the UI.

## Run

```bash
cargo run
```
