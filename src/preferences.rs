use crate::{
    output,
    workstation::{home, private_write, record, state_dir},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
#[derive(Clone, Serialize, Deserialize)]
pub struct Undo {
    pub label: String,
    pub path: Option<PathBuf>,
    pub content: Option<String>,
    pub schema: Option<String>,
    pub key: Option<String>,
    pub value: Option<String>,
    pub unit: Option<String>,
}
fn remember(u: Undo) -> Result<(), String> {
    let path = state_dir().join("undo.json");
    let mut values: Vec<Undo> = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    values.push(u);
    private_write(&path, &serde_json::to_string_pretty(&values).unwrap())
}
pub fn undos() -> Vec<Undo> {
    fs::read_to_string(state_dir().join("undo.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}
fn backup(path: &Path, label: &str) -> Result<(), String> {
    remember(Undo {
        label: label.into(),
        path: Some(path.into()),
        content: match fs::read_to_string(path) {
            Ok(s) => Some(s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.to_string()),
        },
        schema: None,
        key: None,
        value: None,
        unit: None,
    })
}
pub fn restore(index: usize) -> Result<String, String> {
    let mut entries = undos();
    let u = entries
        .get(index)
        .ok_or("History changed; refresh first")?
        .clone();
    if let Some(p) = &u.path {
        let config = home().join(".config/autostart");
        let apps = home().join(".local/share/applications");
        if !(p.parent() == Some(config.as_path()) || p.parent() == Some(apps.as_path()))
            || p.extension().and_then(|s| s.to_str()) != Some("desktop")
        {
            return Err("Unsupported restore destination".into());
        }
        if let Some(s) = &u.content {
            private_write(p, s)?;
        } else if p.exists() {
            fs::remove_file(p).map_err(|e| e.to_string())?;
        }
    }
    if let (Some(schema), Some(key), Some(value)) = (&u.schema, &u.key, &u.value) {
        if schema != "org.gnome.desktop.interface" || key != "enable-animations" {
            return Err("Unsupported preference".into());
        }
        output("/usr/bin/gsettings", &["set", schema, key, value])?;
    }
    if let Some(unit) = &u.unit {
        validate_unit(unit)?;
        output(
            "/usr/bin/systemctl",
            &[
                "--user",
                u.value
                    .as_deref()
                    .filter(|v| ["enable", "disable"].contains(v))
                    .ok_or("Invalid service restoration")?,
                unit,
            ],
        )?;
    }
    entries.remove(index);
    private_write(
        &state_dir().join("undo.json"),
        &serde_json::to_string_pretty(&entries).unwrap(),
    )?;
    record("undo", &u.label)?;
    Ok(format!("Restored {}", u.label))
}
pub fn animations(enabled: bool) -> Result<String, String> {
    let schema = "org.gnome.desktop.interface";
    let key = "enable-animations";
    let before = output("/usr/bin/gsettings", &["get", schema, key])?;
    remember(Undo {
        label: "Desktop animations".into(),
        path: None,
        content: None,
        schema: Some(schema.into()),
        key: Some(key.into()),
        value: Some(before),
        unit: None,
    })?;
    output(
        "/usr/bin/gsettings",
        &["set", schema, key, if enabled { "true" } else { "false" }],
    )?;
    record("settings", &format!("Desktop animations: {enabled}"))?;
    Ok("Applied. Previous value is available in Recovery.".into())
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Startup {
    pub id: String,
    pub title: String,
    pub source: PathBuf,
    pub enabled: bool,
}
fn desktop_value(s: &str, key: &str) -> Option<String> {
    let mut main = false;
    for line in s.lines() {
        if line.starts_with('[') {
            main = line == "[Desktop Entry]";
        } else if main {
            if let Some(v) = line.strip_prefix(&format!("{key}=")) {
                return Some(v.into());
            }
        }
    }
    None
}
fn replace_key(s: &str, key: &str, value: &str) -> String {
    let mut lines = vec![];
    let mut main = false;
    let mut written = false;
    for line in s.lines() {
        if line.starts_with('[') {
            if main && !written {
                lines.push(format!("{key}={value}"));
                written = true;
            }
            main = line == "[Desktop Entry]";
        }
        if main && line.starts_with(&format!("{key}=")) {
            if !written {
                lines.push(format!("{key}={value}"));
                written = true;
            }
        } else {
            lines.push(line.into());
        }
    }
    if main && !written {
        lines.push(format!("{key}={value}"));
    }
    lines.join("\n") + "\n"
}
pub fn startups() -> Vec<Startup> {
    let mut entries = BTreeMap::new();
    for dir in [
        PathBuf::from("/etc/xdg/autostart"),
        home().join(".config/autostart"),
    ] {
        if let Ok(es) = fs::read_dir(dir) {
            for e in es.flatten() {
                let p = e.path();
                if p.extension().and_then(|s| s.to_str()) != Some("desktop") {
                    continue;
                }
                if let Ok(s) = fs::read_to_string(&p) {
                    let id = e.file_name().to_string_lossy().to_string();
                    entries.insert(
                        id.clone(),
                        Startup {
                            id: id.clone(),
                            title: desktop_value(&s, "Name").unwrap_or(id),
                            source: p,
                            enabled: desktop_value(&s, "Hidden").as_deref() != Some("true")
                                && desktop_value(&s, "X-GNOME-Autostart-enabled").as_deref()
                                    != Some("false"),
                        },
                    );
                }
            }
        }
    }
    entries.into_values().collect()
}
pub fn set_startup(id: &str, enabled: bool) -> Result<String, String> {
    let entry = startups()
        .into_iter()
        .find(|e| e.id == id)
        .ok_or("Startup entry disappeared")?;
    let dest = home().join(".config/autostart").join(id);
    let s = fs::read_to_string(&entry.source).map_err(|e| e.to_string())?;
    backup(&dest, &format!("Startup: {}", entry.title))?;
    let s = replace_key(
        &replace_key(&s, "Hidden", if enabled { "false" } else { "true" }),
        "X-GNOME-Autostart-enabled",
        if enabled { "true" } else { "false" },
    );
    private_write(&dest, &s)?;
    record("startup", &format!("{} enabled={enabled}", entry.title))?;
    Ok("Saved for your next login. Existing applications remain running.".into())
}
fn validate_unit(unit: &str) -> Result<(), String> {
    if unit.contains('/')
        || !(unit.ends_with(".service") || unit.ends_with(".timer"))
        || !unit
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-@".contains(&b))
    {
        return Err("Invalid user unit".into());
    }
    if [
        "dev-maintenance",
        "tend",
        "gnome",
        "dbus",
        "pipewire",
        "wireplumber",
        "xdg",
    ]
    .iter()
    .any(|s| unit.contains(s))
    {
        return Err("Manage this service through its dedicated settings.".into());
    }
    let p = home().join(".config/systemd/user").join(unit);
    let m = fs::symlink_metadata(p).map_err(|e| e.to_string())?;
    if !m.is_file() || m.file_type().is_symlink() {
        return Err("Only locally installed, regular user units can be changed.".into());
    }
    Ok(())
}
pub fn user_units() -> Vec<(String, bool)> {
    let dir = home().join(".config/systemd/user");
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let id = e.file_name().to_str()?.to_owned();
            validate_unit(&id).ok()?;
            let enabled = output("/usr/bin/systemctl", &["--user", "is-enabled", &id])
                .map(|s| s == "enabled")
                .unwrap_or(false);
            Some((id, enabled))
        })
        .collect()
}
pub fn set_unit(unit: &str, enable: bool) -> Result<String, String> {
    validate_unit(unit)?;
    let prior = output("/usr/bin/systemctl", &["--user", "is-enabled", unit])
        .map(|s| s == "enabled")
        .unwrap_or(false);
    remember(Undo {
        label: format!("User service: {unit}"),
        path: None,
        content: None,
        schema: None,
        key: None,
        value: Some(if prior { "enable" } else { "disable" }.into()),
        unit: Some(unit.into()),
    })?;
    output(
        "/usr/bin/systemctl",
        &["--user", if enable { "enable" } else { "disable" }, unit],
    )?;
    record("service", &format!("{unit} enabled={enable}"))?;
    Ok("Future activation changed. No service was started or stopped.".into())
}
pub fn wayland(id: &str) -> Result<String, String> {
    if !["chatgpt.desktop", "code.desktop"].contains(&id) {
        return Err("Unsupported launcher".into());
    }
    let target = home().join(".local/share/applications").join(id);
    let source = [
        target.clone(),
        PathBuf::from("/usr/local/share/applications").join(id),
        PathBuf::from("/usr/share/applications").join(id),
    ]
    .into_iter()
    .find(|p| p.is_file())
    .ok_or("Application launcher is not installed")?;
    let s = fs::read_to_string(source).map_err(|e| e.to_string())?;
    let exec = desktop_value(&s, "Exec").ok_or("Missing launch command")?;
    if exec.contains("--ozone-platform=") {
        return Err("This launcher already specifies a platform. Restore its earlier override before changing it.".into());
    }
    backup(&target, &format!("Wayland launcher: {id}"))?;
    private_write(
        &target,
        &replace_key(&s, "Exec", &format!("{exec} --ozone-platform=wayland")),
    )?;
    record(
        "graphics",
        &format!("{id}: native Wayland requested for next launch"),
    )?;
    Ok("Saved for the next full application launch. Quit/reopen the app when your work is saved. Recovery restores the original launcher.".into())
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemorySettings {
    pub swappiness: u32,
    pub zram_mb: u32,
    pub compression: String,
}
impl MemorySettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.swappiness > 100
            || !(512..=8192).contains(&self.zram_mb)
            || !["lz4", "zstd"].contains(&self.compression.as_str())
        {
            Err("Swappiness: 0–100; zram cap: 512–8192 MiB; compression: lz4 or zstd.".into())
        } else {
            Ok(())
        }
    }
}
