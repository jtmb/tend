//! Shared Tend cleanup policy. Stable flock inode serializes policy replacement
//! against readers; all cleanup entry points hold a reader for their operation.
#[cfg(feature = "docs-demo")]
use crate::workstation as w;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    os::unix::{
        fs::{MetadataExt, OpenOptionsExt},
        process::CommandExt,
    },
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
pub const CONFIG: &str = "/etc/tend/exclusions.json";
pub const LOCK: &str = "/etc/tend/exclusions.lock";
pub const READY: &str = "/etc/tend/exclusions-ready";
pub const ADAPTER: &str = "/usr/local/lib/dev-maintenance/tend_cleanup_policy.py";
pub const MAX_BYTES: usize = 512 * 1024;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub path: PathBuf,
    pub enabled: bool,
    pub note: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    pub version: u32,
    pub revision: u64,
    pub paths: Vec<Entry>,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            version: 1,
            revision: 0,
            paths: vec![],
        }
    }
}
impl Policy {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.paths.len() > 64 {
            return Err(
                "Exclusions version must be 1; at most 64 protected paths are supported.".into(),
            );
        }
        let mut paths = std::collections::HashSet::new();
        for entry in &self.paths {
            if !entry.path.is_absolute()
                || entry
                    .path
                    .to_str()
                    .is_none_or(|s| s.len() > 4096 || s.chars().any(char::is_control))
                || entry
                    .path
                    .components()
                    .any(|c| matches!(c, Component::ParentDir))
                || entry.note.chars().count() > 120
                || entry.note.chars().any(char::is_control)
                || !paths.insert(&entry.path)
            {
                return Err("Use unique absolute paths without parent traversal or control characters; notes may contain up to 120 characters.".into());
            }
        }
        Ok(())
    }
    pub fn active(&self) -> bool {
        self.paths.iter().any(|p| p.enabled)
    }
    pub fn read() -> Result<Self, String> {
        let config = config_path();
        if !config.exists()
            && (lock_path().exists() || (config == Path::new(CONFIG) && Path::new(READY).exists()))
        {
            return Err("The exclusions policy is missing; restore it before continuing.".into());
        }
        read_at(&config, owner())
    }
    pub fn compile(&self) -> Result<Compiled, String> {
        self.validate()?;
        let mut paths = Vec::new();
        for entry in self.paths.iter().filter(|e| e.enabled) {
            paths.push(entry.path.clone());
            let canonical = resolve_missing(&entry.path)?;
            if canonical != entry.path {
                paths.push(canonical);
            }
        }
        Ok(Compiled {
            policy: self.clone(),
            paths,
        })
    }
}
#[derive(Clone, Debug)]
pub struct Compiled {
    pub policy: Policy,
    paths: Vec<PathBuf>,
}
impl Compiled {
    pub fn protects(&self, path: &Path) -> bool {
        self.paths.iter().any(|root| path.starts_with(root))
    }
    pub fn overlaps(&self, root: &Path) -> bool {
        self.paths
            .iter()
            .any(|p| p.starts_with(root) || root.starts_with(p))
    }
    pub fn bulk_reason(&self, kind: &str, root: Option<&Path>) -> Option<String> {
        if !self.policy.active() {
            return None;
        }
        match kind {
            "docker"=>Some("Docker pruning cannot apply protected-path exclusions to engine-managed cache; category skipped.".into()),
            "trash"=>Some("Empty Trash spans desktop and mounted-volume trash stores; category skipped while exclusions are active.".into()),
            "npm-verify"=>Some("npm verification can garbage-collect cache entries without path exclusions; verification skipped.".into()),
            "pip"|"apt"=>match root {Some(root) if !self.overlaps(root)=>None,Some(_)=>Some(format!("A protected path overlaps the {kind} cache; entire category skipped.")),None=>Some(format!("The {kind} cache location could not be verified; entire category skipped."))},
            _=>None,
        }
    }
}
pub(crate) fn resolve_missing(path: &Path) -> Result<PathBuf, String> {
    let mut existing = path;
    let mut missing = Vec::new();
    loop {
        match existing.canonicalize() {
            Ok(mut root) => {
                for name in missing.iter().rev() {
                    root.push(name);
                }
                return Ok(root);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let name = existing
                    .file_name()
                    .ok_or("Protected path has no resolvable ancestor")?;
                missing.push(name.to_os_string());
                existing = existing.parent().ok_or("Invalid protected path")?;
            }
            Err(e) => {
                return Err(format!(
                    "Protected path cannot be resolved: {}: {e}. Cleanup was stopped.",
                    path.display()
                ))
            }
        }
    }
}
fn config_path() -> PathBuf {
    #[cfg(feature = "docs-demo")]
    if std::env::var_os("TEND_DEMO_DIR").is_some() {
        return w::home().join(".config/tend/exclusions.json");
    }
    PathBuf::from(CONFIG)
}
fn lock_path() -> PathBuf {
    #[cfg(feature = "docs-demo")]
    if std::env::var_os("TEND_DEMO_DIR").is_some() {
        return w::home().join(".config/tend/exclusions.lock");
    }
    PathBuf::from(LOCK)
}
fn owner() -> u32 {
    #[cfg(feature = "docs-demo")]
    if std::env::var_os("TEND_DEMO_DIR").is_some() {
        return unsafe { libc::geteuid() };
    }
    0
}
fn checked(file: &File, owner: u32) -> Result<(), String> {
    let m = file.metadata().map_err(|e| e.to_string())?;
    if !m.is_file() || m.uid() != owner || m.mode() & 0o022 != 0 {
        return Err(
            "Exclusions and their lock must be protected regular files owned by the administrator."
                .into(),
        );
    }
    Ok(())
}
fn parents(path: &Path, owner: u32) -> Result<(), String> {
    let system = owner == 0;
    for dir in path.parent().into_iter().flat_map(Path::ancestors) {
        let m = fs::symlink_metadata(dir).map_err(|e| e.to_string())?;
        if !m.is_dir() || m.file_type().is_symlink() || m.uid() != owner || m.mode() & 0o022 != 0 {
            return Err("Exclusions directory is not protected; cleanup stopped.".into());
        }
        if !system {
            break;
        }
    }
    Ok(())
}
fn read_at(path: &Path, owner: u32) -> Result<Policy, String> {
    if path.exists() && path.parent().is_some_and(Path::exists) {
        parents(path, owner)?;
    }
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Policy::default()),
        Err(e) => return Err(format!("Cannot read exclusions: {e}")),
    };
    checked(&file, owner)?;
    let mut data = Vec::new();
    file.take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() > MAX_BYTES {
        return Err("Exclusions exceed the 512 KiB policy limit.".into());
    }
    let policy: Policy = serde_json::from_slice(&data)
        .map_err(|e| format!("Invalid exclusions; cleanup stopped: {e}"))?;
    policy.validate()?;
    Ok(policy)
}
pub struct Guard {
    pub compiled: Compiled,
    _lock: Option<File>,
}
impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(file)=&self._lock {let _=FileExt::unlock(file);}
    }
}
impl Guard {
    pub fn acquire() -> Result<Self, String> {
        Self::at(&config_path(), &lock_path(), owner(), false)
    }
    pub fn exclusive() -> Result<Self, String> {
        Self::at(&config_path(), &lock_path(), owner(), true)
    }
    fn at(config: &Path, lock: &Path, owner: u32, exclusive: bool) -> Result<Self, String> {
        if lock.exists() && lock.parent().is_some_and(Path::exists) {
            parents(lock, owner)?;
        }
        let file=match OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW|libc::O_NONBLOCK).open(lock) {
            Ok(file)=>{checked(&file,owner)?;if exclusive {FileExt::try_lock_exclusive(&file)}else {FileExt::try_lock_shared(&file)}.map_err(|_|"Cleanup or an exclusion change is active; retry after it finishes.".to_string())?;Some(file)},
            Err(e) if e.kind()==std::io::ErrorKind::NotFound&&fs::symlink_metadata(config).is_err()&&!exclusive&&!(config==Path::new(CONFIG)&&Path::new(READY).exists())=>None,
            Err(e)=>return Err(format!("Exclusion locking is unavailable; install matching Tend integration before cleanup: {e}")),
        };
        if file.is_some() && !config.exists() {
            return Err("The exclusions policy is missing; cleanup was stopped. Restore it before continuing.".into());
        }
        let policy = read_at(config, owner)?;
        // A writer must be able to repair a formerly valid path that now has a
        // symlink loop or permission failure. It needs the revision/schema,
        // while cleanup readers must resolve every active protection.
        let compiled = if exclusive {Compiled {policy, paths:vec![]}} else {policy.compile()?};
        Ok(Self {
            compiled,
            _lock: file,
        })
    }
}
pub fn ready() -> bool {
    parents(Path::new(READY), 0).is_ok()
        && fs::symlink_metadata(READY).is_ok_and(|m| {
            m.is_file() && !m.file_type().is_symlink() && m.uid() == 0 && m.mode() & 0o022 == 0
        })
        && fs::read_to_string(READY).is_ok_and(|s| s.trim() == "1")
        && protected_file(Path::new(ADAPTER), 0)
        && fs::metadata(ADAPTER).is_ok_and(|m| m.len() <= 128 * 1024)
        && probe("/usr/bin/python3", &[ADAPTER, "--capabilities"])
            .is_some_and(|text| capabilities_match(&text, "maintenance-adapter"))
}
fn protected_file(path: &Path, owner: u32) -> bool {
    (owner != 0 || parents(path, owner).is_ok())
        && fs::symlink_metadata(path).is_ok_and(|m| {
            m.is_file() && !m.file_type().is_symlink() && m.uid() == owner && m.mode() & 0o022 == 0
        })
}
pub fn binary_supports(path: &Path, role: &str, owner: u32) -> bool {
    protected_file(path, owner)
        && probe(path.to_str().unwrap_or(""), &["capabilities"])
            .is_some_and(|text| capabilities_match(&text, role))
}
/// Read-only compatibility queries: bounded output, no input, and a three-second
/// lifetime. Only this newly spawned process group is stopped on timeout.
pub fn probe(program: &str, args: &[&str]) -> Option<String> {
    use std::os::fd::AsRawFd;
    let mut child = Command::new(program)
        .args(args)
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let fd = stdout.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    let mut data = Vec::new();
    let start = Instant::now();
    let mut success = false;
    if flags >= 0 && unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } >= 0 {
        'query: loop {
            let mut bytes = [0; 1024];
            loop {
                match stdout.read(&mut bytes) {
                    Ok(0) => break,
                    Ok(n) => {
                        data.extend_from_slice(&bytes[..n]);
                        if data.len() > 8192 || start.elapsed() >= Duration::from_secs(3) {
                            break 'query;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break 'query,
                }
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    // Drain the finite pipe once more after observing exit.
                    if stdout
                        .take(8193 - data.len() as u64)
                        .read_to_end(&mut data)
                        .is_ok()
                    {
                        success = status.success() && data.len() <= 8192;
                    }
                    break;
                }
                Ok(None) if start.elapsed() < Duration::from_secs(3) => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                _ => break,
            }
        }
    }
    if !success {
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
    }
    let _ = child.wait();
    success.then(|| String::from_utf8(data).ok()).flatten()
}
pub fn capabilities_match(text: &str, role: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .is_some_and(|v| {
            v["protocol"].as_u64().is_some_and(|p| p >= 3)
                && v["role"].as_str() == Some(role)
                && v["cleanup_exclusions"] == true
                && v["explained_cleanup"] == true
        })
}
pub fn integration_supported() -> bool {
    if !ready() {
        return false;
    }
    binary_supports(
        Path::new("/usr/local/libexec/tend-admin"),
        "administrator",
        0,
    ) && binary_supports(Path::new("/usr/local/libexec/tend-worker"), "worker", 0)
        && binary_supports(
            &crate::workstation::home().join("scripts/tend-worker"),
            "worker",
            unsafe { libc::geteuid() },
        )
}
pub fn apt_root() -> Option<PathBuf> {
    let text = probe(
        "/usr/bin/apt-config",
        &["shell", "TEND_ARCHIVES", "Dir::Cache::archives/d"],
    )?;
    let args = gtk::glib::shell_parse_argv(text.trim()).ok()?;
    if args.len() != 1 {
        return None;
    }
    let word = args[0].to_str()?.strip_prefix("TEND_ARCHIVES=")?;
    let path = Path::new(word);
    if !path.is_absolute() {
        return None;
    }
    resolve_missing(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    fn fixture() -> PathBuf {
        let p = PathBuf::from("/tmp/opencode").join(format!(
            "tend-exclusions-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&p).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
        p
    }
    #[test]
    fn paths_use_components_and_resolve_aliases_and_missing_descendants() {
        let root = fixture();
        fs::create_dir(root.join("protected")).unwrap();
        symlink(root.join("protected"), root.join("alias")).unwrap();
        let policy = Policy {
            paths: vec![Entry {
                path: root.join("alias/new/subfolder"),
                enabled: true,
                note: String::new(),
            }],
            ..Default::default()
        };
        let c = policy.compile().unwrap();
        assert!(c.protects(&root.join("protected/new/subfolder/file")));
        assert!(!c.protects(&root.join("protected/new/subfolder-other/file")));
        assert!(c.overlaps(&root.join("protected")));
        assert!(c.bulk_reason("docker", None).is_some());
        assert!(c
            .bulk_reason("pip", Some(&root.join("unrelated")))
            .is_none());
    }
    #[test]
    fn shared_readers_block_changes_and_bad_policies_fail_closed() {
        let root = fixture();
        let config = root.join("policy.json");
        fs::write(&config, serde_json::to_vec(&Policy::default()).unwrap()).unwrap();
        fs::set_permissions(&config, fs::Permissions::from_mode(0o644)).unwrap();
        let lock = root.join("policy.lock");
        fs::write(&lock, "").unwrap();
        fs::set_permissions(&lock, fs::Permissions::from_mode(0o644)).unwrap();
        let uid = unsafe { libc::geteuid() };
        let a = Guard::at(&config, &lock, uid, false).unwrap();
        let b = Guard::at(&config, &lock, uid, false).unwrap();
        assert!(Guard::at(&config, &lock, uid, true).is_err());
        drop(a);
        drop(b);
        assert!(Guard::at(&config, &lock, uid, true).is_ok());
        fs::write(&config, "{broken").unwrap();
        assert!(Guard::at(&config, &lock, uid, false).is_err());
        assert_eq!(fs::read_to_string(&config).unwrap(), "{broken");
        let alias = root.join("alias");
        symlink(&config, &alias).unwrap();
        assert!(Guard::at(&alias, &lock, uid, false).is_err());
    }
    #[test]
    fn strict_schema_and_path_validation() {
        assert!(serde_json::from_str::<Policy>(r#"{"restart":true}"#).is_err());
        for p in ["relative", "/a/../b"] {
            assert!(Policy {
                paths: vec![Entry {
                    path: p.into(),
                    enabled: true,
                    note: String::new()
                }],
                ..Default::default()
            }
            .validate()
            .is_err());
        }
    }
    #[test]
    fn shared_schema_matches_maintenance_adapter() {
        let cases: serde_json::Value =
            serde_json::from_str(include_str!("../tests/exclusion-cases.json")).unwrap();
        for case in cases.as_array().unwrap() {
            let valid = serde_json::from_value::<Policy>(case["policy"].clone())
                .is_ok_and(|p| p.validate().is_ok());
            assert_eq!(valid, case["valid"].as_bool().unwrap(), "{}", case["name"]);
        }
    }
    #[test]
    fn writer_can_repair_an_exclusion_that_no_longer_resolves() {
        let root=fixture();let config=root.join("policy.json");let lock=root.join("policy.lock");
        let loop_path=root.join("loop");symlink(&loop_path,&loop_path).unwrap();
        let policy=Policy {paths:vec![Entry {path:loop_path,enabled:true,note:String::new()}],..Default::default()};
        fs::write(&config,serde_json::to_vec(&policy).unwrap()).unwrap();fs::set_permissions(&config,fs::Permissions::from_mode(0o644)).unwrap();
        fs::write(&lock,"").unwrap();fs::set_permissions(&lock,fs::Permissions::from_mode(0o644)).unwrap();
        let uid=unsafe {libc::geteuid()};
        assert!(Guard::at(&config,&lock,uid,false).is_err());
        assert_eq!(Guard::at(&config,&lock,uid,true).unwrap().compiled.policy,policy);
    }
    #[test]
    fn missing_files_and_unsafe_parent_do_not_remove_protection() {
        let root = fixture();
        let config = root.join("policy.json");
        let lock = root.join("policy.lock");
        let uid = unsafe { libc::geteuid() };
        assert!(Guard::at(&config, &lock, uid, false).is_ok());
        fs::write(&lock, "").unwrap();
        assert!(Guard::at(&config, &lock, uid, false).is_err());
        fs::write(&config, "{}").unwrap();
        fs::set_permissions(&config, fs::Permissions::from_mode(0o644)).unwrap();
        fs::rename(&lock, root.join("retained-lock")).unwrap();
        assert!(Guard::at(&config, &lock, uid, false).is_err());
        fs::write(&lock, "").unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o775)).unwrap();
        assert!(Guard::at(&config, &lock, uid, false).is_err());
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    }
    #[test]
    fn capability_queries_are_bounded_and_require_explicit_support() {
        assert_eq!(
            probe("/usr/bin/printf", &["%s", "$(literal)"]),
            Some("$(literal)".into())
        );
        assert!(probe("/usr/bin/head", &["-c", "9000", "/dev/zero"]).is_none());
        let started = Instant::now();
        assert!(probe("/usr/bin/sleep", &["5"]).is_none());
        assert!(started.elapsed() < Duration::from_secs(4));
        assert!(capabilities_match(
            &crate::capabilities::json("worker"),
            "worker"
        ));
        assert!(!capabilities_match(
            &crate::capabilities::json("administrator"),
            "worker"
        ));
        assert!(!capabilities_match(
            r#"{"protocol":2,"role":"worker","cleanup_exclusions":true,"explained_cleanup":true}"#,
            "worker"
        ));
        assert!(!capabilities_match(
            r#"{"protocol":3,"role":"worker","cleanup_exclusions":true}"#,
            "worker"
        ));
    }
}
