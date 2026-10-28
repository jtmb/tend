# Performance verification

An earlier optimized expansion candidate passes the default-monitoring resource
gates and native interaction test below. Later optimized repeats miss the
interaction target at both low and normal application priority. The latest
rendering/date-cache source changes require a new optimized measurement; they
are not established as passing by earlier results. These results describe controlled checks
on the development machine, not every workload or physical GPU configuration.

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
