# Settings transfer

Open **Preferences → Settings transfer** to export preferences or import a
versioned Tend settings file. Exported files include project and protected paths;
keep them private when those names are sensitive.

## Export

**Export settings** writes a local `tend-settings.json` file with owner-only
permissions. It includes readable settings and effective defaults. Unreadable
sections are omitted with a visible explanation in Details.

Supported sections:

- Appearance and window behavior
- Health monitoring
- History limits
- Registered workspaces
- Manual and cron cleanup categories and retention
- Daily cleanup policy
- Additional folder cleanup rules
- User cleanup schedule
- Maintenance timing and reminders
- Global cleanup exclusions

Credentials, passwords, authentication rules, restart/login policies, raw
history, diagnostic sessions and machine-level tuning are not included.

## Import and review

1. Choose **Import settings** and a local file. Tend validates the complete
   bundle, including its version, fields, ranges and size limit.
2. Review the section comparison. **Previous changes / Next changes** expose
   every changed field, including large rule and protected-path collections.
3. Choose **Load as inactive drafts**. This only saves a private draft file.
   Existing settings and jobs remain active as previously configured.
4. Choose a section and **Review selected section**. Its native editor allows
   changes before **Save changes**. **Discard changes** restores the currently
   saved section. Arrays support selection, adding and removal from the draft.
5. Repeat for the remaining sections. **Review imported drafts** reopens the
   saved drafts after closing the dialog or app.

Monitoring, cleanup schedules, maintenance activation and additional folder
rules start disabled in imported drafts. Users can explicitly enable them while
reviewing. Category policies can affect existing enabled jobs after saving; the
review explains this. Loading or previewing an import never runs cleanup,
maintenance, package updates or a restart.

Each section saves separately. A changed user configuration is detected under
its existing save lock, preserving concurrent changes and the review draft.
Existing unsaved page edits must be saved or discarded before importing that
section. Successful saves refresh the related page and invalidate affected
cleanup previews.

Maintenance imports preserve the existing after-maintenance behavior. Shared
exclusions keep the receiving machine's revision and require matching helper,
workers and daily integration. System saves use the existing narrow helper and
never open an authentication prompt when authorization is unavailable.

## Format and limits

The JSON envelope uses `format: "tend-settings"`, schema `version: 1`, an export
timestamp and a whitelisted map of sections. Unknown sections/fields and future
versions are rejected. The file is limited to 1 MiB, with section-specific count
and size limits. Import does not follow a leaf symlink or read a non-regular file.

Inactive drafts live in `~/.config/tend/import-drafts.json`, with owner-only
permissions. That file is read on request, never activated at startup. The
normal settings files remain authoritative after a reviewed save.

## Verification

The optional `docs-demo` build exercises real Export/Import buttons and all ten
native section editors in a fresh fixture. Only the file picker's returned path
is supplied by the harness. No desktop service, installed application or live
cron entry is modified.

```sh
cargo build --features docs-demo --bins --examples -j1
python3 docs/demo/design-checks.py bundles --parent ../performance/native-fixtures
```

The test covers inactive drafts, paged comparisons, Save/Discard, invalid input,
existing drafts, concurrent changes, stale controls, cancellation, export errors,
late callbacks and private file permissions. Six user-setting writers also run
inside a networkless mount/PID namespace with recorded external commands; stale
saves leave existing settings and unrelated cron entries unchanged. System
saves are unavailable in the native preview and have separate helper contracts.
