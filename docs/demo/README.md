# Recording a working README demo

The GIF records real actions and their completed results in a separate native
Tend preview. It runs the ordinary cleanup preview against isolated sample files,
loads a public sample journal through the production decoder, filters six events
to two errors, types `editor` to leave one match, and opens that event's details.
CPU and memory readings are live. Sample data is disclosed in the captions.
The recording uses process-local reduced motion and the normal 1100×760 window
to avoid headless frame-clock transitions; it does not change desktop preferences.

The `docs-demo` Cargo feature is excluded from normal builds. It supplies an
isolated application home and a sample journal input; it does not replace preview
or filtering logic. Deletion, process signals and privileged helper calls are
blocked in the recording build. The choreography never invokes apply, save,
export, scheduling or restart actions. The installed Tend instance is untouched.

Optional authoring dependencies: `xvfb` and Python Pillow. From the repository root:

```sh
cargo build --locked --features docs-demo --bin tend -j 1
demo_dir=$(python3 docs/demo/prepare.py)
xvfb-run -a -s '-screen 0 1280x900x24' env \
  DBUS_SESSION_BUS_ADDRESS="unix:path=$demo_dir/no-session-bus" \
  GDK_BACKEND=x11 GSK_RENDERER=cairo GTK_A11Y=none \
  GSETTINGS_BACKEND=memory GIO_USE_VFS=local \
  XDG_CONFIG_HOME="$demo_dir/home/.config" XDG_CACHE_HOME="$demo_dir/cache" \
  XDG_DATA_HOME="$demo_dir/data" TEND_DEMO_HOME="$demo_dir/home" \
  TEND_DEMO_DIR="$demo_dir/frames" target/debug/tend
python3 docs/demo/prepare.py --verify "$demo_dir"
python3 docs/demo/encode.py "$demo_dir/frames" assets/demo.gif
```

`prepare.py` creates a fresh directory on each run and never deletes anything.
Two cache files (3 MiB total) are older than the 14-day rule; a third is recent.
The verification step compares SHA-256, file sizes and modification timestamps of
all six inputs after recording. Leave the fixtures in place when deletion is
prohibited. Do not reuse a frame directory: capture refuses stale frame sets.

The recording asserts the preview result, 6 → 2 → 1 event counts and selected
process details. `evidence.json` records these outcomes; `actions.json` records
the actual widget targets. The encoder requires this evidence before generating
a 38-second looping GIF. Its pointer highlights mapped targets for the real
callbacks; it does not add fake operations or replace app output. The shared
palette reserves thin chart hues and merges unchanged frames without changing
the total playback time.

Inspect decoded scenes after every completed action, then review the GIF inside
the rendered README at desktop and narrow widths. Check for private data,
clipped controls, readable text and a working loop. The static gallery provides
a still-image alternative. Capture may take longer than playback on a busy host. The recorder runs below
GTK frame-clock priority so PNG encoding cannot starve allocation or scrolling.

## Policy editor and design checks

With a **fresh** prepared directory and the same isolated environment above,
set `TEND_QA_DIR="$demo_dir/qa"` as well as `TEND_DEMO_DIR` to run the native
interaction checks instead of GIF recording. This mode intentionally edits only
the fixture policy files: save/reload, discard, unsaved defaults, stale previews,
independent policies, confirmation cancellation, and malformed-file errors.
Set `TEND_QA_STATE_ONLY=1` for a focused replay of interactions and error states
after a complete matrix run.
It captures all 17 pages at desktop/dark and narrow/light sizes, both cleanup
page bottoms, and the rule/confirmation/error states. It exits the separate
preview through the real Quit action. No cleanup, updates, cron save, system
policy save or process action is activated. Cache files and sample journal
contents remain unchanged; the policy-file manifest will differ by design.
Do not reuse this intentionally corrupted QA home for the public demo.

`cargo test --locked -j 1 --all-targets` skips the legacy isolated file-deletion
test by default. It is marked ignored because preview-only verification must
retain fixtures. Daily-runner command tests use mocks; they never spawn the
listed maintenance commands. Python checks: `python3 -m unittest discover -s tests -v`.

## Complete design matrix and panel checks

After building with `docs-demo`, these helpers create a fresh fixture per invocation
and retain every input and capture:

```sh
python3 docs/demo/design-checks.py matrix
python3 docs/demo/design-checks.py accessibility
python3 docs/demo/design-checks.py panel
python3 docs/demo/design-checks.py gallery
python3 docs/demo/design-checks.py demo
```

The matrix renders every page at 720, 960 and 1280 window pixels in both themes
(client captures exclude window shadows). It checks footer bounds, real keyboard
traversal, defaults/discard, failed manual and cron saves, Appearance rollback,
lossless daily/weekly/custom cron initialization and existing cleanup guards.
Shared-footer test adapters cover saving, success, failure, invalid and unavailable
states without a system write. Cron writes are explicitly blocked in this build.
Accessibility captures cover all seventeen pages at 200% device scale, 135% text size
and native high contrast, light and dark. Adjacent JSON records the actual scale,
DPI, contrast mode and footer visibility. These are process-local settings. Static dialog captures additionally disable
animations in that preview process and wait for a fresh paintable image.
Animation timing on the physical desktop is a separate check.

The panel check uses a private bus with **no service activation directories**.
A fake host inspects Tend's real StatusNotifierItem and DBusMenu objects, observes
the actual Close button hiding the preview, opens it through the menu, then proves
Quit exits that preview process. It never touches the user's session bus.


## Expansion checks

`python3 docs/demo/design-checks.py expansion` runs the phase 1–3 workflows in a
fresh retained preview: two metadata scans and file-change details; workspace
add/edit/save/discard/remove, sampled cwd matching, Compose label fixtures and
cached storage; expandable app/process views; and marked diagnostic recordings,
saving, loading, comparison and cancellation. It preserves all six prepared
inputs and never closes the installed Tend instance. Preview assertion failures
have core dumps disabled for this test process tree.

`cargo build --locked -j 1 --features docs-demo --example resource-probe --example index-probe`
builds optional measurement tools. `resource-probe 600` measures the shared
collector for ten minutes, then verifies that it settles after demand ends.
`index-probe 1000000` writes generated metadata to a new retained private fixture;
it measures indexing memory and duration, not actual filesystem traversal speed.
The fixture uses a 2048 MiB history budget and creates no synthetic content files.
These tools are excluded from normal app builds and installation.


## Whole-app release checks

`design-checks.py release --binary target/release/tend` measures native
navigation and local log-filter feedback, then captures all seventeen pages at
720 and 1100 pixels with keyboard-focus checks. The JSON records actual device
scale, text DPI and contrast. Add `--variant scale-200`, `large-text` or
`high-contrast`, and `--theme light` or `dark`, for process-local variants.
The same flags work with the health, cleanup, expansion and bundle workflows.
No installed app, desktop preference or job is changed.

See [Functional verification](../TESTING.md#expansion-release-gates) for the
optimized whole-app reference comparison and ten-minute monitoring measurement.
The `budget_probe.rs` and `release_qa.rs` modules are compiled only with
`docs-demo`; neither is included in installed builds.
