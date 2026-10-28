#!/usr/bin/python3
"""Measure one owned native preview; run on a fresh private session bus.
No host settings, jobs or applications are modified. Readings are per process.
"""
import argparse,hashlib,json,os,subprocess,time,sys
from pathlib import Path
import gi
from gi.repository import Gio,GLib
p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--root',type=Path,required=True);p.add_argument('--enabled-seconds',type=int,default=0);p.add_argument('--baseline-seconds',type=int,default=60);args=p.parse_args()
address=os.environ.get('DBUS_SESSION_BUS_ADDRESS','')
if '/run/user/' in address or 'unix:path=/tmp/' not in address:raise SystemExit('A fresh private dbus-run-session is required')
root=args.root.resolve();home=root/'home';out=root/'budget';out.mkdir(exist_ok=True)
assert (home/'demo-journal.jsonl').is_file()
env=dict(os.environ,GDK_BACKEND='x11',GSK_RENDERER='cairo',GTK_A11Y='none',GSETTINGS_BACKEND='memory',GIO_USE_VFS='local',XDG_CONFIG_HOME=str(home/'.config'),XDG_CACHE_HOME=str(root/'cache'),XDG_DATA_HOME=str(root/'data'),TEND_DEMO_HOME=str(home),TEND_DEMO_DIR=str(root/'frames'),TEND_APP_BUDGET_QA=str(out))
(home/'.config/tend/theme').write_text('2')
conn=Gio.bus_get_sync(Gio.BusType.SESSION,None);registered={};signals={'layout':0,'tooltip':0,'notifications':0}
watcher=Gio.DBusNodeInfo.new_for_xml('''<node><interface name="org.kde.StatusNotifierWatcher"><method name="RegisterStatusNotifierItem"><arg type="s" direction="in"/></method></interface></node>''')
def register(c,sender,path,interface,method,params,invocation):registered['sender']=sender;invocation.return_value(GLib.Variant('()',()))
conn.register_object('/StatusNotifierWatcher',watcher.interfaces[0],register,None,None)
owner=Gio.bus_own_name_on_connection(conn,'org.kde.StatusNotifierWatcher',Gio.BusNameOwnerFlags.NONE,None,None)
notify=Gio.DBusNodeInfo.new_for_xml('''<node><interface name="org.freedesktop.Notifications"><method name="Notify"><arg type="s" direction="in"/><arg type="u" direction="in"/><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="as" direction="in"/><arg type="a{sv}" direction="in"/><arg type="i" direction="in"/><arg type="u" direction="out"/></method></interface></node>''')
def notified(c,sender,path,interface,method,params,invocation):signals['notifications']+=1;invocation.return_value(GLib.Variant('(u)',(signals['notifications'],)))
conn.register_object('/org/freedesktop/Notifications',notify.interfaces[0],notified,None,None);notification_owner=Gio.bus_own_name_on_connection(conn,'org.freedesktop.Notifications',Gio.BusNameOwnerFlags.NONE,None,None)
for interface,name,key in [('com.canonical.dbusmenu','LayoutUpdated','layout'),('org.kde.StatusNotifierItem','NewToolTip','tooltip')]:conn.signal_subscribe(None,interface,name,None,None,Gio.DBusSignalFlags.NONE,lambda *a,k=key:signals.__setitem__(k,signals[k]+1))
log=(out/'application.log').open('w');child=subprocess.Popen([str(args.binary.resolve())],env=env,stdout=log,stderr=subprocess.STDOUT)
loop=GLib.MainLoop();started=time.monotonic();mode=None;phase_start=None;points=[];measurements={};failure=None;stage={};sent_finish=False
hz=os.sysconf('SC_CLK_TCK');page_bytes=os.sysconf('SC_PAGE_SIZE')
def reading():
    fields=Path(f'/proc/{child.pid}/stat').read_text().rpartition(')')[2].split()
    return {'time':time.monotonic(),'cpu':(int(fields[11])+int(fields[12]))/hz,'rss':int(fields[21])*page_bytes,'major_faults':int(fields[9])}
def action(name):conn.call_sync(stage['bus'],'/TendBudget','org.gtk.Actions','Activate',GLib.Variant('(sava{sv})',(name,[],{})),None,Gio.DBusCallFlags.NONE,3000,None)
def begin(label):
    global mode,phase_start,points
    mode=label;phase_start=time.monotonic();points=[reading()]
def finish_measurement(label):
    points.append(reading());first,last=points[0],points[-1];wall=last['time']-first['time']
    measurements[label]={'seconds':wall,'cpu_percent_one_core':100*(last['cpu']-first['cpu'])/wall,'rss_start':first['rss'],'rss_end':last['rss'],'rss_peak':max(v['rss'] for v in points),'major_faults':last['major_faults']-first['major_faults'],'samples':len(points)}
def tick():
    global mode,stage,failure,sent_finish
    try:
        try:stage=json.loads((out/'stage.json').read_text())
        except (FileNotFoundError,json.JSONDecodeError):pass
        if child.poll() is not None:
            assert child.returncode==0,f'Preview exited {child.returncode}'
            assert stage.get('phase')=='complete',stage
            assert stage['extra']['stopped'],stage
            assert registered,'Private panel host was never registered'
            report={'scope':'optimized native whole app under Xvfb/Cairo; common-page warmup then hidden; default monitoring, optional crash reads off','binary':str(args.binary.resolve()),'binary_sha256':hashlib.sha256(args.binary.read_bytes()).hexdigest(),'kernel':os.uname().release,'measurements':measurements,'panel_registered':bool(registered),'signals':signals,'collector_stopped':True,'final_stats':stage['stats'],'fixture':str(root)}
            (out/'result.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report),flush=True);loop.quit();return False
        if time.monotonic()-started>args.enabled_seconds+args.baseline_seconds+180:raise AssertionError('Budget run exceeded deadline')
        if stage.get('phase')=='failed':raise AssertionError(stage)
        if mode is None and stage.get('phase')=='baseline_ready':
            assert not stage['stats'].get('history_exists',False),'History was created without recording/monitoring'
            assert stage['stats'].get('subscribers',0)==0,stage
            begin('idle_hidden')
        elif mode=='idle_hidden':
            points.append(reading())
            if time.monotonic()-phase_start>=args.baseline_seconds:
                finish_measurement(mode)
                if args.enabled_seconds:action('enable');mode='warming'
                else:action('finish');sent_finish=True;mode='stopping'
        elif mode=='warming' and stage.get('phase')=='enabled_ready':
            assert stage['stats'].get('subscribers')==1,stage
            begin('enabled_hidden')
        elif mode=='enabled_hidden':
            points.append(reading())
            if time.monotonic()-phase_start>=args.enabled_seconds:finish_measurement(mode);action('finish');sent_finish=True;mode='stopping'
        return True
    except Exception as e:failure=str(e);loop.quit();return False
GLib.timeout_add(500,tick)
try:loop.run()
finally:
    if child.poll() is None:child.terminate() # Only the preview just created here.
    child.wait(timeout=10);log.close()
    if failure:raise SystemExit(failure)
