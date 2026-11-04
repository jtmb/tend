# Performance verification

The latest optimized candidate passes the interaction and default
background-monitoring gates below. These are controlled Xvfb/Cairo checks on
the development machine, not guarantees for every workload or physical GPU.
Matching production builds, deployment and publication remain separate release
gates.

## Current candidate — November 2, 2026

Artifact: `tend-selection-hint-0.3.0-docs`, SHA-256
`e2dc3939f7737f8c57a832a1d359982b6854f49b4e3d6c0ae63cd57afbd848a7`,
4,706,904 bytes, built from this working tree with the `docs-demo` feature. This
artifact includes the selection-hint fix and the added SQLITE_FULL/native
regression checks, so the results below describe the current source rather than
an earlier checkpoint.

| Measurement | Result | Gate |
| --- | --- | --- |
| Native first painted feedback | Worst 84.769 ms across 65 interactions | <100 ms |
| First Task Manager feedback | 36.866 ms | <100 ms |
| First Disk cleanup feedback | 84.769 ms first visit, 37.242 ms on revisit | <100 ms |
| First Diagnostics feedback | 58.308 ms | <100 ms |
| Visible folder-rule open / close | 29.116 / 14.709 ms | <100 ms |
| Reference hidden idle peak RSS | 169.54 MiB over 60.073 s | Same baseline workload |
| Candidate hidden idle peak RSS | 168.48 MiB over 60.055 s | Additional ≤32 MiB |
| Enabled hidden monitoring peak RSS | 168.48 MiB over 600.075 s | Additional ≤32 MiB |
| Enabled CPU, one-core share | 0.026663% | <0.5% |
| Major faults during measured intervals | 0 | Reported observation |

All nine whole-app gates pass, including panel registration, zero subscriptions
and verified collection stop. There were 45 shared collections including warmup
plus 45 panel layout/tooltip signals; no notice or history write occurred during
the measured run, and optional crash reads were off. No compiler or rendering
suite ran alongside measurement. The candidate's enabled peak is 1.05 MiB below
this reference; that is an observation of this pair, not a claim of guaranteed
memory savings. Idle CPU stayed below the kernel counter's resolution, which is
not the same as permanently zero.

Building this artifact required three guarded attempts: two were deferred while
host swap was nearly exhausted, and one was killed by systemd-oomd after 4m22s
when system-wide memory pressure stayed above 50% for 20 seconds (build peak
797 MiB, inside the guard's own 1 GiB cap). The successful build completed in
5m25s. The blocked attempts are retained; no compiler error occurred.

The same artifact passes the native workflows used for the feature checks:

- Expansion: 301 sampled processes; first actual rows at 132.114 ms, complete
  rows at 1245.162 ms. Waiting state before data, automatic sampling after
  paint, selection clearing with disabled actions, identity/selection retention
  and Quit during population all pass.
- Release: 65 interactions, 34 page captures, four expanded retention captures,
  keyboard traversal, 1,000-record journal paging/selection/export and all 206
  fixture cleanup candidates.
- Accessibility: 102 page captures across the six theme/large-text/high-contrast/
  200%-scale variants pass. A repeated six-variant dialog sweep (cleanup,
  settings-bundle and health workflows, light and dark) adds 174 dialog captures
  with no failure.
- Settings, health, panel, activity, memory-postmortem, rules and feature-matrix
  workflows pass. No real cleanup, notification, schedule or process signal was
  performed.

Private evidence in the sibling performance directory:

- `tend-selection-hint-whole-app-budget.json`
- `budget-artifacts/tend-selection-hint-0.3.0-docs`
- `tend-release-current-release-native.log`
- `tend-release-current-expansion-opt.log`
- `tend-release-current-cleanup-opt.log`
- `tend-release-current-accessibility-opt.log`
- `tend-release-current-bundles-opt.log`
- `tend-release-current-health-opt.log`
- `tend-release-current-panel-opt.log`
- `tend-release-current-activity-opt.log`
- `tend-release-current-matrix-opt.log`
- `tend-release-current-memory-opt.log`
- `tend-release-current-rules.log`
- `tend-release-current-budget.log`

Fixtures: `tend-workflow-wpu9ijc5` (reference), `tend-workflow-ffy4s4bq`
(candidate resources), `tend-workflow-6d23o73a` (release),
`tend-workflow-4lu4zbx2` (expansion), `tend-workflow-c7spk7qg` (accessibility),
`tend-workflow-411brtzg` (cleanup) and `tend-workflow-37or65ts` (settings
bundles).

## Previous candidate — October 28, 2026

Artifact: `tend-task-waiting-0.3.0-docs`, SHA-256
`777ceafde5d5fed3ce8782b9761c2a5a7c3dccc581651cc2cd95fc4361ff0bd6`.
It remains retained evidence for its own source checkpoint and predates the
selection-hint fix and added checks above.

| Measurement | Result | Gate |
| --- | --- | --- |
| Native first painted feedback | Worst 89.290 ms across 65 interactions | <100 ms |
| First Task Manager feedback | 37.601 ms | <100 ms |
| First Disk cleanup / Maintenance feedback | 33.736 / 32.756 ms | <100 ms |
| Visible folder-rule open / close | 39.377 / 19.358 ms | <100 ms |
| Reference hidden idle peak RSS | 172.61 MiB over 60.058 s | Same baseline workload |
| Candidate hidden idle peak RSS | 169.98 MiB over 60.055 s | Additional ≤32 MiB |
| Enabled hidden monitoring peak RSS | 169.98 MiB over 600.077 s | Additional ≤32 MiB |
| Enabled CPU, one-core share | 0.043328% | <0.5% |
| Major faults during measured intervals | 0 | Reported observation |

A separate native functional workflow loads all 292 sampled processes: first
actual rows at 135.203 ms and complete rows at 930.256 ms.

Private evidence: `tend-task-waiting-manifest.json`,
`tend-task-waiting-whole-app-budget.json`,
`tend-task-waiting-release-native.log`, `tend-task-waiting-expansion.log` and
`tend-task-waiting-rust.log`. Fixtures: `tend-workflow-ye71265b` (latency/layout),
`tend-workflow-y_xbyimt` (full workflows), `tend-workflow-fnrubgf8` (reference)
and `tend-workflow-v81f4w7i` (candidate resources).

Earlier measurements below remain evidence for their recorded artifacts.

## Whole application

Both binaries use the release profile, the same kernel (7.0.0-42-generic),
GTK/Xvfb with Cairo, a private session bus/panel host and a fresh app home. Each
visits the same six existing pages, hides the window, then settles before the
sixty-second idle measurement. The candidate then enables default health
monitoring, warms up, runs for ten minutes and disables it. No compiler or
rendering suite runs during that measurement.

| Measurement | Reference | Expansion | Gate |
| --- | ---: | ---: | --- |
| Hidden idle peak RSS | 172.18 MiB | 183.12 MiB | Additional ≤32 MiB |
| Enabled monitoring peak RSS | — | 183.12 MiB | Additional ≤32 MiB over reference |
| Enabled CPU, one-core share | — | 0.0467% | <0.5% |
| Enabled interval | — | 600.09 seconds | ≥600 seconds |
| Major faults during each interval | 0 | 0 | Reported observation |

The expansion's peak RSS increase is **10.95 MiB**
over the reference. This is an incremental figure; it is not total application
memory. Idle CPU time did not advance at the kernel counter's resolution during
the separate sixty-second intervals. That is not a claim of permanent zero CPU.

The private panel registered in both runs. The candidate completed 45 shared
collections across warmup/monitoring, returned to zero subscriptions, then showed
no further collection over a complete subsequent interval. Both owned previews
exited normally. No history database was created without an event/recording.

Scope: default monitoring, **optional crash reads off**. This result does not
measure journal-query subprocess cost, physical desktop notifications, GPU frame
latency, filesystem traversal speed or whether tuning improves a user's workload.
No live cleanup, updates, reboot or login change was performed.

Reference source: `06a6aae00e5eeff3dffcc01325a84bcd59b420a0`, with the same
common-page measurement choreography added. Retained binary SHA-256:

- Reference: `c595148924421cf6388bfb0d5059e99a8acef83dff3c6fdf5b77ea87cccc1db9`
- Candidate: `24b7beebec092b2c4920657fc0679b5edbdb89686b7d68dc6254cfa2682d5d0c`

Raw JSON, per-run inputs and build logs are retained in the local delivery
record outside the checkout. Reproduce with
[the whole-app budget driver](demo/run-budget.py), following
[its scope and build instructions](TESTING.md#expansion-release-gates).

## Native interactions

The optimized candidate passes 63 real navigation/filter callbacks to first
painted feedback: three passes across seventeen pages, six small-journal filters
and six 1,000-event journal filters. Worst feedback is **90.711 ms**, against the
100 ms target, under Xvfb/Cairo. This measures initial feedback, not completion
of animations or asynchronous external operations.

The same run checks 25-event result pages, mapping a filtered selection back to
the correct source event, exporting all 500 matching records across pages, and
preserving a later filter's status after an earlier export completes. All
seventeen pages have narrow/wide captures and keyboard-focus checks with GTK
criticals fatal. Original sample files and timestamps are preserved.

## Storage indexing

The current optimized million-entry generated metadata probe passes: **0.66 MiB**
additional peak process RSS against the 128 MiB target, **49.13 seconds**
and **401.77 MiB** of database/sidecar files.
The 2048 MiB test history budget is explicit; the normal default remains 512 MiB.

This is generated metadata indexing, not filesystem traversal speed. Process RSS
does not include all filesystem cache or the database's tmpfs storage. The probe
used memory-backed `/tmp`; after it exited, its fixture was moved to the retained
on-disk evidence folder and all database bytes were verified. It therefore does
not keep occupying tmpfs space. These timings do not establish disk scan or disk
write throughput. The raw result and relocation fingerprints are retained in
`tend-expansion-million-entry-index-release.json` in the local delivery record.


## Final source reconciliation in progress

The retained pre-rendering-fix docs candidate is `123ef37eee56c578b687b191de6b41ef1bc8580feb71ea47ecaf2755efbf7a9b`.
All 63 functional interaction observations and journal checks complete, but its
latest low-priority run misses the 100 ms target: worst first feedback is
2266.105 ms on initial Task Manager navigation; some other navigation/filter
observations also exceed 100 ms. This is a failed latency gate, despite the
native process exiting successfully. CPU pressure was around 50% while unrelated
existing apps remained running. The test inherited nice 10; the desktop app
normally runs at nice 0. A subsequent normal-priority comparison also misses the
target: the earlier candidate's worst feedback is 252.237 ms and this candidate's
is 1957.126 ms, under CPU pressure of approximately 49–65%. Those measurements
do not isolate code cost from scheduling; neither is a release pass.

The ten-minute resource repeat for that retained binary completes without
compilation or native rendering suites alongside it. All nine gates pass:
reference idle peak 171.242 MiB, candidate idle peak 178.125 MiB, enabled peak
178.133 MiB (+6.891 MiB over reference), and 0.043305% of one core over
600.388 seconds. It records 45 collections, zero subscriptions and verified stop.
There are 18 major faults in the enabled interval and one actual sustained-event
notification/history record on the private test bus; no real desktop notice is
sent. The idle intervals record no CPU-counter advance or major faults.

These figures are retained in `tend-expansion-whole-app-budget-final.json` and
are separate from the earlier +10.95 MiB/0.0467% result above. They are not
measurements of the latest process-row, hidden-view or date-cache fixes at
source checkpoint `df410c4`. A corrected native harness excludes widget lookup
and assertion traversal from the callback-to-paint interval. Current source
reuses live process rows, paces new rows, avoids hidden-view rendering and caches
timezone/log identities with midnight invalidation. Its latest exploratory debug
run still exceeds 100 ms in some views. Final optimized latency and exact-artifact
resource gates remain pending; no failed run is overwritten or discarded.

## Retention-source measurement and remaining work

Source checkpoint `2f698ae` keeps cleanup retention controls in native
disclosures with current-draft summaries. Its retained docs binary SHA-256 is
`3c8265fffd46d4adf98d16155bf279eef155bfa943c0397e3d0176588be73ca9`.
The optimized native workflow completes 63 observations, 34 page captures, four
expanded retention captures, all ten field edit/discard cases and the journal
selection/pagination/export checks. The wall-time gate still fails: Task Manager
110.989 ms and System logs 127.686 ms; the other 61 observations are below 100 ms.
Their main-thread CPU times are 45.357 and 36.923 ms respectively. Host CPU
pressure is approximately 37–46%. CPU time alone does not identify every wait
or exempt it from the interaction gate.

Disk cleanup and Daily cleanup first feedback are 73.792 and 68.786 ms, using
64.014 and 64.558 ms of main-thread CPU. The preceding instrumented build uses
about 80 ms CPU for those forms. These recorded runs support a smaller initial
layout cost; they do not prove every interaction meets the target.

The separate whole-app run for this exact retained artifact passes all nine
gates in `tend-expansion-whole-app-budget-retention.json`:

| Measurement | Reference | Retention candidate |
| --- | ---: | ---: |
| Hidden idle peak RSS | 170.71 MiB | 167.17 MiB |
| Enabled monitoring peak RSS | — | 168.16 MiB |
| Enabled CPU, one-core share | — | 0.04330% |
| Enabled interval | — | 600.407 seconds |
| Major faults during enabled interval | — | 63 |

The enabled peak is 2.55 MiB below the reference in this pair, and 0.99 MiB above
the candidate's own hidden idle peak. Negative incremental RSS is an observation
of this run, not a promise that enabling features reduces memory. Default
monitoring still has optional crash reads off. Both idle intervals record no
CPU-counter advance or major faults. There are 45 collections, two notices on
the private test bus, zero subscriptions after disabling and verified collection
stop; original fixture inputs remain unchanged. No real desktop notification or
live system action is performed.

Newer source defers automatic full process sampling until Task Manager's first
paint and cancels that pending start when leaving/hiding. The log-results stack
also stops measuring its hidden view, and only the collector thread yields at
lower Linux priority. The optimized artifact SHA-256
`e3daa2a8c7123bf6bcf81faa95f37a31b3d263934677f32360ce8c592a0a6726`
passes all expanded native workflows and 97 Rust checks, but its interaction
repeat still fails: Disk cleanup 111.186 ms and Maintenance 109.361 ms, with all
other 61 observations below 100 ms. Their main-thread CPU is 71.126/75.719 ms and
kernel run-queue wait is 33.865/30.395 ms. Waiting contributes to the observed
delay; it is not subtracted from the gate. Task Manager first feedback is
78.648 ms. Runtime inspection confirms GUI nice 0 and sampler nice 5.

These newer changes are **not** represented by the resource figures above and
require their own final resource reconciliation. Updated accessibility/demo and
delivery remain pending. Unavailable test-only run-queue readings are null.
