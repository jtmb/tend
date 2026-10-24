//! On-demand workstation operations. No scheduler or reboot paths.
use crate::output;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};
pub fn home() -> PathBuf {
    #[cfg(feature = "docs-demo")]
    if std::env::var_os("TEND_DEMO_DIR").is_some() {
        return std::env::var_os("TEND_DEMO_HOME")
            .map(PathBuf::from)
            .expect("Documentation recording requires an isolated demo home");
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/nonexistent"))
}
pub fn state_dir() -> PathBuf {
    home().join(".local/state/tend")
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn artifact_id() -> String {
    // Keep separate exports/history entries even when a user clicks twice in
    // one second, or two completed background actions report at the same time.
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!(
        "{}-{}-{:09}-{}",
        time.as_secs(),
        std::process::id(),
        time.subsec_nanos(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}
pub fn bytes(n: u64) -> String {
    let units = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024. && i < 4 {
        v /= 1024.;
        i += 1;
    }
    format!("{v:.1} {}", units[i])
}
pub fn private_write(path: &Path, text: &str) -> Result<(), String> {
    fs::create_dir_all(path.parent().ok_or("Missing parent")?).map_err(|e| e.to_string())?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp = path.with_extension(format!("{}.{nonce}.tmp", std::process::id()));
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(|e| e.to_string())?;
    f.write_all(text.as_bytes())
        .and_then(|_| f.sync_all())
        .map_err(|e| e.to_string())?;
    fs::rename(tmp, path).map_err(|e| e.to_string())
}
pub fn record(kind: &str, text: &str) -> Result<(), String> {
    let dir = state_dir().join("reports");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}-{kind}.json", artifact_id()));
    private_write(
        &path,
        &serde_json::to_string_pretty(
            &serde_json::json!({"time_unix":now(),"kind":kind,"report":text}),
        )
        .unwrap(),
    )
}
pub fn available() -> u64 {
    output("/usr/bin/df", &["-B1", "--output=avail", "/"])
        .ok()
        .and_then(|s| s.lines().last()?.trim().parse().ok())
        .unwrap_or(0)
}
pub fn command(program: &str, args: &[&str]) -> String {
    output(program, args).unwrap_or_else(|e| format!("Unavailable: {e}"))
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileSize {
    pub path: PathBuf,
    pub size: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scan {
    pub root: PathBuf,
    pub allocated: u64,
    pub files: u64,
    pub skipped: u64,
    pub cancelled: bool,
    pub entries: Vec<FileSize>,
    pub largest: Vec<FileSize>,
    pub builds: Vec<FileSize>,
}
pub fn scan(
    root: &Path,
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicU64>,
) -> Result<Scan, String> {
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, 10);
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let meta = fs::metadata(&root).map_err(|e| e.to_string())?;
    if !meta.is_dir() {
        return Err("Choose a directory to scan.".into());
    }
    let dev = meta.dev();
    let mut result = Scan {
        root: root.clone(),
        allocated: 0,
        files: 0,
        skipped: 0,
        cancelled: false,
        entries: vec![],
        largest: vec![],
        builds: vec![],
    };
    let mut totals = BTreeMap::<PathBuf, u64>::new();
    let mut builds = BTreeMap::<PathBuf, u64>::new();
    let mut pending = vec![(root.clone(), None::<PathBuf>)];
    let mut seen = HashSet::new();
    while let Some((p, build)) = pending.pop() {
        if cancel.load(Ordering::Relaxed) {
            result.cancelled = true;
            break;
        }
        let m = match fs::symlink_metadata(&p) {
            Ok(m) => m,
            Err(_) => {
                result.skipped += 1;
                continue;
            }
        };
        if m.file_type().is_symlink() || m.dev() != dev {
            result.skipped += 1;
            continue;
        }
        let build = build.or_else(|| {
            p.file_name()
                .and_then(|s| s.to_str())
                .filter(|s| {
                    ["target", "node_modules", ".venv", "venv", "build", "dist"].contains(s)
                })
                .map(|_| p.clone())
        });
        if m.is_dir() {
            match fs::read_dir(&p) {
                Ok(es) => {
                    for e in es {
                        match e {
                            Ok(e) => pending.push((e.path(), build.clone())),
                            Err(_) => result.skipped += 1,
                        }
                    }
                }
                Err(_) => result.skipped += 1,
            };
        } else if m.is_file() && seen.insert((m.dev(), m.ino())) {
            let size = m.blocks() * 512;
            result.allocated += size;
            result.files += 1;
            progress.store(result.files, Ordering::Relaxed);
            if let Ok(rel) = p.strip_prefix(&root) {
                if let Some(first) = rel.components().next() {
                    *totals.entry(root.join(first.as_os_str())).or_default() += size;
                }
            }
            if let Some(b) = build {
                *builds.entry(b).or_default() += size;
            }
            result.largest.push(FileSize { path: p, size });
            if result.largest.len() > 200 {
                result.largest.sort_by_key(|v| std::cmp::Reverse(v.size));
                result.largest.truncate(100);
            }
        }
        if result.files >= 1_000_000 || pending.len() > 250_000 {
            result.cancelled = true;
            break;
        }
    }
    result.entries = totals
        .into_iter()
        .map(|(path, size)| FileSize { path, size })
        .collect();
    result.builds = builds
        .into_iter()
        .map(|(path, size)| FileSize { path, size })
        .collect();
    result.entries.sort_by_key(|v| std::cmp::Reverse(v.size));
    result.builds.sort_by_key(|v| std::cmp::Reverse(v.size));
    result.largest.sort_by_key(|v| std::cmp::Reverse(v.size));
    result.largest.truncate(50);
    Ok(result)
}
pub fn scan_text(s: &Scan) -> String {
    let mut text = format!(
        "{}\nAllocated: {} • {} files • {} skipped\n{}\n",
        s.root.display(),
        bytes(s.allocated),
        s.files,
        s.skipped,
        if s.cancelled {
            "Partial scan (cancelled or safety limit reached)"
        } else {
            "Scan complete; symlinks and other filesystems excluded"
        }
    );
    for (title, items) in [
        ("Folders and files", &s.entries),
        ("Largest files", &s.largest),
        ("Development folders (report only)", &s.builds),
    ] {
        text.push_str(&format!("\n{title}\n"));
        for item in items.iter().take(100) {
            text.push_str(&format!(
                "{:>12}  {}\n",
                bytes(item.size),
                item.path.display()
            ));
        }
    }
    text
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Cleanup {
    pub version: u32,
    pub apt: bool,
    pub docker_build: bool,
    pub docker_images: bool,
    pub npm_logs: bool,
    pub pip: bool,
    pub thumbnails: bool,
    pub trash: bool,
    pub age_days: u32,
    pub keep_gb: u32,
    pub image_days: Option<u32>,
    pub thumbnail_days: Option<u32>,
    pub npm_log_days: u32,
}
impl Default for Cleanup {
    fn default() -> Self {
        Self {
            version: 1,
            apt: true,
            docker_build: true,
            docker_images: true,
            npm_logs: true,
            pip: false,
            thumbnails: false,
            trash: false,
            age_days: 14,
            keep_gb: 5,
            image_days: None,
            thumbnail_days: None,
            npm_log_days: 30,
        }
    }
}
impl Cleanup {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || !(7..=3650).contains(&self.age_days)
            || !(1..=100).contains(&self.keep_gb)
            || !(7..=3650).contains(&self.image_days())
            || !(1..=3650).contains(&self.thumbnail_days())
            || !(1..=3650).contains(&self.npm_log_days)
        {
            Err("Docker ages must be 7–3650 days, user-cache ages 1–3650 days, and build-cache allowance 1–100 GB.".into())
        } else {
            Ok(())
        }
    }
    pub fn image_days(&self) -> u32 {
        self.image_days.unwrap_or(self.age_days)
    }
    pub fn thumbnail_days(&self) -> u32 {
        self.thumbnail_days.unwrap_or(self.age_days)
    }
    pub fn docker_commands(&self) -> Vec<(&'static str, Vec<String>)> {
        let mut commands = vec![];
        if self.docker_build {
            commands.push((
                "Docker build cache",
                vec![
                    "builder".into(),
                    "prune".into(),
                    "--all".into(),
                    "--force".into(),
                    "--filter".into(),
                    format!("until={}h", self.age_days * 24),
                    "--keep-storage".into(),
                    format!("{}GB", self.keep_gb),
                ],
            ));
        }
        if self.docker_images {
            commands.push((
                "Docker dangling images",
                vec![
                    "image".into(),
                    "prune".into(),
                    "--force".into(),
                    "--filter".into(),
                    format!("until={}h", self.image_days() * 24),
                ],
            ));
        }
        commands
    }
    pub fn load() -> Result<Self, String> {
        Self::read_from(&home().join(".config/tend/cleanup.json"))
    }
    pub fn read_from(path: &Path) -> Result<Self, String> {
        let value = match fs::read_to_string(path) {
            Ok(s) => serde_json::from_str::<Self>(&s)
                .map_err(|e| format!("Saved cleanup policy is invalid: {e}"))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => return Err(format!("Cannot read saved cleanup policy: {e}")),
        };
        value.validate()?;
        Ok(value)
    }
    pub fn save(&self) -> Result<(), String> {
        self.validate()?;
        let _lock = crate::cleanup_rules::cleanup_lock()?;
        private_write(
            &home().join(".config/tend/cleanup.json"),
            &serde_json::to_string_pretty(self).unwrap(),
        )
    }
}
fn cache() -> PathBuf {
    home().join(".cache")
}

fn old_files(root: &Path, days: u32) -> Vec<PathBuf> {
    if root.canonicalize().map(|p| p != root).unwrap_or(true) {
        return vec![];
    }
    let mut files = vec![];
    let mut todo = vec![root.to_owned()];
    let cutoff = now().saturating_sub(days as u64 * 86400);
    let dev = fs::symlink_metadata(root).map(|m| m.dev()).unwrap_or(0);
    while let Some(p) = todo.pop() {
        let Ok(m) = fs::symlink_metadata(&p) else {
            continue;
        };
        if m.file_type().is_symlink() || m.dev() != dev {
            continue;
        }
        if m.is_dir() {
            if let Ok(es) = fs::read_dir(p) {
                todo.extend(es.filter_map(Result::ok).map(|e| e.path()));
            }
        } else if m.is_file() && m.mtime() >= 0 && (m.mtime() as u64) < cutoff {
            files.push(p);
        }
        if files.len() + todo.len() > 250_000 {
            break;
        }
    }
    files
}
fn cache_roots(c: &Cleanup) -> Vec<(&'static str, PathBuf, u32)> {
    let mut r = vec![];
    if c.npm_logs {
        r.push((
            "npm diagnostic logs",
            home().join(".npm/_logs"),
            c.npm_log_days,
        ));
    }
    if c.thumbnails {
        r.push((
            "Old thumbnails",
            cache().join("thumbnails"),
            c.thumbnail_days(),
        ));
    }
    r
}
pub fn cleanup_preview(c: &Cleanup) -> Result<String, String> {
    c.validate()?;
    let mut s = format!(
        "Cleanup preview • {} days • Docker allowance {} GB\n\n",
        c.age_days, c.keep_gb
    );
    if c.apt {
        s.push_str(&format!(
            "Obsolete APT downloads (simulation):\n{}\n\n",
            command("/usr/bin/apt-get", &["--simulate", "autoclean"])
        ));
    }
    if c.docker_build || c.docker_images {
        s.push_str(&format!(
            "Docker inventory (usage is not a deletion estimate):\n{}\n\n",
            command("/usr/bin/docker", &["system", "df"])
        ));
        s.push_str(&format!("Selected: {}{}\nBuild-cache age: {} days · dangling-image creation age: {} days. Docker decides eligibility at execution. Containers, volumes and tagged images are preserved.\n\n",if c.docker_build{"build cache "}else{""},if c.docker_images{"dangling images"}else{""},c.age_days,c.image_days()));
    }
    for (title, root, age) in cache_roots(c) {
        let files = old_files(&root, age);
        let total: u64 = files
            .iter()
            .filter_map(|p| fs::symlink_metadata(p).ok())
            .map(|m| m.blocks() * 512)
            .sum();
        s.push_str(&format!(
            "{title}: {} files, {}, older than {age} days\n",
            files.len(),
            bytes(total)
        ));
    }
    if c.pip {
        s.push_str(&format!(
            "\nPip download/wheel cache (all entries selected):\n{}\n",
            command(
                "/usr/bin/python3",
                &[
                    "-m",
                    "pip",
                    "--cache-dir",
                    &cache().join("pip").to_string_lossy(),
                    "cache",
                    "info"
                ]
            )
        ));
    }
    if c.trash {
        s.push_str("\nTrash: all items in the desktop Trash will be permanently deleted, including other mounted volumes supported by GIO.\n");
        s.push_str(&command("/usr/bin/gio", &["trash", "--list"]));
    }
    s.push_str("\nDeletion cannot be undone. Cached packages may need downloading again. No project directories or installed dependencies are selected.");
    Ok(s)
}
// Directory descriptors prevent following a parent that is swapped to a symlink.
pub(crate) fn remove_old_file(root: &Path, path: &Path, days: u32) -> Result<bool, String> {
    use std::{
        ffi::CString,
        os::{
            fd::{AsRawFd, FromRawFd, OwnedFd},
            unix::ffi::OsStrExt,
        },
    };
    if root.canonicalize().map(|p| p != root).unwrap_or(true) {
        return Err("Cache root contains a symbolic link or is unavailable".into());
    }
    let rel = path.strip_prefix(root).map_err(|e| e.to_string())?;
    let parts: Vec<_> = rel.components().collect();
    if parts.is_empty()
        || parts
            .iter()
            .any(|p| !matches!(p, std::path::Component::Normal(_)))
    {
        return Err("Invalid cache path".into());
    }
    let croot = CString::new(root.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
    let fd = unsafe { libc::open(croot.as_ptr(), flags) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let mut dir = unsafe { OwnedFd::from_raw_fd(fd) };
    let mut root_stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe { libc::fstat(dir.as_raw_fd(), root_stat.as_mut_ptr()) } < 0 {
        return Err("Cache folder unavailable".into());
    }
    let dev = unsafe { root_stat.assume_init() }.st_dev;
    for part in &parts[..parts.len() - 1] {
        let name = CString::new(part.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
        let fd = unsafe { libc::openat(dir.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        dir = unsafe { OwnedFd::from_raw_fd(fd) };
        let mut info = std::mem::MaybeUninit::<libc::stat>::uninit();
        if unsafe { libc::fstat(dir.as_raw_fd(), info.as_mut_ptr()) } < 0
            || unsafe { info.assume_init() }.st_dev != dev
        {
            return Err("Cache folder moved to another filesystem".into());
        }
    }
    let name =
        CString::new(parts.last().unwrap().as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    let mut info = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe {
        libc::fstatat(
            dir.as_raw_fd(),
            name.as_ptr(),
            info.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } < 0
    {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let info = unsafe { info.assume_init() };
    if info.st_mode & libc::S_IFMT != libc::S_IFREG
        || info.st_dev != dev
        || info.st_mtime < 0
        || info.st_mtime as u64 >= now().saturating_sub(days as u64 * 86400)
    {
        return Ok(false);
    }
    if unsafe { libc::unlinkat(dir.as_raw_fd(), name.as_ptr(), 0) } < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(true)
}
fn daily_log_candidates(root: &Path, days: u32) -> Result<Vec<PathBuf>, String> {
    if !root.exists() {
        return Ok(vec![]);
    }
    if root.canonicalize().map(|p| p != root).unwrap_or(true) {
        return Err("npm log folder contains a symbolic link".into());
    }
    let cutoff = now().saturating_sub(days as u64 * 86400);
    let mut files = vec![];
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let m = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if path.extension().and_then(|s| s.to_str()) == Some("log")
            && m.is_file()
            && !m.file_type().is_symlink()
            && m.mtime() >= 0
            && (m.mtime() as u64) < cutoff
        {
            files.push(path);
        }
    }
    Ok(files)
}
/// Daily npm cleanup retains the existing top-level *.log selection.
pub fn daily_npm_logs(days: u32) -> Result<String, String> {
    #[cfg(feature = "docs-demo")]
    if std::env::var_os("TEND_DEMO_DIR").is_some() {
        return Err("Deletion is disabled in documentation recordings".into());
    }
    if !(1..=3650).contains(&days) || unsafe { libc::geteuid() } == 0 {
        return Err("Invalid user log cleanup request".into());
    }
    let root = home().join(".npm/_logs");
    let mut count = 0;
    let mut errors = vec![];
    for path in daily_log_candidates(&root, days)? {
        match remove_old_file(&root, &path, days) {
            Ok(true) => count += 1,
            Ok(false) => {}
            Err(e) => errors.push(e),
        }
    }
    if errors.is_empty() {
        Ok(format!("npm diagnostic logs: removed {count} expired logs"))
    } else {
        Err(format!(
            "npm logs: removed {count}; failures: {}",
            errors.join("; ")
        ))
    }
}
pub fn cleanup_user(c: &Cleanup) -> Result<String, String> {
    #[cfg(feature = "docs-demo")]
    if std::env::var_os("TEND_DEMO_DIR").is_some() {
        return Err("Deletion is disabled in documentation recordings.".into());
    }
    c.validate()?;
    if output("/usr/bin/id", &["-u"])? == "0" {
        return Err("User cleanup cannot run as root".into());
    }
    let failed = std::cell::Cell::new(false);
    let run_cleanup = |p: &str, args: &[&str]| match crate::output(p, args) {
        Ok(s) => s,
        Err(e) => {
            failed.set(true);
            format!("Failed: {e}")
        }
    };
    let before = available();
    let mut log = String::new();
    for (title, args) in c.docker_commands() {
        let refs: Vec<_> = args.iter().map(String::as_str).collect();
        log.push_str(&format!(
            "{title}:\n{}\n",
            run_cleanup("/usr/bin/docker", &refs)
        ));
    }
    for (title, root, days) in cache_roots(c) {
        let mut count = 0;
        let mut failures = 0;
        for p in old_files(&root, days) {
            match remove_old_file(&root, &p, days) {
                Ok(true) => count += 1,
                Ok(false) => {}
                Err(_) => failures += 1,
            }
        }
        if failures > 0 {
            failed.set(true);
        }
        log.push_str(&format!(
            "{title}: removed {count} files; {failures} failures\n"
        ));
    }
    if c.pip {
        log.push_str(&format!(
            "Pip cache:\n{}\n",
            run_cleanup(
                "/usr/bin/python3",
                &[
                    "-m",
                    "pip",
                    "--cache-dir",
                    &cache().join("pip").to_string_lossy(),
                    "cache",
                    "purge"
                ]
            )
        ));
    }
    if c.trash {
        log.push_str(&format!(
            "Trash:\n{}\n",
            run_cleanup("/usr/bin/gio", &["trash", "--empty"])
        ));
    }
    log.push_str(&format!("\nRoot free space before: {}\nAfter: {}\nChange: {:.1} MiB (includes concurrent disk activity)\n",bytes(before),bytes(available()),(available() as i128-before as i128) as f64/1048576.));
    record("cleanup", &log)?;
    if failed.get() {
        Err(log)
    } else {
        Ok(log)
    }
}
#[derive(Clone, Default, Serialize)]
pub struct Sample {
    pub time: u64,
    pub total: u64,
    pub idle: u64,
    pub memory_total: u64,
    pub memory_available: u64,
    pub swap_total: u64,
    pub swap_free: u64,
    pub page_in: u64,
    pub page_out: u64,
    pub pressure: String,
    pub details: String,
}
pub fn sample() -> Sample {
    let stat = fs::read_to_string("/proc/stat").unwrap_or_default();
    let nums: Vec<u64> = stat
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .skip(1)
        .filter_map(|n| n.parse().ok())
        .collect();
    let mem = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let value = |key: &str| {
        mem.lines()
            .find(|s| s.starts_with(key))
            .and_then(|s| s.split_whitespace().nth(1))
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0)
            * 1024
    };
    let vm = fs::read_to_string("/proc/vmstat").unwrap_or_default();
    let pages = |key: &str| {
        vm.lines()
            .find(|s| s.starts_with(key))
            .and_then(|s| s.split_whitespace().nth(1))
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0)
    };
    let mut pressure = String::new();
    for kind in ["cpu", "memory", "io"] {
        pressure.push_str(&format!(
            "{kind}: {}",
            fs::read_to_string(format!("/proc/pressure/{kind}"))
                .unwrap_or_else(|_| "Unavailable\n".into())
        ));
    }
    let mut details = format!(
        "Load: {}\nUptime: {}\n",
        fs::read_to_string("/proc/loadavg")
            .unwrap_or_default()
            .trim(),
        fs::read_to_string("/proc/uptime")
            .unwrap_or_default()
            .split_whitespace()
            .next()
            .unwrap_or("?")
    );
    for base in ["/sys/class/thermal", "/sys/class/power_supply"] {
        if let Ok(es) = fs::read_dir(base) {
            for e in es.flatten() {
                let p = e.path();
                if let Ok(t) = fs::read_to_string(p.join("temp")) {
                    if let Ok(t) = t.trim().parse::<f64>() {
                        details.push_str(&format!(
                            "{}: {:.1} °C\n",
                            fs::read_to_string(p.join("type"))
                                .unwrap_or_default()
                                .trim(),
                            t / 1000.
                        ));
                    }
                }
                if let Ok(c) = fs::read_to_string(p.join("capacity")) {
                    details.push_str(&format!(
                        "Battery: {}% {}\n",
                        c.trim(),
                        fs::read_to_string(p.join("status"))
                            .unwrap_or_default()
                            .trim()
                    ));
                }
            }
        }
    }
    if let Ok(mm) = fs::read_to_string("/sys/block/zram0/mm_stat") {
        let fields: Vec<u64> = mm
            .split_whitespace()
            .filter_map(|s| s.parse().ok())
            .collect();
        if fields.len() >= 3 {
            details.push_str(&format!(
                "zram: {} original data, {} physical memory; {} saved\n",
                bytes(fields[0]),
                bytes(fields[2]),
                bytes(fields[0].saturating_sub(fields[2]))
            ));
        }
    }
    Sample {
        time: now(),
        total: nums.iter().take(8).sum(),
        idle: nums.get(3).copied().unwrap_or(0) + nums.get(4).copied().unwrap_or(0),
        memory_total: value("MemTotal:"),
        memory_available: value("MemAvailable:"),
        swap_total: value("SwapTotal:"),
        swap_free: value("SwapFree:"),
        page_in: pages("pswpin "),
        page_out: pages("pswpout "),
        pressure,
        details,
    }
}
pub fn sample_text(s: &Sample, prev: Option<&Sample>) -> String {
    let cpu = prev
        .filter(|p| s.total > p.total)
        .map(|p| {
            format!(
                "{:.1}%",
                100. * (1. - s.idle.saturating_sub(p.idle) as f64 / (s.total - p.total) as f64)
            )
        })
        .unwrap_or_else(|| "Collecting next sample…".into());
    let paging = prev
        .map(|p| {
            format!(
                "{} pages in / {} pages out since previous sample",
                s.page_in.saturating_sub(p.page_in),
                s.page_out.saturating_sub(p.page_out)
            )
        })
        .unwrap_or_else(|| "Waiting for comparison".into());
    format!("CPU {cpu}\nMemory {} / {} used (available-memory estimate)\nSwap {} / {} used\nPaging: {paging}\n\nPressure: percentage of time tasks were stalled\n{}\n{}\n",bytes(s.memory_total.saturating_sub(s.memory_available)),bytes(s.memory_total),bytes(s.swap_total.saturating_sub(s.swap_free)),bytes(s.swap_total),s.pressure,s.details)
}
pub fn system_report() -> Result<String, String> {
    let s = sample();
    let mut text = sample_text(&s, None);
    for (title, program, args) in [
        (
            "Mounted filesystems",
            "/usr/bin/df",
            vec!["-hT", "-x", "tmpfs", "-x", "devtmpfs"],
        ),
        ("Compressed swap", "/usr/sbin/zramctl", vec![]),
        ("Swap devices", "/usr/sbin/swapon", vec!["--show"]),
        (
            "Failed system services",
            "/usr/bin/systemctl",
            vec!["--failed", "--no-pager", "--plain"],
        ),
        (
            "Failed user services",
            "/usr/bin/systemctl",
            vec!["--user", "--failed", "--no-pager", "--plain"],
        ),
        ("Startup time", "/usr/bin/systemd-analyze", vec!["time"]),
    ] {
        text.push_str(&format!("\n{title}\n{}\n", command(program, &args)));
    }
    text.push_str(&format!(
        "\nSwappiness: {}\nReboot required by installed updates: {}\nRoot below 20 GB free: {}\n",
        fs::read_to_string("/proc/sys/vm/swappiness")
            .unwrap_or_default()
            .trim(),
        Path::new("/var/run/reboot-required").exists(),
        available() < 20_000_000_000
    ));
    Ok(text)
}
pub fn verify_npm() -> Result<String, String> {
    let fallback = home().join(".opencode/bin/npm");
    let mut cmd = if fallback.is_file() {
        let mut c = Command::new(&fallback);
        let path = std::env::var("PATH").unwrap_or_default();
        c.env(
            "PATH",
            format!("{}:{path}", fallback.parent().unwrap().display()),
        );
        c
    } else {
        Command::new("npm")
    };
    let r = cmd
        .args(["cache", "verify"])
        .output()
        .map_err(|e| e.to_string())?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&r.stdout),
        String::from_utf8_lossy(&r.stderr)
    );
    record("npm-verify", &text)?;
    if r.status.success() {
        Ok(text)
    } else {
        Err(text)
    }
}
pub fn development_report() -> Result<String, String> {
    let mut s = String::new();
    for (title, p, args) in [
        (
            "Docker storage",
            "/usr/bin/docker",
            vec!["system", "df", "-v"],
        ),
        (
            "Container resource snapshot",
            "/usr/bin/docker",
            vec!["stats", "--no-stream"],
        ),
        (
            "Pip cache",
            "/usr/bin/python3",
            vec!["-m", "pip", "cache", "info"],
        ),
    ] {
        s.push_str(&format!("{title}\n{}\n\n", command(p, &args)));
    }
    if let Ok(ids) = output("/usr/bin/docker", &["ps", "-q"]) {
        for id in ids.lines().take(100) {
            s.push_str(&command("/usr/bin/docker",&["inspect","--format","{{.Name}} log driver={{.HostConfig.LogConfig.Type}} options={{json .HostConfig.LogConfig.Config}} log path={{.LogPath}}",id]));
            s.push('\n');
        }
    }
    s.push_str("\nMissing per-container max-size/max-file options may inherit daemon defaults; inspect the daemon policy before concluding logs are uncapped. Project sizes are available in the disk scan.\n");
    Ok(s)
}
#[derive(Clone)]
pub struct Process {
    pub pid: u32,
    pub name: String,
    pub rss: u64,
    pub ticks: u64,
    pub start: u64,
    pub cpu: f64,
}
pub fn processes(previous: &[Process], elapsed: f64) -> Vec<Process> {
    let uid = fs::metadata(home()).map(|m| m.uid()).unwrap_or(u32::MAX);
    let mut list = vec![];
    if let Ok(es) = fs::read_dir("/proc") {
        for e in es.flatten() {
            let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else {
                continue;
            };
            if e.metadata().map(|m| m.uid() != uid).unwrap_or(true) {
                continue;
            }
            let Ok(s) = fs::read_to_string(e.path().join("stat")) else {
                continue;
            };
            let Some(end) = s.rfind(')') else { continue };
            let fields: Vec<_> = s[end + 1..].split_whitespace().collect();
            let n = |i: usize| {
                fields
                    .get(i)
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0)
            };
            let ticks = n(11) + n(12);
            let start = n(19);
            let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f64;
            let cpu = previous
                .iter()
                .find(|p| p.pid == pid && p.start == start)
                .map(|p| 100. * ticks.saturating_sub(p.ticks) as f64 / hz / elapsed.max(0.01))
                .unwrap_or(0.);
            let name = s
                .find('(')
                .map(|i| s[i + 1..end].to_string())
                .unwrap_or_default();
            let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(1) as u64;
            list.push(Process {
                pid,
                name,
                rss: n(21) * page,
                ticks,
                start,
                cpu,
            });
        }
    }
    list
}
pub fn signal_process(p: &Process, force: bool) -> Result<String, String> {
    #[cfg(feature = "docs-demo")]
    if std::env::var_os("TEND_DEMO_DIR").is_some() {
        return Err("Process signals are disabled in documentation recordings.".into());
    }
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    if p.pid == std::process::id()
        || [
            "gnome-shell",
            "gnome-session",
            "gnome-keyring",
            "dbus",
            "systemd",
            "pipewire",
            "wireplumber",
        ]
        .iter()
        .any(|n| p.name.starts_with(n))
    {
        return Err(
            "This process provides the desktop session. Use its dedicated settings instead.".into(),
        );
    }
    // Pin the kernel process identity before checking its owner/start time.
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, p.pid, 0) } as i32;
    if fd < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    if !processes(&[], 1.)
        .iter()
        .any(|v| v.pid == p.pid && v.start == p.start)
    {
        return Err("Process exited or changed identity. Refresh the list.".into());
    }
    let result = unsafe {
        libc::syscall(
            libc::SYS_pidfd_send_signal,
            fd.as_raw_fd(),
            if force { libc::SIGKILL } else { libc::SIGTERM },
            std::ptr::null::<libc::siginfo_t>(),
            0,
        )
    };
    if result < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    record(
        "process",
        &format!(
            "{} requested for {} ({})",
            if force { "Force quit" } else { "Quit" },
            p.name,
            p.pid
        ),
    )?;
    Ok("Signal sent; application recovery depends on the app.".into())
}

pub fn export_report(text: &str, json: bool) -> Result<String, String> {
    let dir = home().join("Documents/Tend reports");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let p = dir.join(format!(
        "report-{}.{}",
        artifact_id(),
        if json { "json" } else { "txt" }
    ));
    let body = if json {
        serde_json::to_string_pretty(&serde_json::json!({"created_unix":now(),"report":text}))
            .unwrap()
    } else {
        text.into()
    };
    private_write(&p, &body)?;
    Ok(format!("Saved {}", p.display()))
}
pub fn history() -> Result<String, String> {
    if !state_dir().join("reports").exists() {
        return Ok("No Tend reports have been saved yet.".into());
    }
    let mut paths: Vec<_> = fs::read_dir(state_dir().join("reports"))
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .collect();
    paths.sort();
    let mut text = String::new();
    for p in paths.iter().rev().take(40) {
        if let Ok(raw) = fs::read_to_string(p) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
                let when = value["time_unix"]
                    .as_i64()
                    .map(crate::presentation::when)
                    .unwrap_or_else(|| "Unknown date".into());
                text.push_str(&format!(
                    "{} · {}\n{}\n\n",
                    when,
                    value["kind"].as_str().unwrap_or("report"),
                    value["report"].as_str().unwrap_or("No report text")
                ));
            }
        }
    }
    Ok(text)
}
pub fn run_with_input(program: &str, args: &[&str], input: &str) -> Result<String, String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    child
        .stdin
        .take()
        .ok_or("Missing input pipe")?
        .write_all(input.as_bytes())
        .map_err(|e| e.to_string())?;
    let r = child.wait_with_output().map_err(|e| e.to_string())?;
    if r.status.success() {
        Ok(String::from_utf8_lossy(&r.stdout).into())
    } else {
        Err(format!(
            "{}\n{}",
            String::from_utf8_lossy(&r.stdout),
            String::from_utf8_lossy(&r.stderr)
        ))
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;
    use std::os::unix::{ffi::OsStrExt, fs::symlink};
    #[test]
    fn repeated_report_ids_are_unique_even_with_parallel_completions() {
        let threads: Vec<_> = (0..8)
            .map(|_| std::thread::spawn(|| (0..100).map(|_| artifact_id()).collect::<Vec<_>>()))
            .collect();
        let ids: Vec<_> = threads.into_iter().flat_map(|t| t.join().unwrap()).collect();
        assert_eq!(ids.iter().collect::<HashSet<_>>().len(), ids.len());
    }
    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let p = PathBuf::from("/tmp/opencode").join(format!(
                "tend-boundary-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn old_file(&self, relative: &str) -> PathBuf {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, b"preserve outside cache").unwrap();
            let name = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
            let times = [libc::timespec {
                tv_sec: (now() - 60 * 86400) as libc::time_t,
                tv_nsec: 0,
            }; 2];
            assert_eq!(
                unsafe { libc::utimensat(libc::AT_FDCWD, name.as_ptr(), times.as_ptr(), 0) },
                0
            );
            path
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn cache_cleanup_rejects_replaced_parent_and_preserves_external_file() {
        let t = Scratch::new();
        let cache = t.0.join("cache");
        fs::create_dir(&cache).unwrap();
        let original = t.old_file("cache/sub/log");
        let outside = t.old_file("outside/log");
        assert!(old_files(&cache, 30).contains(&original));
        fs::rename(cache.join("sub"), cache.join("original-sub")).unwrap();
        symlink(t.0.join("outside"), cache.join("sub")).unwrap();
        assert!(remove_old_file(&cache, &original, 30).is_err());
        assert!(outside.exists());
        assert!(cache.join("original-sub/log").exists());
    }
    #[test]
    fn daily_npm_preview_selects_only_expired_top_level_regular_logs() {
        let t = Scratch::new();
        let root = t.0.join("logs");
        fs::create_dir_all(root.join("sub")).unwrap();
        t.old_file("logs/old.log");
        t.old_file("logs/old.cache");
        t.old_file("logs/sub/nested.log");
        fs::write(root.join("recent.log"), "new").unwrap();
        symlink(root.join("old.log"), root.join("linked.log")).unwrap();
        assert_eq!(
            daily_log_candidates(&root, 14).unwrap(),
            vec![root.join("old.log")]
        );
        for file in [
            "old.log",
            "old.cache",
            "sub/nested.log",
            "recent.log",
            "linked.log",
        ] {
            assert!(root.join(file).exists());
        }
    }
    #[test]
    #[ignore = "Deletes isolated test files; requires explicit cleanup-test opt-in"]
    fn cleanup_removes_only_old_regular_files_inside_the_selected_root() {
        let t = Scratch::new();
        let root = t.0.join("cache");
        let old = t.old_file("cache/old");
        let external = t.old_file("outside/data");
        let fresh = root.join("fresh");
        fs::write(&fresh, b"new").unwrap();
        symlink(&external, root.join("link")).unwrap();
        assert_eq!(old_files(&root, 30), vec![old.clone()]);
        assert!(!remove_old_file(&root, &fresh, 30).unwrap());
        assert!(!remove_old_file(&root, &root.join("link"), 30).unwrap());
        assert!(remove_old_file(&root, &external, 30).is_err());
        assert!(remove_old_file(&root, &old, 30).unwrap());
        assert!(fresh.exists() && external.exists());
        assert!(!old.exists());
    }
    #[test]
    fn scan_counts_hardlinks_once_and_does_not_follow_directory_links() {
        let t = Scratch::new();
        let data = t.old_file("scan/data");
        let external = t.old_file("outside/secret");
        fs::hard_link(&data, t.0.join("scan/duplicate")).unwrap();
        symlink(external.parent().unwrap(), t.0.join("scan/link")).unwrap();
        let result = scan(
            &t.0.join("scan"),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicU64::new(0)),
        )
        .unwrap();
        assert_eq!(result.files, 1);
        assert_eq!(result.allocated, fs::metadata(data).unwrap().blocks() * 512);
        assert_eq!(result.skipped, 1);
        assert!(result
            .largest
            .iter()
            .all(|f| f.path.starts_with(t.0.join("scan"))));
    }
    #[test]
    fn scan_honors_cancellation_before_reading_contents() {
        let t = Scratch::new();
        t.old_file("scan/a");
        let result = scan(
            &t.0.join("scan"),
            Arc::new(AtomicBool::new(true)),
            Arc::new(AtomicU64::new(0)),
        )
        .unwrap();
        assert!(result.cancelled);
        assert_eq!(result.files, 0);
    }
}

#[cfg(test)]
mod cleanup_policy_tests {
    use super::*;
    #[test]
    fn legacy_policy_keeps_selections_and_shared_retention() {
        let c: Cleanup = serde_json::from_str(
            r#"{"apt":false,"docker_build":false,"trash":true,"age_days":45,"keep_gb":9}"#,
        )
        .unwrap();
        c.validate().unwrap();
        assert!(!c.apt && !c.docker_build && c.trash);
        assert_eq!(
            (
                c.age_days,
                c.image_days(),
                c.thumbnail_days(),
                c.npm_log_days
            ),
            (45, 45, 45, 30)
        );
        let reloaded: Cleanup = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(c, reloaded);
    }
    #[test]
    fn independent_retention_reaches_exact_docker_commands() {
        let c = Cleanup {
            age_days: 21,
            image_days: Some(60),
            thumbnail_days: Some(90),
            npm_log_days: 45,
            keep_gb: 8,
            ..Default::default()
        };
        let commands = c.docker_commands();
        assert_eq!(commands.len(), 2);
        assert_eq!(
            commands[0].1,
            vec![
                "builder",
                "prune",
                "--all",
                "--force",
                "--filter",
                "until=504h",
                "--keep-storage",
                "8GB"
            ]
        );
        assert_eq!(
            commands[1].1,
            vec!["image", "prune", "--force", "--filter", "until=1440h"]
        );
        assert_eq!(cache_roots(&c)[0].2, 45);
        assert!(Cleanup {
            docker_build: false,
            docker_images: false,
            ..c
        }
        .docker_commands()
        .is_empty());
    }
    #[test]
    fn invalid_saved_policy_never_falls_back_to_enabled_defaults() {
        let root = PathBuf::from(format!(
            "/tmp/opencode/tend-policy-test-{}-{}",
            std::process::id(),
            now()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("cleanup.json");
        assert!(Cleanup::read_from(&path).is_ok());
        for (i, text) in [
            "{",
            r#"{"version":2}"#,
            r#"{"keep_gb":0}"#,
            r#"{"command":"reboot"}"#,
            r#"{"npm_log_days":0}"#,
        ]
        .iter()
        .enumerate()
        {
            let fixture = root.join(format!("invalid-{i}.json"));
            fs::write(&fixture, text).unwrap();
            assert!(Cleanup::read_from(&fixture).is_err());
            assert_eq!(fs::read_to_string(&fixture).unwrap(), *text);
        }
    }
}
