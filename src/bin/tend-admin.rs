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
        .mode(0o644)
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
        Some("run") => run(&["start", "--no-block", "dev-maintenance.service"]),
        _ => Err("Supported operations: apply, run, notify.".into()),
    }
}
