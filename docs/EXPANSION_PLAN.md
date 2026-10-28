# Tend expansion — implementation and completion ledger

Goal: deliver all twelve approved features in tested phases, preserving Rust/GTK,
bounded resources and local data. No restarts, application closures, new jobs,
or restart/login/authentication policy changes during implementation or delivery.

## Planning checkpoint — October 26, 2026

The existing tracked goal remains active. This plan covers the entire feature
set; source work already present is subject to the same completion gates as new
work. Phases 1–4 have source implementation and recorded checks, with final
resource, accessibility and deployment gates outstanding. None of this
constitutes a completed expansion release. The planning checkpoint changed
documentation only; subsequent implementation is recorded separately in the
completion ledger and checkpoint. The running app and schedules are unchanged.

## Agreed defaults

- Monitoring is opt-in while Tend runs; Quit stops every collector. No autostart.
- File-level history is collected by requested scans, without content reads or
  persistent filesystem watchers. Retain two complete snapshots per root and
  thirty days of changes, under a configurable 512 MiB total default budget.
- Imports are validated, previewed, disabled drafts. Existing active jobs remain
  unchanged until reviewed explicitly. Never import restart/login/auth settings.
- Native user interface; only Diagnostics and Workspaces add navigation pages.
- One new direct dependency: rusqlite 0.41.0 with only bundled SQLite enabled.

## Phase ledger

| Phase | Requirements | Completion evidence |
| --- | --- | --- |
| 1 — Foundations | Shared demand-driven sampler; stable process identity; structured observations; private bounded history; helper capabilities; performance baseline. | Collector, identities, observations, private SQLite store and explicit capabilities implemented; unit/native checks pass. Collector CPU/memory/stop probes pass; optimized full-app budgets remain pending. |
| 2 — Processes and diagnostics | App groups/process trees; three-minute evidence-based slowdown capture (2s default, max15m); markers; saved workload comparisons and uncertainty. | App groups/tree, bounded recording, markers, save/load and comparisons implemented. Real-widget workflows and regressions pass; final accessibility/resource gates pending. |
| 3 — Storage and workspaces | Lossless file metadata, added/removed/changed comparisons, incomplete-scan protection, retention/quota; registered projects with corrected process/container mappings and cached storage reports. | Streaming metadata snapshots and paged change history implemented and tested for core/native workflows. Million-entry generated index meets memory target. Workspaces and native/sandbox contracts implemented; final accessibility/performance and remaining boundary gates pending. |
| 4 — Cleanup protection | Typed explained plans; global exclusions in manual/cron/daily paths; skip incompatible bulk operations; preview invalidation; matching helper/worker/adapter capabilities. | Typed plans, paged explanations and a shared editor implemented. Rust, Python, native cleanup/expansion, visual matrix and isolated integration contracts pass. Matching production deployment and final resource/accessibility gates remain pending. |
| 5 — History | Maintenance phase outcomes/duration/retries/catch-up/net-space observations; incident timeline across crashes, updates, maintenance and recorded pressure; no invented legacy history or causal claims. | Bounded durable phase receipts, private reports and native paginated timeline implemented. Rust/native/sandbox checks pass; one legacy namespace notification mock SIGSEGV is explicitly unverified. Matching deployment and final resource/accessibility gates remain pending. |
| 6 — Alerts and panel | Optional15s cheap hidden monitoring; bounded optional crash reads; configurable alert hold/cooldown/snooze/recovery; fresh/paused panel summary and non-destructive shortcuts. | Opt-in shared monitoring, native editor, bounded optional crash reads, alert state machine and live panel implemented. Rust/native/private-bus/panel/visual/contract checks pass. Optimized ten-minute whole-app and final accessibility gates remain pending in phase 7. |
| 7 — Portability and release | Versioned settings bundles with per-section draft review; all twelve features integrated; complete native/contract/a11y/performance tests; docs/demo; backed-up user install and publication. | All twelve features now have source implementation: versioned settings bundles, inactive drafts, ten native review sections, history limits and clock-order fixes pass Rust/native/sandbox checks. Complete resource/accessibility/integration gates, final documentation/demo, deployment and publication remain outstanding. |

## Phased delivery plan

Work proceeds in dependency order. Each phase ends with an evidence checkpoint:
what changed, what was exercised, measured resource use where applicable, and
remaining limitations. Existing unverified work is audited before adding more.

### 1. Shared foundations and budgets

- Complete the single demand-driven sampling service and stable process
  identities. Mark unavailable samples and counter/suspend resets explicitly.
- Keep bounded queues and lazy, private history storage. Apply retention and
  disk quotas to the database, sidecars and staging together.
- Establish the optimized whole-app baseline, in addition to collector probes.
  Check helper capabilities before enabling dependent controls.

**Exit gate:** no startup scans or Docker calls; collectors stop when no view or
opt-in monitor needs them; Quit terminates collection. Tests cover resets,
permission gaps, corrupt history, low space and bounded writes.

### 2. Understand slowdowns

Features: **app/process grouping, slowdown diagnostics, performance comparisons**.

- Provide app groups and expandable process trees with search and resource
  sorting. Preserve identity checks when inspecting a process.
- Record an explicitly started workload session, including CPU, memory, paging,
  pressure and contributing processes. Let users add workload markers.
- Save and compare sessions using the same metrics and expose changed context,
  missing samples and insufficient evidence. Describe observations without
  claiming that correlation proves the cause of a slowdown.

**Exit gate:** real-widget recording, cancellation, markers, save/load and
comparison pass; recycled PIDs and missing data cannot create false results;
leaving the page releases collection and large models.

### 3. Explain storage growth and development workloads

Features: **full file-level storage-growth history, developer workspaces**.

- Scan requested folders for metadata only. Compare added, removed and changed
  files, with logical and allocated size and hard-link handling.
- Show incomplete scans honestly and preserve the previous complete baseline.
  Keep two complete snapshots per root and thirty days of changes.
- Register project folders; show associated processes, explicitly inspected
  local container metadata and cached storage results. Keep uncertain mappings
  labelled and offer navigation to the relevant existing tools.

**Exit gate:** cancellation, partial scans, unusual filenames, mount changes,
permissions, quota failures and deleted roots cannot report false removals.
The million-entry memory gate and native workspace workflows pass.

### 4. Make cleanup understandable and consistently protected

Features: **explained cleanup, global cleanup exclusions**.

- Preview the selected items, why each is eligible, expected consequences and
  estimated space. Allow inspection of the full paginated selection.
- Edit shared protected paths through the UI. Enforce them in manual cleanup,
  existing cron cleanup and daily maintenance, including custom folder rules.
- Skip a bulk operation when it cannot honor an exclusion. Show the reason.
- Invalidate previews after relevant edits, expiry or policy changes. Recheck
  file identity and eligibility immediately before removal.
- Require matching administrator, user worker and maintenance adapter support;
  unsupported installations must show an actionable unavailable state.

**Exit gate:** all executors honor the same policy, concurrent edits cannot
weaken protection mid-run, and malformed policies fail closed. Exercise actual
UI previews and confirmations with fixtures; destructive command contracts run
only in an isolated sandbox. No live cleanup is needed to validate this phase.

### 5. Make events and maintenance traceable

Features: **maintenance history, incident timeline**.

- Record phase outcomes, duration, retries, deferred/catch-up runs, errors and
  observed net space changes. Distinguish estimates from observed values.
- Combine available crashes, updates, maintenance and recorded pressure into a
  filterable, paginated timeline with readable timestamps and event details.
- Link to logs and diagnostics where evidence exists. Identify legacy gaps;
  never fabricate prior activity or infer a cause from proximity alone.
- Keep operation success separate from history-writing success, so an audit
  failure cannot repeat a successful cleanup.

**Exit gate:** partial failures, retries, clock changes, absent history and
duplicate events are covered. Existing schedules and restart behavior are
unchanged; simulations verify history without running system maintenance.

### 6. Optional monitoring and useful top-panel status

Features: **opt-in health alerts, top-panel status**.

- Offer lightweight monitoring while Tend runs, including when its window is
  hidden. Default it off; do not install a daemon, autostart or scheduled job.
- Provide configurable thresholds, sustained-condition timing, cooldown,
  snooze and recovery notifications. Bound optional crash-log reads.
- Show fresh, paused or stale status in the existing top-panel integration,
  with shortcuts to diagnostics and reports. Keep destructive actions behind
  their normal preview and confirmation flow.

**Exit gate:** all alert transitions and disabled states pass; hiding and
reopening do not duplicate collectors; Quit stops monitoring. Measure a full
ten-minute enabled idle run against the CPU and memory budgets.

### 7. Portable settings, consistent design and release

Feature: **settings export/import**, plus integration of all twelve features.

- Export a versioned settings bundle. Validate imports, present a per-section
  diff and load disabled drafts for explicit review/save. Exclude secrets,
  restart/login/authentication settings and raw diagnostic history.
- Keep only two new navigation destinations: Diagnostics and Workspaces.
  Integrate storage, cleanup, history and monitoring into the related pages.
- Audit Save/Discard placement, dirty state, loading/errors/empty states,
  readable dates and units, keyboard navigation and accessible labels.
- Repeat native feature workflows, sandbox command contracts and regression
  tests. Check light/dark, high contrast, large text, 200% scale and narrow/wide
  windows. Compare optimized whole-app budgets against phase 1.
- Update the guide, feature documentation and a real-workflow README demo.
  Prepare backed-up user binaries and matching narrow integration artifacts;
  publish only after the required checks pass. The running app can load the
  release at its next ordinary launch.

**Exit gate:** every feature has a recorded pass or an explicitly resolved
limitation; imports cannot silently activate jobs; all resource gates pass;
rollback is documented and prior settings are preserved. No restart is
scheduled or triggered, and existing applications are not closed for delivery.

## Resource gates

Measure the same reference workload before and after. No scans, Docker queries
or history indexing at startup. No overlapping collection or scans. Idle/hidden
incremental memory target <=32 MiB. Enabled background CPU target <0.5% of one
core over ten idle minutes. Scan/index additional working memory <=128 MiB for
one million generated metadata entries. Routine UI interaction target <100ms.
History limits include database, sidecars and staging; pause writes with <2 GiB
free on the history filesystem, preserving the last complete baseline.

## Required verification and delivery

- Native actions use actual buttons/confirmations, bounded asynchronous waits,
  field validation, failure states, imports/exports and private-bus panel checks.
- Exercise PID reuse/orphans/permission gaps, counter resets and suspend gaps.
- Exercise hard links, filename bytes, partial/cancelled scans, mount changes,
  quota/full-disk/corrupt database recovery without false deletion reports.
- Every cleanup executor must enforce exclusions, even if an entire bulk
  category must be skipped. Older integration must never claim protection.
- Audit/history failures must not repeat successful destructive operations.
- Test all alert state transitions and complete shutdown on Quit.
- Repeat narrow/wide, light/dark, large text, high contrast and 200% scale.
- Use retained fixtures/sandboxed system commands. Existing system/user jobs,
  restart/login/authentication settings and running apps stay untouched.
- Prepare narrow version-matched integration deployment separately from routine
  user installation. Do not prompt for passwords or broaden authorization.
- Publish source/docs and tested user binaries with adjacent backups; running
  Tend loads changes on its next ordinary launch. Mark goal complete only after
  every requirement and required integration has authoritative passing evidence.
