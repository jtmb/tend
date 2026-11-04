//! Desktop identity and ancestry grouping. Never reads arguments/environments
//! or executes desktop launch commands. Unknown processes retain their identity.
use crate::{observations::ProcessIdentity, task_stats::Task, workstation};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock, RwLock},
};

#[derive(Clone, Debug)]
pub struct DesktopApplication {
    pub id: String,
    pub name: String,
    pub executable: String,
}
#[derive(Default)]
pub struct Catalog {
    applications: HashMap<String, DesktopApplication>,
}
impl Catalog {
    pub fn from_applications(entries: impl IntoIterator<Item = DesktopApplication>) -> Self {
        let mut applications = HashMap::new();
        for mut app in entries.into_iter().take(4096) {
            let key = Path::new(&app.executable)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            if key.len() > 1024 || key.is_empty() {
                continue;
            }
            app.name = app
                .name
                .chars()
                .filter(|c| !c.is_control())
                .take(160)
                .collect();
            if app.name.is_empty() {
                app.name = key.clone();
            }
            app.executable = key.clone();
            if ![
                "env", "sh", "bash", "zsh", "fish", "python", "python3", "node", "npm", "java",
                "electron", "ruby", "flatpak", "snap", "true",
            ]
            .contains(&key.as_str())
            {
                applications.entry(key).or_insert(app);
            }
        }
        Self { applications }
    }
    pub fn load() -> Self {
        let mut sources = BTreeMap::new();
        for dir in [
            PathBuf::from("/usr/share/applications"),
            PathBuf::from("/usr/local/share/applications"),
            workstation::home().join(".local/share/applications"),
        ] {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten().take(5000) {
                    if entry.path().extension().and_then(|s| s.to_str()) == Some("desktop") {
                        let id = entry.file_name();
                        if id.to_str().is_none_or(|s| s.chars().any(char::is_control)) {
                            continue;
                        }
                        if sources.len() < 4096 || sources.contains_key(&id) {
                            sources.insert(id, entry.path());
                        }
                    }
                }
            }
        }
        let mut apps = Vec::new();
        for (id, path) in sources {
            let Ok(file) = fs::File::open(path) else {
                continue;
            };
            let mut text = String::new();
            if file.take(64 * 1024).read_to_string(&mut text).is_err() {
                continue;
            }
            let mut main = false;
            let mut values = HashMap::new();
            for line in text.lines() {
                if line.starts_with('[') {
                    main = line == "[Desktop Entry]";
                } else if main {
                    if let Some((key, value)) = line.split_once('=') {
                        values.entry(key).or_insert(value);
                    }
                }
            }
            if values.get("Hidden") == Some(&"true")
                || values.get("Type").is_some_and(|v| *v != "Application")
            {
                continue;
            }
            let Some(exec) = values.get("Exec") else {
                continue;
            };
            let Ok(args) = gtk::glib::shell_parse_argv(exec) else {
                continue;
            };
            let Some(program) = args.first() else {
                continue;
            };
            let program = program.to_string_lossy();
            let executable = Path::new(program.as_ref())
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            apps.push(DesktopApplication {
                id: id.to_string_lossy().into_owned(),
                name: values
                    .get("Name")
                    .copied()
                    .unwrap_or(program.as_ref())
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(160)
                    .collect(),
                executable,
            });
        }
        Self::from_applications(apps)
    }
    pub fn identify(&self, task: &Task) -> Option<&DesktopApplication> {
        let key = Path::new(&task.executable)
            .file_name()?
            .to_str()?
            .to_lowercase();
        self.applications.get(&key)
    }
}
fn catalog_lock() -> &'static RwLock<Arc<Catalog>> {
    static CATALOG: OnceLock<RwLock<Arc<Catalog>>> = OnceLock::new();
    CATALOG.get_or_init(|| RwLock::new(Arc::new(Catalog::load())))
}
pub fn catalog() -> Arc<Catalog> {
    catalog_lock().read().unwrap().clone()
}
pub fn refresh_catalog() {
    *catalog_lock().write().unwrap() = Arc::new(Catalog::load());
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApplicationGroup {
    pub id: String,
    pub name: String,
    pub desktop_id: Option<String>,
    pub uid: u32,
    pub members: Vec<ProcessIdentity>,
    pub memory_bytes: u64,
    pub cpu_percent: Option<f64>,
    pub read_bytes_per_second: Option<f64>,
    pub write_bytes_per_second: Option<f64>,
}
fn parent<'a>(task: &Task, tasks: &HashMap<u32, &'a Task>) -> Option<&'a Task> {
    tasks
        .get(&task.parent_pid)
        .copied()
        .filter(|p| p.pid != task.pid && p.uid == task.uid && p.start <= task.start)
}
fn boundary(task: &Task) -> bool {
    [
        "systemd",
        "gnome-shell",
        "gnome-session",
        "dbus-daemon",
        "dbus-broker",
        "gdm-",
        "bash",
        "zsh",
        "fish",
        "sh",
        "login",
        "sshd",
    ]
    .iter()
    .any(|n| {
        task.name == *n || ((*n == "gnome-session" || *n == "gdm-") && task.name.starts_with(n))
    })
}
pub fn applications(tasks: &[Task], boot: &str, catalog: &Catalog) -> Vec<ApplicationGroup> {
    let map: HashMap<_, _> = tasks.iter().map(|t| (t.pid, t)).collect();
    let mut groups: BTreeMap<String, (String, Option<String>, u32, Vec<&Task>)> = BTreeMap::new();
    for original in tasks {
        let mut root = original;
        let mut seen = HashSet::new();
        let mut desktop = None;
        for _ in 0..128 {
            if !seen.insert(root.pid) {
                root = original;
                break;
            }
            if let Some(app) = catalog.identify(root) {
                desktop = Some(app);
                break;
            }
            let Some(next) = parent(root, &map) else {
                break;
            };
            if boundary(root) || boundary(next) {
                break;
            }
            root = next;
        }
        let (id, name, desktop_id) = if let Some(app) = desktop {
            (
                format!("desktop:{}:{}", original.uid, app.id),
                app.name.clone(),
                Some(app.id.clone()),
            )
        } else {
            (
                format!(
                    "process:{boot}:{}:{}:{}",
                    original.uid, root.pid, root.start
                ),
                root.name.clone(),
                None,
            )
        };
        groups
            .entry(id)
            .or_insert_with(|| (name, desktop_id, original.uid, vec![]))
            .3
            .push(original);
    }
    let mut result: Vec<_> = groups
        .into_iter()
        .map(|(id, (name, desktop_id, uid, members))| {
            let total = |value: fn(&Task) -> Option<f64>| {
                members
                    .iter()
                    .try_fold(0., |sum, t| value(t).map(|v| sum + v))
            };
            ApplicationGroup {
                id,
                name,
                desktop_id,
                uid,
                memory_bytes: members
                    .iter()
                    .fold(0u64, |sum, t| sum.saturating_add(t.memory)),
                cpu_percent: total(|t| t.cpu),
                read_bytes_per_second: total(|t| t.read_rate),
                write_bytes_per_second: total(|t| t.write_rate),
                members: members
                    .iter()
                    .map(|t| ProcessIdentity {
                        boot_id: boot.into(),
                        pid: t.pid,
                        start_ticks: t.start,
                    })
                    .collect(),
            }
        })
        .collect();
    result.sort_by(|a, b| {
        b.memory_bytes
            .cmp(&a.memory_bytes)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.id.cmp(&b.id))
    });
    result
}
#[derive(Clone, Debug)]
pub struct ProcessNode {
    pub index: usize,
    pub children: Vec<ProcessNode>,
}
pub fn forest(tasks: &[Task]) -> Vec<ProcessNode> {
    let by_pid: HashMap<_, _> = tasks.iter().map(|t| (t.pid, t)).collect();
    let mut children: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    let mut roots = Vec::new();
    for (index, task) in tasks.iter().enumerate() {
        if let Some(p) = parent(task, &by_pid) {
            children.entry(p.pid).or_default().push(index);
        } else {
            roots.push(index)
        }
    }
    fn node(
        index: usize,
        tasks: &[Task],
        children: &BTreeMap<u32, Vec<usize>>,
        seen: &mut HashSet<usize>,
        depth: usize,
    ) -> Option<ProcessNode> {
        if depth >= 128 || !seen.insert(index) {
            return None;
        }
        Some(ProcessNode {
            index,
            children: children
                .get(&tasks[index].pid)
                .into_iter()
                .flatten()
                .filter_map(|i| node(*i, tasks, children, seen, depth + 1))
                .collect(),
        })
    }
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for index in roots.into_iter().chain(0..tasks.len()) {
        if let Some(n) = node(index, tasks, &children, &mut seen, 0) {
            result.push(n)
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task(pid: u32, parent_pid: u32, start: u64, name: &str, exe: &str) -> Task {
        Task {
            pid,
            parent_pid,
            start,
            name: name.into(),
            executable: exe.into(),
            uid: 1000,
            memory: 100,
            cpu: Some(1.),
            ..Default::default()
        }
    }
    #[test]
    fn app_children_group_and_unrelated_interpreters_stay_separate() {
        let cat = Catalog::from_applications([DesktopApplication {
            id: "editor.desktop".into(),
            name: "Editor".into(),
            executable: "/opt/editor".into(),
        }]);
        let tasks = vec![
            task(1, 0, 1, "systemd", "/usr/bin/systemd"),
            task(10, 1, 10, "editor", "/opt/editor"),
            task(11, 10, 11, "renderer", "/opt/renderer"),
            task(20, 1, 20, "python", "/usr/bin/python3"),
            task(30, 1, 30, "python", "/usr/bin/python3"),
        ];
        let groups = applications(&tasks, "boot", &cat);
        let editor = groups.iter().find(|g| g.desktop_id.is_some()).unwrap();
        assert_eq!(editor.name, "Editor");
        assert_eq!(editor.members.len(), 2);
        assert_eq!(editor.cpu_percent, Some(2.));
        assert_eq!(groups.iter().filter(|g| g.name == "python").count(), 2);
    }
    #[test]
    fn reused_parent_and_cycles_do_not_lose_or_duplicate_processes() {
        let tasks = vec![
            task(1, 2, 10, "a", "a"),
            task(2, 1, 10, "b", "b"),
            task(3, 4, 20, "orphan", "orphan"),
            task(4, 0, 30, "new-parent", "new-parent"),
        ];
        let tree = forest(&tasks);
        fn count(nodes: &[ProcessNode]) -> usize {
            nodes.iter().map(|n| 1 + count(&n.children)).sum()
        }
        assert_eq!(count(&tree), tasks.len());
        assert!(tree.iter().any(|n| tasks[n.index].pid == 3));
        let groups = applications(&tasks, "boot", &Catalog::default());
        let identities: HashSet<_> = groups.iter().flat_map(|g| g.members.iter()).collect();
        assert_eq!(identities.len(), 4);
    }
}
