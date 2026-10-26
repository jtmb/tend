# Tend expansion — implementation and completion ledger

Goal: deliver all twelve approved features in tested phases, preserving Rust/GTK,
bounded resources and local data. No restarts, application closures, new jobs,
or restart/login/authentication policy changes during implementation or delivery.

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
| 4 — Cleanup protection | Typed explained plans; global exclusions in manual/cron/daily paths; skip incompatible bulk operations; preview invalidation; matching helper/worker/adapter capabilities. | Not implemented. |
| 5 — History | Maintenance phase outcomes/duration/retries/catch-up/net-space observations; incident timeline across crashes, updates, maintenance and recorded pressure; no invented legacy history or causal claims. | Not implemented. |
| 6 — Alerts and panel | Optional15s cheap hidden monitoring; bounded optional crash reads; configurable alert hold/cooldown/snooze/recovery; fresh/paused panel summary and non-destructive shortcuts. | Not implemented. |
| 7 — Portability and release | Versioned settings bundles with per-section draft review; all twelve features integrated; complete native/contract/a11y/performance tests; docs/demo; backed-up user install and publication. | Not implemented. |

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
