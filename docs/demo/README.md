# Recording the README demo

The hero GIF is captured from real GTK widgets in a separate, non-unique Tend
preview on Xvfb. It is an automated UI walkthrough, not a benchmark. Performance
counters are live; no sample graphs or log records are fabricated. No private
logs are loaded. No settings, schedules or cleanup policies are saved.

The capture code is behind the `docs-demo` Cargo feature and is **excluded from
normal builds**. It only changes the visible page, selects resource rows,
changes in-memory filters, scrolls and invokes the read-only resource refresh.
It never invokes cleanup, updates, process termination, privileged actions or
restart hooks. The installed Tend instance is not affected.

Optional authoring dependencies: `xvfb` and Python Pillow.

From the repository root:

```sh
cargo build --locked --features docs-demo --bin tend -j 1
demo_dir=$(mktemp -d /tmp/tend-demo.XXXXXX)
mkdir -p "$demo_dir"/frames "$demo_dir"/config "$demo_dir"/cache "$demo_dir"/data
xvfb-run -a -s '-screen 0 1280x900x24' env \
  DBUS_SESSION_BUS_ADDRESS="unix:path=$demo_dir/no-session-bus" \
  GDK_BACKEND=x11 GSK_RENDERER=cairo GTK_A11Y=none \
  GSETTINGS_BACKEND=memory GIO_USE_VFS=local \
  XDG_CONFIG_HOME="$demo_dir/config" XDG_CACHE_HOME="$demo_dir/cache" \
  XDG_DATA_HOME="$demo_dir/data" TEND_DEMO_DIR="$demo_dir/frames" \
  target/debug/tend
python3 docs/demo/encode.py "$demo_dir/frames" assets/demo.gif
```

The capture warms up real counters, then records a 32-second walkthrough at
eight output frames per second. On a busy machine the capture may take longer
than playback. The encoder adds explanatory captions and a shared
palette for crisp text. Review all scenes for private data and correct rendering
before committing the GIF. The static screenshot gallery remains available
below the demo for readers who prefer still images.
