# Crabdash

> [!NOTE]
> This project is under active development and features may change as the project evolves.

![Screenshot of the app (v0.1.1)](assets/screenshot_v0.1.0.png)

Crabdash is a native desktop dashboard for managing machines and services (such as homelabs).

It provides a single interface for inspecting and controlling:

- local system services
- Docker containers
- disks and mounts
- live CPU, memory, swap, load, and uptime
- remote Linux, macOS, and Windows machines over SSH

The goal is to replace scattered terminal commands with a focused control panel while still allowing quick fallbacks to the terminal when needed.

Crabdash is built as a native desktop application using **Rust** and **[GPUI](https://www.gpui.rs/)** — the same GPU-accelerated UI framework that powers the [Zed](https://zed.dev) code editor.

## Architecture

The workspace has five crates:

- `crabdash`: executable entry point and logging setup.
- `app`: GPUI application composition, feature controllers/views, and local desktop integration.
- `machines`: machine identity, local/SSH transport, and domain-specific operations on the selected machine.
- `services`: platform-neutral Docker and system-service contracts and actions.
- `utils`: shared data models, arguments, raw command output, and domain parsers.

Inside `app`, `features/` declares Docker, disks, system services, live system resources, shared logs, machines, terminals,
preferences, workspaces, polling, and notifications. Each feature owns its controller, view/editor, and
feature-specific types, including table filtering and ordering in `table.rs`. `components/` contains reusable UI primitives; `content/`
only composes the main panels and navigation. `app.rs` owns the root entity and
connects these modules, and `desktop/runtime.rs` owns application lifecycle.

Local desktop features live in `app/src/desktop/{about,appearance,controls,menus,startup,tray,window}/`.
Each declares adjacent `linux.rs`, `macos.rs`, and `windows.rs` implementations behind a common
interface, selected with host `cfg` at the module boundary. Linux uses the GPUI menu bar,
client window controls, systemd login service, and StatusNotifier tray. macOS backends use
AppKit menus, a persistent status item, and login registration. The macOS
**Liquid Glass** preference enables a native sidebar and toolbar. Refresh uses a
stock toolbar button; Terminal and Workspaces form a grouped control. Add machine
uses a stock Glass button on macOS 26 and later. Older releases use standard
buttons. Turning Liquid Glass off restores the shared dashboard sidebar and
toolbar without replacing the window or its terminal sessions. Native controls
disable while application overlays are open.
Windows backends use caption hit areas, the notification area, and per-user login startup.
Unavailable startup/tray capabilities remain visible in Preferences.

Selected-machine operations live in `machines/src/{docker,disks,services,terminal}`.
Disk and system-service modules dispatch to sibling Linux/macOS/Windows backends using the
**selected machine's platform**, so machine backends are compiled on every host for
SSH management. Linux service inventory uses one `systemctl show` query for all
loaded services, including inactive and failed units, instead of starting a
process for each unit's properties. Its property parser preserves unit identity
and reports malformed output without publishing a partial inventory. The Services table
builds visible rows and keeps the same service at the scroll position during refreshes,
including when inline logs change height.
Terminal sessions share one API with `local.rs` and `ssh.rs` transports. Local
workers live in sibling `local/unix.rs` and `local/windows.rs` modules. Windows
services input, output, resizing, and process exit independently, so synchronous
ConPTY operations do not prevent output draining or shutdown. Its pinned PTY
patch uses a fresh console cursor and closes unused pipe handles before native
teardown; see `vendor/portable-pty/CRABDASH_PATCHES.md`.
Local Windows queries and terminals resolve bundled Windows PowerShell under
`SystemRoot`, so an edited `PATH` does not break machine discovery or telemetry.
SSH queries resolve PowerShell on the target machine.
Docker discovery uses the same Windows target-side resolver locally and over SSH,
including expanded PATH entries, updated user/machine PATH, and standard Docker
Desktop install locations. Linux and WSL use their native Linux Docker CLI.
A missing CLI is distinct from an installed CLI whose daemon cannot be reached.
Corresponding machine output parsers live in
`utils/src/{disks,services}/{linux,macos,windows}.rs`; raw `Output` has no domain parsing.

GPUI is pinned to 0.2.2 with a local text-measurement patch in `vendor/gpui`.
Each truncation measurement starts from the original UTF-8 text runs, preventing
Unicode sidebar/table labels from aborting on macOS during flex remeasurement.
See `vendor/gpui/CRABDASH_PATCHES.md` for the patch and native regression checks.

When adding a feature, declare its module, keep domain work out of the shell and
reusable components, and put platform implementations next to each other with
matching interfaces. Use host `cfg` only for local desktop integration; use runtime
machine-platform dispatch for remote operations. Preferences and saved machine
formats are independent of this source layout.

Machine identity discovery lives in `machines/src/system_info/`, with adjacent
Linux/macOS/Windows backends selected by the machine's platform. Linux reads `os-release`
over the existing local/SSH transport. Distro logos and display labels live in
`app/src/features/machines/logos.rs`; their attributed SVGs are embedded from
`app/assets/machine-logos/` and work offline in installed builds. Unknown Linux
distributions use Tux, Windows machines use the bundled Windows logo, and saved
machines without distro metadata migrate on refresh. Right-click a machine and
choose **Rename** to save a display name, including for localhost. Display names
survive refreshes without changing connection settings or the actual hostname.
SSH identity discovery also probes Windows CIM when `uname` succeeds with an
unrecognized identity, including Cygwin/MSYS. Recognized Linux, WSL, and Darwin
targets retain their Unix detection path.

## Features

- [x] System overview (hostname, OS version, architecture)
- [x] Docker container control (start, stop, restart)
- [x] Remote machine support via SSH (keyless, SSH key, and Tailscale)
- [x] Disk and mount inspection
- [x] System keychain integration for credential storage
- [x] Live CPU, memory, process, network, disk and GPU statistics
- [x] System service management (`systemd`, `launchd`, Windows services)
- [x] Docker inspect and logs
- [ ] Quick command execution and logs

## Run

```bash
cargo run
```

## Build Dependencies

The project is based on GPUI and therefore largely depends on the same build dependencies as Zed.

Check out their documentation to get started: [Building Zed](https://zed.dev/docs/development/)

On Fedora, install Rust, Zig, and the native build dependencies:

```bash
sudo dnf install rust cargo zig gcc gcc-c++ clang cmake make pkgconf-pkg-config \
  alsa-lib-devel fontconfig-devel glib2-devel libva-devel wayland-devel \
  libxcb-devel libxkbcommon-x11-devel openssl-devel libzstd-devel \
  vulkan-loader mesa-vulkan-drivers sqlite-devel \
  perl-FindBin perl-IPC-Cmd perl-File-Compare perl-File-Copy git
cargo build --locked
```

Linux windows include GPUI title-bar controls. Hold Alt to show the application
menus and their underlined access keys in the same bar; Alt+C/F/E/V/W/H opens a
menu directly. The menus remain visible while a menu is open, and include
shortcuts for editing actions even when no text field is focused. Hold Alt to
reveal tab shortcuts. The icon-only Terminal button keeps its shortcut in the
tooltip and stays anchored beside the workspace switcher and window controls. Ctrl+1/2/3/4
selects Docker/Disks/Services/System; Ctrl+Shift+M maximizes or restores the window. Use
Ctrl+N to add a machine, Ctrl+R to refresh, Ctrl+J for the terminal, F10 for the
menus, and F11 for full screen. Error notifications stay visible until dismissed
or cleared by a successful action, including across background refreshes.

On Linux, **Crabdash → Preferences → General → Start at login** enables the local
`crabdash.service` systemd user unit under `$XDG_CONFIG_HOME/systemd/user` (or
`~/.config/systemd/user`). It opens the installed executable at the next graphical
login and exits with the desktop session. Disabling the preference removes the
login registration without closing an already running window. Install the binary
at a stable path before enabling the preference; it does not affect remote machines.

**Start minimised** launches Crabdash in the background, including when started
by the login service. This preference is saved in
`$XDG_CONFIG_HOME/crabdash/preferences.json` (or `~/.config/crabdash/preferences.json`).
The Linux tray icon opens the existing window and offers **Show Crabdash** and
**Preferences**, and **Quit**. Closing a window keeps its sessions running when a tray is available;
use Quit to exit. Without tray support, closing the last window exits normally.
Crabdash runs one process and one dashboard per OS user. Launching it again,
including from the login service, asks the existing process to show its retained
window and exits. **Show Crabdash** and **Preferences** restore that same window,
preserving its layouts and terminal sessions. If an existing process cannot be
reached, the new launch exits with an error instead of opening another instance.
Minimizing through Crabdash or closing to the tray pauses System sampling and
automatic table reads for that dashboard; restoring it refreshes visible panes
and starts fresh counter baselines. SSH sessions remain available. The visible
dashboard continues updating when unfocused. macOS and Windows also detect native
minimization; Wayland cannot report external minimization, so Linux tracks
Crabdash's own minimize and restore actions.

The **Workspaces** button beside Terminal saves and switches named workspaces.
Save as and Rename select the existing name, so typing replaces it immediately.
Drag tabs along a tab strip to reorder them. Drag onto the edge of a content pane
to create a split, or onto its center to move the tab into that pane. The preview
shows where the tab will land. Split dividers resize the panes, and empty panes
collapse automatically. Tabs stay inside the dashboard window. **Save as** creates
a named workspace with an inline name editor. Use the pencil control to rename an
existing workspace, then the check control or Enter to save its name.

Narrow tab strips scroll horizontally. Selecting a tab, resizing its pane, or
changing font size reveals the active tab; regular dashboard updates preserve
manual scrolling. Revealing waits until a drag finishes so the drop target stays put.

Workspaces save pane arrangement, tab order and selection, split sizes, sidebar
width and visibility, and terminal visibility and drawer height in `workspaces.json` beside
Preferences. Changes save automatically. Existing flat layouts retain their tab
order and selection when migrated. Invalid saved files are preserved until
explicit recovery; save, rename, and delete controls stay disabled until recovery
successfully writes the reset layouts.

Automatic table refresh updates Docker, Disks, and Services when their pages are
visible, including unfocused split panes. Showing a hidden page refreshes it even
with automatic refresh disabled. Ctrl+R refreshes all three tables and machine
information; switching or rearranging tabs refreshes newly visible pages.

The **System** tab samples the selected machine every two seconds by default while visible,
including in an unfocused split. Its mosaic adapts to the pane width, showing CPU,
memory, network, disk activity, machine details, and a card for each graphics adapter.
CPU cards show every processor on machines with up to eight logical processors.
Larger machines show an eight-processor preview with **Show all** / **Show fewer**
controls; processor order stays stable and every reading remains accessible.
Processor meters share equally sized columns and fill narrow panes.
Network and disk cards keep totals and history visible, with **Show interfaces/devices**
controls for the full inventory. Each machine remembers its expanded sections while
the app runs; device rows retain their reported order.
The process table supports filtering by name, PID or user and sorting across the full
sampled inventory, including idle processes. User names have their own sortable column
and remain visible in compact rows. Unix collectors report effective owners with
numeric UID fallbacks; Windows uses creation-checked process tokens with local names
or SID fallbacks. Protected or unreadable owners stay unavailable without discarding
the process. Owner collection requires no elevation; Windows avoids per-process
network account lookups. CPU percentages measure a share of the machine's total CPU
capacity. The list renders visible rows and retains the collectors' 8,192-process
safety limit. Counts distinguish search matches, sampled processes, and the detected
total, with incomplete samples identified explicitly. CPU and throughput start with
**Sampling…** until two counter
snapshots are available. Resource sampling is independent of
automatic table refresh; **Preferences → System → Sample interval** adjusts it
from 2 to 60 seconds, including for slower SSH connections. Changes apply while running;
charts use actual elapsed sample times, with solid/dashed legends for paired I/O and
gaps for unavailable samples. Chart footers show their vertical range: throughput
adapts to recent activity, while CPU, memory and GPU usage use a fixed 0–100% scale.
Hover a plot to inspect the nearest captured values and their age. Paired readings
use the same capture time; missing measurements remain **Unavailable**.
Live history stays in memory and resets after a reboot
or replacement connection. Hiding System or a sleep/clock interruption breaks the
history and rebaselines cumulative counters before live rates resume. Delayed
samples change the status to **Waiting** after three sample intervals, including
while a collector is still running. Failures retain the last readings with an
explicit failed-sample status; successful samples restore **Live**. Each complete
sample shares a 30-second deadline across all queries, including SSH connection
and session queue waits. A timeout retains the last readings and retries on the
next scheduled or manual refresh. Cancellation targets the owned direct local
child or failed SSH session; it does not guarantee termination of descendants.
Existing workspaces gain System in the focused pane
without changing their selection or split sizes.

Resource collectors live beside each other in `machines/src/resources/`: Linux
reads procfs (including WSL), Windows uses CIM counters, and macOS uses built-in
tools. Processes, network, disks and GPUs each declare their own domain module,
with Linux, macOS and Windows collectors beside each other. These command collectors
also work through SSH from any supported host;
native desktop integrations remain selected by host `cfg`. macOS reports estimated
available memory. Local macOS CPU uses native aggregate and per-core counters, with
stable logical CPU names; macOS over SSH uses the target's aggregate `top` interval.
If local native CPU sampling is unavailable, aggregate `top` is used as a fallback.
Windows has no Unix load average. GPU telemetry
depends on the adapter and driver: missing measurements are shown as unavailable.
Linux re-enumerates DRM devices each sample and reads telemetry once per physical
adapter, with optional NVIDIA tools. Device removal and unavailable driver data
appear on the next sample. WSL discovery also checks the projected
`/usr/lib/wsl/lib/nvidia-smi` when it is absent from `PATH`. If optional telemetry
queries are unsupported, an inventory-only query retains device names and PCI
identities without inventing measurements. Both attempts share one timeout.
Windows discovers adapters on the measured host through
DXGI and D3DKMT, retaining idle devices when counters are missing. Performance counters
join by exact logical adapter and physical GPU identity; hardware names join through
exact PNP registry keys. Memory capacity includes dedicated video and reserved system
memory, excluding shared memory and dynamic budgets. Linked GPUs retain separate
capacities; an unavailable capacity stays unknown. Resource scripts travel through
stdin on local and SSH Windows connections, under the same collection deadline.
macOS reads driver telemetry when exposed.
Linux and macOS network totals exclude the loopback interface; VPN and virtual
interfaces remain included. CPU and GPU meters follow the configured system
accent alongside the history graphs.

Preferences has **General**, **System**, **Terminal**, and **Interface** sections:

- General: login/background behavior, automatic refresh, refresh interval, and recent log limits.
- System: resource sampling interval, independent of automatic table refresh.
- Terminal: installed monospaced font, font size, line height, TERM/terminfo, true-colour advertisement, scrollback buffer, and initial panel rows. Font changes update existing terminals and log views; environment and buffer settings affect new sessions.
- Interface: system or installed font, font size, sidebar width, minimum equal tab widths, persistent shortcut hints, and system accent colours. macOS also has an independent Liquid Glass toggle. Tabs expand together to fit their titles and shortcuts; showing hints does not move them. Controls scale with the interface font size.

**Use system accent** colours the icon and title of the selected tab in the
focused pane. Selected tabs in other split panes stay neutral, and tab backgrounds
retain the dashboard surface colour. The preference also colours primary resource
graphs and the selected machine in the shared sidebar. macOS uses the AppKit
control accent, Linux reads the desktop Settings portal, and Windows observes
the system UI accent. Unavailable accents use the normal dashboard palette.
Liquid Glass and system accent preferences apply immediately when saved;
AppKit controls retain their standard system appearance.

Docker, disk, and service tables share rounded cards, neutral controls and consistent labels,
status filters, and a live search field. Click a data-column heading to sort;
click it again to reverse the order. Changing a filter, search, or sort returns
the list to the top. Filters and sorting survive refreshes within the session.
Disk searches include partition names and mount paths while retaining the parent
device tree. Inactive or stopped items use neutral status labels; failures use red.

Container and service logs share an inline viewer on the same surface as their
table rows, with consistent spacing, a recent-log header, line count, and Copy
logs control. Log text respects terminal font settings and ANSI colours, and
long output scrolls in both directions inside a bounded panel.

Click a container name or its chevron to inspect its image, state and health,
restart policy, port mappings, mounts, and creation time inside the table. Details
load on demand and offer a refresh control. Unknown values remain unavailable;
exposed ports without a binding are shown as unpublished. Container identities
use full Docker IDs internally, with short IDs in the table. Inspection uses the
same local/SSH Docker transport on every platform.

Use **Apply** to save, **Cancel** to discard edits, or **Restore defaults** to prepare the default settings. Start at login is saved immediately. Preferences use the JSON path above on Linux and `~/Library/Application Support/Crabdash/preferences.json` on macOS. Old startup-only preferences gain defaults automatically. Errors remain visible until dismissed.

The compatibility default is `TERM=xterm-256color` with the bundled JetBrains Mono font. The terminal-type chooser also offers `xterm` and `vt100` for older hosts, and `xterm-ghostty` for hosts with Ghostty's terminfo installed. The selected TERM is sent to both local and SSH pseudo-terminals; Crabdash does not install terminfo on remote machines. See [Ghostty's terminfo guidance](https://ghostty.org/docs/help/terminfo).

The terminal follows new output at the live prompt. Scroll up with the mouse or
Shift+Page Up to read history without new output moving your view. Shift+Page Down
scrolls forward; Ctrl+Shift+Home/End jumps to the beginning/latest output. Click
**Latest output** or type to return to the live prompt. Cursor keys respect terminal
application mode, and paste respects bracketed-paste mode. While the terminal has
focus, shell editing keys such as Ctrl+A, Ctrl+E, Ctrl+U, and Ctrl+W go to the shell.

Drag over terminal text to select it; double-click selects a word and triple-click
selects a logical line, including soft wraps. Holding Alt while dragging selects
a rectangle. Dragging at the grid's top or bottom scrolls through history.
Copy uses **⌘C** on macOS or **Ctrl+Shift+C**/**Ctrl+Insert** on Linux and Windows;
the Edit menu's Copy targets the focused terminal pane. **Ctrl+C** interrupts the
shell on macOS and Linux; on Windows it copies a selection and otherwise interrupts.
Typing or pasting clears the selection. Copy preserves Unicode and soft wraps.

The terminal drawer combines tabs and the grouped **+** and Hide controls in one header row;
the controls stay at its upper-right corner, with `user@host` available in each tab's tooltip. Use **+**
to open another shell, select a tab to switch shells, or close that tab to stop its
session. Drag tabs to reorder them or to a terminal pane's edge to create a split;
drop onto its center to join tabs. Splits stay inside the drawer, and each visible
shell receives its own pane size. Hiding the drawer preserves its sessions and history.
An open drawer follows machine selection, including after adding or deleting a
machine. Returning to a machine restores its existing sessions and splits.
Double-click a tab title to rename that live session. Enter saves the name and
Escape cancels; clearing the name restores the shell's automatic title. These
session names last while the app runs.
Typing and pasting while a shell connects is buffered for that session. A failed
connection or closed session discards its pending input.

Use **Ctrl+Shift+T** (**⌘T** on macOS) to open a terminal session. With a terminal
focused, **Ctrl+Shift+W** (**⌘⇧W**) closes that session and **Ctrl+PageUp/PageDown**
(**⌥⌘←/→**) switches tabs within its pane. These commands also appear in the File
and View menus; session actions are disabled while editing a modal or dragging.

With a terminal focused, **Ctrl+Shift+D** (**⌘D**) opens a new shell in a split
to its right; **Ctrl+Shift+E** (**⌘⇧D**) opens one below. These commands appear
in the File menu. Each split starts an independent session and keeps the original
pane's selected tab. Use **Ctrl+Shift+arrow** (**⌃⌘arrow**) or the Window menu
to focus an adjacent terminal pane. Focus follows the pane geometry and stops at
the drawer's edges.

Drag an empty area of the terminal's upper header or its upper edge to resize it smoothly in pixels;
buttons and tabs retain their own click and drag actions. Split dividers resize
terminal panes. The shell's grid changes only when another complete row or column
fits, while PTY pixel dimensions follow the drawable content and display scale.
Initial panel rows in Preferences set the height until the drawer is resized.
Each workspace remembers its chosen pixel height; smaller windows temporarily
limit it, and enlarging the window restores it. Dragging can use the available
window height.

GNOME requires AppIndicator support to display tray icons. On Fedora, install
`gnome-shell-extension-appindicator` and enable
`appindicatorsupport@rgcjonas.gmail.com` with `gnome-extensions enable`. If the
extension was newly installed, a new desktop login may be needed to load it.

## Windows builds

Native Windows support is experimental until the Windows validation job and device checks
have run. The x86-64 GNU release executable has been cross-built on Fedora with its
application icon, DPI manifest and all sixteen embedded Direct3D shaders. Windows
rendering, terminals, tray and login startup still require device checks. WSL uses
the Linux backend and requires a graphical session such as WSLg; tray and graphical
login startup depend on the session's desktop and systemd capabilities.

Build native `.exe` files on Windows with Rust's `x86_64-pc-windows-msvc` target,
Visual Studio C++ build tools, the Windows SDK's `fxc.exe`, and Zig **0.16.0**. Set
`GPUI_FXC_PATH` to the SDK shader compiler and initialize the Ghostty submodule before
building:

```powershell
git submodule update --init --recursive
cargo build --locked --release --target x86_64-pc-windows-msvc -p crabdash
cargo test --locked --workspace --target x86_64-pc-windows-msvc -- --test-threads=1
```

Linux cross-builds use Rust's `x86_64-pc-windows-gnu` target, MinGW-w64 GCC/binutils
with an MSVCRT-compatible runtime, Zig **0.16.0**, and a host-native **vkd3d 2.1**
shader compiler. Fedora's `mingw64-gcc`, `mingw64-binutils` and `mingw64-crt` provide
the tested C toolchain. Set `GPUI_VKD3D_COMPILER` to the compiler's absolute path;
older vkd3d 1.17 cannot compile GPUI's structured-buffer shaders. Resources follow
the Windows target on every host, and release builds embed Shader Model 4.1 DXBC:

```sh
rustup target add x86_64-pc-windows-gnu
export CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc
export AR_x86_64_pc_windows_gnu=x86_64-w64-mingw32-ar
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
export RC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-windres
export GPUI_VKD3D_COMPILER=/absolute/path/to/vkd3d-compiler
cargo build --locked --release --target x86_64-pc-windows-gnu -p crabdash
```

Distribute the release executable with the bundled IBM Plex Sans font notice.
Debug builds compile shaders from the build checkout at runtime and are intended
for development, rather than distribution.

The **Build and Release** workflow configures MSVC, the Windows SDK and Zig,
builds release shaders and the executable, runs workspace and PTY cleanup tests,
and packages the `.exe` with notices. Windows Preferences use `%APPDATA%/Crabdash/preferences.json`.
Windows login startup uses Crabdash's own value under the current user's
[Run key](https://learn.microsoft.com/en-us/windows/win32/setupapi/run-and-runonce-registry-keys).
Preferences read its command as well as its presence. A different or malformed command
keeps Start at login on with a repair message; turn it off and on to register the current
executable. Reading the setting never rewrites an existing registration. This status
describes the Run registration, not external Windows startup-blocking policies.

Windows service logs show recent Service Control Manager events; they are distinct from
an application's own log files. Docker over Windows SSH uses encoded PowerShell transport
and direct native argument handling.

## Releases

Download platform packages from [GitHub Releases](https://github.com/th0jensen/crabdash/releases).
The release pipeline builds and tests native executables for macOS Apple Silicon and
Intel, Linux ARM64 and x86-64, and Windows x86-64. WSLg uses the matching Linux package.
Linux packages are built on Ubuntu 22.04 and require glibc 2.35 or newer, Wayland or
X11, Vulkan, and the runtime libraries documented in the archive. macOS requires
13 or newer; Liquid Glass is available on macOS 26. Windows requires Windows 10
version 1809 or newer and Direct3D 11. Windows interactive device validation remains
pending.

Every package includes third-party license notices. macOS app bundles are ad-hoc
signed and verified, but are not Developer ID signed or notarized. Gatekeeper may
require **System Settings → Privacy & Security → Open Anyway** on first launch.
macOS packaging rejects dependencies on Homebrew or build-machine dylibs; Linux
packaging checks the declared glibc baseline and resolves its system libraries.

CI runs on pull requests, master pushes, and manual dispatch. Version tags trigger
publication only after all five jobs pass and their packages and receipts are
verified. `SHA256SUMS` covers every release asset. Release notes live in
`releases/<tag>.md`; the tag must match the Cargo workspace and lockfile versions.

## Motivation

Crabdash started as a tool for managing my own homelab machines and containers without constantly jumping between SSH sessions, terminal commands, and the desktop environment on-device. The existing tools were either too heavyweight, web-based, or required running a separate server. Crabdash is a native binary that runs on your machine and talks directly to your infrastructure.

Docker container controls include Start, Stop, Restart, Pause/Resume, logs and
Remove, with Total/Running/Paused/Stopped filters. Run remains available with an
empty list. Removal asks for confirmation, stops active containers gracefully,
and keeps images, volumes and bind-mounted files. Force removal is an explicit
option. Pending actions are tracked separately for each machine.

Run uses compact Container, Network & storage, Environment, and Advanced tabs,
with a collapsible command preview. Its neutral actions match the table controls.
Run creates detached containers, with optional open stdin. The preview quotes
individual arguments; the command field supports shell-style quoting without
shell expansion. Invalid resource limits and conflicting options are rejected
before submission. Docker errors remain in the modal and preserve the form for
retry. Parameter validation lives in `features/docker/run_parameters.rs`, form
state in `run.rs`, presentation in `run_modal.rs`, and CLI dispatch in
`machines/src/docker.rs`; local and SSH machines use the same arguments.
