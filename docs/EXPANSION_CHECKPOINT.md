# Expansion checkpoints — phases 1–7 in progress

Source branch: `codex/tend-expansion`, starting from `06a6aae`.

This is a chronological evidence ledger. Earlier sections describe their source
checkpoint, not current capability support. The latest section and phase table
in EXPANSION_PLAN.md show the remaining work.
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

## Phase 6 — opt-in monitoring and live panel

Source checkpoint only. The installed Tend process (PID 111567) remains open;
no user monitoring policy was created and no production binary was replaced.

- A default-off private policy configures CPU, available memory, root free disk
  space, memory-full and I/O-full PSI thresholds, sustained duration, repeat
  cooldown, recovery notices and optional new-crash checks.
- The native monitoring editor lives under Appearance, with page-local
  Save/Discard, an explicit close button, runtime status and snooze/resume.
  Saving preserves edits made during an asynchronous save. Closing disconnects
  its signal graph; the native test confirms its Save widget is released.
- Enabled monitoring subscribes to the existing shared Basic sampler at 15s.
  Hidden windows keep it active; disabled state releases it. Quit removes the
  subscription and cancels optional owned log queries before sampler shutdown.
  No daemon, autostart, timer, filesystem watcher or per-sample history is added.
- Alerts require sustained readings, use hysteresis and per-condition cooldown,
  and suppress warnings/recoveries during snooze. Missing readings, sleep gaps
  and clock discontinuities reset continuity; unknown readings cannot imply
  recovery. Repeats after a gap require a fresh sustained condition.
- Optional crash checks are bounded to visible recent reports every five minutes.
  They begin after opt-in, deduplicate bounded fingerprints rather than retain
  messages, have cooldown, and do not replay old reports after clock rollback.
  A typed reader avoids a second encoded report copy. Query cancellation stops
  only the newly created read-only query process group.
- The panel exposes current, off/snoozed or stale status and non-destructive
  shortcuts to Diagnostics, System logs, Activity and Storage. Existing close,
  host-loss recovery and Quit behavior passes its private-bus protocol test.
- Notification delivery is tested against an isolated real D-Bus interface, not
  the desktop: initial warning, repeat, recovery and fixture-crash notices all
  arrived. The native workflow also tests Save/Discard, hidden delivery,
  snooze/resume, duplicate crashes, disabling and alert-history filtering.
- Authoritative suite: 82 library tests plus one panel test passed; one legacy
  Rust fixture deletion test remains intentionally ignored. Python: 55 passed,
  one legacy notify-send namespace mock skipped (SIGSEGV remains unverified).
  The existing native visual/feature matrix and updated panel test passed.
- Evidence: ../performance/tend-expansion-phase6-{rust,debug-build,native,panel,
  matrix,python}.log. Latest health fixture: native-fixtures/tend-workflow-nf6vp5k6;
  panel: tend-workflow-t62bao6r; matrix: tend-workflow-bxakptns. All six demo input
  files retain their original bytes and timestamps. No GTK warnings/criticals
  were found; software Xvfb emits its expected unavailable-DRI3 warnings.
- Fifteen schedule/config/login fingerprints still match the original baseline:
  ../performance/tend-expansion-phase6-invariants.json.

Remaining phase 7 gates include optimized whole-app CPU/memory/interaction
measurements (including ten minutes enabled), the full accessibility matrix,
settings bundles and reviewed disabled drafts, a native history-limit editor,
snapshot ordering under wall-clock rollback, final all-feature audit, README
workflow demo, matching backed-up deployment and publication. Do not declare
the expansion complete or deployed from these source-only checks.

## Phase 7 — first boundary and settings checkpoint

- Completed scans now select/retain baselines by monotonic SQLite snapshot ID,
  rather than wall-clock date. A backward system-clock change cannot select an
  older baseline or evict a newer complete scan. The scan report discloses the
  discontinuity. Change-list timestamps retain their recorded date; pagination
  is tested across both out-of-order dates without missing/duplicating records.
- Added a shared History limits dialog from Storage, Activity and Preferences.
  Users can review/save/discard the 128–2048 MiB budget and 1–365 day private
  retention. Saving performs no scan or cleanup; the UI explains snapshot and
  system-maintenance limits. Details are expandable, with an explicit close
  button and persistent page-local footer.
- Renamed the existing Appearance destination to Preferences (same navigation
  ID), distinguishing automatically saved appearance from reviewed monitoring
  and history settings. No additional navigation destination was introduced.
- History settings reads reject symlinks, FIFOs, non-private files and oversized
  input without blocking or replacing the file. The native invalid-JSON case
  preserves the original file and does not activate defaults on close.
- Native tests exercise all three entry points, Save/Discard, edits made during
  an asynchronous save, reloaded values, invalid settings, released dialog
  widgets, narrow/wide screenshots and preservation of all original inputs.
- Latest Rust suite: 84 library tests plus one panel test passed, one existing
  fixture deletion test ignored. The native monitoring/history suite passes
  with four notifications on a private test bus. Evidence:
  ../performance/tend-expansion-phase7-history-{rust,build,native}.log and
  native-fixtures/tend-workflow-_hn8n0yw.

Still outstanding: versioned settings bundles and per-section disabled-draft
review; final source/runtime boundary audit (including notification test coverage,
alert-history write backpressure and accessibility labels); optimized whole-app
resource/interaction gates and ten-minute enabled monitoring; complete design
and accessibility matrix; real-workflow README demo; matching backed-up
deployment and publication. Existing applications and system policies are
untouched, and the goal remains active.

## Phase 7 — settings transfer checkpoint

All twelve features now have source implementations. This is not a release or
proof of the remaining optimized resource/accessibility/deployment gates.

- Versioned, whitelisted settings bundles export ten preference/policy sections.
  Files are owner-only; credentials, restart/login/auth settings, raw histories
  and machine-level tuning are excluded. Unknown fields/versions and oversized
  or non-regular inputs are rejected. Leaf symlinks are not followed.
- Import shows a complete paginated per-section comparison, ordered naturally
  for array indices. Loading creates inactive private drafts only. Jobs,
  monitoring and folder rules start disabled in those drafts; no active setting
  changes until the user explicitly saves a reviewed section.
- Native forms cover all sections, including bounded project/rule/path lists,
  field validation, Add/Remove from draft and page-local Save/Discard. The
  existing pages refresh after a save. Existing unsaved drafts are preserved.
- User setting writers compare expected values after acquiring their shared
  save lock. The native concurrent-history test and isolated contracts for all
  six user writers verify stale saves are refused without changing files/jobs.
  The receiving exclusions revision and existing restart behavior are retained.
- Import actions are single-flight. Picker cancellation and export failures
  re-enable controls. Leaving Preferences invalidates pending loads immediately
  on selection change, including during GTK animations. Closing a comparison
  cancels its pending review. Old array/discard widgets have generation guards.
- Every review/comparison tracks and disconnects its signal graph on close;
  native tests confirm released Save widgets. Shared control rows now expose
  accessible labels/descriptions. Full accessibility evaluation remains pending.
- Administrator checks are bounded; capability checks happen on the review
  worker. Root and cron saves remain blocked in the isolated native preview;
  no production authentication/scheduling action is used for testing.
- Current authoritative checks: 89 library tests plus one panel test passed;
  one existing fixture-deletion test is intentionally ignored. All 57 Python
  tests passed with no skips. The former notification-script SIGSEGV is replaced
  by a no-I/O ELF argument verifier with positive, forced-failure and disabled
  dispatch checks. This proves helper dispatch, not physical desktop display;
  actual notification protocol delivery is separately tested on a private bus.
- Native settings transfer passes all ten sections, complete diff paging,
  Save/Discard, errors/cancel/retry, stale/concurrent state and callback lifetime.
  Fixture: ../performance/native-fixtures/tend-workflow-fipxv61f. Monitoring and
  history regression: tend-workflow-bhwlzac_. Full existing visual matrix:
  tend-workflow-sb8q7fs7. Cache/journal input bytes and timestamps are preserved;
  only intended fixture settings changed. No GTK warnings/criticals were found.
- Logs: ../performance/tend-expansion-phase7-bundle-{rust,build,native,health,
  matrix,python}.log. See SETTINGS_TRANSFER.md for the actual format and workflow.

Remaining: final audit of all requirements and error boundaries (including
appearance multi-file failures and monitoring-review units), optimized full-app
baseline/comparison and ten-minute enabled monitoring, interaction/resource
gates, complete accessibility/large-text/scale/contrast and design passes,
real-workflow README/GIF refresh, matching backed-up deployment and publication.
Nothing is installed or published by this checkpoint. No existing application,
restart/login/auth policy or real system/user job has been changed.

## Phase 7 — release audit and responsiveness fixes

Source checkpoint only; no installed binary, system/user job, restart, login or
administrator authorization policy has been changed.

- Appearance imports now compare current values under the save lock and restore
  both original preference files after a partial write failure. An incomplete
  rollback is reported honestly. Imported monitoring thresholds include units.
- Alert history has a bounded eight-report backlog, one writer at a time, with
  at most 4 KiB per report. Overflow produces a counted history-gap record.
  Optional crash notices are also retained. The native test confirms all seven
  expected records, including a queued burst, and four private-bus notices.
- The panel's menu status is compact; the full resource summary stays in the
  tooltip. Monitoring controls have explicit accessible labels.
- System logs use a native virtualized list and 25-event result pages. All
  matching loaded events remain browsable/exportable. Selection maps to source
  indices, and filters no longer copy full events or rebuild hundreds of rows.
  Typed journal parsing and snapshot export run on workers. A completed export
  cannot overwrite a newer filter's status. Weak control references avoid new
  signal-reference cycles.
- Keyboard traversal reproduced a GTK critical through collapsed expander
  content. A retained GDB trace and upstream GtkExpander source identified its
  child-visible focus check. Tend now aligns that flag with expansion state;
  the corrected native run completes with GTK criticals fatal, not suppressed.
- The optimized native run checks three passes of navigation and filtering,
  including 1,000 generated journal events, paging, source selection and export
  of all 500 filtered matches. All 63 first-painted-feedback measurements meet
  the 100 ms target; the worst is 90.711 ms under Xvfb/Cairo. This does not measure
  physical GPU latency or completion of external queries/animations.
- All seventeen pages have narrow/wide captures and real keyboard-focus checks
  in that run. Full theme/scale/contrast/large-text and dialog review is still
  outstanding; these captures alone do not certify complete accessibility.
- The existing feature regression targets generic text/JSON report exports on
  Storage, where that retained utility lives. Structured Activity exports have
  their own timeline workflow coverage.
- The user installer now checks root-owned administrator/worker/adapter
  capabilities and readiness before replacing user files. Four preflight tests
  cover roles, flags, trust, timeouts/failures and incomplete markers. The live
  read-only check correctly identifies the installed legacy integration as
  needing its separate upgrade. The narrow installer dry run passes and writes
  nothing; no authentication prompt was opened.
- Current authoritative suites: 91 library tests plus one panel test passed;
  one legacy fixture-deletion test remains intentionally ignored. All 61 Python
  tests passed with no skips. Optimized native release, broader feature,
  health/history and all ten settings-transfer section workflows pass with no
  GTK warnings/criticals. Expected Xvfb DRI3 warnings remain.
- Retained fixtures: release `tend-workflow-af4lz3nl`, broader features
  `tend-workflow-fpzhqvhp`, health `tend-workflow-mgjg174i`, settings
  `tend-workflow-tpjc0nlh`, all under `../performance/native-fixtures/`.
  Logs: `../performance/tend-expansion-phase7-candidate-*.log`,
  `tend-expansion-phase7-final-audit-{rust,python}.log`, and
  `tend-expansion-phase7-deployment-dry-run.log`. Prepared cache/journal inputs
  retain their original bytes and timestamps. Fifteen protected configuration
  and schedule fingerprints still match the original baseline.

The optimized reference/whole-app ten-minute monitoring measurement is running
separately, without compilation or rendering suites. Its result must be inspected
before claiming the CPU/RAM gates. Remaining gates include the complete visual
and accessibility matrix, real-workflow README/GIF review, matching backed-up
installation and publication. The existing tracked goal remains active.


### Optimized whole-app result

All whole-app gates pass in `../performance/tend-expansion-whole-app-budget.json`:
60.06-second reference/candidate idle intervals and 600.09 seconds of enabled
monitoring, +10.95 MiB peak RSS over reference (32 MiB limit), 0.0467% of one
core (0.5% limit), zero measured major faults, one active monitoring subscriber
and verified stop after disabling. Both private panel hosts registered; both
owned previews exited normally. No unsolicited history database was created.
See PERFORMANCE.md for the exact scope and retained binary hashes. Optional
crash-query cost and physical GPU/desktop behavior are outside this run.
The earlier paragraph describing this measurement as running is superseded by
this result. Full accessibility, documentation/demo, integration deployment and
publication still remain; the goal is active.


### Final audit, native rendering and working demo

The source is now version 0.3.0. This is still an active release goal: matching
system installation and publication have not yet happened.

- Current core contracts pass: 92 library tests and one panel test, with one
  legacy fixture-deletion test intentionally ignored; all 63 Python tests pass.
  The latest affected administrator contracts additionally reject overflowing
  receipt dates. User scheduled receipts fsync their directory ancestry before
  execution; a sandbox-only directory-fsync failure prevents cleanup, while a
  completed-receipt failure cannot repeat the operation. Legitimate .local
  directory aliases remain supported. No live maintenance was executed.
- All seventeen pages pass six accessibility variants (102 retained captures),
  with 24 page-footer geometry checks. Leaf captures exit successfully; the
  original outer accessibility wrapper was later terminated with status 143
  after completing captures, input verification and its final evidence files.
  Independent metadata/PNG validation confirms all 102 results. Do not describe
  that outer wrapper as a clean exit.
- Ten settings-review sections and comparison dialogs pass all six variants.
  There are 174 dialog captures and 324 checked footer actions across settings,
  monitoring/history and cleanup/exclusions. The 132 settings captures were
  repeated with process-local reduced motion after visual review caught opening
  transitions in earlier images. All six settled contact sheets were reviewed;
  the desktop preferences are unchanged. Metadata validates device scale, text
  DPI, contrast, theme, PNG dimensions and footer bounds. Screen-reader and
  physical-desktop animation interoperability remain distinct checks.
- The recording exposed expensive hidden Task Manager table updates. Charts now
  skip rebuilding hidden process/group models. Switching tabs refreshes from
  the current cached snapshot. The repeated expansion workflow verifies empty
  hidden models during live chart collection and populated data on returning
  to Processes, plus storage, workspace, grouping and diagnostics workflows.
  Fixture: ../performance/native-fixtures/tend-workflow-8l69p0hy. Its direct
  callback navigation produced one nonfatal GtkText focus-out warning; it is
  not a clean physical-input/session interoperability test.
- The app-owned recorder now runs below GTK frame-clock priority: PNG encoding
  on a busy host previously starved allocation and scrolling. It uses the normal
  1100×760 window and preview-local reduced motion, with actual visible widget
  bounds checked before actions. Failed capture process groups are terminated
  so an owned preview cannot continue consuming CPU after its wrapper exits.
- The refreshed demo completes real preview and log workflows: two eligible
  files / 3 MiB, six loaded events → two errors → one editor match, and selected
  PID 4100 details. All six sample input hashes, sizes and timestamps remain
  unchanged. Source fixture: /tmp/opencode/tend-workflow-ismpu387. The shared-
  palette GIF is 629,483 bytes, 1090×820, 38 seconds, looping; its 304 source
  captures merge into 106 encoded frames. Decoded action/result scenes were
  visually reviewed. The five gallery images were refreshed, with adjacent
  local backups retained and excluded from publication.
- Local rendering of the actual README passes desktop and 390-pixel viewport
  review: opening/demo, feature grouping and installation. All six image assets
  load; there is no horizontal page overflow. Code blocks scroll independently.
  Public GitHub animation verification is still pending publication.

Raw checks: ../performance/tend-expansion-page-accessibility-verified.json,
tend-expansion-dialog-accessibility-verified.json, tend-expansion-readme-media.json,
tend-expansion-phase7-settled3-*.log and tend-expansion-phase7-frame-clock-demo.log.
The optimized docs candidate builds successfully (SHA-256
123ef37eee56c578b687b191de6b41ef1bc8580feb71ea47ecaf2755efbf7a9b).
Normal release binaries are being built. The final native/resource reconciliation,
narrow integration dry run, protected-setting fingerprints, deployment and
publication remain outstanding. Nothing has scheduled or triggered a restart,
closed an existing application, or changed live cleanup, login or authentication
policy. The only terminated app processes were owned isolated QA previews.


### Frozen artifacts and unresolved final latency gate

Both optimized builds complete: normal app SHA-256
29b14cefca0413dbcc9ef21c06933f81fe9952b66bb33afb8926780faa9d005e
(4,035,160 bytes), administrator
d50aa2b0d8497fc4924dba4b8bac7552a90086267925f30246dcace832b440a4,
worker 440e39ad4872c638518e9e10ecde04c7dd063506a6596e153623ef400e8f59fd.
Two nonfatal unused-import warnings are retained in the normal build log.
The matching integration dry run passes all ten destinations and changes nothing.
All fifteen protected settings/schedule fingerprints still match the baseline.

The frozen candidate's final native repeat completes functional checks, all
63 observations and 34 captures, but **fails** the latency target: worst
2266.105 ms on the first Task Manager navigation, with several other observations
over 100 ms. Fixture: tend-workflow-hgurg0kp. The native harness historically
reports this as an explicit JSON gate while exiting successfully; process exit
must not be used as proof of meeting the metric. The test inherited nice 10
while the normal app uses nice 0. Host CPU pressure was around 50%; existing
OpenCode/ChatGPT and other apps are left alone. This does not yet establish the
cause of the difference. A normal-priority comparison is pending.

The final ten-minute resource repeat is now running separately against the
retained reference; there is no compiler or native rendering suite alongside
that measurement. The tracked goal remains active. Nothing is installed or
published by this checkpoint, and no restart/schedule/authentication policy is
changed. This result supersedes any wording implying every final gate passes.


### Resource repeat completed; process-render optimization under verification

The frozen optimized candidate passes all nine resource gates in
../performance/tend-expansion-whole-app-budget-final.json: reference/candidate
idle peaks 171.242 / 178.125 MiB, monitoring peak 178.133 MiB (+6.890625 MiB),
0.043305% of one core over 600.388 seconds, 45 collections, zero subscriptions
and verified stop. This run observed 18 major faults and one sustained-condition
notification/history record on the private test bus; it did not notify the real
desktop. Earlier zero-fault/no-event observations are a separate run.

The normal-priority comparison also misses the interaction target: earlier
candidate worst 252.237 ms, final frozen candidate 1957.126 ms. Both runs retained
all inputs and had CPU pressure around 49–65%. Results are in
../performance/tend-expansion-normal-priority-comparison.json. These are not
release-ready latency results.

The native timing harness now resolves test widgets before starting the measured
callback-to-paint interval and records lookup/callback costs separately. Its
functional assertions run after first paint, avoiding assertion traversal inside
the measured interval. A corrected debug run still misses the target (203.725 ms),
so the work is not being declared complete based on a measurement correction.

An owned GDB preview shows Ui::update wall times of 338–794 ms. This includes
GDB and host scheduling, so it is diagnostic evidence rather than a release
benchmark. It identifies repeated process-row rebuilds as real work to remove.
The current source reuses Task GObjects by PID/start identity, updates weakly
registered cells, fills new rows in paced 25-row batches, cancels population on
leaving/hiding, and restores selection by identity. A PID reuse contract passes.
Only the visible Apps/Process tree model is built. Page navigation drops its
extra crossfade; desktop animation settings remain unchanged.

The first native run confirms complete process population, stable row objects and
selection across refresh, and process links through an excluding search. It then
hits the old immediate-population assertion on returning from charts; that test
now waits for staged data instead. Pending-population Quit coverage is added.
The new paced implementation is being built/repeated before optimized validation.
A preliminary debug run before pacing misses the target at 181.347 ms. No release
claim follows from it.

Source checkpoint 44a88d2 retains the completed audit/docs/demo work. New rendering
changes remain uncommitted. Prior normal binaries are hash-verified and retained
under ../performance/release-0.3.0; they predate this latest UI optimization.
No source has been uploaded or installed. No existing apps, schedules, restart,
login or authentication policy have changed. Only owned QA displays and the
completed local documentation server were stopped. The goal remains active.


### Process population and date-cache contracts pass

The paced native expansion workflow passes in tend-workflow-bq5frod2 and, after
avoiding hidden Performance cards/homogeneous tab measurement, in
`tend-workflow-_do2voft`. Both preserve every prepared input, show the full
sampled process count, retain object identity and selection across refresh,
select a process through an existing excluding search, release hidden models,
and exit successfully with process population still pending. Storage/workspace,
group/tree and diagnostic recording/comparison checks pass in the same run.

New process rows arrive in 25-row/32-ms batches rather than an always-ready idle
that could fill the entire table before the next frame. Surviving task objects
and weakly registered cells are reused. Only the active subview builds widgets;
normal page navigation no longer adds a crossfade. No hardware/display preference
or running application was changed.

Log indices are reused and timezone rules cached without freezing relative dates:
a Toronto day key invalidates row identities at midnight or on reload. Three
presentation tests and three UI/model tests pass, covering daylight/standard-time
midnight, date-cache invalidation, PID reuse and the panel protocol. Current
normal compilation is warning-free in these checks. The latest exploratory debug
feedback run before the day-key guard has worst 142.528 ms: journal filters pass,
but a few page paints exceed 100 ms. This is not an optimized release gate pass.
The optimized build/interaction/resource reconciliation must still be repeated
for this frozen source before installation or publication. Previous passing
resource results refer to the retained earlier binary, not this new build.

### Optimized responsive-source checkpoint — latency remains unresolved

Full Rust checks now pass 96 tests (93 library, three UI/model); the one legacy
fixture-deletion test remains intentionally ignored. The optimized docs build
finishes in 4m18s without warnings. Retained artifact
`tend-responsive-df410c4-docs` is 4,555,352 bytes, SHA-256
`6bea4944f4fed12b1dce1f11b529b0f23b49de3ed14283bc814109a62a8d1347`.

The corrected native release driver completes all 63 observations, 34 captures,
keyboard traversal and journal pagination/selection/export checks in fixture
`tend-workflow-2prks3rv`. Five observations exceed 100 ms; worst feedback is
328.005 ms on first Task Manager navigation. First Cleanup schedule is 201.902 ms,
Diagnostics 122.004 ms, Disk cleanup 111.323 ms and Storage 108.517 ms. The driver
correctly exits unsuccessfully for the latency gate after preserving all input
fingerprints. Niceness is zero, no compiler runs alongside it, and CPU pressure
is approximately 24–29%. This remains a failed gate rather than proof that
changing application priority resolves it. Current narrow Task Manager and wide
System logs captures were visually inspected; readable dates and scrolling are
preserved.

The same artifact passes the full native expansion workflow in
`tend-workflow-ym9d_z07`, including complete staged process population, retained
identity/selection, group links through excluding searches, hidden model release,
metadata comparisons, workspace edits and two recorded diagnostic sessions.
Quit during pending process population exits the owned preview successfully.
All six prepared inputs remain unchanged. Logs:
`tend-expansion-phase7-responsive-full-rust.log`,
`tend-expansion-phase7-responsive-release-build.log`,
`tend-expansion-phase7-responsive-release-native.log` and
`tend-expansion-phase7-responsive-expansion.log` in the local performance record.

The documentation-only native harness now also records main-thread CPU and
before-paint/layout/paint phase observations, retaining the same wall-time target.
This is to distinguish rendering work from scheduling waits, not to subtract
either from the gate. The instrumented optimized test build is pending. No
installed files, live cleanup, jobs, restarts, applications or authentication
settings are changed by this checkpoint. The goal remains active.

### Frame diagnosis and cleanup disclosure under verification

The instrumented artifact SHA-256
`709d9e3c5d6d522195bb59a13a1a386b1c9629f9e8cd24e905a13331d46e0c28`
completes native workflows and all input preservation in `tend-workflow-lrr1mim_`.
It still fails the gate: Disk cleanup 111.278 ms and Daily cleanup 101.521 ms,
with about 80 ms of main-thread CPU each. Task Manager is 91.462 ms in this
separate run; that variation does not erase the earlier 328.005 ms observation.
Phase records distinguish before-paint/layout/paint elapsed and thread CPU time,
without changing the wall-time target. Logs are
`tend-expansion-phase7-frame-phases-build.log` and
`tend-expansion-phase7-frame-phases-native.log`.

Both cleanup pages now put the five retention fields in a native disclosure,
with a visible current-draft summary when collapsed. All settings remain editable;
enabling a category enables its associated limits. Weak-reference change handlers
update the summary without collectors, timers or background writes. Discard
restores values and the summary. The existing expander focus guard also covers
this section. This aims to reduce initial layout work, not to remove functionality.

The updated debug native workflow passes all ten field edit/discard cases and
four expanded captures in `tend-workflow-2mall25e`; current collapsed wide and
expanded narrow cleanup captures were visually inspected. The debug latency
gate still fails (worst 236.973 ms); no performance improvement is claimed from
that run. The policy regression also passes Save/Discard/defaults, invalidation,
independent policies and failure-state checks in `tend-workflow-gaky7c8h`, with
the disclosure opened before editing. Original cache/journal inputs remain
preserved. No live policy or cron job is saved.

The optimized retention candidate is building next. Its latency, exact-artifact
resource and updated accessibility checks remain release gates. Test code is
excluded from default installed builds. No user app, system schedule, restart,
login or authentication policy is changed.

### Optimized retention workflow complete; whole-app budget running

Retained optimized docs artifact `tend-retention-0.3.0-docs` has SHA-256
`3c8265fffd46d4adf98d16155bf279eef155bfa943c0397e3d0176588be73ca9`.
Its native check in `tend-workflow-4mbgk_iw` completes all 63 observations,
34 page captures, four expanded retention captures, keyboard traversal, log
pagination/selection/export checks, ten category-enabled field edits and Discard
restoration. All six input fingerprints remain unchanged. The driver correctly
exits unsuccessfully for the unresolved wall-time target: Task Manager first
feedback is 110.989 ms (45.357 ms main-thread CPU) and System logs is 127.686 ms
(36.923 ms CPU). The other 61 observations are below 100 ms. Host CPU pressure
is approximately 37–46%; neither that observation nor the CPU split excuses the
failed gate or proves a unique cause.

Disk cleanup first feedback is 73.792 ms (64.014 ms CPU), Daily cleanup
68.786 ms (64.558 ms CPU). The preceding optimized instrumented run records
approximately 80 ms CPU for these forms. These are recorded per-run results,
not a claim that every workload or the entire latency gate now passes.
The earlier failed runs remain retained. Log:
`tend-expansion-phase7-retention-release-native.log`.

The exact retained candidate is now undergoing the normal-priority whole-app
reference/ten-minute monitoring run, without a compiler or native rendering
suite alongside it. Results will be recorded in
`tend-expansion-whole-app-budget-retention.json`; the active command log is
`tend-expansion-phase7-retention-budget.log`. Updated accessibility, public
demo reconciliation and final matching production builds/deployment/publication
remain pending. The protected-file/crontab repeat confirms all fifteen baseline
fingerprints still match in `tend-expansion-phase7-responsive-invariants.json`.
No installation or live system action is performed. The goal remains active.

### Retention-source budget passes; first-paint lifecycle work pending

The retained `3c8265ff…` candidate completes its ten-minute reference comparison
in `tend-expansion-whole-app-budget-retention.json`, all nine gates passing.
Reference/candidate idle peaks are 170.707/167.172 MiB; monitoring peak is
168.160 MiB, 0.988 MiB over its own idle peak and 2.547 MiB below this reference.
This pair's negative incremental RSS is an observation, not guaranteed savings.
Monitoring uses 0.043304% of one core over 600.407 seconds, with 45 collections,
63 major faults, two private-bus notices/history events and verified stop at zero
subscriptions. Both idle intervals show no CPU-counter advance or major faults.
All six inputs are preserved. Fixtures: reference `tend-workflow-jfixfgkk`,
candidate `tend-workflow-zuodbj4d`. No compiler or native rendering suite runs
alongside the measurement, and no real desktop notice is sent.

Source checkpoint `2f698ae` retains that tested cleanup disclosure work. Newer
uncommitted source schedules Task Manager's automatic subscription after its
first frame, with an owned, cancellable one-shot frame handler. Leaving/hiding,
focus loss, speed changes and close cancel the handler before replacing the
subscription. The initial view says it is collecting; completed data is still
required by native workflow assertions. The log-results stack measures only
its visible child. The new native regression checks leaving before first paint
does not collect, then requires automatic population without forcing Refresh.
Test-only frame probes include cumulative run-queue wait deltas when available;
they do not replace or subtract from wall-time feedback.

These newer changes are not included in the completed budget result. Full Rust
and debug native builds are running next, after the budget command exits zero.
All installation, final accessibility/demo reconciliation and publication gates
remain pending. No existing application, job, restart, login or authentication
policy is changed. The goal stays active.

### First-paint lifecycle and collector priority verified; latency still fails

The first-paint lifecycle passes full Rust checks (96 tests at that point) and
the native expansion workflow in `tend-workflow-7xj5z0ca`: leaving before paint
starts no subscription/collection, returning automatically fills all 311 sampled
rows without forcing Refresh, and selection, grouping, storage, workspaces and
recording/comparison checks pass. The debug feedback run in
`tend-workflow-69q4te9w`, recorded in
`tend-expansion-phase7-after-paint-native-feedback.log`, still misses the latency
gate (147.981 ms worst).

The collector subsequently lowers only its own Linux thread priority by five,
best effort. A kernel-backed contract verifies the caller is unchanged. All
97 Rust tests pass (94 library, three UI/model; one legacy deletion test ignored).
The optimized docs artifact is `tend-after-paint-0.3.0-docs`, SHA-256
`e3daa2a8c7123bf6bcf81faa95f37a31b3d263934677f32360ce8c592a0a6726`.
An independent read-only inspection of the owned native preview records its
GUI thread at nice 0 and `tend-sampler` at nice 5 in
`tend-expansion-after-paint-thread-priorities.json`.

The optimized expansion workflow passes in `tend-workflow-mma_jcrh`, with all
322 sampled processes, automatic after-paint startup, cancellation before paint,
stable identity/selection, storage/workspace and recording/comparison coverage,
and Quit during population. All six inputs remain unchanged. The optimized
release workflow in `tend-workflow-9e4q7mro` completes 63 observations, 34 page
captures plus four expanded retention captures, keyboard checks, all ten limit
edit/discard cases and journal mapping/pagination/export checks. It correctly
exits unsuccessfully: Disk cleanup 111.186 ms and Maintenance 109.361 ms exceed
100 ms; the other 61 observations pass. Their thread CPU times are 71.126 and
75.719 ms, with measured run-queue wait of 33.865 and 30.395 ms. These kernel
observations establish contention contributes, without excluding rendering work
or waiving the wall-time gate. Task Manager's first feedback is 78.648 ms.

Logs: `tend-expansion-phase7-priority-full-rust.log`,
`tend-expansion-phase7-after-paint-release-build.log`,
`tend-expansion-phase7-after-paint-release-native.log` and
`tend-expansion-phase7-after-paint-optimized-expansion.log`. No compiler or other
rendering suite runs alongside optimized feedback measurement. The test inspector
only reads priorities; it does not change them. Every owned preview exits, and
no test display/compiler remains running afterward.

Next work: reduce the two remaining form costs, reproduce stale folder-rule UI
saves (direct writers currently use unchecked `save`, although a checked writer
already exists), then reconcile the exact frozen artifact's resources,
accessibility and public demo. Matching production builds/deployment and
publication remain outstanding. The completed retention-source budget remains
valid only for its retained prior artifact. No installed app/helper, existing
job, restart, login or authentication policy is changed. The goal remains active.


## October 28, 2026 — Recorded OpenCode-terminal OOM acceptance

The user supplied a real low-memory termination as an acceptance case for the
existing expansion goal. Read-only kernel evidence confirms a global OOM at
8:00:45 PM EDT: Node PID 6557 had 5.15 GiB anonymous resident memory and
2.78 GiB swapped pages, with 228 KiB of 4 GiB host swap free. The requested
production backend finds this and two earlier kernel kills. A concurrently
running OpenCode-launched Node test that constructs TypeScript programs is a
plausible job, but surviving metadata does not map the command directly to the
victim PID. A separate completed typecheck is explicitly ruled out as that
killed job. OpenCode source, sessions and jobs were not changed or rerun.

Added `memory_incidents`: requested, bounded kernel-journal postmortems with
trusted transport checks, victim PID/boot/time association, checked units,
unknown missing data, and distinct global/cgroup/allocation-constrained cases.
The reader requests at most five kills, with at most 1,024 records / 2 MiB /
10 seconds per context read. It never joins an exited PID to a current process.
Stable event identities prevent context availability from producing duplicate
notifications or timeline entries. Diagnostics adds a collapsed memory-failure
explanation with local exports; System logs includes OOM kills without core
dumps, and Activity includes them under pressure and failed-result filters.
Existing opt-in crash alerts use the same reader; no new background schedule,
dependency, service or navigation page is added.

`cargo test --locked -j 1` passes **108 tests** (105 library, three binary),
with the existing destructive legacy fixture test still ignored. Eleven new
contracts cover the recorded counters, missing context, spoofed messages,
PID/boot/time mismatches, cgroup and constrained allocation, overflow/page units,
cancellation, stable identities and crash-source victim decoding. The real
read-only `memory-report` example uses this same production backend.
Authoritative logs/manifests in the sibling private performance folder:
`tend-opencode-oom-final-tests.log`, `tend-opencode-oom-build-manifest.json`,
and `oom-opencode-20261028/tend-memory-report.txt` / `FINDINGS.md`.

Final native checks invoke the real explanation button, export text/JSON,
filter by victim PID, select evidence, distinguish Activity pressure from
traditional crashes, include OOM under Failed, exclude optional incident reads,
and preserve unknown values when context is truncated. All six original fixture
inputs remain unchanged. Final fixtures:

- Standard light: `tend-workflow-4wvvgna4`.
- Large text / dark: `tend-workflow-u0ye6sx7`.
- 200% scale / light: `tend-workflow-g7ylmp2y`.
- High contrast / dark: `tend-workflow-mpeo0_wv`.

Each contains native action coordinates, two diagnostic viewport captures,
System logs and Activity captures, local exports and evidence JSON. Affected
captures were visually inspected. Singular record/event labels discovered in
that inspection were corrected. The final binary hash and build scope are in
the manifest. These are unoptimized functional/visual checks, **not** a new
optimized performance or release-budget result. The first fixture
`tend-workflow-94aehak8` failed because its driver selected a hidden dropdown
search entry; the driver now selects the actual mapped PID search field and
checks its focus/bounds. Failed evidence is retained.

Also completed the previously requested stale folder-rule save audit: actual
Save buttons reproduced an unchecked overwrite (`tend-workflow-1_y_l4sc`).
Add/Edit/Remove now use checked baselines, including across automatic-rule
confirmation; a failed save preserves the real editor draft. Reload rules
refreshes the list and invalidates previews. Native regression
`tend-workflow-me8gg6qo` passes all four races, draft preservation and a fresh
save. Traditional activity regression `tend-workflow-2zc12km8` and private-bus
health/notification regression `tend-workflow-9ik0hb0i` pass. No actual cleanup,
cron save, package update or user notification is performed.

`tend-opencode-oom-invariants.json` confirms all fifteen protected config/job
hashes still match the prior checkpoint. No existing app, login/authentication
policy, maintenance schedule, installed binary/helper or restart policy changes.
Only disk swap is currently active; no zram configuration is present on this
boot, and no swap setting was changed. The existing goal remains **active**.
Optimized interaction/resource reconciliation, matching production deployment,
final demo reconciliation and publication remain outstanding; no prior passing
budget is attributed to this new source.

## Repeated memory-failure acceptance case — October 28, 2026, 11:25 PM EDT

The second screenshot corresponds to the recorded 9:44:54 PM kernel OOM:
Node PID 46694 held 3.43 GiB anonymous RAM and 511.7 MiB swap, with only
24 KiB host swap free. The terminal scope subsequently stopped at 9:46:24 PM.
A fresh kernel-journal query at 11:25 PM showed no later memory kill.
Read-only OpenCode execution metadata establishes overlapping Node tests and
TypeScript checks, but does not establish which command owned the killed PID.
No memory leak is inferred. This agent's Rust build also overlapped and added
load; CPU/disk niceness did not provide a memory bound.

The same Tend candidate postmortem backend decoded the real event. A second
sanitized recorded fixture now verifies the victim, units, exhausted swap,
separate event identity and absence of context from the earlier kill. All
eleven targeted postmortem tests pass. Three build-guard contracts also pass.
The targeted Rust compilation completed in 21.59 seconds inside the guard.

`scripts/limited-build.py` is dry-run-first and limits this agent's Cargo jobs
to a transient user scope: 768 MiB memory high, 1 GiB memory maximum, 256 MiB
swap maximum and one Cargo job. It defers when headroom is insufficient or a
large Node job is present, and interrupts only its owned build if pressure
rises. A tiny probe verified the actual kernel limit values. No exhaustion
test or real user-process signal test was performed. OpenCode was not edited,
stopped or restarted. This does not solve its unbounded parallel checks.

Private records remain in the sibling
`performance/oom-opencode-20261028-214454` directory: `FINDINGS.md`, kernel and
terminal JSONL, actual Tend report, cgroup-limit probe and targeted test logs.
The expanded candidate is not installed over the existing app. Full release
reconciliation and the tracked feature-delivery goal remain incomplete.
