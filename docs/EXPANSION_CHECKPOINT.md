# Expansion checkpoint — phases 1–4 (partial)

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

Final boundary/visual/resource gates for phases 1–4 and phases 5–7 in [the plan](EXPANSION_PLAN.md), including remaining storage boundary/performance checks,
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

## Cleanup protection checkpoint

- Typed previews explain eligibility and consequences, and expose every selected
  file through a 200-row paginated native table. Execution rechecks the saved
  inode, size, ownership and timestamps; a changed file is skipped. Relevant
  policy edits, policy revision changes and expiry invalidate approval.
- One shared exclusions editor is available from manual and daily cleanup.
  Draft add/edit/remove and Discard use the shared footer. Activation is gated
  on the installed administrator, both workers and the maintenance adapter
  advertising explicit support. Unsupported installations permit review but
  cannot claim active protection.
- The root-owned policy and stable lock serialize changes against all Tend
  cleanup paths. Missing files, invalid schemas, unsafe permissions and locking
  failures stop cleanup. Literal paths and canonical aliases are protected.
- Docker pruning, Empty Trash and npm verification skip while path exclusions
  are active. APT resolves its actual configured archive directory; pip resolves
  its explicitly selected cache. Those categories skip on overlap or unknown
  locations. These exclusions do not replace Ubuntu's separate tmpfiles, Snap
  or OpenCode policies.
- Cache enumeration is bounded and reports incomplete reads instead of
  approving a partial scan silently. Protected directories are not traversed.
  The preview candidate budget is 16 MiB; this is a selection budget, not a
  measured peak-RSS claim.
- A path that later develops a symlink loop stops cleanup readers, while the
  exclusive writer can still replace it with a valid policy. Newly submitted
  unresolvable paths are rejected. An integration upgrade refuses to recreate a
  missing policy when prior installation markers exist, preserving the error
  instead of silently dropping unknown protections.
- The integration installer preserves existing policy bytes and the lock inode
  on upgrades, refuses replacement during active cleanup through the shared lock, checks release
  capabilities, and refuses enforcement rollback while exclusions are active.
  All installer trials used retained staging trees.
- A repeat test found that an inherited descriptor can briefly retain a scan
  lock after completion. Scan/exclusion guards now unlock explicitly. The scan
  regression retains a duplicate descriptor to reproduce that ownership case
  deterministically, without starting another process.

### Evidence and limits

- Rust: 66 library tests and one panel protocol test passed; the existing
  opt-in fixture deletion test remains ignored.
- Python: 42 passed, one mocked notification test skipped because the host
  blocks mock execution. Administrator tests use a networkless mount namespace
  with fixture-only writes and recorded package/service commands.
- Positive isolated contracts verify policy revision conflicts, writer/reader
  contention, obsolete-adapter rejection, APT skips, pip cache aliases,
  protected-file survival, stale-policy rejection and replacement-file survival.
- Native cleanup evidence:
  `../performance/native-fixtures/tend-workflow-uccug7yg/cleanup/cleanup-evidence.json`.
  All 206 candidates were paged; draft CRUD, validation, unsupported-save gating,
  daily bulk-skip disclosure and model release on navigation passed. Original
  six demo inputs remained byte-exact; no deletion/system action was invoked.
- Existing native visual matrix:
  `../performance/native-fixtures/tend-workflow-ngv9civk/qa/`.
  Expansion regression:
  `../performance/native-fixtures/tend-workflow-fylfwsyi/expansion/`.
- Tests were moved to retained fixtures on the main disk after the real low-space
  guard refused history writes on `/tmp`. The guard was not weakened and earlier
  evidence was not deleted. Sandbox helpers now share immutable fixture binaries
  instead of making a multi-MiB copy for every test.
- Detailed logs are in `../performance/tend-expansion-phase4-*.log`; all fifteen
  live configuration/schedule fingerprints still match the original baseline in
  `../performance/tend-expansion-phase4-invariants.json`.

This is source/test evidence, not an installed release. The installed app,
administrator integration and running applications remain untouched. Final
whole-app budgets, complete accessibility checks and timeout/cancellation audits
remain required. Maintenance/incident history, alerts/panel status and settings
bundles are the next implementation phases; the overall goal stays active.

## Phase 5 — bounded maintenance history and incident timeline

Source checkpoint only; production binaries and settings remain unchanged.

- Root phase receipts are persisted before execution. Completed receipts restore
  legacy state after a state-save failure; uncertain outcomes require review and
  cannot automatically repeat a cleanup. Seven existing phases are wrapped
  without changing their scheduling or restart code.
- The system database is bounded to 4 MiB plus at most 4 MiB DELETE journal,
  reserved inside the user-selected total history budget. Private audit failures
  preserve the action result. User scheduled cleanup also writes a durable
  receipt before running and protects Tend's own state from folder rules.
- Activity combines paginated maintenance outcomes, private reports, diagnostic
  sessions and explicitly requested bounded crash reads. Filters, readable dates,
  details, exports and raw journal links use actual native widgets. Unknown prior
  history and pending outcomes are labelled rather than guessed.
- Read-only external queries have deadlines/output limits and owned process
  groups; child queries terminate if their owner exits. No mutating maintenance
  command is given a timeout that could interrupt an update.
- Final phase-5 suite: 75 library tests and one panel test passed; 56 Python tests
  ran, 55 passed and one legacy mocked notification skipped. Retained stderr
  shows the namespace mock exited SIGSEGV; its cause and real delivery are not
  established by that skip.
- Native activity, existing visual matrix, expansion and cleanup suites passed.
  Latest activity fixture: ../performance/native-fixtures/tend-workflow-xx9djfu6.
  At 720px the redundant Source column hides; event details retain the source.
  Latest GTK/Adwaita log has no warnings or critical messages.
- Evidence logs: ../performance/tend-expansion-phase5-*.log. Fifteen protected
  configuration/schedule/login fingerprints match the original baseline in
  ../performance/tend-expansion-phase5-invariants.json.

Remaining: phase 6 alerts/panel, phase 7 portability/release, final complete-app
resource/accessibility gates, snapshot chronology under wall-clock rollback, and
a native history-budget/retention editor. No deployment or publication yet.
