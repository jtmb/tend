<h1 align="center"><img src="assets/io.github.jtmb.Tend.svg" width="38" height="38" alt=""> Tend</h1>

<p align="center"><strong>Disk cleanup, live performance, and readable system logs.</strong><br>Native Rust · GTK 4 &amp; libadwaita · Ubuntu / GNOME · <a href="LICENSE">MIT</a></p>

<p align="center"><a href="#install">Install</a> &nbsp;·&nbsp; <a href="#features">Features</a> &nbsp;·&nbsp; <a href="docs/USER_GUIDE.md">User guide</a> &nbsp;·&nbsp; <a href="https://github.com/jtmb/tend/issues">Issues</a></p>

<p align="center"><a href="assets/demo.gif"><img src="assets/demo.gif" width="1100" alt="Tend in use: run a cleanup preview, inspect eligible files, follow live performance, load logs, filter errors, search editor and inspect an event"></a></p>

<p align="center"><sub>A working demo: cleanup preview → live charts → log filters → event details.<br>Isolated sample files and logs; real preview and filter code. Nothing deleted or restarted.</sub></p>

## Features

| Storage & cleanup | Task Manager | Logs & crashes |
| :--- | :--- | :--- |
| Find large files and development directories. Set separate daily and manual/cron policies with per-category ages and cache limits. Preview eligible files and add custom folder rules. | Search and sort processes. Follow CPU, memory, disk and network graphs. Inspect a process and export a snapshot. | Load recent events and crash reports. Narrow by time, severity or text. Select an event for details and export filtered results. |

<p><a href="assets/task-manager.png"><img src="assets/task-manager.png" width="49%" alt="Tend Task Manager with CPU history and resource selection"></a> <a href="assets/cleanup.png"><img src="assets/cleanup.png" width="49%" alt="Tend manual and cron cleanup categories with independent retention controls"></a></p>

<details>
<summary>More screenshots — system logs and performance settings</summary>
<br>
<p><a href="assets/system-logs.png"><img src="assets/system-logs.png" width="49%" alt="System log reader with source, severity and text filters"></a> <a href="assets/performance.png"><img src="assets/performance.png" width="49%" alt="Desktop and memory settings in Tend"></a></p>
</details>

### More tools, when you need them

- **Developer maintenance** — Docker cache and log reporting, npm cache verification and Python cache review.
- **Desktop & memory controls** — login applications, animations, Wayland launcher overrides, swappiness and future-boot zram settings. Supported changes can be restored through Recovery.
- **Independent cleanup policies** — separate daily-maintenance and manual/cron settings, with a persistent Save changes / Discard footer, preview invalidation and readable policy errors.
- **Cleanup schedules** — daily, weekly or custom cron schedules, missed-run catch-up and saved results. New schedules start disabled.
- **Top-panel access** — open Tend or quit from its panel icon. Close the window to keep it available without a dock window.

### Small by design

Real GTK widgets, with no embedded browser, telemetry or advertising. Opening Tend does not start a folder scan, Docker query or package refresh. Task Manager samples every two seconds by default while selected and focused; collection pauses when you leave it. Process lists are virtualized, charts keep 60 samples and log reads are bounded.

The title-bar close button can keep Tend in Ubuntu’s top panel. Its menu offers **Open Tend** and **Quit Tend**. If no panel host is available, closing minimizes to the dock. **Quit Tend**, dock Quit and Alt+F4 always exit; live collection pauses when hidden. Scheduled maintenance runs independently of the window.

## Install

**Tend 0.2 currently integrates with an existing Ubuntu/GNOME `dev-maintenance` installation and its trusted helper.** It does not yet provision that engine on a fresh machine. Read the [integration prerequisites](docs/USER_GUIDE.md#install-or-upgrade) before installing.

Requires Rust, GTK 4.12+ and libadwaita 1.5+ development packages.

```sh
sudo apt install cargo rustc libgtk-4-dev libadwaita-1-dev pkg-config
git clone https://github.com/jtmb/tend.git
cd tend
./scripts/install.sh
```

Daily policy editing requires the [cleanup integration update](docs/USER_GUIDE.md#daily-policy-integration) on existing installations. The editor shows its status; routine interface installation does not replace the system helper.

Open **Tend** from the application launcher; its icon stays in the top panel while it is running. Routine interface updates do not invoke sudo or pkexec. See the [user guide](docs/USER_GUIDE.md) for helper authorization and recovery.

<details>
<summary>Updating an existing installation</summary>

```sh
git pull --ff-only
./scripts/install.sh
```

Choose **Quit Tend**, then reopen the app to use the new binary. Installation does not enable cleanup schedules or change the existing maintenance/restart policy.

</details>

## You choose what changes

Cleanup starts with a preview and confirmation; deletion is permanent. Docker cleanup preserves containers, networks, volumes and tagged images. Folder rules exclude symlinks, other filesystems and protected repository/dependency paths. Tuning benefits should be measured on your workload.

Tend's manual cleanup and separate user cleanup schedules do not request a reboot. The existing system maintenance engine has its own configured restart policy. Zram changes take effect at a future ordinary boot. Tend never stores your login password.

## Documentation & contributing

[User guide](docs/USER_GUIDE.md) · [Implementation](docs/IMPLEMENTATION.md) · [Record the demo](docs/demo/README.md) · [Source](src/) · [Report an issue](https://github.com/jtmb/tend/issues)

Focused fixes and bug reports are welcome. Include your Ubuntu/GNOME versions, installation method and reproduction steps. Review logs before sharing; they can contain private paths and app names. Keep changes native, responsive and modest in resource use.

[MIT licensed](LICENSE) · Free and open-source software.
