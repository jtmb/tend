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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub name: String,
    pub folder: PathBuf,
    pub pattern: String,
    pub days: u32,
    pub recursive: bool,
    pub enabled: bool,
}
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
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
    let state=home.join(".local/state/tend");
    if root.starts_with(&state)||state.canonicalize().is_ok_and(|path|root.starts_with(path)) {
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
            return Err("Choose a specific user-owned folder inside your home. Credentials, configuration, repositories, dependency directories and Tend’s own state are protected.".into());
        }
        Ok(())
    }
}
impl Policy {
    pub fn load() -> Result<Self, String> {
        match crate::config_io::read(&w::home().join(".config/tend/rules.json"),512*1024,Some(unsafe{libc::geteuid()}),false)? {
            Some(s)=>serde_json::from_slice(&s).map_err(|e|e.to_string()),None=>Ok(Self::default())
        }
    }
    pub fn save(&self) -> Result<(), String> {self.save_checked(None)}
    pub fn save_checked(&self,expected:Option<&Self>)->Result<(),String>{
        let _lock = cleanup_lock()?;
        if let Some(expected)=expected{if Self::load()?!=*expected{return Err("Saved cleanup rules changed. Review again.".into());}}
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
pub(crate) fn candidates(r: &Rule) -> Result<Vec<PathBuf>, String> {
    r.validate()?;
    let exclusions = crate::exclusions::Guard::acquire()?;
    let root = r.folder.canonicalize().map_err(|e| e.to_string())?;
    let home = w::home().canonicalize().map_err(|e| e.to_string())?;
    let dev = fs::metadata(&root).map_err(|e| e.to_string())?.dev();
    let cutoff = w::now().saturating_sub(r.days as u64 * 86400);
    let mut todo = vec![root.clone()];
    let mut files = vec![];
    let mut seen = HashSet::new();
    let mut visited = 0usize;
    while let Some(p) = todo.pop() {
        if exclusions.compiled.protects(&p) {
            continue;
        }
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
    let _exclusions = crate::exclusions::Guard::acquire()?;
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
    let cleanup=w::Cleanup {apt:false,docker_build:false,docker_images:false,npm_logs:false,pip:false,thumbnails:false,trash:false,..Default::default()};
    w::audit_result("custom-cleanup",crate::cleanup_plan::Plan::build(&cleanup,p,false)?.execute_unrecorded())
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    /// Only exact numeric daily/weekly schedules have a simple editor. Preserve
    /// all other expressions verbatim; this method never changes the schedule.
    pub fn simple_time(&self) -> Option<(u32, u32, u32, u32)> {
        let parts: Vec<_> = self.expression.split_whitespace().collect();
        if parts.len() != 5 || parts[2] != "*" || parts[3] != "*" {
            return None;
        }
        let minute = parts[0].parse::<u32>().ok().filter(|n| *n < 60)?;
        let hour = parts[1].parse::<u32>().ok().filter(|n| *n < 24)?;
        if parts[4] == "*" {
            Some((0, hour, minute, 0))
        } else {
            Some((
                1,
                hour,
                minute,
                parts[4].parse::<u32>().ok().filter(|n| *n <= 6)?,
            ))
        }
    }
    pub fn load() -> Result<Self, String> {
        match crate::config_io::read(&w::home().join(".config/tend/cleanup-schedule.json"),8192,Some(unsafe{libc::geteuid()}),false)? {
            Some(s)=>serde_json::from_slice(&s).map_err(|e|e.to_string()),None=>Ok(Self::default())
        }
    }
    pub fn save(&self) -> Result<String, String> {self.save_checked(None)}
    pub fn save_checked(&self,expected:Option<&Self>)->Result<String,String>{
        validate_cron(&self.expression)?;
        let _lock = cleanup_lock()?;
        if let Some(expected)=expected{if Self::load()?!=*expected{return Err("Saved cleanup schedule changed. Review again.".into());}}
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
pub struct CleanupLock(fs::File);
impl Drop for CleanupLock {fn drop(&mut self) {let _=FileExt::unlock(&self.0);}}
pub fn cleanup_lock() -> Result<CleanupLock, String> {
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
    Ok(CleanupLock(f))
}
pub fn run_custom(p: &Policy) -> Result<String, String> {
    let _lock = cleanup_lock()?;
    apply(p)
}
pub fn scheduled_preview() -> Result<String, String> {
    let mut c = w::Cleanup::load()?;
    c.apt = false;
    Ok(format!(
        "{}\nAPT runs only in daily system maintenance.",
        w::cleanup_preview(&c)?
    ))
}
fn read_scheduled_state(path:&Path)->Result<serde_json::Value,String> {
    use std::io::Read;
    let file=match fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW|libc::O_NONBLOCK).open(path) {
        Ok(file)=>file,Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(serde_json::json!({})),Err(e)=>return Err(format!("Cleanup receipt unavailable; nothing repeated: {e}")),
    };
    let m=file.metadata().map_err(|e|e.to_string())?;
    if !m.is_file()||m.uid()!=unsafe {libc::geteuid()}||m.mode()&0o077!=0 {return Err("Cleanup receipt must be a private, user-owned regular file.".into());}
    let mut text=String::new();file.take(64*1024+1).read_to_string(&mut text).map_err(|e|e.to_string())?;
    if text.len()>64*1024 {return Err("Cleanup receipt exceeds 64 KiB; existing file preserved.".into());}
    let state:serde_json::Value=serde_json::from_str(&text).map_err(|e|format!("Invalid cleanup receipt; nothing repeated: {e}"))?;
    let object=state.as_object().ok_or("Invalid cleanup receipt object")?;
    let allowed=["last_due","failed_due","attempts","report","finished_unix","running_due","started_unix","duration_ms","previous_uncertain_due"];
    let dates=["last_due","failed_due","finished_unix","running_due","started_unix","previous_uncertain_due"];
    if object.keys().any(|key|!allowed.contains(&key.as_str()))||object.iter().any(|(key,v)|if key=="report" {!v.is_string()}else {!v.is_null()&&v.as_u64().is_none()})||object.iter().any(|(key,v)|dates.contains(&key.as_str())&&!v.is_null()&&v.as_i64().is_none())||state["attempts"].as_u64().is_some_and(|n|n>3) {
        return Err("Invalid cleanup receipt fields; existing file preserved.".into());
    }
    Ok(state)
}
// File fsync + rename alone does not persist a newly created directory entry.
// Commit the receipt's directory chain before allowing a cleanup to execute.
fn persist_receipt_with(path:&Path,home:&Path,text:&str,mut sync:impl FnMut(&Path)->Result<(),String>)->Result<(),String>{
    if !path.parent().is_some_and(|p|p.ancestors().any(|p|p==home)){return Err("Cleanup receipt is outside the expected home directory.".into());}
    w::private_write(path,text)?;
    let mut directory=path.parent().unwrap();
    loop{
        sync(directory).map_err(|e|format!("Cleanup receipt directory sync failed at {}: {e}",directory.display()))?;
        if directory==home{return Ok(());}
        directory=directory.parent().ok_or("Cleanup receipt directory chain is incomplete")?;
    }
}
fn persist_scheduled_receipt(path:&Path,text:&str)->Result<(),String>{
    persist_receipt_with(path,&w::home(),text,|directory|{
        let file=fs::OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY).open(directory).map_err(|e|e.to_string())?;
        file.sync_all().map_err(|e|e.to_string())
    })
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
    let _exclusions = crate::exclusions::Guard::acquire()?;
    let path = w::state_dir().join("scheduled-cleanup.json");
    let mut state=read_scheduled_state(&path)?;
    if ["last_due","failed_due","running_due"].iter().any(|key|state[*key].as_i64().is_some_and(|seen|seen>due)) {
        return Err("The clock moved before a recorded cleanup date. No cleanup was repeated.".into());
    }
    if state["running_due"].as_i64()==Some(due) {
        return Err("The previous cleanup outcome is uncertain. Review its receipt before rerunning; no automatic replay was performed.".into());
    }
    if state["last_due"].as_i64() == Some(due) {
        return Ok("Latest due cleanup already completed.".into());
    }
    if state["failed_due"].as_i64() == Some(due) && state["attempts"].as_u64().unwrap_or(0) >= 3 {
        return Err("Retry limit reached for latest due cleanup. Review report.".into());
    }
    if !apply_changes {
        return Ok(format!(
            "Dry run: latest due timestamp {due}\n{}\n{}",
            scheduled_preview()?,
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
    let mut c = w::Cleanup::load()?;
    let apt = c.apt;
    c.apt = false;
    let attempt=if state["failed_due"].as_i64()==Some(due) {state["attempts"].as_u64().unwrap_or(0)+1}else {1};
    let mut report=format!("Scheduled user cleanup\nDue: {}\nStarted: {}\nAttempt {attempt} of 3\nDelay from schedule: {:.1} minutes\n",crate::presentation::when(due),crate::presentation::when(now),(now-due).max(0) as f64/60.);
    if let Some(old)=state["running_due"].as_i64() {state["previous_uncertain_due"]=old.into();report.push_str("An earlier maintenance date has an uncertain result; this run processes only the latest due date.\n");}
    state["running_due"]=due.into();state["started_unix"]=now.into();
    persist_scheduled_receipt(&path,&serde_json::to_string_pretty(&state).map_err(|e|e.to_string())?)?;
    let started=crate::observations::boot_millis();
    if apt {
        report.push_str("APT cache remains handled by the existing system maintenance job.\n");
    }
    let mut success = true;
    for result in
        [crate::cleanup_plan::Plan::build(&c, &policy, false).and_then(|p| p.execute_unrecorded())]
    {
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
    state.as_object_mut().unwrap().remove("running_due");
    let duration=started.zip(crate::observations::boot_millis()).and_then(|(a,b)|b.checked_sub(a));
    state["duration_ms"]=duration.into();state["finished_unix"]=w::now().into();
    if let Some(duration)=duration {report.push_str(&format!("\nElapsed: {:.1} seconds",duration as f64/1000.));}
    let mut end=report.len().min(8192);while !report.is_char_boundary(end) {end-=1;}
    state["report"]=format!("{}{}",&report[..end],if end<report.len(){"\nReceipt report truncated; saved history may contain more."}else{""}).into();
    if let Err(error)=persist_scheduled_receipt(&path,&serde_json::to_string_pretty(&state).map_err(|e|e.to_string())?) {
        report.push_str(&format!("\nThe operation result is above; its completed receipt could not be saved: {error}. The running receipt blocks automatic replay for this date."));
    }
    w::audit_result("scheduled-cleanup",if success {Ok(report)}else {Err(report)})

}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scheduled_receipt_syncs_ancestry_and_stops_on_sync_failure(){
        let parent=std::env::var_os("TEND_TEST_FIXTURES").map(PathBuf::from).unwrap_or_else(||PathBuf::from("/tmp/opencode"));
        let root=parent.join(format!("tend-receipt-contract-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let path=root.join(".local/state/tend/scheduled-cleanup.json");let mut synced=vec![];
        persist_receipt_with(&path,&root,"{\"running_due\":1}",|directory|{assert!(fs::read_to_string(&path).unwrap().contains("running_due"));synced.push(directory.to_path_buf());Ok(())}).unwrap();
        assert_eq!(synced,vec![root.join(".local/state/tend"),root.join(".local/state"),root.join(".local"),root.clone()]);
        let error=persist_receipt_with(&path,&root,"{\"running_due\":2}",|_|Err("injected sync failure".into())).unwrap_err();
        assert!(error.contains("directory sync failed"));assert!(fs::read_to_string(&path).unwrap().contains(":2"));
        let outside=root.with_extension("outside").join("scheduled-cleanup.json");assert!(!outside.exists());
        assert!(persist_receipt_with(&outside,&root,"{}",|_|Ok(())).is_err());assert!(!outside.exists());
    }
    #[test]
    fn cleanup_rules_cannot_remove_tend_receipts_or_live_history() {
        let home=Path::new("/home/fixture");
        assert!(protected(&home.join(".local/state/tend"),home));
        assert!(protected(&home.join(".local/state/tend/history.sqlite3"),home));
        assert!(!protected(&home.join(".local/state/fixture-cache"),home));
    }
    #[test]
    fn simple_cron_editor_is_lossless_and_conservative() {
        for (expression, expected) in [
            ("0 10 * * *", Some((0, 10, 0, 0))),
            ("05 08 * * 4", Some((1, 8, 5, 4))),
            ("59 23 * * 0", Some((1, 23, 59, 0))),
            ("*/15 8-18 * * 1-5", None),
            ("0 10 1 * *", None),
            ("0 10 * 2 *", None),
            ("60 10 * * *", None),
            ("0 24 * * *", None),
            ("0 10 * * 7", None),
            ("0 10 * * MON", None),
        ] {
            let s = Schedule {
                expression: expression.into(),
                ..Default::default()
            };
            assert_eq!(s.simple_time(), expected, "{expression}");
            assert_eq!(s.expression, expression);
        }
    }
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
