#!/usr/bin/python3
"""Compare prebuilt reference/candidate previews, then measure ten idle minutes.
Build both binaries first. Do not compile or run rendering suites during this run.
Every app owns a fresh fixture, private bus and virtual display; no host writes.
"""
import argparse,json,subprocess,time
from pathlib import Path
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--reference',type=Path,required=True);p.add_argument('--candidate',type=Path,required=True)
p.add_argument('--parent',type=Path,required=True);p.add_argument('--output',type=Path,required=True)
args=p.parse_args();repo=Path(__file__).resolve().parents[2]
reports={}
for name,binary,enabled in [('reference',args.reference,0),('candidate',args.candidate,600)]:
    root=Path(subprocess.check_output(['python3',str(repo/'docs/demo/prepare.py'),'--parent',str(args.parent.resolve())],text=True).strip())
    print(json.dumps({'stage':name,'fixture':str(root),'enabled_seconds':enabled}),flush=True)
    subprocess.run(['dbus-run-session','--config-file='+str(repo/'docs/demo/private-bus.conf'),'--','xvfb-run','-a','/usr/bin/python3',str(repo/'docs/demo/budget-check.py'),'--binary',str(binary.resolve()),'--root',str(root),'--baseline-seconds','60','--enabled-seconds',str(enabled)],check=True)
    subprocess.run(['python3',str(repo/'docs/demo/prepare.py'),'--verify',str(root)],check=True)
    reports[name]=json.loads((root/'budget/result.json').read_text())
ref=reports['reference']['measurements']['idle_hidden'];candidate=reports['candidate']['measurements'];idle=candidate['idle_hidden'];enabled=candidate['enabled_hidden']
gates={'same_kernel':reports['reference']['kernel']==reports['candidate']['kernel'],
       'reference_duration':ref['seconds']>=60,'candidate_idle_duration':idle['seconds']>=60,'monitor_duration':enabled['seconds']>=600,
       'idle_memory':idle['rss_peak']-ref['rss_peak']<=32*1024**2,
       'enabled_memory':enabled['rss_peak']-ref['rss_peak']<=32*1024**2,
       'enabled_cpu':enabled['cpu_percent_one_core']<0.5,
       'collectors_stop':all(r['collector_stopped'] and r['final_stats']['subscribers']==0 for r in reports.values()),
       'panel_registered':all(r['panel_registered'] for r in reports.values())}
report={'scope':'optimized whole native app; common page warmup, private panel host, Xvfb/Cairo, hidden idle; default monitoring with optional crash reads off. Not a measurement of physical GPU frame latency or optional journal reads.',
        'reports':reports,'idle_increment_mib':(idle['rss_peak']-ref['rss_peak'])/1024**2,'enabled_increment_mib':(enabled['rss_peak']-ref['rss_peak'])/1024**2,
        'monitor_increment_mib':(enabled['rss_peak']-idle['rss_peak'])/1024**2,'gates':gates,'passed':all(gates.values())}
args.output.parent.mkdir(parents=True,exist_ok=True);args.output.write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({'stage':'complete','passed':report['passed'],'gates':gates,'report':str(args.output)}),flush=True)
if not report['passed']:raise SystemExit(1)
