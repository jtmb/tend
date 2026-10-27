#!/usr/bin/python3
"""Monitor real Tend widgets and notification calls on a fresh private bus.
No desktop services, installed processes or host maintenance commands are used.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import gi
from gi.repository import Gio, GLib

address=os.environ.get('DBUS_SESSION_BUS_ADDRESS','')
if not os.environ.get('TEND_HEALTH_QA') or not os.environ.get('TEND_DEMO_DIR') or '/run/user/' in address or 'unix:path=/tmp/' not in address:
    raise SystemExit('Use an isolated prepared home and dbus-run-session private bus')
root=Path(os.environ['TEND_HEALTH_QA']);root.mkdir(exist_ok=True)
conn=Gio.bus_get_sync(Gio.BusType.SESSION,None)
watcher=Gio.DBusNodeInfo.new_for_xml('''<node><interface name="org.kde.StatusNotifierWatcher">
<method name="RegisterStatusNotifierItem"><arg type="s" direction="in"/></method>
<property name="IsStatusNotifierHostRegistered" type="b" access="read"/>
<property name="ProtocolVersion" type="i" access="read"/></interface></node>''')
notification=Gio.DBusNodeInfo.new_for_xml('''<node><interface name="org.freedesktop.Notifications">
<method name="Notify"><arg type="s" direction="in"/><arg type="u" direction="in"/><arg type="s" direction="in"/>
<arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="as" direction="in"/>
<arg type="a{sv}" direction="in"/><arg type="i" direction="in"/><arg type="u" direction="out"/></method>
</interface></node>''')
item={};notices=[]
def register(connection,sender,path,interface,method,parameters,invocation):
    item['sender']=sender
    invocation.return_value(GLib.Variant('()',()))
def notify(connection,sender,path,interface,method,parameters,invocation):
    values=parameters.unpack()
    assert values[0]=='Tend' and values[2]=='io.github.jtmb.Tend' and values[5]==[]
    notices.append(dict(title=values[3],body=values[4],urgency=values[6]['urgency']))
    (root/'notifications.json').write_text(json.dumps(notices,indent=2)+'\n')
    invocation.return_value(GLib.Variant('(u)',(len(notices),)))
conn.register_object('/StatusNotifierWatcher',watcher.interfaces[0],register,lambda *args:GLib.Variant('b',True) if args[-1]=='IsStatusNotifierHostRegistered' else GLib.Variant('i',0),None)
conn.register_object('/org/freedesktop/Notifications',notification.interfaces[0],notify,None,None)
owners=[Gio.bus_own_name_on_connection(conn,name,Gio.BusNameOwnerFlags.NONE,None,None) for name in ['org.kde.StatusNotifierWatcher','org.freedesktop.Notifications']]
child=subprocess.Popen([str(Path('target/debug/tend').resolve())],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
loop=GLib.MainLoop();started=time.monotonic();failure=None;live_status=False

def tick():
    global failure,live_status
    try:
        if child.poll() is not None:
            assert child.returncode==0,f'Native monitor fixture failed ({child.returncode})'
            report=json.loads((root/'health-evidence.json').read_text())
            assert len(notices)==4,notices
            assert live_status,'Panel never exposed live monitoring status'
            report.update(private_bus_notifications=True,live_panel_tooltip=True,quit_stops_preview=True)
            (root/'health-evidence.json').write_text(json.dumps(report,indent=2)+'\n')
            print(json.dumps(report));loop.quit();return False
        if time.monotonic()-started>60:raise AssertionError('Monitoring check exceeded deadline')
        if item and not (root/'health-evidence.json').exists():
            properties=conn.call_sync(item['sender'],'/StatusNotifierItem','org.freedesktop.DBus.Properties','GetAll',GLib.Variant('(s)',('org.kde.StatusNotifierItem',)),None,Gio.DBusCallFlags.NONE,3000,None).unpack()[0]
            if 'Monitoring live' in properties['ToolTip'][3]:live_status=True
        return True
    except Exception as exc:
        # The app can finish between poll() and GetAll. Confirm its exit and
        # evidence on the next tick instead of treating normal disconnect as a
        # protocol failure; a nonzero child status still fails above.
        if isinstance(exc,GLib.Error) and (root/'health-evidence.json').exists():return True
        failure=str(exc);loop.quit();return False
GLib.timeout_add(100,tick)
try:
    loop.run()
finally:
    if child.poll() is None:child.terminate() # Only this private preview child.
    out,err=child.communicate(timeout=5)
    sys.stdout.write(out.decode());sys.stderr.write(err.decode())
    if failure:raise SystemExit(failure)
