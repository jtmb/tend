# Twelve additions, one native app

These features share Tend's Rust/GTK interface and one demand-driven sampler.
They add only two navigation destinations: **Diagnostics** and **Workspaces**.
The expansion is undergoing release verification; source implementation is not
proof that matching system integration is installed. See the
[completion ledger](EXPANSION_PLAN.md) for current gates and evidence.

## Find evidence of a slowdown

**Task Manager** adds app groups and expandable process trees alongside the
existing resource graphs. Search, sort and inspect individual processes. Process
identities include boot ID and start time so a reused PID cannot inherit an old
observation. Group totals are observations, not permission to terminate a group.

**Diagnostics** records an explicitly started workload, at two-second intervals
by default, for up to fifteen minutes. Add markers when starting a build or
noticing a stall. Finish, save and reopen a session; compare two saved sessions
using CPU, memory, paging and pressure measurements. Missing counters, sampling
gaps and different workload context remain visible. A correlation is not a
proven cause, and low idle CPU alone does not prove better responsiveness.

After an application is stopped for low memory, open **Diagnostics → Memory
failures and OOM kills → Explain recent memory failures**. This requested report
reads the last 24 hours of available kernel evidence and shows the killed
process, historical RAM/swap figures, affected scope and other recorded resident
sets. It distinguishes whole-machine exhaustion from a recorded cgroup limit.
Missing context remains unknown; current free memory can recover after a kill.
The victim is not automatically the application that caused all the pressure,
and this report cannot reconstruct a vanished command line or prove a leak.

**System logs → Last crashed apps** also includes kernel OOM kills without core
dumps. Activity places them under **Pressure & diagnostics**, with result
**OOM kill**; the **Failed** result filter includes them. Optional crash alerts
use the same bounded incident reader without adding another polling schedule.

## See what grew and which project owns it

**Storage** can record metadata snapshots of a requested folder and show added,
removed and changed files. It does not read file contents, hash them or install a
filesystem watcher. Unusual filenames are retained losslessly. Partial,
cancelled, quota-limited or inaccessible scans do not replace the last complete
baseline or turn unreadable files into false removals.

Two complete snapshots per root and thirty days of changes are retained by
default. **History limits**, available from Storage, Activity and Preferences,
sets the private retention and shared budget: 512 MiB by default, configurable
from 128–2048 MiB. Database sidecars and staging count toward the budget. New
history writes pause below 2 GiB free on its filesystem. Changing limits does
not start a scan or cleanup.

**Workspaces** registers project folders without executing project code. It
shows conservatively associated processes, cached storage results and explicitly
requested local Compose metadata. Ambiguous mappings are labelled; remote Docker
metadata does not prove that a local folder owns those resources.

## Review cleanup and protect paths

**Additional cleanup rules** is expandable on Disk cleanup, with a visible saved/enabled count. Open it to add, edit, remove or reload folder rules; saving a rule is separate from the page footer.

**Disk cleanup** explains the selected files, eligibility, expected effects and
estimated space before confirmation. Page through the complete selection. A
policy edit, expired preview or changed file identity invalidates or skips the
selection rather than silently deleting a replacement.

**Global cleanup exclusions** protect paths across Tend's manual cleanup,
existing user cleanup schedules and daily maintenance. They require matching
administrator, worker and daily integration. The editor reports availability;
reviewing an unavailable policy does not activate protection. Malformed or
unreadable installed protection stops cleanup.

Bulk actions that cannot honor path exclusions are skipped with a reason. These
protections cover Tend's executors; they do not reconfigure Ubuntu tmpfiles,
Snap maintenance or another program's cleanup policy. Exclusions cannot restore
files already removed. Saving a cleanup policy is separate from running it.

## Follow maintenance and incidents

**Activity** combines available maintenance outcomes, saved diagnostics, private
reports and requested readable crash records into a filterable, paginated
incident timeline. Open details, follow the relevant logs and export locally.
Unknown earlier history is labelled instead of reconstructed.

**Maintenance history** records outcomes, duration, attempts, deferred/catch-up
runs and observed net space changes. Net free-space change includes concurrent
writes and can differ from a cleanup estimate. Durable operation receipts keep
a history-write failure from replaying completed maintenance; uncertain outcomes
require review. This adds reporting to the existing engine, not new schedules.

## Opt into alerts and use the top panel

Open **Preferences → Health monitoring** to configure CPU, available memory,
root free space and memory/I/O pressure conditions. Monitoring starts **off**.
It uses a fifteen-second shared sample while Tend runs, including while hidden.
Configure sustained-condition duration, repeat cooldown and recovery notices;
optional crash checks read bounded recent metadata every five minutes.

Snooze notifications for an hour or resume them from the panel. Missing readings,
suspend gaps and clock changes cannot imply recovery. Notifications and local
history can fail independently; the status reports delivery/history problems.

The top-panel menu provides current/off/snoozed/stale status and shortcuts to
Diagnostics, System logs, Activity and Storage. Closing the window can leave
Tend available there. **Quit Tend stops all collectors.** No monitoring daemon,
autostart or new scheduled job is installed.

## Move settings with review

**Preferences → Settings transfer** exports a versioned owner-only file with ten
supported sections. Imported settings are compared and loaded as inactive
drafts. Review and save each section separately. Monitoring, maintenance,
cleanup schedules and folder rules begin disabled in imported drafts. Existing
active jobs are unchanged until a reviewed save.

Credentials, restart/login/authentication policy and raw diagnostic history are
excluded. Exported project and protected paths can still be private information.
See [Settings transfer](SETTINGS_TRANSFER.md) for exact limits and workflows.

## Boundaries and delivery

Opening these tools does not run cleanup, updates or a restart. Their release
checks use retained fixture data, private D-Bus sessions and isolated command
contracts. They do not close existing applications or change the existing
restart/login/authentication policy. Matching system integration is deployed
separately from the user interface; unavailable controls explain what is missing.
