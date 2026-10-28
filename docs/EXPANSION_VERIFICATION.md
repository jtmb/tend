# Expansion verification map

This maps the twelve requirements to concrete checks. A source implementation,
a fixture workflow and an installed system integration are different results.
The expansion remains in release verification; do not read this as a completed
installation or a guarantee about external package repositories/desktop hosts.

| Feature | Evidence to inspect | Remaining release work |
| --- | --- | --- |
| Slowdown diagnostics | Native `expansion` workflow: explicit recordings, markers, finish/cancel, save/load; diagnostic unit checks for gaps and context. | Optimized candidate workflow passes; page accessibility variants pass. Installed integration remains a delivery gate. |
| App/process grouping | Native `expansion`: expandable Apps and Process tree, filters and details; identity/grouping unit checks. | Optimized candidate workflow and page accessibility variants pass. |
| File-level storage growth | Native `expansion`: complete snapshots, changes and details; snapshot/retention/clock/partial-scan tests. Million-entry generated index probe. | Candidate workflow, optimized million-entry memory probe and page variants pass. Filesystem traversal speed is not established by that probe. |
| Developer workspaces | Native `expansion`: CRUD drafts, Save/Discard, sampled cwd association, explicit Compose metadata and cached storage. Docker sandbox contracts. | Candidate workflow and page accessibility variants pass. |
| Explained cleanup | Native `cleanup`: all 206 fixture candidates, pagination, reasons, draft invalidation and cancel. Sandboxed replacement-file and bulk-action contracts. | Native workflow, six dialog variants and sandbox contracts pass; matching installed integration remains pending. No live deletion is a release test. |
| Performance comparisons | Native `expansion`: two saved recordings, markers and comparison; missing/changed-context disclosure tests. | Candidate workflow and page variants pass; this does not prove the user's workload became faster. |
| Optional health alerts | Candidate `health`: hidden delivery, warning/repeat/recovery, snooze, crash deduplication, disabled/Quit behavior, queued history; deterministic engine tests. | Whole-app default CPU/RAM and six dialog variants pass. Optional crash-query cost has a separate, unmeasured resource scope. |
| Incident timeline | Native `activity`: root receipts/private reports/diagnostics/crashes, category/result/text filters, pagination, details, export and late-result rejection. | Candidate workflow and page accessibility variants pass. |
| Global cleanup exclusions | Native `cleanup` editor and command contracts across manual, scheduled and daily executors; malformed policies, aliases, concurrent changes and capability checks. | Native editor, six dialog variants and command contracts pass; matching root/user installation remains pending. Ubuntu/other applications' cleaners are outside this policy. |
| Maintenance history | Durable-receipt/state-save/crash/clock/rollback contracts; native `activity` reads the writer's fixture database. | Native workflow and durable-receipt contracts pass; system deployment remains pending. No maintenance/restart is invoked for testing. |
| Settings export/import | Candidate `bundles`: all ten sections, complete comparison, inactive drafts, Save/Discard, validation/CAS, cancellation, stale callbacks, private export and round trip. | All ten sections pass in six dialog variants; installation remains pending. File-picker results are supplied by the harness; desktop picker interoperability is separate. |
| Top-panel status | Candidate `health` plus private-bus `panel` workflow: SNI/DBusMenu properties/signals, shortcuts, host loss, close/show/Quit. | Private-bus Close/Show/Quit workflow passes; installed desktop rendering remains a separate check. |

## Shared gates

- **Interaction:** the earlier candidate passes 63 real navigation/filter
  callbacks to first painted feedback; worst 90.711 ms under Xvfb/Cairo. All
  seventeen pages have narrow/wide captures and keyboard-focus checks. The
  1,000-event journal fixture verifies 25-event pages, selection mapping, export
  of all matches and preservation of a later filter's status. Neither external
  query completion nor physical GPU latency is measured by this result. The
  later low-priority repeat completes functional checks but misses the latency
  target (worst 2266.105 ms); its normal desktop-priority repeat also fails
  (1957.126 ms). Latest process-row/hidden-view/date-cache fixes require optimized
  verification using the corrected callback-to-paint harness. Native expansion
  checks pass stable selection, complete staged population, process links through
  excluding searches and Quit during population; they are not latency evidence.
- **Error boundaries:** 96 Rust tests passed, one legacy fixture-deletion test
  intentionally ignored; 63 Python tests passed with no skips. Review their
  scope in TESTING.md. No real cleanup, updates, process termination, cron save,
  authentication change or restart is performed by these release checks.
- **Resources:** the optimized reference comparison and ten-minute default-monitoring
  measurement pass: +10.95 MiB peak RSS over reference and 0.0467% of one core
  over 600 seconds, with verified collection stop. See [the measurements](PERFORMANCE.md)
  for binary hashes, scope and exclusions.
  A separate retained pre-rendering-fix repeat also passes (+6.891 MiB,
  0.043305% of one core over 600.388 seconds); neither result proves the latest
  artifact's resource use. Final exact-artifact reconciliation is pending.
- **Appearance/accessibility:** light/dark, high contrast, large text and
  200% scale checks cover all 17 pages (102 captures) and 174 dialog captures.
  All 324 measured dialog footer actions are within their window bounds.
  The 132 settings-review captures were repeated with preview-local reduced
  motion after finding transition frames; all six settled contact sheets were
  visually reviewed. Keyboard-focus checks
  alone do not establish every screen reader or desktop integration.
- **Delivery:** integration dry run passes. The installed legacy helper fails
  the new read-only compatibility preflight as intended. Installation must use
  matching backed-up components; authorization policy must not be broadened.
  Local README/demo rendering passes. Public-page animation verification,
  deployment and publication remain pending.

Latest fixture locations and authoritative logs are recorded in
[the chronological checkpoint](EXPANSION_CHECKPOINT.md). The
[phase plan](EXPANSION_PLAN.md) remains the completion ledger.
