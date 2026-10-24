#!/usr/bin/python3
"""Exercise Tend's real panel protocol on an isolated dbus-run-session bus.

Run under dbus-run-session and xvfb-run with prepared TEND_DEMO_* / XDG paths.
Never points at the desktop bus, changes schedules, or invokes a system action.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import gi
from gi.repository import Gio, GLib

if not os.environ.get('TEND_PANEL_QA') or not os.environ.get('TEND_DEMO_DIR'):
    raise SystemExit('Prepare an isolated demo home and set TEND_PANEL_QA to its evidence file')
# dbus-run-session creates a socket below /tmp; refuse an ordinary user bus.
address = os.environ.get('DBUS_SESSION_BUS_ADDRESS', '')
if '/run/user/' in address or 'unix:path=/tmp/' not in address:
    raise SystemExit('Use a fresh private bus: dbus-run-session -- xvfb-run ...')
conn = Gio.bus_get_sync(Gio.BusType.SESSION, None)
info = Gio.DBusNodeInfo.new_for_xml('''<node><interface name="org.kde.StatusNotifierWatcher">
<method name="RegisterStatusNotifierItem"><arg type="s" direction="in"/></method>
<property name="IsStatusNotifierHostRegistered" type="b" access="read"/>
<property name="ProtocolVersion" type="i" access="read"/>
</interface></node>''')
item = {}


def register(connection, sender, path, interface, method, parameters, invocation):
    item['sender'] = sender
    item['path'] = parameters.unpack()[0]
    invocation.return_value(GLib.Variant('()', ()))


conn.register_object('/StatusNotifierWatcher', info.interfaces[0], register,
                     lambda *args: GLib.Variant('b', True) if args[-1] == 'IsStatusNotifierHostRegistered' else GLib.Variant('i', 0), None)
owner = Gio.bus_own_name_on_connection(conn, 'org.kde.StatusNotifierWatcher', Gio.BusNameOwnerFlags.NONE, None, None)
child = subprocess.Popen([str(Path('target/debug/tend').resolve())], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
loop = GLib.MainLoop()
path = Path(os.environ['TEND_PANEL_QA'])
started = time.monotonic()
state = 0
failure = None


def call(interface, method, signature, values, object_path='/Menu'):
    return conn.call_sync(item['sender'], object_path, interface, method,
                          GLib.Variant(signature, values), None, Gio.DBusCallFlags.NONE, 3000, None)


def tick():
    global state, failure
    try:
        if time.monotonic()-started > 22:
            raise AssertionError('Panel smoke test timed out')
        if child.poll() is not None:
            if state != 3 or child.returncode != 0:
                raise AssertionError(f'Preview exited unexpectedly ({child.returncode})')
            report = json.loads(path.read_text())
            report.update(protocol_properties=True, menu_layout=True, quit_exits=True, private_bus=True)
            path.write_text(json.dumps(report, indent=2)+'\n')
            print(json.dumps(report))
            loop.quit()
            return False
        if not item or not path.exists():
            return True
        report = json.loads(path.read_text())
        if state == 0 and report.get('closed_to_panel'):
            props = call('org.freedesktop.DBus.Properties', 'GetAll', '(s)', ('org.kde.StatusNotifierItem',), '/StatusNotifierItem').unpack()[0]
            assert props['Title'] == 'Tend' and props['Status'] == 'Active'
            assert props['IconName'] == 'io.github.jtmb.Tend-symbolic'
            assert props['Menu'] == '/Menu'
            layout = call('com.canonical.dbusmenu', 'GetLayout', '(iias)', (0,-1,[])).unpack()
            assert layout[0] == 1 and len(layout[1][2]) == 3
            labels = [row[1].get('label') for row in layout[1][2]]
            assert labels == ['Open Tend',None,'Quit Tend'], labels
            call('com.canonical.dbusmenu', 'Event', '(isvu)', (1,'clicked',GLib.Variant('i',0),0))
            state = 1
        elif state == 1 and report.get('closed_again'):
            Gio.bus_unown_name(owner)
            state = 2
        elif state == 2 and report.get('recovered_host_loss'):
            call('com.canonical.dbusmenu', 'Event', '(isvu)', (3,'clicked',GLib.Variant('i',0),0))
            state = 3
    except Exception as exc:
        failure = str(exc)
        loop.quit()
        return False
    return True


GLib.timeout_add(100, tick)
try:
    loop.run()
finally:
    if child.poll() is None:
        child.terminate()  # Only the child created by this isolated check.
    out, err = child.communicate(timeout=5)
    if failure:
        print(err.decode(), file=sys.stderr)
        raise SystemExit(failure)
