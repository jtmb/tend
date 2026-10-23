# Tend

**Native Ubuntu storage, monitoring and performance settings.** Written in Rust with GTK 4 and libadwaita.

Scan disk usage, preview cleanup, inspect resource pressure, manage startup entries and adjust supported desktop and memory settings. Open Tend from the dock when you need it. No embedded browser, telemetry, advertising or resident Tend daemon.

![Tend overview](assets/overview.png)

## Features

| Page | Controls and reports |
| --- | --- |
| **Overview** | Free disk space, CPU/memory/swap snapshot, maintenance status and recent results. |
| **Storage** | Cancellable folder scans; allocated size, largest files, and development directories; open locations in Files. |
| **Disk cleanup** | Category selection, preview, age/cache allowances, execution results and free-space change. |
| **Task Manager** | Windows 11–inspired Processes and Performance views: live searchable/sortable process table; CPU, memory, per-disk and per-adapter graphs; available GPU counters; process controls, update speed and snapshot export. |
| **Startup & services** | Login application overrides, optional local user-service enablement, boot timing and last session-restoration report. |
| **Performance settings** | Desktop animations, Ubuntu display controls, ChatGPT/VS Code Wayland launchers, swappiness and zram configuration. |
| **Development tools** | Docker storage/resource reports, local JSON log sizes and limits, pip cache report and npm cache verification. |
| **Updates & reports** | Pending package list, independent stable-update action, system health, history and text/JSON export. |
| **Recovery** | Restore saved launcher/startup/animation/service settings and the previous memory configuration. |
| **Schedule / Activity / Appearance** | Existing daily maintenance schedule, warning notifications, journal and system/light/dark appearance. |

<details>
<summary>Disk cleanup</summary>

![Disk cleanup settings](assets/cleanup.png)

</details>

<details>
<summary>Performance settings</summary>

![Performance settings](assets/performance.png)

</details>

## Task Manager

![Task Manager performance view](assets/task-manager.png)

Processes includes name, CPU, resident memory, physical disk I/O, PID, user and state. Search by name, PID or user; click headers to sort. CPU is a share of total machine capacity. Memory includes shared pages, so adding process RSS can exceed physical RAM usage. Restricted counters display an em dash. Other users’ processes are read-only; desktop services and Tend itself are protected from termination.

Performance shows CPU, memory, physical-disk active time and read/write rates, and receive/send rates per network adapter. Physical network adapters appear before virtual Docker adapters. GPU usage appears only when the kernel exposes a readable utilization counter. Linux does not provide per-process network rates through these counters, so network traffic is shown per adapter. An expandable section retains paging, pressure, battery and temperature readings where available. Export explicitly saves the current resource/process snapshot locally.

## Low overhead

- No folder scans, Docker queries or package refreshes at startup.
- Task Manager samples every two seconds by default **only while its page is selected**, with pause/1/2/5-second controls. Resource graphs retain 60 samples in memory. Closing the window stops sampling.
- Process tables use native GTK list virtualization and sortable columns. Collection runs on a worker thread without overlapping samples. Expensive reports refresh on request. Scans run on a lower-priority worker thread with progress and cancellation.
- Scans skip symlinks and other filesystems, count allocated blocks once per inode, and report incomplete results if cancelled or their safety limits are reached.
- Tend uses systemd for the existing scheduled maintenance. Upgrading Tend does not create a new timer or change that schedule.

The interface can report pressure and compare saved observations; it does not claim a tuning change has improved a workload merely because a number changed.

## Cleanup policy

Preview and confirmation are required in the UI. **Clean selected** never invokes updates or the maintenance reboot hook.

| Category | Manual action |
| --- | --- |
| APT | Remove obsolete downloaded packages with `apt-get autoclean`. No autoremove. |
| Docker build cache | Prune unused cache older than the selected age, with a selected storage allowance. Defaults: 14 days / 5 GB. |
| Docker images | Remove dangling images older than the selected age. |
| npm diagnostic logs | Remove regular files older than 30 days from `~/.npm/_logs`. |
| Pip cache | Optional purge of downloaded packages and wheels in `~/.cache/pip`. |
| Thumbnails | Optional removal of old regular files in `~/.cache/thumbnails`. |
| Trash | Optional permanent emptying of desktop Trash, including mounted-volume trash supported by GIO. |

Containers, volumes, networks, tagged images, repositories, virtual environments and installed project dependencies are not cleanup targets. Development directories appear in storage reports only. Cached packages may need downloading again after cleanup.

Docker’s inventory is not an exact deletion estimate: the daemon decides unused eligibility at execution. The final root free-space difference includes concurrent writes and may be negative. User caches on another mount do not contribute to the root filesystem’s delta.

Manual cleanup preferences are saved independently from the **existing daily maintenance policy** shown under Appearance. Ubuntu’s existing temporary-file cleanup, journal limits, log rotation, Snap maintenance and SSD trimming remain in place.

Implementation follows the separate [Docker build-cache pruning](https://docs.docker.com/reference/cli/docker/builder/prune/) and [image pruning](https://docs.docker.com/reference/cli/docker/image/prune/) controls. User-file cleanup runs as the desktop user, with directory-descriptor checks to avoid following replacement symlinks.

## Tuning and recovery

- **Animations:** saves the old GNOME value before applying a change.
- **Display scaling:** opens Ubuntu Displays, which owns monitor compatibility and its revert confirmation.
- **Native Wayland:** creates a user override for the main ChatGPT or VS Code launcher. It takes effect after the application next fully exits and starts. Check input, clipboard, dialogs and suspend/resume during ordinary use; restore the override if compatibility worsens. Tend does not close these applications.
- **Startup entries:** creates per-user overrides. Disabling an entry affects future login; it does not stop the running application.
- **User services:** enable/disable optional regular unit files installed in `~/.config/systemd/user`. Core desktop and maintenance units are excluded. No start/stop command is issued.
- **Swappiness:** applies the chosen value immediately and saves a boot-time setting. The UI accepts 0–100; the baseline control value is 60.
- **zram:** offers LZ4/Zstandard and a 512–8192 MiB cap, using `min(ram / 2, cap)` at priority 100. Configuration takes effect at the next ordinary boot. Disk swap remains available. No live swapoff or restart is performed. Ubuntu’s generator can be installed separately from the page if missing. See the [generator configuration reference](https://github.com/systemd/zram-generator/blob/main/man/zram-generator.conf.md).

Recovery restores individual user changes; undo repeated edits to the same setting in reverse order. Memory recovery restores the settings saved immediately before the last memory change. **Deletion cannot be undone.** Process termination is also not reversible, and even a normal termination request can lose unsaved work.

## Updates and restarts

The independent update action refreshes APT metadata and uses the configured unattended-upgrades repository policy. It retains package holds and installs only eligible updates. It requires AC power, shares the maintenance lock, and disables unattended-upgrades’ automatic reboot for that run. It does not call the scheduled maintenance service or its post-maintenance restart hook.

The **existing** scheduled maintenance policy is separate. Schedule still displays its restart setting so it is visible and editable. Merely opening, upgrading or using Tend’s new tools does not change it. Tend never reads or stores your keyring password.

## Install or upgrade

Tend currently integrates with an existing `dev-maintenance` deployment on Ubuntu/GNOME. It is not a general-purpose maintenance-engine installer for a fresh computer. Required integration:

- `/usr/local/lib/dev-maintenance/dev-maintenance.py` and its systemd service/timer;
- the existing session-restoration helper for restoration reports;
- Polkit and a GNOME session for native administrator prompts and desktop preferences.

Requires Rust, GTK 4.12+ and libadwaita 1.5+ development packages:

```sh
sudo apt install cargo rustc libgtk-4-dev libadwaita-1-dev pkg-config
git clone https://github.com/jtmb/tend.git
cd tend
./scripts/install.sh
```

The build uses one job at a time. Regular installation and updates replace only the interface under `~/scripts/tend`, its user desktop launcher and its user icon. **They do not invoke sudo or pkexec and do not open password dialogs.** The dock keeps the same application identity. Backups of replaced user files are kept beside them.

The trusted system helper must already be installed. If it is missing, the installer stops with an explanation instead of opening an authentication prompt. A first-time administrator setup for the existing maintenance deployment can be performed explicitly with `pkexec /usr/bin/python3 "$PWD/scripts/install.py"`; this is separate from routine interface updates. Helper changes also require an explicit administrator-managed upgrade, and are never automatically attempted by the user installer.

Close and reopen Tend to use an interface update. Open it from the dock or application launcher.

## Remember Tend authorization

To let Tend’s supported system actions run without repeated password prompts for your active local desktop account:

```sh
pkexec /usr/bin/python3 "$PWD/scripts/upgrade.py" --remember-authorization
```

Ubuntu requires administrator authentication once to install this rule. No login password is stored. The rule grants only the installed, root-owned Tend helper permission; it does not authorize shells, Python scripts, arbitrary commands or other administrator applications. The helper has no command to launch the full maintenance service or its reboot hook.

Remove `/etc/polkit-1/rules.d/49-tend-local-user.rules` as administrator to restore normal prompts, or uninstall Tend. Anyone controlling your unlocked desktop can use these Tend actions under this policy.

## Data and permissions

| Location | Contents |
| --- | --- |
| `/etc/tend/settings.json` | Existing maintenance schedule settings. |
| `~/.config/tend/` | App appearance and manual-cleanup preferences. |
| `~/.local/state/tend/` | Local reports and user-setting recovery history. |
| `~/Documents/Tend reports/` | Explicitly exported text/JSON reports. |
| `/etc/tend/memory-backup.json` | Previous memory settings saved by the administrator helper. |
| `/etc/sysctl.d/99-tend-memory.conf` | Tend’s swappiness override, if applied. |
| `/etc/systemd/zram-generator.conf.d/99-tend.conf` | Tend’s deferred zram override, if applied. |

Reports can include paths, process names, container names and package versions. Export/copy is explicit; no report is uploaded automatically. Review reports before sharing.

Root operations accept constrained typed settings and fixed commands. File scans and ordinary reports need no administrator access. The worker refuses to clean user files as root. Process actions use kernel process handles to avoid signalling a different process after PID reuse.

## Development

```sh
cargo fmt
cargo build --locked -j 1
cargo build --release --locked -j 1
```

| Source | Responsibility |
| --- | --- |
| `src/main.rs` | Existing schedule, activity, appearance and app shell. |
| `src/workbench.rs` | Storage, tuning and recovery pages. |
| `src/task_manager.rs` | Native Task Manager tables, controls and resource graphs. |
| `src/task_stats.rs` | Read-only process, disk, network and GPU counters. |
| `src/workstation.rs` | Scans, reports, cache operations and process counters. |
| `src/preferences.rs` | User preferences, startup entries, launcher overrides and recovery. |
| `src/bin/tend-admin.rs` | Constrained privileged operations. |
| `src/bin/tend-worker.rs` | User-level cleanup worker. |

The release uses size optimization and stripped binaries. The [implementation notes](docs/IMPLEMENTATION.md) document phases and operation boundaries. An optional GitHub Actions example remains in `docs/github-actions-check.yml`.

## Uninstall

Use Recovery first if you also want to undo tuning changes. The uninstaller preserves reports, user preference history and separately applied tuning settings.

```sh
pkexec /usr/bin/python3 "$PWD/scripts/uninstall.py"
```

Remove the user interface first with `rm -- "$HOME/scripts/tend" "$HOME/.local/share/applications/io.github.jtmb.Tend.desktop" "$HOME/.local/share/icons/hicolor/scalable/apps/io.github.jtmb.Tend.svg"`. The administrator uninstall then restores the original maintenance integration and schedules. Unpin Tend from the dock afterward.

## License

[MIT](LICENSE).
