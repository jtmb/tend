use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    io::{BufRead, BufReader, Read},
    process::Command,
    os::unix::fs::OpenOptionsExt,
};
#[derive(Clone, Serialize, Deserialize)]
pub struct Entry {
    pub time: i64,
    pub source: String,
    pub priority: u8,
    pub message: String,
    pub crash: bool,
    pub details: String,
}
fn string(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or("").chars().take(1024).collect()
}
pub fn decode(line: &str) -> Option<Entry> {
    let v: Value = serde_json::from_str(line).ok()?;
    if v["_TRANSPORT"].as_str()==Some("kernel") && v["MESSAGE"].as_str().is_some_and(|s|s.starts_with("Out of memory: Killed process ")||s.starts_with("Memory cgroup out of memory: Killed process ")) {
        if let Some(incident)=crate::memory_incidents::parse(line,0).into_iter().next(){return Some(incident.entry());}
    }
    let time = v["__REALTIME_TIMESTAMP"].as_str()?.parse::<i64>().ok()? / 1_000_000;
    let priority = v["PRIORITY"]
        .as_str()
        .and_then(|s| s.parse::<u8>().ok())
        .unwrap_or(6);
    let comm = string(&v, "_COMM");
    let unit = string(&v, "_SYSTEMD_UNIT");
    let userunit = string(&v, "_SYSTEMD_USER_UNIT");
    let exe = string(&v, "COREDUMP_EXE");
    let crash =
        !exe.is_empty() || v["MESSAGE_ID"].as_str() == Some("fc2e22bc6ee647b6b90729ab34a250b1");
    let source = if crash {
        format!(
            "Crash · {}",
            if exe.is_empty() {
                comm.as_str()
            } else {
                exe.as_str()
            }
        )
    } else if !userunit.is_empty() {
        userunit
    } else if !unit.is_empty() {
        unit
    } else if !comm.is_empty() {
        comm
    } else {
        string(&v, "SYSLOG_IDENTIFIER")
    };
    let message = if let Some(s) = v["MESSAGE"].as_str() {
        let mut end=s.len().min(32*1024);while !s.is_char_boundary(end) {end-=1;}
        format!("{}{}",&s[..end],if end<s.len(){"\n[Message truncated to 32 KiB]"}else{""})
    } else {
        "Binary message omitted".into()
    };
    let details = format!(
        "Source: {source}\nProcess: {}\nPID: {}\nExecutable: {}\nSignal: {}\n\n{}",
        string(&v, "_COMM"),
        if crash {
            string(&v, "COREDUMP_PID")
        } else {
            string(&v, "_PID")
        },
        exe,
        string(&v, "COREDUMP_SIGNAL_NAME"),
        message
    );
    Some(Entry {
        time,
        source,
        priority,
        message,
        crash,
        details,
    })
}
pub fn load(source: u32, period: u32, limit: u32) -> Result<String, String> {
    load_cancellable(source,period,limit,&std::sync::atomic::AtomicBool::new(false))
}
pub fn load_cancellable(source: u32, period: u32, limit: u32, cancelled: &std::sync::atomic::AtomicBool) -> Result<String, String> {
    serde_json::to_string(&entries_cancellable(source,period,limit,cancelled)?).map_err(|e|e.to_string())
}
/// Typed path for opt-in monitoring, avoiding a second encoded copy of reports.
pub fn entries_cancellable(source: u32, period: u32, limit: u32, cancelled: &std::sync::atomic::AtomicBool) -> Result<Vec<Entry>, String> {
    if cancelled.load(std::sync::atomic::Ordering::Relaxed) {return Err("Log query cancelled.".into());}
    #[cfg(feature = "docs-demo")]
    if std::env::var_os("TEND_DEMO_DIR").is_some() {
        // Public demo input, parsed with the normal journal decoder. Never read
        // the user's journal/crash reports in a documentation recording.
        let input = fs::read_to_string(crate::workstation::home().join("demo-journal.jsonl"))
            .map_err(|e| e.to_string())?;
        let mut entries: Vec<_> = input
            .lines()
            .filter_map(decode)
            .filter(|e| source != 2 || e.crash)
            .take(limit as usize)
            .collect();
        if source==2 {entries.extend(crate::memory_incidents::load(period,cancelled)?.into_iter().map(|e|e.entry()));}
        entries.sort_by_key(|e|std::cmp::Reverse(e.time));entries.truncate(limit as usize);
        return Ok(entries);
    }
    let mut cmd = Command::new("/usr/bin/journalctl");
    cmd.args([
        "--quiet",
        "--no-pager",
        "--output=json",
        "--output-fields=__REALTIME_TIMESTAMP,_BOOT_ID,_TRANSPORT,PRIORITY,_COMM,_PID,_SYSTEMD_UNIT,_SYSTEMD_USER_UNIT,SYSLOG_IDENTIFIER,MESSAGE,COREDUMP_EXE,COREDUMP_PID,COREDUMP_SIGNAL_NAME,MESSAGE_ID",
        "--lines",
        &limit.clamp(100, 5000).to_string(),
    ]);
    if period == 0 {
        cmd.arg("--boot");
    } else {
        cmd.args([
            "--since",
            match period {
                1 => "-1h",
                2 => "-24h",
                _ => "-7d",
            },
        ]);
    }
    if source == 1 {
        cmd.arg("--user");
    }
    if source == 2 {
        cmd.arg("MESSAGE_ID=fc2e22bc6ee647b6b90729ab34a250b1");
    }
    let out = crate::queries::run_cancellable(&mut cmd,std::time::Duration::from_secs(15),8*1024*1024,cancelled)?;
    let mut entries: Vec<_> = out
        .lines()
        .filter_map(decode)
        .collect();
    if source == 2 {
        entries.extend(crate::memory_incidents::load(period,cancelled)?.into_iter().map(|e|e.entry()));
        if let Ok(files) = fs::read_dir("/var/crash") {
            let mut reports:Vec<_>=files.flatten().take(1000).map(|f|f.path()).filter_map(|p|{
                let m=fs::symlink_metadata(&p).ok()?;
                (m.is_file()&&!m.file_type().is_symlink()&&p.extension().and_then(|s|s.to_str())==Some("crash")).then(||(m.modified().ok(),p))
            }).collect();
            reports.sort_by_key(|(time,_)|std::cmp::Reverse(*time));
            for (_,p) in reports.into_iter().take(100) {
                if cancelled.load(std::sync::atomic::Ordering::Relaxed) {return Err("Log query cancelled.".into());}
                if p.extension().and_then(|s| s.to_str()) != Some("crash") {
                    continue;
                }
                if !fs::symlink_metadata(&p)
                    .map(|m| m.file_type().is_file())
                    .unwrap_or(false)
                {
                    continue;
                }
                let Ok(file) = fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW|libc::O_NONBLOCK).open(&p) else {
                    continue;
                };
                if !file.metadata().is_ok_and(|m|m.is_file()) {continue;}
                let mut fields = std::collections::HashMap::new();
                for line in BufReader::new(file.take(64 * 1024)).lines().take(1000) {
                    let Ok(line) = line else { break };
                    if line.starts_with("CoreDump:") {
                        break;
                    }
                    if let Some((k, v)) = line.split_once(": ") {
                        if ["ExecutablePath", "Date", "Signal", "Package", "ProblemType"]
                            .contains(&k)
                        {
                            fields.insert(k.to_owned(), v.to_owned());
                        }
                    }
                }
                let time = fs::metadata(&p)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                let age = crate::workstation::now() as i64 - time;
                let within = match period {
                    0 => {
                        let uptime = fs::read_to_string("/proc/uptime")
                            .ok()
                            .and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok())
                            .unwrap_or(0.);
                        age <= uptime as i64
                    }
                    1 => age <= 3600,
                    2 => age <= 86400,
                    _ => age <= 604800,
                };
                if !within {
                    continue;
                }
                let name = fields
                    .get("ExecutablePath")
                    .cloned()
                    .unwrap_or_else(|| p.file_name().unwrap().to_string_lossy().into());
                let message = format!(
                    "{} · signal {} · {}",
                    fields
                        .get("ProblemType")
                        .map(String::as_str)
                        .unwrap_or("Crash"),
                    fields
                        .get("Signal")
                        .map(String::as_str)
                        .unwrap_or("unknown"),
                    &fields
                        .get("Date")
                        .map(|s| crate::presentation::source_date(s))
                        .unwrap_or_default()
                );
                entries.push(Entry{time,source:format!("Crash report · {name}"),priority:3,message,crash:true,details:format!("Application: {name}\nReport: {}\nPackage: {}\nReported: {} (timezone not supplied by report)\nReport updated: {}\nSignal: {}\n\nOnly report headers are read; crash memory and environment are omitted.",p.display(),fields.get("Package").map(String::as_str).unwrap_or(""),fields.get("Date").map(|s|crate::presentation::source_date(s)).unwrap_or_default(),crate::presentation::exact(time),fields.get("Signal").map(String::as_str).unwrap_or(""))});
            }
        }
    }
    entries.sort_by_key(|e| std::cmp::Reverse(e.time));
    entries.truncate(limit as usize);
    Ok(entries)
}
pub fn time(value: i64) -> String {
    crate::presentation::when(value)
}
/// OOM context may become unavailable or more complete on a later read. Keep
/// notification/timeline identity tied to the recorded event, not that context.
pub fn identity(entry:&Entry)->String{
    if entry.source.starts_with("Out of memory · ") {
        if let Some(id)=entry.details.lines().find(|s|s.starts_with("Event ID: ")){return format!("oom:{id}");}
        let boot=entry.details.lines().find(|s|s.starts_with("Boot ID: ")).unwrap_or("");
        format!("{}:{}:{boot}",entry.time,entry.source)
    }else{format!("{}:{}:{}:{}",entry.time,entry.source,entry.message,entry.details)}
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn structured_crash_is_identified_without_guessing_from_severity() {
        let e=decode(r#"{"__REALTIME_TIMESTAMP":"1700000000000000","PRIORITY":"3","COREDUMP_EXE":"/usr/bin/example","MESSAGE":"Process dumped core"}"#).unwrap();
        assert!(e.crash);
        assert_eq!(e.time, 1700000000);
        assert!(e.source.contains("example"));
        let ordinary=decode(r#"{"__REALTIME_TIMESTAMP":"1700000000000000","PRIORITY":"3","MESSAGE":"Network error"}"#).unwrap();
        assert!(!ordinary.crash);
    }
    #[test]
    fn trusted_oom_uses_victim_pid_without_a_core_dump() {
        let line=include_str!("../tests/fixtures/opencode-kernel-oom.jsonl").lines().last().unwrap();
        let event=decode(line).unwrap();
        assert!(event.crash);
        assert!(event.source.contains("node (PID 6557)"));
        assert!(event.details.contains("Anonymous resident memory: 5.15 GiB"));
        assert!(!decode(&line.replace("\"kernel\"", "\"stdout\"")).unwrap().crash);
    }
}
