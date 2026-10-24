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
