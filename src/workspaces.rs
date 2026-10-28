//! Registered project roots, conservative process evidence and on-demand Docker
//! labels. Never runs project code or reads environments/launch arguments.
use crate::{observations::ProcessIdentity, task_stats::Task, workstation as w};
use serde::{Deserialize, Serialize};
use std::os::unix::process::CommandExt;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::Read,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub root: PathBuf,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub version: u32,
    pub projects: Vec<Workspace>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            projects: vec![],
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.projects.len() > 50 {
            return Err(
                "Workspace version must be 1; at most 50 projects can be registered.".into(),
            );
        }
        let mut ids = HashSet::new();
        let mut paths = HashSet::new();
        for p in &self.projects {
            if p.id.is_empty()
                || p.id.len() > 100
                || !p.id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                || !ids.insert(&p.id)
                || p.name.trim().is_empty()
                || p.name.chars().count() > 80
                || p.name.chars().any(char::is_control)
                || p.root
                    .to_str()
                    .is_none_or(|s| s.len() > 4096 || s.chars().any(char::is_control))
                || !p.root.is_absolute()
                || p.root == Path::new("/")
                || p.root
                    .components()
                    .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
                || !paths.insert(&p.root)
            {
                return Err("Use unique project folders and IDs, an absolute folder path and a name of 1–80 characters.".into());
            }
        }
        Ok(())
    }
    pub fn read() -> Result<Self, String> {
        let settings:Self=match crate::config_io::read(&w::home().join(".config/tend/workspaces.json"),256*1024,Some(unsafe{libc::geteuid()}),false)? {
            Some(bytes)=>serde_json::from_slice(&bytes).map_err(|e|format!("Invalid workspace settings: {e}"))?,None=>Self::default()
        };
        settings.validate()?;
        Ok(settings)
    }
    pub fn save(&self) -> Result<(), String> {self.save_checked(None)}
    pub fn save_checked(&self,expected:Option<&Self>)->Result<(),String>{
        self.validate()?;let _guard=crate::config_io::lock(&w::home().join(".config/tend/workspaces.json"))?;
        if let Some(expected)=expected{if Self::read()?!=*expected{return Err("Saved settings changed. Review again before saving.".into());}}

        self.validate()?;
        w::private_write(
            &w::home().join(".config/tend/workspaces.json"),
            &serde_json::to_string_pretty(self).map_err(|e| e.to_string())?,
        )
    }
    pub fn add(&mut self, name: &str, root: &Path) -> Result<String, String> {
        let root = root
            .canonicalize()
            .map_err(|e| format!("Project folder unavailable: {e}"))?;
        if !root.is_dir() {
            return Err("Choose a project directory.".into());
        }
        let id = format!(
            "workspace-{:x}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            std::process::id()
        );
        let project = Workspace {
            id: id.clone(),
            name: name.trim().into(),
            root,
        };
        let mut draft = self.clone();
        draft.projects.push(project);
        draft.validate()?;
        *self = draft;
        Ok(id)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Association {
    pub process: ProcessIdentity,
    pub name: String,
    pub workspace_id: String,
    pub evidence: String,
    pub memory: u64,
    pub cpu: Option<f64>,
}
fn matched<'a>(path: &Path, settings: &'a Settings) -> Option<&'a Workspace> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return None;
    }
    settings
        .projects
        .iter()
        .filter(|p| path.starts_with(&p.root))
        .max_by_key(|p| p.root.components().count())
}
pub fn processes(tasks: &[Task], boot: &str, settings: &Settings) -> Vec<Association> {
    let uid = unsafe { libc::geteuid() };
    let map: HashMap<_, _> = tasks.iter().map(|t| (t.pid, t)).collect();
    let mut result = Vec::new();
    for original in tasks.iter().filter(|t| t.uid == uid) {
        let mut current = original;
        let mut seen = HashSet::new();
        for depth in 0..128 {
            if !seen.insert(current.pid) {
                break;
            }
            if let Some(project) = current
                .working_directory
                .as_deref()
                .and_then(|p| matched(p, settings))
            {
                result.push(Association {
                    process: ProcessIdentity {
                        boot_id: boot.into(),
                        pid: original.pid,
                        start_ticks: original.start,
                    },
                    name: original.name.clone(),
                    workspace_id: project.id.clone(),
                    evidence: if depth == 0 {
                        "Working directory matches".into()
                    } else {
                        format!(
                            "Ancestor PID {} has a matching working directory",
                            current.pid
                        )
                    },
                    memory: original.memory,
                    cpu: original.cpu,
                });
                break;
            }
            // An accessible cwd outside every registered project is stronger
            // evidence than ancestry. Do not assign a different project by guess.
            if current.working_directory.is_some() {
                break;
            }
            let Some(parent) = map
                .get(&current.parent_pid)
                .filter(|p| p.uid == uid && p.pid != current.pid && p.start <= current.start)
            else {
                break;
            };
            current = parent;
        }
    }
    result
}
#[derive(Clone, Debug, Default)]
pub struct Totals {
    pub processes: u32,
    pub memory: u64,
    pub cpu: Option<f64>,
}
pub fn totals(associations: &[Association]) -> BTreeMap<String, Totals> {
    let mut result = BTreeMap::<String, Totals>::new();
    for p in associations {
        let total = result.entry(p.workspace_id.clone()).or_insert(Totals {
            cpu: Some(0.),
            ..Default::default()
        });
        total.processes += 1;
        total.memory = total.memory.saturating_add(p.memory);
        total.cpu = total.cpu.zip(p.cpu).map(|(a, b)| (a + b).clamp(0., 100.));
    }
    result
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub state: String,
    pub compose_directory: Option<String>,
    pub compose_project: Option<String>,
    pub compose_service: Option<String>,
    #[serde(default)]
    pub workspace_id: Option<String>,
}
// Fixed executable, bounded output and lifetime; only newly spawned query
// children are terminated on timeout. No existing apps/containers are touched.
fn docker(args: &[&str], timeout: Duration) -> Result<String, String> {
    let mut child = Command::new("/usr/bin/docker")
        .args(args)
        .process_group(0)
        .env("SSH_ASKPASS_REQUIRE", "never")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Docker unavailable: {e}"))?;
    let out = child.stdout.take().unwrap();
    let err = child.stderr.take().unwrap();
    let read = |reader: Box<dyn Read + Send>, limit: u64| {
        std::thread::spawn(move || {
            let mut value = Vec::new();
            reader
                .take(limit + 1)
                .read_to_end(&mut value)
                .map(|_| value)
        })
    };
    let stdout = read(Box::new(out), 2 * 1024 * 1024);
    let stderr = read(Box::new(err), 64 * 1024);
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if started.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(20))
            }
            Ok(None) => {
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                let _ = child.wait();
                break Err("Docker query timed out.".to_string());
            }
            Err(e) => {
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                let _ = child.wait();
                break Err(e.to_string());
            }
        }
    };
    let out = stdout
        .join()
        .map_err(|_| "Docker output reader stopped")?
        .map_err(|e| e.to_string())?;
    let err = stderr
        .join()
        .map_err(|_| "Docker error reader stopped")?
        .map_err(|e| e.to_string())?;
    if out.len() > 2 * 1024 * 1024 || err.len() > 64 * 1024 {
        return Err("Docker output exceeded the bounded query size.".into());
    }
    if !status?.success() {
        return Err(format!(
            "Docker query failed: {}",
            String::from_utf8_lossy(&err)
        ));
    }
    String::from_utf8(out).map_err(|_| "Docker returned invalid text.".into())
}
fn decode_containers(output: &str, settings: &Settings) -> Result<Vec<Container>, String> {
    let mut result = Vec::new();
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        if result.len() >= 500 {
            return Err("Container metadata exceeds 500 records.".into());
        }
        let mut container: Container =
            serde_json::from_str(line).map_err(|e| format!("Invalid Docker metadata: {e}"))?;
        if container.id.len() != 64
            || !container.id.bytes().all(|b| b.is_ascii_hexdigit())
            || container.name.len() > 512
            || container.state.len() > 80
            || [
                &container.compose_directory,
                &container.compose_project,
                &container.compose_service,
            ]
            .iter()
            .any(|value| value.as_ref().is_some_and(|s| s.len() > 4096))
        {
            return Err("Docker metadata exceeds the bounded field sizes.".into());
        }
        container.workspace_id = container
            .compose_directory
            .as_deref()
            .map(Path::new)
            .filter(|p| p.is_absolute())
            .and_then(|p| matched(p, settings))
            .map(|p| p.id.clone());
        result.push(container);
    }
    Ok(result)
}
pub fn containers(settings: &Settings) -> Result<Vec<Container>, String> {
    #[cfg(feature = "docs-demo")]
    if std::env::var_os("TEND_DEMO_DIR").is_some() {
        let file = std::fs::File::open(w::home().join("demo-workspaces-docker.jsonl"))
            .map_err(|_| "The isolated Docker label fixture is unavailable.".to_string())?;
        let mut text = String::new();
        file.take(2 * 1024 * 1024 + 1)
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
        if text.len() > 2 * 1024 * 1024 {
            return Err("Docker fixture exceeds the query budget.".into());
        }
        return decode_containers(&text, settings);
    }
    let started = Instant::now();
    let endpoint = if std::env::var_os("DOCKER_CONTEXT").is_some() {
        docker(
            &[
                "context",
                "inspect",
                "--format",
                "{{.Endpoints.docker.Host}}",
            ],
            Duration::from_secs(3),
        )?
    } else {
        match std::env::var("DOCKER_HOST") {
            Ok(value) if !value.is_empty() => value,
            _ => docker(
                &[
                    "context",
                    "inspect",
                    "--format",
                    "{{.Endpoints.docker.Host}}",
                ],
                Duration::from_secs(3),
            )?,
        }
    };
    if !endpoint.trim().starts_with("unix://") {
        return Err("Workspace container matching requires a local Docker Unix socket. Remote Compose paths cannot be verified against this computer.".into());
    }
    let ids = docker(
        &["ps", "--all", "--no-trunc", "--format", "{{.ID}}"],
        Duration::from_secs(5),
    )?;
    let ids: Vec<_> = ids.lines().filter(|s| !s.trim().is_empty()).collect();
    if ids.len() > 500
        || ids
            .iter()
            .any(|s| s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err(
            "Docker listing exceeds 500 containers or contains an invalid identifier.".into(),
        );
    }
    let format = r#"{"id":{{json .Id}},"name":{{json .Name}},"state":{{json .State.Status}},"compose_directory":{{json (index .Config.Labels "com.docker.compose.project.working_dir")}},"compose_project":{{json (index .Config.Labels "com.docker.compose.project")}},"compose_service":{{json (index .Config.Labels "com.docker.compose.service")}}}"#;
    let mut result = Vec::new();
    for batch in ids.chunks(50) {
        let remaining = Duration::from_secs(20)
            .checked_sub(started.elapsed())
            .ok_or("Docker inspection exceeded twenty seconds.")?;
        let mut args = vec!["inspect", "--format", format];
        args.extend_from_slice(batch);
        let output = docker(&args, remaining.min(Duration::from_secs(5)))?;
        result.extend(decode_containers(&output, settings)?);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn settings() -> Settings {
        Settings {
            projects: vec![
                Workspace {
                    id: "a".into(),
                    name: "A".into(),
                    root: "/projects/a".into(),
                },
                Workspace {
                    id: "nested".into(),
                    name: "Nested".into(),
                    root: "/projects/a/nested".into(),
                },
            ],
            ..Default::default()
        }
    }
    fn task(pid: u32, parent_pid: u32, start: u64, cwd: Option<&str>) -> Task {
        Task {
            pid,
            parent_pid,
            start,
            uid: unsafe { libc::geteuid() },
            working_directory: cwd.map(PathBuf::from),
            memory: 100,
            cpu: Some(2.),
            ..Default::default()
        }
    }
    #[test]
    fn longest_root_and_direct_cwd_evidence_take_precedence() {
        let tasks = vec![
            task(1, 0, 1, Some("/projects/a")),
            task(2, 1, 2, Some("/projects/a/nested")),
            task(3, 1, 3, Some("/projects/ab")),
            task(4, 1, 4, None),
        ];
        let result = processes(&tasks, "boot", &settings());
        assert_eq!(result.len(), 3);
        assert_eq!(
            result
                .iter()
                .find(|p| p.process.pid == 2)
                .unwrap()
                .workspace_id,
            "nested"
        );
        assert!(!result.iter().any(|p| p.process.pid == 3));
        assert!(result
            .iter()
            .find(|p| p.process.pid == 4)
            .unwrap()
            .evidence
            .starts_with("Ancestor"));
        assert_eq!(totals(&result)["a"].processes, 2);
    }
    #[test]
    fn reused_pids_cycles_and_other_users_never_inherit_a_workspace() {
        let mut other = task(8, 1, 8, Some("/projects/a"));
        other.uid = unsafe { libc::geteuid() } + 1;
        let tasks = vec![
            task(1, 0, 10, Some("/projects/a")),
            task(2, 1, 5, None),
            task(3, 4, 3, None),
            task(4, 3, 3, None),
            other,
        ];
        let result = processes(&tasks, "boot", &settings());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].process.pid, 1);
    }
    #[test]
    fn containers_use_directory_labels_without_guessing_from_project_names() {
        let make = |directory: Option<&str>| {
            serde_json::json!({"id":"a".repeat(64),"name":"api","state":"running","compose_directory":directory,"compose_project":"a","compose_service":"api"}).to_string()
        };
        let values = decode_containers(
            &[
                make(Some("/projects/a/nested")),
                make(Some("/projects/ab")),
                make(None),
                make(Some("/projects/a/../elsewhere")),
            ]
            .join("\n"),
            &settings(),
        )
        .unwrap();
        assert_eq!(values[0].workspace_id.as_deref(), Some("nested"));
        assert!(values[1..].iter().all(|c| c.workspace_id.is_none()));
        assert!(decode_containers("bad json", &settings()).is_err());
    }
    #[test]
    fn duplicate_and_broad_roots_are_rejected_without_applying_a_draft() {
        let mut s = settings();
        s.validate().unwrap();
        s.projects[1].root = s.projects[0].root.clone();
        assert!(s.validate().is_err());
        s.projects[1].root = "/".into();
        assert!(s.validate().is_err());
        assert!(serde_json::from_str::<Settings>(r#"{"restart":true}"#).is_err());
    }
}
