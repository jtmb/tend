use fs2::FileExt;
use std::{
    fs,
    io::{self, Read},
    os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
    path::Path,
    process::Command,
};
use tend::{output, Settings, CONFIG};
fn run(args: &[&str]) -> Result<(), String> {
    output("/usr/bin/systemctl", args).map(|_| ())
}
fn write(path: &str, content: &str) -> Result<(), String> {
    write_mode(path, content, 0o644)
}
fn write_mode(path: &str, content: &str, mode: u32) -> Result<(), String> {
    let p = Path::new(path);
    let parent = p.parent().unwrap();
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o755)
        .create(parent)
        .map_err(|e| e.to_string())?;
    for item in p.ancestors() {
        if let Ok(m) = fs::symlink_metadata(item) {
            if m.file_type().is_symlink() || m.uid() != 0 || m.permissions().mode() & 0o022 != 0 {
                return Err(format!("Unprotected destination: {}", item.display()));
            }
        }
    }
    let temp = p.with_extension("tend-new");
    if temp.exists() {
        return Err("A previous settings write needs administrator review.".into());
    }
    let mut options = fs::OpenOptions::new();
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = options
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    file.write_all(content.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    fs::rename(temp, p).map_err(|e| e.to_string())
}
fn notify(minutes: i32) -> Result<(), String> {
    let s = Settings::read()?;
    let i = [60, 30, 15]
        .iter()
        .position(|n| *n == minutes)
        .ok_or("Invalid warning")?;
    if !s.enabled || !s.warnings[i] {
        return Ok(());
    }
    let now = Command::new("/usr/bin/date")
        .env("TZ", "America/Toronto")
        .arg("+%H:%M:%S")
        .output()
        .map_err(|e| e.to_string())?;
    let value = String::from_utf8_lossy(&now.stdout);
    let parts: Vec<i32> = value
        .trim()
        .split(':')
        .filter_map(|x| x.parse().ok())
        .collect();
    if parts.len() != 3 {
        return Err("Clock unavailable".into());
    }
    let delta = (parts[0] * 3600 + parts[1] * 60 + parts[2]
        - (s.hour as i32 * 3600 + s.minute as i32 * 60 - minutes * 60))
        .rem_euclid(86400);
    if delta > 120 || run(&["is-active", "--quiet", "dev-maintenance.timer"]).is_err() {
        return Ok(());
    }
    let body = format!(
        "Updates and cleanup begin at {:02}:{:02} Toronto time.{}",
        s.hour,
        s.minute,
        if s.reboot {
            " A restart follows; save your work."
        } else {
            ""
        }
    );
    output(
        "/usr/bin/notify-send",
        &[
            "--app-name=Tend",
            "--icon=io.github.jtmb.Tend",
            &format!("Maintenance in {minutes} minutes"),
            &body,
        ],
    )
    .map(|_| ())
}
fn main() {
    if let Err(e) = execute() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
fn execute() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("notify") {
        return notify(
            args.get(1)
                .ok_or("Missing warning")?
                .parse()
                .map_err(|_| "Invalid warning")?,
        );
    }
    if output("/usr/bin/id", &["-u"])? != "0" {
        return Err("Administrator authentication is required.".into());
    }
    if let Some(action) = args.first().map(String::as_str) {
        if [
            "cleanup",
            "updates",
            "memory",
            "memory-restore",
            "install-zram",
            "docker-logs",
        ]
        .contains(&action)
        {
            return workstation_action(action);
        }
    }
    match args.first().map(String::as_str) {
        Some("apply") => {
            let lock = fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open("/run/tend-settings.lock")
                .map_err(|e| e.to_string())?;
            lock.try_lock_exclusive()
                .map_err(|_| "Another settings change is in progress.")?;
            let mut input = String::new();
            io::stdin()
                .take(8192)
                .read_to_string(&mut input)
                .map_err(|e| e.to_string())?;
            let s: Settings = serde_json::from_str(&input).map_err(|e| e.to_string())?;
            s.validate()?;
            if !Path::new("/etc/tend/integration-ready").exists() {
                return Err("Install the maintenance integration before changing settings.".into());
            }
            if run(&["is-active", "--quiet", "dev-maintenance.service"]).is_ok() {
                return Err("Maintenance is running. Save settings after it finishes.".into());
            }
            let mut files = vec![
                (
                    CONFIG.to_string(),
                    serde_json::to_string_pretty(&s).unwrap(),
                ),
                (
                    "/etc/systemd/system/dev-maintenance.timer.d/95-tend.conf".into(),
                    format!(
                        "[Timer]\nOnCalendar=\nOnCalendar=*-*-* {:02}:{:02}:00 America/Toronto\n",
                        s.hour, s.minute
                    ),
                ),
                (
                    "/etc/systemd/system/dev-maintenance.service.d/95-tend.conf".into(),
                    format!(
                        "[Service]\nExecStartPost=\n{}",
                        if s.reboot {
                            "ExecStartPost=/usr/bin/python3 /usr/local/lib/dev-maintenance/reboot-after-maintenance.py --apply\n"
                        } else {
                            ""
                        }
                    ),
                ),
            ];
            for lead in [60, 30, 15].iter() {
                files.push((format!("/etc/systemd/system/tend-warning-{lead}.timer"),format!("[Unit]\nDescription=Tend maintenance warning\n[Timer]\nOnCalendar=*-*-* {} America/Toronto\nAccuracySec=15s\nPersistent=false\nWakeSystem=false\nUnit=tend-warning@{lead}.service\n[Install]\nWantedBy=timers.target\n",s.warning_time(*lead))));
            }
            let backups: Vec<_> = files
                .iter()
                .map(|(p, _)| (p.clone(), fs::read_to_string(p).ok()))
                .collect();
            let units: Vec<String> = std::iter::once("dev-maintenance.timer".into())
                .chain(
                    [60, 30, 15]
                        .iter()
                        .map(|n| format!("tend-warning-{n}.timer")),
                )
                .collect();
            let states: Vec<_> = units
                .iter()
                .map(|u| {
                    (
                        run(&["is-enabled", "--quiet", u]).is_ok(),
                        run(&["is-active", "--quiet", u]).is_ok(),
                    )
                })
                .collect();
            let apply = (|| {
                for (p, v) in &files {
                    write(p, v)?;
                }
                run(&["daemon-reload"])?;
                for (i, u) in units.iter().enumerate() {
                    let enabled = s.enabled && (i == 0 || s.warnings[i - 1]);
                    if enabled {
                        run(&["enable", u])?;
                        run(&["start", u])?;
                    } else {
                        run(&["disable", "--now", u])?;
                    }
                }
                Ok::<(), String>(())
            })();
            if let Err(e) = apply {
                for (p, v) in backups {
                    if let Some(v) = v {
                        let _ = write(&p, &v);
                    } else {
                        let _ = fs::remove_file(p);
                    }
                }
                let _ = run(&["daemon-reload"]);
                for (u, (enabled, active)) in units.iter().zip(states) {
                    let _ = run(&[if enabled { "enable" } else { "disable" }, u]);
                    let _ = run(&[if active { "start" } else { "stop" }, u]);
                }
                return Err(format!(
                    "Could not save settings; restored previous files: {e}"
                ));
            }
            println!("Schedule and warning timers updated.");
            Ok(())
        }
        Some("authorization-check") => { println!("Tend helper authorization verified; no settings changed."); Ok(()) },
        _ => Err("Supported operations: apply, cleanup, updates, memory, memory-restore, install-zram, docker-logs, authorization-check, notify.".into()),
    }
}

fn input_json<T: serde::de::DeserializeOwned>() -> Result<T, String> {
    let mut s = String::new();
    io::stdin()
        .take(8192)
        .read_to_string(&mut s)
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&s).map_err(|e| e.to_string())
}
fn engine_lock() -> Result<fs::File, String> {
    use std::os::unix::fs::OpenOptionsExt;
    let f = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open("/var/lib/dev-maintenance/run.lock")
        .map_err(|e| e.to_string())?;
    if f.metadata().map_err(|e| e.to_string())?.uid() != 0 {
        return Err("Invalid maintenance lock owner".into());
    }
    f.try_lock_exclusive().map_err(|_| {
        "Maintenance or another Tend operation is already running. Try after it finishes."
            .to_string()
    })?;
    Ok(f)
}
fn caller() -> Result<(String, String), String> {
    let uid = std::env::var("PKEXEC_UID")
        .or_else(|_| std::env::var("SUDO_UID"))
        .map_err(|_| "Launch from the desktop user")?;
    let id = uid.parse::<u32>().map_err(|_| "Invalid desktop user")?;
    if id < 1000 {
        return Err("A regular desktop user is required".into());
    }
    let passwd = output("/usr/bin/getent", &["passwd", &uid])?;
    let f: Vec<_> = passwd.split(':').collect();
    if f.len() < 7
        || f[2] != uid
        || !f[0]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        || !f[5].starts_with('/')
    {
        return Err("Invalid desktop account".into());
    }
    Ok((f[0].into(), f[5].into()))
}
const MEMORY_FILES: [&str; 2] = [
    "/etc/sysctl.d/99-tend-memory.conf",
    "/etc/systemd/zram-generator.conf.d/99-tend.conf",
];
fn workstation_action(action: &str) -> Result<(), String> {
    let _lock = engine_lock()?;
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, 10);
    }
    match action {
        "docker-logs" => {
            let (user, home) = caller()?;
            let docker = |args: &[&str]| -> Result<String, String> {
                let mut cmd = vec!["-u", user.as_str(), "--", "/usr/bin/env"];
                let h = format!("HOME={home}");
                cmd.push(&h);
                cmd.push("/usr/bin/docker");
                cmd.extend_from_slice(args);
                output("/usr/sbin/runuser", &cmd)
            };
            let endpoint = docker(&[
                "context",
                "inspect",
                "--format",
                "{{.Endpoints.docker.Host}}",
            ])?;
            if !endpoint.starts_with("unix://") {
                return Err("Log-file sizes are available only for a local Docker socket.".into());
            }
            let ids = docker(&["ps", "-aq", "--no-trunc"])?;
            for id in ids.lines().take(100) {
                if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
                    continue;
                }
                let raw = docker(&[
                    "inspect",
                    "--format",
                    r#"{"Name":{{json .Name}},"LogPath":{{json .LogPath}},"LogConfig":{{json .HostConfig.LogConfig}}}"#,
                    id,
                ])?;
                let value: serde_json::Value =
                    serde_json::from_str(&raw).map_err(|e| e.to_string())?;
                let name = value["Name"].as_str().unwrap_or(id);
                let path = value["LogPath"].as_str().unwrap_or("");
                let allowed = [
                    format!("/var/lib/docker/containers/{id}/{id}-json.log"),
                    format!("{home}/.local/share/docker/containers/{id}/{id}-json.log"),
                ];
                if !allowed.iter().any(|s| s == path) {
                    println!("{name}: log sizes unavailable for this log driver or data location");
                    continue;
                }
                let p = Path::new(path);
                let parent = p.parent().ok_or("Invalid log path")?;
                if parent.canonicalize().map(|p| p != parent).unwrap_or(true) {
                    println!("{name}: log directory unavailable");
                    continue;
                }
                let mut total = 0u64;
                let mut count = 0;
                if let Ok(es) = fs::read_dir(parent) {
                    for e in es.flatten() {
                        let filename = e.file_name();
                        let filename = filename.to_string_lossy();
                        if filename == format!("{id}-json.log")
                            || filename.starts_with(&format!("{id}-json.log."))
                        {
                            if let Ok(m) = fs::symlink_metadata(e.path()) {
                                if m.is_file() && !m.file_type().is_symlink() {
                                    total += m.len();
                                    count += 1;
                                }
                            }
                        }
                    }
                }
                println!(
                    "{name}: {} in {count} log files; configured options: {}",
                    tend::workstation::bytes(total),
                    value["LogConfig"]
                );
            }
            println!("Missing per-container limits can inherit daemon defaults. No logs were read or removed.");
        }
        "cleanup" => {
            let c: tend::workstation::Cleanup = input_json()?;
            c.validate()?;
            let (user, home) = caller()?;
            let before = tend::workstation::available();
            let mut report = String::new();
            if c.apt {
                report.push_str("APT obsolete downloads:\n");
                report.push_str(&tend::workstation::command(
                    "/usr/bin/apt-get",
                    &["-o", "DPkg::Lock::Timeout=0", "autoclean"],
                ));
                report.push('\n');
            }
            let json = serde_json::to_string(&c).unwrap();
            let uid = output("/usr/bin/id", &["-u", &user])?;
            let mut child = Command::new("/usr/sbin/runuser")
                .args([
                    "-u",
                    &user,
                    "--",
                    "/usr/bin/env",
                    &format!("HOME={home}"),
                    &format!("XDG_RUNTIME_DIR=/run/user/{uid}"),
                    &format!("DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/{uid}/bus"),
                    "/usr/local/libexec/tend-worker",
                    "cleanup",
                ])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| e.to_string())?;
            use std::io::Write;
            child
                .stdin
                .take()
                .ok_or("Missing cleanup input")?
                .write_all(json.as_bytes())
                .map_err(|e| e.to_string())?;
            let r = child.wait_with_output().map_err(|e| e.to_string())?;
            report.push_str(&String::from_utf8_lossy(&r.stdout));
            if !r.status.success() {
                report.push_str(&format!(
                    "\nUser cleanup error: {}",
                    String::from_utf8_lossy(&r.stderr)
                ));
            }
            report.push_str(&format!("\nTotal root free-space change including APT: {:.1} MiB (concurrent writes included)",(tend::workstation::available() as i128-before as i128) as f64/1048576.));
            println!("{report}");
        }
        "updates" => {
            if let Ok(es) = fs::read_dir("/sys/class/power_supply") {
                for e in es.flatten() {
                    if fs::read_to_string(e.path().join("status"))
                        .unwrap_or_default()
                        .trim()
                        == "Discharging"
                    {
                        return Err("Connect AC power before installing updates.".into());
                    }
                }
            }
            let mut config = output("/usr/bin/apt-config", &["dump"])?;
            config.push_str("\nDir::Etc::parts \"-\";\nDir::Etc::main \"-\";\nUnattended-Upgrade::Automatic-Reboot \"false\";\nUnattended-Upgrade::Automatic-Reboot-WithUsers \"false\";\n");
            write_mode("/etc/tend/manual-updates.conf", &config, 0o600)?;
            let result = (|| {
                output(
                    "/usr/bin/apt-get",
                    &["-o", "DPkg::Lock::Timeout=0", "update", "--error-on=any"],
                )?;
                let r = Command::new("/usr/bin/unattended-upgrade")
                    .env("APT_CONFIG", "/etc/tend/manual-updates.conf")
                    .env("NEEDRESTART_MODE", "l")
                    .output()
                    .map_err(|e| e.to_string())?;
                if !r.status.success() {
                    return Err(format!(
                        "Updates failed: {}\n{}",
                        String::from_utf8_lossy(&r.stdout),
                        String::from_utf8_lossy(&r.stderr)
                    ));
                }
                Ok(())
            })();
            let _ = fs::remove_file("/etc/tend/manual-updates.conf");
            result?;
            println!("Configured stable updates completed. No restart was requested. Reboot required: {}",Path::new("/var/run/reboot-required").exists());
        }
        "install-zram" => {
            let r = Command::new("/usr/bin/apt-get")
                .args([
                    "-o",
                    "DPkg::Lock::Timeout=0",
                    "install",
                    "--yes",
                    "--no-install-recommends",
                    "systemd-zram-generator",
                ])
                .env("NEEDRESTART_MODE", "l")
                .output()
                .map_err(|e| e.to_string())?;
            if !r.status.success() {
                return Err(String::from_utf8_lossy(&r.stderr).into());
            }
            println!("zram generator installed. Configure its size below; Tend does not restart or disable swap.");
        }
        "memory" => {
            let s: tend::preferences::MemorySettings = input_json()?;
            s.validate()?;
            if !Path::new("/usr/lib/systemd/system-generators/zram-generator").exists() {
                return Err("Install the zram generator first using Prepare zram support.".into());
            }
            let old: Vec<Option<String>> = MEMORY_FILES
                .iter()
                .map(|p| match fs::read_to_string(p) {
                    Ok(s) => Ok(Some(s)),
                    Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
                    Err(e) => Err(e.to_string()),
                })
                .collect::<Result<Vec<_>, String>>()?;
            let runtime =
                fs::read_to_string("/proc/sys/vm/swappiness").map_err(|e| e.to_string())?;
            write(
                "/etc/tend/memory-backup.json",
                &serde_json::to_string(&serde_json::json!({"files":old,"runtime":runtime.trim()}))
                    .unwrap(),
            )?;
            let apply = (|| {
                write(
                    MEMORY_FILES[0],
                    &format!("# Managed by Tend\nvm.swappiness={}\n", s.swappiness),
                )?;
                write(MEMORY_FILES[1],&format!("# Managed by Tend; applied at the next ordinary boot\n[zram0]\nzram-size = min(ram / 2, {})\ncompression-algorithm = {}\nswap-priority = 100\n",s.zram_mb,s.compression))?;
                fs::write("/proc/sys/vm/swappiness", s.swappiness.to_string())
                    .map_err(|e| e.to_string())?;
                Ok::<(), String>(())
            })();
            if let Err(e) = apply {
                for (p, v) in MEMORY_FILES.iter().zip(old) {
                    if let Some(v) = v {
                        let _ = write(p, &v);
                    } else {
                        let _ = fs::remove_file(p);
                    }
                }
                return Err(format!(
                    "Memory settings not applied; previous files restored: {e}"
                ));
            }
            println!("Swappiness applied now. zram configuration saved for your next ordinary boot. Disk swap is preserved; no restart or live swapoff was requested.");
        }
        "memory-restore" => {
            let saved: serde_json::Value = serde_json::from_str(
                &fs::read_to_string("/etc/tend/memory-backup.json").map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            let value = saved["runtime"]
                .as_str()
                .ok_or("Missing previous swappiness")?;
            let n = value
                .parse::<u32>()
                .map_err(|_| "Invalid saved swappiness")?;
            if n > 200 {
                return Err("Invalid saved swappiness".into());
            }
            let files = saved["files"]
                .as_array()
                .ok_or("Missing memory recovery files")?;
            if files.len() != 2 || files.iter().any(|v| !v.is_null() && !v.is_string()) {
                return Err("Invalid memory recovery files".into());
            }
            for (i, p) in MEMORY_FILES.iter().enumerate() {
                if let Some(v) = saved["files"][i].as_str() {
                    write(p, v)?;
                } else if Path::new(p).exists() {
                    fs::remove_file(p).map_err(|e| e.to_string())?;
                }
            }
            fs::write("/proc/sys/vm/swappiness", value).map_err(|e| e.to_string())?;
            fs::remove_file("/etc/tend/memory-backup.json").map_err(|e| e.to_string())?;
            println!("Previous memory settings restored. zram changes take effect at the next ordinary boot; live swap remains active.");
        }
        _ => return Err("Unknown workstation operation".into()),
    }
    Ok(())
}
