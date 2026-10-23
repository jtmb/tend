#!/usr/bin/env python3
"""Create isolated demo inputs, or verify their preservation. Never deletes files."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import tempfile
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--parent", type=Path, default=Path("/tmp/opencode"))
parser.add_argument("--verify", type=Path, help="Verify an existing demo directory; create nothing")
args = parser.parse_args()


def fingerprint(path):
    stat = path.stat()
    return {"bytes": stat.st_size, "mtime_ns": stat.st_mtime_ns,
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


if args.verify:
    root = args.verify.resolve()
    expected = json.loads((root / "manifest.json").read_text())
    actual = {name: fingerprint(root / name) for name in expected}
    if actual != expected:
        raise SystemExit("Demo inputs changed or were removed")
    print(f"All {len(expected)} demo input files preserved byte-for-byte; no cleanup executed")
else:
    args.parent.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix="tend-workflow-", dir=args.parent))
    home = root / "home"
    config = home / ".config/tend"
    cache = home / "sample-cache"
    for directory in (config, cache, root / "frames", root / "cache", root / "data"):
        directory.mkdir(parents=True, exist_ok=True)
    now = int(time.time())
    for name, mib, days in (("old-shader.cache", 2, 30), ("old-download.cache", 1, 21),
                            ("recent-download.cache", 1, 2)):
        path = cache / name
        with path.open("xb") as stream:
            for _ in range(mib):
                stream.write(b"Tend public demo cache\n".ljust(1024 * 1024, b"."))
        os.utime(path, (now - days * 86400, now - days * 86400))
    (config / "rules.json").write_text(json.dumps({"rules": [{
        "name": "Sample cache · preview only", "folder": str(cache), "pattern": "*.cache",
        "days": 14, "recursive": False, "enabled": True}]}))
    (config / "cleanup.json").write_text(json.dumps(dict(
        apt=False, docker_build=False, docker_images=False, npm_logs=False,
        pip=False, thumbnails=False, trash=False, age_days=14, keep_gb=5)))
    messages = [
        ("demo-editor.service", 3, "Language server stopped unexpectedly; workspace is preserved."),
        ("demo-build.service", 3, "Build failed: a dependency could not be resolved."),
        ("demo-build.service", 4, "Incremental build cache is stale; a rebuild is recommended."),
        ("demo-network.service", 4, "Download delayed; retrying the connection."),
        ("demo-maintenance.service", 6, "Cleanup preview finished. No files were removed."),
        ("demo-editor.service", 6, "Workspace indexing completed successfully."),
    ]
    rows = [{"__REALTIME_TIMESTAMP": str((now - i * 60) * 1000000),
             "PRIORITY": str(priority), "_COMM": unit.removesuffix(".service"),
             "_SYSTEMD_USER_UNIT": unit, "_PID": str(4100 + i), "MESSAGE": message}
            for i, (unit, priority, message) in enumerate(messages)]
    (home / "demo-journal.jsonl").write_text("\n".join(json.dumps(row) for row in rows))
    inputs = [path for path in home.rglob("*") if path.is_file()]
    (root / "manifest.json").write_text(json.dumps(
        {str(path.relative_to(root)): fingerprint(path) for path in inputs}, indent=2))
    print(root)
