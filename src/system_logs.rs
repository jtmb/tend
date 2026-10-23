use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    io::{BufRead, BufReader, Read},
    process::Command,
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
    v[key].as_str().unwrap_or("").into()
}
pub fn decode(line: &str) -> Option<Entry> {
    let v: Value = serde_json::from_str(line).ok()?;
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
        s.to_owned()
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
    let mut cmd = Command::new("/usr/bin/journalctl");
    cmd.args([
        "--quiet",
        "--no-pager",
        "--output=json",
        "--output-fields=__REALTIME_TIMESTAMP,PRIORITY,_COMM,_PID,_SYSTEMD_UNIT,_SYSTEMD_USER_UNIT,SYSLOG_IDENTIFIER,MESSAGE,COREDUMP_EXE,COREDUMP_PID,COREDUMP_SIGNAL_NAME,MESSAGE_ID",
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
    let out = cmd.output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into());
    }
    let mut entries: Vec<_> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(decode)
        .collect();
    if source == 2 {
        if let Ok(files) = fs::read_dir("/var/crash") {
            for file in files.flatten().take(1000) {
                let p = file.path();
                if p.extension().and_then(|s| s.to_str()) != Some("crash") {
                    continue;
                }
                if !fs::symlink_metadata(&p)
                    .map(|m| m.file_type().is_file())
                    .unwrap_or(false)
                {
                    continue;
                }
                let Ok(file) = fs::File::open(&p) else {
                    continue;
                };
                let mut fields = std::collections::HashMap::new();
                for line in BufReader::new(file.take(256 * 1024)).lines().take(2000) {
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
                    fields.get("Date").map(String::as_str).unwrap_or("")
                );
                entries.push(Entry{time,source:format!("Crash · {name}"),priority:3,message,crash:true,details:format!("Application: {name}\nReport: {}\nPackage: {}\nDate: {}\nSignal: {}\n\nOnly report headers are read; crash memory and environment are omitted.",p.display(),fields.get("Package").map(String::as_str).unwrap_or(""),fields.get("Date").map(String::as_str).unwrap_or(""),fields.get("Signal").map(String::as_str).unwrap_or(""))});
            }
        }
    }
    entries.sort_by_key(|e| std::cmp::Reverse(e.time));
    entries.truncate(limit as usize);
    serde_json::to_string(&entries).map_err(|e| e.to_string())
}
pub fn time(value: i64) -> String {
    gtk::glib::DateTime::from_unix_local(value)
        .ok()
        .and_then(|d| d.format("%b %d %H:%M:%S").ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Unknown time".into())
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
}
