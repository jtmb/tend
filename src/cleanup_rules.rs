use crate::{output, workstation as w};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    ffi::CString,
    fs,
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub name: String,
    pub folder: PathBuf,
    pub pattern: String,
    pub days: u32,
    pub recursive: bool,
    pub enabled: bool,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    pub rules: Vec<Rule>,
}
fn project(p: &Path) -> bool {
    [".git", "Cargo.toml", "package.json", "pyproject.toml"]
        .iter()
        .any(|s| p.join(s).exists())
}
fn protected(root: &Path, home: &Path) -> bool {
    if root == home || !root.starts_with(home) {
        return true;
    }
    let Some(rel) = root.strip_prefix(home).ok() else {
        return true;
    };
    let components: Vec<_> = rel
        .components()
        .filter_map(|p| p.as_os_str().to_str())
        .collect();
    if components.iter().any(|s| {
        [
            ".ssh",
            ".gnupg",
            ".config",
            ".git",
            "node_modules",
            ".venv",
            "venv",
            "keyrings",
        ]
        .contains(s)
    }) {
        return true;
    }
    [
        home.join(".local"),
        home.join(".local/share"),
        home.join("scripts"),
        home.join("repos"),
    ]
    .contains(&root.to_owned())
        || root.ancestors().take_while(|p| *p != home).any(project)
}
impl Rule {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty()
            || self.name.len() > 80
            || !(1..=3650).contains(&self.days)
            || self.pattern.is_empty()
            || self.pattern.len() > 120
            || self.pattern.contains(['/', '\0', '\n', '\r'])
        {
            return Err("Name required; age 1–3650 days; filename patterns cannot contain a slash or newline.".into());
        }
        let home = w::home().canonicalize().map_err(|e| e.to_string())?;
        let path = self
            .folder
            .canonicalize()
            .map_err(|e| format!("Folder unavailable: {e}"))?;
        let m = fs::symlink_metadata(&self.folder).map_err(|e| e.to_string())?;
        if !m.is_dir()
            || m.file_type().is_symlink()
            || m.uid() != unsafe { libc::geteuid() }
            || protected(&path, &home)
        {
            return Err("Choose a specific user-owned folder inside your home. Credentials, configuration, repositories and dependency directories are protected.".into());
        }
        Ok(())
    }
}
impl Policy {
    pub fn load() -> Result<Self, String> {
        match fs::read_to_string(w::home().join(".config/tend/rules.json")) {
            Ok(s) => serde_json::from_str(&s).map_err(|e| e.to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.to_string()),
        }
    }
    pub fn save(&self) -> Result<(), String> {
        if self.rules.len() > 64 {
            return Err("At most 64 rules are supported.".into());
        }
        for r in &self.rules {
            r.validate()?;
        }
        w::private_write(
            &w::home().join(".config/tend/rules.json"),
            &serde_json::to_string_pretty(self).unwrap(),
        )
    }
}
fn matches(name: &std::ffi::OsStr, patterns: &str) -> bool {
    let Ok(name) = CString::new(name.as_bytes()) else {
        return false;
    };
    patterns.split(',').any(|p| {
        CString::new(p.trim())
            .map(|p| unsafe { libc::fnmatch(p.as_ptr(), name.as_ptr(), libc::FNM_PERIOD) } == 0)
            .unwrap_or(false)
    })
}
fn candidates(r: &Rule) -> Result<Vec<PathBuf>, String> {
    r.validate()?;
    let root = r.folder.canonicalize().map_err(|e| e.to_string())?;
    let home = w::home().canonicalize().map_err(|e| e.to_string())?;
    let dev = fs::metadata(&root).map_err(|e| e.to_string())?.dev();
    let cutoff = w::now().saturating_sub(r.days as u64 * 86400);
    let mut todo = vec![root.clone()];
    let mut files = vec![];
    let mut seen = HashSet::new();
    let mut visited = 0usize;
    while let Some(p) = todo.pop() {
        visited += 1;
        if visited > 250_000 {
            return Err("Rule exceeded 250,000 entries. Choose a smaller folder.".into());
        }
        let m = fs::symlink_metadata(&p).map_err(|e| e.to_string())?;
        if m.file_type().is_symlink() || m.dev() != dev {
            continue;
        }
        if m.is_dir() {
            if p != root && (!r.recursive || protected(&p, &home)) {
                continue;
            }
            for e in fs::read_dir(&p).map_err(|e| e.to_string())? {
                todo.push(e.map_err(|e| e.to_string())?.path());
            }
        } else if m.is_file()
            && m.uid() == unsafe { libc::geteuid() }
            && m.mtime() >= 0
            && (m.mtime() as u64) < cutoff
            && matches(p.file_name().unwrap_or_default(), &r.pattern)
            && seen.insert((m.dev(), m.ino()))
        {
            files.push(p);
        }
        if files.len() + todo.len() > 250_000 {
            return Err("Rule exceeded 250,000 entries. Choose a smaller folder.".into());
        }
    }
    Ok(files)
}
pub fn preview(p: &Policy) -> Result<String, String> {
    let mut text = String::from("Additional cleanup rules — preview only\n");
    for r in p.rules.iter().filter(|r| r.enabled) {
        match candidates(r) {
            Ok(files) => {
                let size: u64 = files
                    .iter()
                    .filter_map(|p| fs::symlink_metadata(p).ok())
                    .map(|m| m.blocks() * 512)
                    .sum();
                text.push_str(&format!(
                    "\n{} · {} · {} days · {} files · {}\n",
                    r.name,
                    r.folder.display(),
                    r.days,
                    files.len(),
                    w::bytes(size)
                ));
                for f in files.iter().take(20) {
                    text.push_str(&format!("  {}\n", f.display()));
                }
                if files.len() > 20 {
                    text.push_str("  …more files\n");
                }
            }
            Err(e) => text.push_str(&format!("\n{}: {e}\n", r.name)),
        }
    }
    Ok(text)
}
pub fn apply(p: &Policy) -> Result<String, String> {
    let mut text = String::new();
    let mut failed = false;
    for r in p.rules.iter().filter(|r| r.enabled) {
        match candidates(r) {
            Ok(files) => {
                let root = r.folder.canonicalize().map_err(|e| e.to_string())?;
                let mut count = 0;
                let mut errors = 0;
                for f in files {
                    match w::remove_old_file(&root, &f, r.days) {
                        Ok(true) => count += 1,
                        Ok(false) => {}
                        Err(_) => errors += 1,
                    }
                }
                if errors > 0 {
                    failed = true;
                }
                text.push_str(&format!(
                    "{}: {count} files removed; {errors} failures\n",
                    r.name
                ));
            }
            Err(e) => {
                failed = true;
                text.push_str(&format!("{}: skipped: {e}\n", r.name));
            }
        }
    }
    w::record("custom-cleanup", &text)?;
    if failed {
        Err(text)
    } else {
        Ok(text)
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Schedule {
    pub enabled: bool,
    pub expression: String,
    pub catch_up: bool,
}
impl Default for Schedule {
    fn default() -> Self {
        Self {
            enabled: false,
            expression: "0 10 * * *".into(),
            catch_up: true,
        }
    }
}
const START: &str = "# BEGIN TEND CLEANUP";
const END: &str = "# END TEND CLEANUP";
fn field(text: &str, min: u32, max: u32, value: u32) -> Result<bool, String> {
    let mut yes = false;
    for part in text.split(',') {
        let (range, step) = if let Some((r, s)) = part.split_once('/') {
            (r, s.parse::<u32>().map_err(|_| "Invalid cron step")?)
        } else {
            (part, 1)
        };
        if step == 0 || step > max.max(1) + 1 {
            return Err("Invalid cron step".into());
        }
        let (a, b) = if range == "*" {
            (min, max)
        } else if let Some((a, b)) = range.split_once('-') {
            (
                a.parse().map_err(|_| "Invalid cron range")?,
                b.parse().map_err(|_| "Invalid cron range")?,
            )
        } else {
            let a = range.parse().map_err(|_| "Invalid cron value")?;
            (a, if part.contains('/') { max } else { a })
        };
        if a < min || b > max || a > b {
            return Err("Cron value out of range".into());
        }
        yes |= value >= a && value <= b && (value - a) % step == 0;
    }
    Ok(yes)
}
pub fn validate_cron(expr: &str) -> Result<(), String> {
    if expr.len() > 120
        || !expr
            .bytes()
            .all(|b| b.is_ascii_digit() || b" */,-".contains(&b))
    {
        return Err("Use five cron fields containing numbers, *, /, comma and hyphen only.".into());
    }
    let f: Vec<_> = expr.split_whitespace().collect();
    if f.len() != 5 {
        return Err("Cron needs minute, hour, day, month and weekday fields.".into());
    }
    for ((lo, hi), s) in [(0, 59), (0, 23), (1, 31), (1, 12), (0, 7)].iter().zip(f) {
        field(s, *lo, *hi, *lo)?;
    }
    Ok(())
}
fn cron_matches(expr: &str, time: i64) -> bool {
    let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
    let t = time as libc::time_t;
    if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
        return false;
    }
    let f: Vec<_> = expr.split_whitespace().collect();
    if f.len() != 5 {
        return false;
    }
    let ok = |i: usize, lo, hi, v| field(f[i], lo, hi, v).unwrap_or(false);
    let dom = ok(2, 1, 31, tm.tm_mday as u32);
    let dow = ok(4, 0, 7, tm.tm_wday as u32) || (tm.tm_wday == 0 && ok(4, 0, 7, 7));
    let day = if !f[2].starts_with('*') && !f[4].starts_with('*') {
        dom || dow
    } else {
        dom && dow
    };
    ok(0, 0, 59, tm.tm_min as u32)
        && ok(1, 0, 23, tm.tm_hour as u32)
        && ok(3, 1, 12, (tm.tm_mon + 1) as u32)
        && day
}
pub fn latest_due(expr: &str, now: i64) -> Option<i64> {
    let start = now - now.rem_euclid(60);
    (0..=366 * 1440)
        .map(|m| start - m * 60)
        .find(|t| cron_matches(expr, *t))
}
pub fn managed_crontab(old: &str, block: &str) -> Result<String, String> {
    let mut text = String::new();
    let mut inside = false;
    let mut found = false;
    for line in old.lines() {
        if line == START {
            if inside || found {
                return Err("Duplicate Tend block; review crontab first.".into());
            }
            inside = true;
            found = true;
            continue;
        }
        if line == END {
            if !inside {
                return Err("Unmatched Tend cron marker".into());
            }
            inside = false;
            continue;
        }
        if !inside {
            text.push_str(line);
            text.push('\n');
        }
    }
    if inside {
        return Err("Unclosed Tend cron block".into());
    }
    if !block.is_empty() {
        text.push_str(START);
        text.push('\n');
        text.push_str(block);
        text.push_str(END);
        text.push('\n');
    }
    Ok(text)
}
impl Schedule {
    pub fn load() -> Result<Self, String> {
        match fs::read_to_string(w::home().join(".config/tend/cleanup-schedule.json")) {
            Ok(s) => serde_json::from_str(&s).map_err(|e| e.to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.to_string()),
        }
    }
    pub fn save(&self) -> Result<String, String> {
        validate_cron(&self.expression)?;
        let _lock = cleanup_lock()?;
        let old = std::process::Command::new("/usr/bin/crontab")
            .arg("-l")
            .output()
            .map_err(|e| e.to_string())?;
        let old = if old.status.success() {
            String::from_utf8_lossy(&old.stdout).into_owned()
        } else if String::from_utf8_lossy(&old.stderr).contains("no crontab") {
            String::new()
        } else {
            return Err(String::from_utf8_lossy(&old.stderr).into());
        };
        let worker = w::home().join("scripts/tend-worker");
        if self.enabled && !worker.is_file() {
            return Err("Install the user cleanup worker before enabling a schedule.".into());
        }
        let path = worker.to_string_lossy();
        if path.contains(['\n', '\r', '%']) {
            return Err("Unsupported worker path".into());
        }
        let quoted = format!("'{}'", path.replace('\'', "'\\''"));
        let block = if self.enabled {
            format!(
                "{} {} scheduled --apply >/dev/null 2>&1\n{}",
                self.expression,
                quoted,
                if self.catch_up {
                    format!("*/15 * * * * {quoted} scheduled --apply >/dev/null 2>&1\n@reboot {quoted} scheduled --apply >/dev/null 2>&1\n")
                } else {
                    String::new()
                }
            )
        } else {
            String::new()
        };
        let new = managed_crontab(&old, &block)?;
        let config = w::home().join(".config/tend/cleanup-schedule.json");
        let old_config = fs::read(&config).ok();
        w::private_write(&config, &serde_json::to_string_pretty(self).unwrap())?;
        let result = crate::workstation::run_with_input("/usr/bin/crontab", &["-"], &new);
        if let Err(e) = result {
            if let Some(bytes) = old_config {
                let _ = w::private_write(&config, &String::from_utf8_lossy(&bytes));
            } else {
                let _ = fs::remove_file(config);
            }
            return Err(e);
        }
        Ok(if self.enabled {
            format!("Cleanup scheduled: {} · system timezone. Missed-run catch-up {}. No update or restart action is scheduled.",self.expression,if self.catch_up{"enabled"}else{"disabled"})
        } else {
            "Cleanup schedule disabled. Other cron jobs preserved.".into()
        })
    }
}
pub fn cleanup_lock() -> Result<fs::File, String> {
    fs::create_dir_all(w::state_dir()).map_err(|e| e.to_string())?;
    let f = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(w::state_dir().join("cleanup.lock"))
        .map_err(|e| e.to_string())?;
    f.try_lock_exclusive()
        .map_err(|_| "Another cleanup or schedule change is in progress".to_string())?;
    Ok(f)
}
pub fn run_custom(p: &Policy) -> Result<String, String> {
    let _lock = cleanup_lock()?;
    apply(p)
}
pub fn scheduled(apply_changes: bool) -> Result<String, String> {
    let s = Schedule::load()?;
    if !s.enabled {
        return Ok("Cleanup schedule disabled.".into());
    }
    validate_cron(&s.expression)?;
    let now = w::now() as i64;
    let Some(due) = latest_due(&s.expression, now) else {
        return Err("No matching date within a year.".into());
    };
    if !s.catch_up && now - due > 300 {
        return Ok("No current scheduled run.".into());
    }
    let _lock = cleanup_lock()?;
    let path = w::state_dir().join("scheduled-cleanup.json");
    let mut state: serde_json::Value = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if state["last_due"].as_i64() == Some(due) {
        return Ok("Latest due cleanup already completed.".into());
    }
    if state["failed_due"].as_i64() == Some(due) && state["attempts"].as_u64().unwrap_or(0) >= 3 {
        return Err("Retry limit reached for latest due cleanup. Review report.".into());
    }
    if !apply_changes {
        return Ok(format!(
            "Dry run: latest due timestamp {due}\n{}",
            preview(&Policy::load()?)?
        ));
    }
    if output(
        "/usr/bin/systemctl",
        &["is-active", "dev-maintenance.service"],
    )
    .is_ok()
    {
        return Ok("Deferred while system maintenance is active.".into());
    }
    let uptime = fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok())
        .unwrap_or(0.);
    if uptime < 1200. {
        return Ok("Deferred until the 20-minute boot grace period ends.".into());
    }
    let policy = Policy::load()?;
    let mut c = w::Cleanup::load();
    let apt = c.apt;
    c.apt = false;
    let mut report = String::from("Scheduled user cleanup\n");
    if apt {
        report.push_str("APT cache remains handled by the existing system maintenance job.\n");
    }
    let mut success = true;
    for result in [w::cleanup_user(&c), apply(&policy)] {
        match result {
            Ok(s) => report.push_str(&s),
            Err(e) => {
                success = false;
                report.push_str(&format!("\nFailure: {e}\n"));
            }
        }
    }
    if success {
        state["last_due"] = due.into();
        state["attempts"] = 0.into();
    } else {
        let attempts = if state["failed_due"].as_i64() == Some(due) {
            state["attempts"].as_u64().unwrap_or(0) + 1
        } else {
            1
        };
        state["failed_due"] = due.into();
        state["attempts"] = attempts.into();
    }
    state["report"] = report.clone().into();
    state["finished_unix"] = now.into();
    w::private_write(&path, &serde_json::to_string_pretty(&state).unwrap())?;
    w::record("scheduled-cleanup", &report)?;
    if success {
        Ok(report)
    } else {
        Err(report)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cron_rejects_commands_and_bad_ranges() {
        for e in [
            "* * * * *; reboot",
            "60 10 * * *",
            "0 24 * * *",
            "*/0 * * * *",
            "@reboot",
            "* * * * *\n",
        ] {
            assert!(validate_cron(e).is_err(), "{e}");
        }
        assert!(validate_cron("*/15 8-18 * * 1-5").is_ok());
    }
    #[test]
    fn managed_block_preserves_unrelated_jobs() {
        let old = "PATH=/usr/bin\n0 3 * * 0 opencode-maintenance\n";
        let changed = managed_crontab(old, "0 10 * * * cleanup\n").unwrap();
        assert!(changed.starts_with(old));
        assert_eq!(managed_crontab(&changed, "").unwrap(), old);
        assert!(managed_crontab("# BEGIN TEND CLEANUP\n", "").is_err());
    }
    #[test]
    fn custom_patterns_are_filename_only() {
        assert!(matches(std::ffi::OsStr::new("a.log"), "*.log,*.tmp"));
        assert!(!matches(std::ffi::OsStr::new("a.rs"), "*.log"));
    }
}
