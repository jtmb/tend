# Tend 0.2 implementation

Goal: deliver the approved storage, monitoring, tuning and recovery tools in a lightweight native app. Do not schedule or trigger additional restarts.

## Phases

1. Storage and reporting: cancellable single-filesystem scans, ranked folders/files, development folder reporting, category-based preview/cleanup, local exports.
2. Monitoring: page-scoped CPU/memory/paging/pressure history, process snapshots, failed services and Docker reports.
3. Tuning: startup overrides, optional user-unit enablement, desktop animation/Wayland settings, deferred zram configuration, swappiness and recovery.
4. Delivery: release build, native layout review, documentation, binary-only upgrade preserving all timers, GitHub publication.

## Operation boundaries

- No scan runs automatically when Tend opens. Full scans require an explicit action and can be cancelled.
- Monitoring samples only while its page is selected and Tend is focused, without a new service.
- Cache previews are advisory snapshots; Docker determines unused eligibility at execution. Trash is an explicit all-items operation.
- Independent typed policies govern manual/user-cron and daily maintenance cleanup. Each has its own editor; cleanup saves cannot change schedules or restart settings.
- User-file cleanup runs as the desktop user. Privileged operations accept typed settings and fixed commands. Cleanup and updates share the maintenance lock.
- The manual updater disables unattended-upgrades automatic reboot and never invokes the maintenance service or its restart hook.
- zram settings take effect on a future ordinary boot. No live swapoff, restart, or new timer is requested.
- Performance snapshots provide evidence for comparison; no setting is claimed to improve a workload before it is measured.
- Routine interface installation replaces user binaries/assets. The narrow cleanup-policy installer additionally patches supported existing engine code, preserving schedules and restart policy.

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

Routine updates now use `scripts/install-user.py`: the UI lives at `~/scripts/tend`, and a per-user desktop launcher overrides the system launcher under the same dock identity. `scripts/install.sh` builds the interface and user worker and never invokes pkexec/sudo. The already-installed root helper and its scoped authorization remain separate. Missing helper prerequisites produce an error, not an automatic authentication prompt. This prevents repeated interface-install prompts without storing a login password or granting an editable installer general administrator access.

## Cleanup rules, cron, dock and logs — 2026-10-23

- Separate typed folder-rule policy avoids changing the strict schema consumed by the already-installed privileged helper. Filename matching, age, ownership, protected paths, project ancestry, same-filesystem traversal and descriptor-based deletion constrain custom cleanup. Existing fixtures exercise the shared deletion boundary.
- User cron manages only a marked block. Validation accepts numeric five-field expressions, with conventional day-of-month/day-of-week matching. A user lock serializes schedule edits/manual cleanup/scheduled cleanup. Persistent latest-occurrence state bounds retries and prevents replay of completed runs. Catch-up polls every 15 minutes and at boot; a 20-minute startup grace applies. New schedules remain disabled until explicitly enabled and saved. Existing root maintenance integration is untouched.
- Close-to-dock minimizes the retained native window. Explicit Show/Hide/Quit application actions and a desktop Quit action provide recovery and exit. Resource collection pauses off-page or without focus and resumes on return. No additional daemon or login service is installed.
- Log reader fetches limited journal fields and at most 1,000 records on demand. Crash mode also reads up to 256 KiB of selected headers per readable Apport report, excluding symlinks and crash memory/environment. Filters are local; exports are explicit. Neither collection nor filtering requests root access.
- Delivery uses the user-only installer for the interface and worker. No root configuration, existing cron jobs or reboot policy is changed; no cleanup is executed on real user data.

### Verification of the additions

All 16 library checks passed, including cron field validation, managed-block preservation, structured crash identification and the existing descriptor-based cleanup boundaries. Native System logs, Cleanup scheduling and Disk cleanup layouts rendered successfully. A read-only integration check exercised current system/user/crash journals, bounded newest-first decoding, latest-due catch-up, protected roots and exclusive cleanup locking. No real cleanup or package update was performed.

The optimized UI and user worker were installed with matching release hashes, without sudo or pkexec. The installed application retained its process after its Close-to-dock action and accepted Show. The explicit installed desktop Quit action exited it successfully; the current UI was then reopened. Existing maintenance/restart configuration hashes and unrelated user cron jobs were preserved. The new user cleanup schedule remains disabled.

## Design and window-manager Quit correction — October 23, 2026

The former close-request handler intercepted GNOME dock Quit as well as title-bar close. Optional minimize behavior now lives only on the custom title-bar close button, alongside native minimize/maximize controls. Ordinary window-manager close and Alt+F4 follow GTK’s exit path; explicit Quit remains available. The Hide action minimizes directly.

The shared design pass groups sidebar navigation with symbolic icons, tightens hierarchy and spacing, uses consistent card geometry, constrains wide-page content, wraps workbench actions and gives the log reader useful empty states. A shared presentation module renders human dates in America/Toronto, shortens routine timestamps and supplies full tooltips/details. Next-maintenance time uses a structured Unix timestamp from systemd, without raw systemd date strings. History and scheduled-cleanup results no longer display raw epochs/JSON timestamps. Apport details distinguish the source’s timezone-unspecified crash date from the file update time.

All native pages are rendered in separate preview instances. The optimized update is installed without reloading the running user app or restarting any other app, service or machine. Private diagnostic observations are stored outside the public repository.

A separate temporary build of the current UI, using the same window close path and a non-unique application instance, exited normally after a window-manager-style close request within the 15-second bound. This exercises the path that previously minimized instead of exiting. The pre-existing user Tend process remains running with its original PID. Fourteen page layouts were rendered without invoking cleanup, updates or tuning actions. Build jobs use low CPU/I/O priority for final delivery.

Final delivery: optimized user binaries match the installed copies; the original Tend PID and maintenance/restart configuration hashes remain unchanged. A standalone probe of the shared presentation source verified Today/Tomorrow, Toronto timezone conversion, the scheduled 8:00 AM time and full second precision in details. No administrator authentication was opened. Private slowdown findings remain in the local performance report, outside GitHub.

## Independent cleanup policies and reliability — October 23, 2026

Manual/cron policy v1 preserves legacy `age_days` as the build-cache age and as
the fallback for previously shared image/thumbnail ages. New independent fields
are saved only when the user saves the editor. Daily policy v1 is stored in
`/etc/tend/cleanup-policy.json`; it has separate categories, Docker ages/allowance,
npm log age and verification interval. Both schemas reject unknown fields,
invalid types/ranges and unsupported versions. Invalid saved configurations fail
closed. Folder-rule and schedule read errors remain visible instead of being
silently replaced by empty/default configuration.

The daily adapter uses one validated snapshot per maintenance invocation. Disabled
phases report an intentional skip; update guards, boot grace, retry budget and
health reporting retain their existing ownership. User cleanup phases execute
through the root-owned worker as the desktop user, sharing the manual/cron lock.
Daily npm targets only expired top-level regular `*.log` files. Config saves share
cleanup locks. Docker commands have fixed arguments and independently tested
retention values. Root helper errors, including APT and worker failures, propagate
to the interface.

Native editors expose save/discard/defaults and dependent controls. Preview
completion is accepted only for the matching editor revision; another rule-file
revision is checked again before execution. Confirmation is required when saving
an enabled automatic policy or expanding its folder rules. Defaults are edit-only
until saved. System policy saves have a dedicated helper action and never call
the schedule/reboot apply action. A protected capability marker gates the new
system interface on older installations. Privileged UI actions check existing
authorization without interactive authentication before invoking the helper.

The narrow installer defaults to dry-run, validates the supported installed engine
layout, compiles its patch, takes the maintenance lock for system application,
backs up files, verifies checksums and installs the capability marker last. Its
rollback restores backed-up contents and renames newly introduced files aside.
It never invokes systemctl, maintenance, reboot, or login integration. Root
deployment remains an explicit administrator handoff when prompt-free authority
is unavailable. The user interface installer remains separate.

Verification uses Rust regression tests, a shared Rust/Python schema contract,
mocked daily commands and integrated engine guards, plus native interaction and
layout capture. The documentation feature is excluded from release builds. Its
public GIF runs real read-only preview/filter callbacks with labeled fixture data
and verifies input hashes. The native QA mode intentionally edits fixture policy
files and cancels confirmations; it never activates installed cleanup or schedules.
The legacy test that actually deletes an isolated fixture is now ignored by
default for preview-only verification. Privileged/destructive host execution is
not claimed as live-tested.

Final validation: 26 Rust tests passed, with the isolated deletion test excluded;
11 Python tests passed, including full staged install/repeat/rollback and unsafe
path rejection and a real worker scheduled dry-run against isolated files. Native checks covered persistence, stale previews, independent
policies, cancelled confirmations and malformed files; 39 layout captures were
reviewed, with final error/status placement re-rendered. Four worker guard calls
rejected invalid/disabled operations in an isolated home. The 38-second public
GIF (about 0.79 MiB) passed all six input-preservation checks. User binaries were
installed without closing the running instance. System deployment remains the
documented administrator handoff; destructive/privileged host execution was not
exercised. Maintenance, cron, login and restart settings were preserved.
