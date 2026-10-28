# Functional verification

Tend is a native desktop application. Its action surface consists of GTK
callbacks, the StatusNotifierItem/DBusMenu interface, the user worker and the
constrained administrator helper. It has no HTTP server or web API.

## Safe local checks

```sh
cargo test --locked -j 1 --all-targets
cargo build --locked -j 1 --features docs-demo --bins --example qa-api
python3 -m unittest discover -s tests -v
TEND_QA_FEATURES=1 python3 docs/demo/design-checks.py matrix
python3 docs/demo/design-checks.py matrix
python3 docs/demo/design-checks.py accessibility
python3 docs/demo/design-checks.py panel
```

The native checks create a separate application instance under Xvfb with a
temporary app home and in-memory desktop preferences. They exercise real widget
callbacks, forms, confirmations and completed results. The normal app and its
settings remain untouched. See [recording and design checks](demo/README.md)
for prerequisites and fixture details. Screenshots may contain local process
names from read-only performance sampling; review before publishing.

`test_admin_sandbox.py` executes the real helper and library settings operations
in an unprivileged bubblewrap user/mount namespace. The host filesystem is
read-only, only temporary fixtures are writable, and there is no network or
desktop session connection. Package, service, crontab and cleanup commands are
recording stubs. It verifies success, validation, lock contention, failure and
rollback contracts; it does **not** install real packages or run real cleanup.
The `qa-api` example is a fixture driver and is never installed with Tend.
Process-signal contracts create disposable `sleep` children inside the private
PID namespace, check graceful/forced termination and identity guards there, and
cannot see or signal desktop applications.

`test_worker_contracts.py` uses a temporary home and fake npm/Node executables.
It verifies the worker accepts PATH-installed npm and the optional OpenCode npm
layout, propagates failures and rejects invalid phases. No real npm cache is
verified or altered. Installer tests use staging trees.

Fixtures and command transcripts are retained under `/tmp/opencode`. Test
capability skips are explicit: a missing sandbox/runtime or a host execution
policy restriction must not be described as a passed integration test.

## Coverage boundaries

Use three separate results when reporting verification:

| Check | What it establishes |
| --- | --- |
| Native workflow | The actual widgets, callbacks, state changes and fixture results work together. |
| Sandboxed contract | Actual helper/library code produces the expected requests and rollback under controlled external-command responses. |
| Live integration | A real service, session, package manager or filesystem action completed on the target machine. |

Keep live deletion, process termination, package installation, service changes,
cron saves and reboot/login testing out of routine verification. They require a
separately authorized live test. The legacy fixture-deletion Rust test is
ignored by default. A successful mocked update is not evidence that the current
repositories, network and package transaction will succeed.

File pickers, external application launches, clipboard interoperability, actual
notifications and desktop-panel rendering also depend on the user's session.
An isolated dialog or D-Bus protocol check does not establish those integrations.
Daily policy saving additionally requires the separately deployed cleanup-policy
integration; a disabled Save button is an availability result, not a successful
save. Record these limits in the delivery report.

## Expansion release gates

Build the `docs-demo` release before measurements. The native drivers accept
`--binary /absolute/path/to/a/docs-demo-binary`; this never selects the installed
app. `design-checks.py release` exercises all seventeen pages at narrow/wide
widths, checks keyboard focus and records three passes of real navigation and
log-filter callbacks to first painted feedback. The 100 ms target concerns
feedback, not completion of an animation, scan or external query. Inspect the
reported `interaction_target_met`; the driver now exits unsuccessfully after
retaining all artifacts/input checks when that metric is false. Historical runs
may have exited successfully while reporting a failed JSON gate. The run-context
also records process niceness and CPU pressure; use normal desktop priority
for interaction checks, rather than maintenance-job priority.

Use `--variant scale-200`, `large-text` or `high-contrast` with `--theme light`
or `dark` for native release/health/bundle workflows. The standalone
`accessibility` mode now captures all seventeen pages across those six variants;
new dialogs additionally need their native workflow checks and visual review.
These process-local settings do not change GNOME's saved preferences. Keyboard
checks are not proof of compatibility with every screen reader.

`docs/demo/run-budget.py` compares prebuilt optimized reference/candidate
binaries using identical common-page warmup, private panel host and Xvfb/Cairo.
It measures sixty seconds hidden idle for each, then six hundred seconds enabled
monitoring, and verifies collection stops afterward. It records binary hashes,
CPU time, peak RSS, major faults and explicit pass/fail gates. Build both binaries
before starting; do not compile or run rendering suites during measurement.
Default monitoring is covered; optional crash-query cost and physical GPU frame
latency are outside this particular measurement.

```sh
/usr/bin/python3 docs/demo/run-budget.py \
  --reference /absolute/path/to/reference-docs-demo \
  --candidate /absolute/path/to/candidate-docs-demo \
  --parent ../performance/native-fixtures \
  --output ../performance/whole-app-budget.json
```

A short harness smoke run is not a ten-minute budget result. Keep source-only
checks, sandbox contracts, installed integration and physical desktop behavior
separate in release reports. See [the expansion ledger](EXPANSION_PLAN.md).

The user installer now performs a read-only compatibility preflight before
replacing the interface or worker. `test_install_checks.py` verifies all four
roles, strict capability flags, deadline/error paths and readiness markers.
The installed legacy helper correctly fails that preflight; this is not proof
that the new system integration has been installed. The narrow installer dry run
is separately recorded, and performs no writes or authentication requests.

The release harness also loads 1,000 generated sample journal events through the
normal decoder, verifies 25-event pagination and bounded native row
materialization, then checks that a filtered selection opens the correct source
event. It restores the original sample journal bytes and timestamps. System logs
now use a native virtualized list, 25-event pages and index selections rather
than rebuilding hundreds of rows or copying full events on each filter change.
Typed journal parsing and snapshot exports run on workers; changing a filter
while exporting cannot overwrite the newer filter status.

Collapsed expander keyboard traversal is checked with GTK criticals fatal.
Tend keeps the child-visible flag aligned with expansion state: upstream
[GtkExpander focus handling](https://github.com/GNOME/gtk/blob/main/gtk/gtkexpander.c)
checks that flag before visiting content, while collapsed content is unparented.
A retained GDB reproduction identified this path. This is a targeted focus guard,
not suppression of GTK diagnostics.

The final Task Manager regression switches from process/group views to live
charts, verifies that hidden process models stay empty during collection, then
switches back and checks the latest cached process data. Charts no longer cause
hidden ColumnViews to bind hundreds of rows on every sample.

Static dialog captures use process-local reduced motion to avoid intermediate
spring/opacity frames under Xvfb. Adjacent JSON records this setting along with
scale, text DPI, theme, contrast and footer geometry. The capture driver owns a
separate process group and terminates that group on failure/timeout so a failed
preview cannot remain consuming CPU. It never signals installed applications.


Live process rows now retain GObject identity by PID/start time. New identities
populate in 25-row batches paced 32 ms apart; population is cancelled on hiding
or leaving the view. Native expansion checks verify complete sampled coverage,
selection/object reuse across refresh, process links through a prior excluding
search, no hidden process population while charting, and Quit during population.
Unit contracts cover PID reuse. Only the selected process/group/resource view
builds its widgets; captured counter histories remain shared and bounded.

Log index objects are reused within a query and calendar day. Date checks cover
Toronto midnight in both daylight and standard time, and invalidate cached rows
after a date change or reload. Timezone rules are retained per thread; relative
dates still use the current clock. These changes add no system job or permanent
collector. The short population timer is scoped to a visible, incomplete model.
