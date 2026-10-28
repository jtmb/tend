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
