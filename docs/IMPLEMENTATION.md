# Tend 0.2 implementation

Goal: deliver the approved storage, monitoring, tuning and recovery tools in a lightweight native app. Do not schedule or trigger additional restarts.

## Phases

1. Storage and reporting: cancellable single-filesystem scans, ranked folders/files, development folder reporting, category-based preview/cleanup, local exports.
2. Monitoring: page-scoped CPU/memory/paging/pressure history, process snapshots, failed services and Docker reports.
3. Tuning: startup overrides, optional user-unit enablement, desktop animation/Wayland settings, deferred zram configuration, swappiness and recovery.
4. Delivery: release build, native layout review, documentation, binary-only upgrade preserving all timers, GitHub publication.

## Operation boundaries

- No scan runs automatically when Tend opens. Full scans require an explicit action and can be cancelled.
- Monitoring samples only while its page is selected, without a new service.
- Cache previews are advisory snapshots; Docker determines unused eligibility at execution. Trash is an explicit all-items operation.
- Cleanup preferences govern the manual action. The existing daily maintenance policy remains independent and is displayed separately.
- User-file cleanup runs as the desktop user. Privileged operations accept typed settings and fixed commands. Cleanup and updates share the maintenance lock.
- The manual updater disables unattended-upgrades automatic reboot and never invokes the maintenance service or its restart hook.
- zram settings take effect on a future ordinary boot. No live swapoff, restart, or new timer is requested.
- Performance snapshots provide evidence for comparison; no setting is claimed to improve a workload before it is measured.
- Root installation replaces only app binaries and assets on existing deployments. Existing schedules and restart policy are left untouched.

## Verification scope

Compile and inspect native layouts. Do not execute cleanup, upgrades, process signals, service changes, tuning changes or restarts during delivery. Use isolated filesystem fixtures for destructive-path boundary checks; do not exercise those paths against real user data.

## Delivery record — 2026-10-23

- Phases 1–3 implemented in the Rust UI, user worker and constrained root helper.
- Optimized release build completed with Rust 1.93.1, using one build job.
- Overview, cleanup, performance and schedule pages rendered and inspected. Screenshots are in `assets/`.
- Installer Python sources parsed; repository whitespace check passed.
- No cleanup, upgrade, process termination, service change, tuning change, or reboot action was exercised during delivery.
- Nine library tests passed, including isolated cache-deletion boundaries, replacement symlinks, fresh/external-file preservation, hard-link accounting, cancellation and launcher-section preservation. Fixtures were confined to `/tmp/opencode`. Privileged host actions remain unexercised.
- Existing-install upgrades use `scripts/upgrade.py`, which replaces only binaries and desktop assets. A local delivery record outside the repository tracks unchanged timer/configuration hashes.

## Local Tend authorization

The user requested avoiding repeated password prompts. Optional setup grants the active local desktop user access only to the root-owned Tend helper. It stores no password and leaves other administrator actions under their normal policy. The legacy full-maintenance launcher command was removed from the helper. The installer accepts root-group-writable root-owned directories, while rejecting writable executable files, world-writable paths, symlinks and untrusted ownership. Seven source-predicate cases passed for these installation boundaries.

## Installed delivery

The app, administrator helper and user worker were installed and their hashes matched the optimized release binaries. The installed helper's read-only authorization check succeeded without an authentication dialog. The installed UI launched successfully. Schedule, reminder and restart-hook hashes matched the pre-upgrade baseline. No cleanup, package update, tuning change or machine restart was performed during delivery. The user-specific optional authorization rule stores no password.

## Windows 11–inspired Task Manager

Replaces the original monitor/process pages with a native two-tab Task Manager. The virtualized process table supports search, numeric column sorting, user filtering, details and protected termination controls. Resource selection shows 60-sample CPU/memory/disk/network graphs and readable kernel GPU counters. Physical network adapters sort before virtual adapters. Sampling defaults to two seconds with pause/1/2/5-second controls and stops off-page. Collection uses one worker at a time; unavailable counters remain explicit. Sensor/pressure/paging diagnostics remain available in an expander. Twelve library tests passed, including counter resets, network field parsing and live collector sanity. Native process and performance layouts were rendered for review.

## Password prompt correction

Routine updates now use `scripts/install-user.py`: the UI lives at `~/scripts/tend`, and a per-user desktop launcher overrides the system launcher under the same dock identity. `scripts/install.sh` builds only the interface and never invokes pkexec/sudo. The already-installed root helper and its scoped authorization remain separate. Missing helper prerequisites produce an error, not an automatic authentication prompt. This prevents repeated interface-install prompts without storing a login password or granting an editable installer general administrator access.
