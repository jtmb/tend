# Performance verification

An earlier optimized expansion candidate passes the default-monitoring resource
gates and native interaction test below. The final source is still being
reconciled; its latest low-priority interaction run did not meet the target. These results describe controlled checks
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

The final docs candidate is `123ef37eee56c578b687b191de6b41ef1bc8580feb71ea47ecaf2755efbf7a9b`.
All 63 functional interaction observations and journal checks complete, but its
latest low-priority run misses the 100 ms target: worst first feedback is
2266.105 ms on initial Task Manager navigation; some other navigation/filter
observations also exceed 100 ms. This is a failed latency gate, despite the
native process exiting successfully. CPU pressure was around 50% while unrelated
existing apps remained running. The test inherited nice 10; the desktop app
normally runs at nice 0. A normal-priority comparison is required before attributing
the difference to a code regression or claiming the final interaction gate.

The ten-minute resource repeat uses the frozen optimized candidate and reference,
without compilation or native rendering suites alongside it. Its final result
is pending. Earlier measurements remain retained and are not overwritten.
