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

- **Interaction:** the current optimized candidate
  `tend-selection-hint-0.3.0-docs` passes **65** actual native interactions,
  worst **84.769 ms**, under Xvfb/Cairo. This covers three visits to each of the
  seventeen pages, small/1,000-event journal filtering, and opening/closing the
  visible folder-rule disclosure (29.116 / 14.709 ms). First feedback is
  36.866 ms for Task Manager and 58.308 ms for Diagnostics; Disk cleanup is
  84.769 ms on its first visit and 37.242 ms when revisited. A separate
  functional workflow records first actual process rows at 132.114 ms and all
  301 sampled rows at 1245.162 ms, verifies waiting state before data, identity
  and selection retention, clears stale details when filtering removes the
  selection, and cancels sampling when leaving before paint. First feedback is
  not data-collection completion or physical GPU latency. Earlier failures and
  the three blocked builds remain in the chronological checkpoint; their gates
  were not weakened to obtain this result.
- **Error boundaries:** the previous artifact's checkpoint records 109 Rust tests
  (106 library plus three binary) and 63 Python tests, one legacy
  fixture-deletion test intentionally ignored. The current tree passes **110
  Rust tests** (107 library plus three binary) and **66 Python tests** with no
  skips; the added Rust contract injects `SQLITE_FULL` privately rather than
  filling the host filesystem. Python fixtures must live on a filesystem with
  at least 2 GiB free because the production history guard correctly refuses
  writes below that. Review their scope in TESTING.md. No real cleanup, updates,
  process termination, cron save, authentication change or restart is performed
  by these release checks.
- **Resources:** the current candidate passes all nine gates: 168.48 MiB enabled
  peak RSS versus 169.54 MiB reference, and 0.026663% of one core over 600.075
  seconds, with verified collection stop and zero subscriptions. See
  [the measurements](PERFORMANCE.md) for the artifact hash, scope and
  exclusions. Both the Linux unit contract and read-only inspection of the
  optimized native preview verify the GUI priority is unchanged while its
  collector runs at a lower priority.
- **Appearance/accessibility:** fresh light/dark, high contrast, large text and
  200% scale captures cover all 17 pages (102 images) for this artifact, and a
  repeated six-variant dialog sweep adds 174 dialog captures for cleanup,
  settings-bundle and health workflows. The earlier checkpoint's 324
  footer-bound checks remain evidence for that source; their inspection was not
  repeated in this sweep. Keyboard-focus checks alone do not establish every
  screen reader or desktop integration.
- **Delivery:** matching production `tend`, `tend-worker` and `tend-admin`
  binaries were built from this tree and installed. The root integration was
  deployed through the documented administrator path with adjacent backups and
  a rollback manifest; the user interface/worker followed with no password. All
  fifteen protected schedule/login/restart fingerprints match the baseline. No
  authorization policy was broadened. README stills and the demo GIF were
  refreshed from this artifact and reviewed; the public README check follows
  publication.

Latest fixture locations and authoritative logs are recorded in
[the chronological checkpoint](EXPANSION_CHECKPOINT.md). The
[phase plan](EXPANSION_PLAN.md) remains the completion ledger.


The recorded OpenCode-terminal memory-failure acceptance case adds eleven Rust
contracts, bringing the latest core run to 108 passing tests plus one ignored
legacy destructive test. The real requested journal backend explains the
historical kernel kills; isolated native workflows cover explanations, local
exports, PID filters, Activity categories and missing context across standard,
large-text, 200% and high-contrast variants. Existing activity, private-bus
health alerts and stale-rule editor regressions pass. This is functional/visual
evidence for the recorded source, not an updated optimized release budget.
See the latest checkpoint for private artifact names and remaining delivery gates.
