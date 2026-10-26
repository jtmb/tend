# Expansion checkpoint — phases 1–3 (partial)

Source branch: `codex/tend-expansion`, starting from `06a6aae`.
The expansion remains in progress. No release, installed binary, real cleanup,
maintenance schedule, restart/login/authentication setting or existing running
application was changed in this checkpoint.

## Implemented

- One demand-driven worker shared by Task Manager and explicit diagnostic
  recordings. Event-driven GTK delivery and one latest-value slot per consumer.
  Foreground views release their subscription on hiding/leaving. Quit stops the
  collector. No automatic background monitoring is enabled.
- Process identity includes boot ID, PID and start ticks. App names use lazily
  read desktop files; generic interpreters retain distinct process families.
  Apps and Process tree use native virtualized expandable tables, search, user
  filters and links to existing per-process details. No app-group termination.
- Explicit two-second diagnostic recordings, maximum fifteen minutes, with
  bounded samples and workload markers. Saved local recordings can be compared;
  short runs and changed context are disclosed. Unavailable counters stay
  unavailable. No causation or responsiveness improvement is inferred.
- Private lazy SQLite history, keyset event pagination, record/event/database
  budgets, retention, low-space guard and preservation of unreadable databases.
- Read-only worker `capabilities`, `observe` and `history` operations. Helpers
  explicitly report exclusions and structured maintenance events unsupported
  until those later phases exist.

## Evidence

- Rust: 54 library tests plus one panel protocol test passed; one legacy fixture
  deletion test intentionally ignored.
- Python: 28 sandbox/contract tests passed; one mocked desktop notification
  skipped because the host blocks mock execution. No live update/cleanup/reboot.
- Existing native feature matrix passed in retained fixture
  `/tmp/opencode/tend-workflow-839qu453`.
- Expansion native interaction evidence and narrow/desktop captures:
  `/tmp/opencode/tend-workflow-uavo2pch/expansion`.
  Both tree views expanded, filtered and opened process details. Two real sampled
  recordings were marked, finished, saved and loaded. Comparisons disclosed
  insufficient evidence; a discarded recording did not change saved history.
- The first native trial caught a reentrant mutable state borrow when GTK emitted
  a resource selection signal. Both sample delivery paths now release the
  exclusive borrow before updating widgets; the same workflow passes.
- A ten-minute collector budget probe is recorded separately. It is not a
  substitute for final optimized whole-app measurements or million-entry storage
  tests. The implementation ledger retains those gates as outstanding.

## Storage-history checkpoint

- A descriptor-anchored traversal records metadata without following symlinks,
  loading file contents, or maintaining filesystem watchers. SQLite batch and
  page sizes are bounded; hard-link accounting is deduplicated.
- Two complete snapshots per folder are retained, plus thirty days of metadata
  changes. Partial, cancelled, lock-contended or quota-failed scans preserve the
  previous complete baseline and cannot report false removals. Unusual filename
  bytes are stored losslessly and displayed with hexadecimal detail if needed.
- Native Storage tests created a baseline, moved a retained fixture file outside
  the scanned root, scanned again, loaded both changes and inspected details.
- The ten-minute collector probe measured 0.082% of one core and 0.58 MiB extra
  RSS. A corrected stop probe waits for an in-flight read to settle, then checks
  a whole subsequent interval; it passed. Full-app gates remain outstanding.
- The million-entry generated index completed in 91.5 seconds in a development
  build, with 5.8 MiB additional peak RSS and a 402 MiB database including
  sidecars (2048 MiB test budget). It is not a filesystem traversal speed
  measurement. The first run exposed a missing join index; the corrected query
  uses an indexed lookup on both snapshot and path.
- Persistent filesystem UUIDs guard comparisons against recycled device/inode
  values. If unavailable, identity is scoped to this boot and comparisons reset
  conservatively after reboot. Partial or replaced roots cannot be promoted.
- Snapshot promotion atomically retires the older baseline. Deferred compaction
  failures retain two published baselines, preserve change history, report a
  warning and retry during the next requested scan.
- Workspaces support reviewed add/edit/remove drafts, Save/Discard, a finite
  two-second shared-collector burst, cached scan reporting, navigation to Storage
  and explicitly requested local Compose label inspections. Matching uses cwd or
  guarded ancestry, and longest-root precedence; remote/container-label-only
  associations are never treated as resource ownership.
- Native workspace CRUD, live cwd matching, fixture Compose inspection, cached
  scan and navigation pass. Sandbox Docker contracts verify context/ps/inspect
  only, no environment/argument inspection, bounded output/time, failure states
  and refusal to infer local paths from a remote endpoint.

## Remaining

Final boundary/visual/resource gates for phases 1–3 and phases 4–7 in [the plan](EXPANSION_PLAN.md), including remaining storage boundary/performance checks,
shared cleanup protections, maintenance/incident
history, alerts/panel, import drafts, resource/accessibility matrix and release.


## Runtime controls and remaining audit items

The task views release their models when monitoring pauses and invalidate late
manual responses by generation. This keeps stale snapshots from reappearing
after leaving the page. Recordings use Linux boot time to include suspend and
mark sampling discontinuities, rather than interpreting sleep as continuous
measurement. Snapshot promotion and retirement are atomic; physical metadata
compaction can retry without repeating a successful scan.

Matching helper/worker/maintenance-adapter installation is still a later phase.
No root helper, installed app, system/user schedule or authentication policy was
changed. The final optimized whole-app CPU/RAM, keyboard/a11y/theme/scale matrix,
all-feature coverage and release remain required, not presumed passing.


Phase 7 must also audit wall-clock rollback and history cursor ordering, chart
rendering across missing samples, long labels/error detail presentation, and
late asynchronous responses after navigation. Current native evidence exercises
positive workflows and cancellation; it does not certify every interleaving.
