//! Versioned, whitelisted preferences. Importing only writes a private draft;
//! this module never invokes a command, scheduler, cleanup or privileged helper.
use crate::{
    cleanup_rules as rules, config_io, daily_policy, exclusions, health, history, workspaces,
    workstation as w,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};
pub const MAX_BYTES: usize = 1024 * 1024;
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    Appearance,
    Monitoring,
    History,
    Workspaces,
    ManualCleanup,
    DailyCleanup,
    FolderRules,
    UserSchedule,
    Maintenance,
    Exclusions,
}
impl Section {
    pub const ALL: [Self; 10] = [
        Self::Appearance,
        Self::Monitoring,
        Self::History,
        Self::Workspaces,
        Self::ManualCleanup,
        Self::DailyCleanup,
        Self::FolderRules,
        Self::UserSchedule,
        Self::Maintenance,
        Self::Exclusions,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance and window behavior",
            Self::Monitoring => "Health monitoring",
            Self::History => "History limits",
            Self::Workspaces => "Workspaces",
            Self::ManualCleanup => "Manual and cron cleanup policy",
            Self::DailyCleanup => "Daily cleanup policy",
            Self::FolderRules => "Additional cleanup rules",
            Self::UserSchedule => "User cleanup schedule",
            Self::Maintenance => "Maintenance schedule and reminders",
            Self::Exclusions => "Global cleanup exclusions",
        }
    }
    pub fn canonical(self, value: &Value) -> Result<Value, String> {
        fn parsed<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T, String> {
            serde_json::from_value(v.clone()).map_err(|e| e.to_string())
        }
        let result = match self {
            Self::Appearance => {
                let s: Appearance = parsed(value)?;
                s.validate()?;
                json(s)
            }
            Self::Monitoring => {
                let s: health::Settings = parsed(value)?;
                s.validate()?;
                json(s)
            }
            Self::History => {
                let s: history::Settings = parsed(value)?;
                s.validate()?;
                json(s)
            }
            Self::Workspaces => {
                let s: workspaces::Settings = parsed(value)?;
                s.validate()?;
                json(s)
            }
            Self::ManualCleanup => {
                let mut s: w::Cleanup = parsed(value)?;
                s.validate()?;
                s.image_days = Some(s.image_days());
                s.thumbnail_days = Some(s.thumbnail_days());
                json(s)
            }
            Self::DailyCleanup => {
                let s: daily_policy::Policy = parsed(value)?;
                s.validate()?;
                json(s)
            }
            Self::FolderRules => {
                let s: rules::Policy = parsed(value)?;
                validate_rules(&s)?;
                json(s)
            }
            Self::UserSchedule => {
                let s: rules::Schedule = parsed(value)?;
                rules::validate_cron(&s.expression)?;
                json(s)
            }
            Self::Maintenance => {
                let s: Maintenance = parsed(value)?;
                s.validate()?;
                json(s)
            }
            Self::Exclusions => {
                let s: Exclusions = parsed(value)?;
                exclusions::Policy {
                    version: 1,
                    revision: 0,
                    paths: s.paths.clone(),
                }
                .validate()?;
                json(s)
            }
        }?;
        if serde_json::to_vec(&result)
            .map_err(|e| e.to_string())?
            .len()
            > 512 * 1024
        {
            return Err("A settings section exceeds 512 KiB.".into());
        }
        Ok(result)
    }
    pub fn imported(self, value: &Value) -> Result<Value, String> {
        let mut v = self.canonical(value)?;
        match self {
            Self::Monitoring | Self::UserSchedule | Self::Maintenance => {
                v["enabled"] = Value::Bool(false);
            }
            Self::FolderRules => {
                for r in v["rules"].as_array_mut().ok_or("Invalid rule list")? {
                    r["enabled"] = Value::Bool(false);
                }
            }
            _ => {}
        }
        Ok(v)
    }
    pub fn current(self) -> Result<Value, String> {
        let home = w::home();
        let config = home.join(".config/tend");
        let uid = unsafe { libc::geteuid() };
        fn load<T: serde::de::DeserializeOwned + Default + Serialize>(
            p: &Path,
            uid: u32,
            max: usize,
        ) -> Result<Value, String> {
            let value: T = match config_io::read(p, max, Some(uid), false)? {
                Some(bytes) => serde_json::from_slice(&bytes)
                    .map_err(|e| format!("Invalid {}: {e}", p.display()))?,
                None => T::default(),
            };
            json(value)
        }
        fn system(p: &str, demo: &str) -> (PathBuf, u32) {
            #[cfg(feature = "docs-demo")]
            if std::env::var_os("TEND_DEMO_DIR").is_some() {
                return (w::home().join(demo), unsafe { libc::geteuid() });
            }
            let _ = demo;
            (p.into(), 0)
        }
        let v = match self {
            Self::Appearance => json(Appearance::read_at(
                &gtk::glib::user_config_dir().join("tend"),
            )?)?,
            Self::Monitoring => json(health::Settings::read()?)?,
            Self::History => json(history::Settings::read()?)?,
            Self::Workspaces => {
                load::<workspaces::Settings>(&config.join("workspaces.json"), uid, 256 * 1024)?
            }
            Self::ManualCleanup => load::<w::Cleanup>(&config.join("cleanup.json"), uid, 8192)?,
            Self::FolderRules => {
                load::<rules::Policy>(&config.join("rules.json"), uid, 512 * 1024)?
            }
            Self::UserSchedule => {
                load::<rules::Schedule>(&config.join("cleanup-schedule.json"), uid, 8192)?
            }
            Self::DailyCleanup => {
                let (p, uid) = system(daily_policy::CONFIG, "demo-daily-policy.json");
                load::<daily_policy::Policy>(&p, uid, 8192)?
            }
            Self::Maintenance => {
                let (p, uid) = system(crate::CONFIG, "demo-maintenance-settings.json");
                let s: crate::Settings =
                    serde_json::from_value(load::<crate::Settings>(&p, uid, 8192)?)
                        .map_err(|e| e.to_string())?;
                s.validate()?;
                json(Maintenance::from(&s))?
            }
            Self::Exclusions => {
                let p = exclusions::Policy::read()?;
                json(Exclusions { paths: p.paths })?
            }
        };
        self.canonical(&v)
    }
}
fn json(value: impl Serialize) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Appearance {
    pub theme: u8,
    pub close_to_panel: bool,
}
impl Appearance {
    pub fn validate(&self) -> Result<(), String> {
        if self.theme > 2 {
            Err("Choose System, Light or Dark appearance.".into())
        } else {
            Ok(())
        }
    }
    fn read_at(root: &Path) -> Result<Self, String> {
        let uid = unsafe { libc::geteuid() };
        let theme = match config_io::read(&root.join("theme"), 64, Some(uid), false)? {
            None => 0,
            Some(b) => std::str::from_utf8(&b)
                .map_err(|e| e.to_string())?
                .trim()
                .parse::<u8>()
                .map_err(|_| "Invalid saved color scheme")?,
        };
        let close_to_panel = match config_io::read(&root.join("background"), 64, Some(uid), false)?
        {
            None => true,
            Some(b) => match std::str::from_utf8(&b).map_err(|e| e.to_string())?.trim() {
                "true" => true,
                "false" => false,
                _ => return Err("Invalid saved window behavior.".into()),
            },
        };
        let value = Self {
            theme,
            close_to_panel,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn save_checked(&self, expected: &Self) -> Result<(), String> {
        self.save_at_with(
            &gtk::glib::user_config_dir().join("tend"),
            expected,
            |path, value| match value {
                Some(text) => w::private_write(path, text),
                None => match std::fs::remove_file(path) {
                    Ok(()) => Ok(()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(e) => Err(e.to_string()),
                },
            },
        )
    }
    fn save_at_with(
        &self,
        root: &Path,
        expected: &Self,
        mut write: impl FnMut(&Path, Option<&str>) -> Result<(), String>,
    ) -> Result<(), String> {
        self.validate()?;
        let _guard = config_io::lock(&root.join("appearance.json"))?;
        if Self::read_at(root)? != *expected {
            return Err("Saved appearance changed. Review again before saving.".into());
        }
        let paths = [root.join("theme"), root.join("background")];
        let uid = unsafe { libc::geteuid() };
        let originals = paths
            .iter()
            .map(|p| {
                config_io::read(p, 64, Some(uid), false).and_then(|b| {
                    b.map(String::from_utf8)
                        .transpose()
                        .map_err(|e| e.to_string())
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let values = [self.theme.to_string(), self.close_to_panel.to_string()];
        for (index, (path, value)) in paths.iter().zip(&values).enumerate() {
            if let Err(error) = write(path, Some(value)) {
                let mut rollback = vec![];
                for i in 0..=index {
                    if let Err(e) = write(&paths[i], originals[i].as_deref()) {
                        rollback.push(e);
                    }
                }
                return if rollback.is_empty() {
                    Err(format!(
                        "Appearance was not saved. Previous preferences restored: {error}"
                    ))
                } else {
                    Err(format!("Some appearance preferences changed; rollback was incomplete. Reopen this review after fixing the storage error. Save: {error}; restore: {}",rollback.join("; ")))
                };
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Maintenance {
    pub enabled: bool,
    pub hour: u8,
    pub minute: u8,
    pub warnings: [bool; 3],
}
impl Default for Maintenance {
    fn default() -> Self {
        Self::from(&crate::Settings::default())
    }
}
impl From<&crate::Settings> for Maintenance {
    fn from(s: &crate::Settings) -> Self {
        Self {
            enabled: s.enabled,
            hour: s.hour,
            minute: s.minute,
            warnings: s.warnings,
        }
    }
}
impl Maintenance {
    pub fn validate(&self) -> Result<(), String> {
        if self.hour > 23 || self.minute > 59 {
            Err("Choose a valid 24-hour maintenance time.".into())
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exclusions {
    pub paths: Vec<exclusions::Entry>,
}
fn validate_rules(s: &rules::Policy) -> Result<(), String> {
    if s.rules.len() > 64 {
        return Err("At most 64 folder rules are supported.".into());
    }
    for r in &s.rules {
        if r.name.trim().is_empty()
            || r.name.len() > 80
            || r.name.chars().any(char::is_control)
            || !(1..=3650).contains(&r.days)
            || r.pattern.is_empty()
            || r.pattern.len() > 120
            || r.pattern.chars().any(char::is_control)
            || r.pattern.contains('/')
            || !r.folder.is_absolute()
            || r.folder == Path::new("/")
            || r.folder
                .to_str()
                .is_none_or(|s| s.len() > 4096 || s.chars().any(char::is_control))
            || r.folder
                .components()
                .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        {
            return Err(
                "Invalid cleanup rule: review its name, absolute folder, filename pattern and age."
                    .into(),
            );
        }
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub format: String,
    pub version: u32,
    pub exported_at: u64,
    pub sections: BTreeMap<Section, Value>,
}
impl Bundle {
    pub fn empty() -> Self {
        Self {
            format: "tend-settings".into(),
            version: 1,
            exported_at: w::now(),
            sections: BTreeMap::new(),
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.format != "tend-settings" || self.version != 1 || self.sections.len() > 10 {
            return Err(
                "Unsupported Tend settings bundle. Expected tend-settings version 1.".into(),
            );
        }
        for (k, v) in &self.sections {
            k.canonical(v)?;
        }
        Ok(())
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_BYTES {
            return Err("Settings bundle exceeds 1 MiB.".into());
        }
        let mut b: Self =
            serde_json::from_slice(bytes).map_err(|e| format!("Invalid settings bundle: {e}"))?;
        b.validate()?;
        for (k, v) in &mut b.sections {
            *v = k.canonical(v)?;
        }
        Ok(b)
    }
    pub fn read(path: &Path) -> Result<Self, String> {
        let bytes =
            config_io::read(path, MAX_BYTES, None, false)?.ok_or("Settings bundle not found")?;
        Self::parse(&bytes)
    }
    pub fn encode(&self) -> Result<String, String> {
        self.validate()?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        if text.len() > MAX_BYTES {
            return Err("Settings bundle exceeds 1 MiB after formatting.".into());
        }
        Ok(text)
    }
    pub fn imported(&self) -> Result<Self, String> {
        self.validate()?;
        let mut b = self.clone();
        for (k, v) in &mut b.sections {
            *v = k.imported(v)?;
        }
        Ok(b)
    }
    pub fn capture() -> (Self, Vec<String>) {
        let mut b = Self::empty();
        let mut warnings = vec![];
        for k in Section::ALL {
            match k.current() {
                Ok(v) => {
                    b.sections.insert(k, v);
                }
                Err(e) => warnings.push(format!("{} omitted: {e}", k.title())),
            }
        }
        (b, warnings)
    }
    pub fn save_export(&self, path: &Path) -> Result<(), String> {
        w::private_write(path, &self.encode()?)
    }
    pub fn save_drafts(&self) -> Result<(), String> {
        self.imported()?.save_export(&draft_path())
    }
    pub fn read_drafts() -> Result<Option<Self>, String> {
        match config_io::read(
            &draft_path(),
            MAX_BYTES,
            Some(unsafe { libc::geteuid() }),
            true,
        )? {
            Some(b) => Ok(Some(Self::parse(&b)?.imported()?)),
            None => Ok(None),
        }
    }
}
pub fn draft_path() -> PathBuf {
    w::home().join(".config/tend/import-drafts.json")
}
pub fn comparison(
    section: Section,
    current: &Value,
    draft: &Value,
) -> Vec<(String, String, String)> {
    fn flatten(v: &Value, prefix: &str, out: &mut BTreeMap<String, String>) {
        match v {
            Value::Object(m) => {
                for (k, v) in m {
                    let key = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{prefix}.{k}")
                    };
                    flatten(v, &key, out);
                }
            }
            Value::Array(v) => {
                out.insert(prefix.into(), format!("{} items", v.len()));
                for (i, v) in v.iter().enumerate() {
                    flatten(v, &format!("{prefix}.{}", i + 1), out);
                }
            }
            _ => {
                let value = match v {
                    Value::String(s) => s.clone(),
                    Value::Bool(b) => if *b { "On" } else { "Off" }.into(),
                    Value::Null => "Default".into(),
                    _ => v.to_string(),
                };
                out.insert(prefix.into(), value);
            }
        }
    }
    let (mut before, mut after) = (BTreeMap::new(), BTreeMap::new());
    flatten(current, "", &mut before);
    flatten(draft, "", &mut after);
    let keys = before
        .keys()
        .chain(after.keys())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let mut keys = keys.into_iter().collect::<Vec<_>>();
    keys.sort_by(|a, b| {
        let mut a = a.split('.');
        let mut b = b.split('.');
        loop {
            match (a.next(), b.next()) {
                (Some(a), Some(b)) => {
                    let order = match (a.parse::<usize>(), b.parse::<usize>()) {
                        (Ok(a), Ok(b)) => a.cmp(&b),
                        _ => a.cmp(b),
                    };
                    if order != std::cmp::Ordering::Equal {
                        return order;
                    }
                }
                (None, None) => return std::cmp::Ordering::Equal,
                (None, _) => return std::cmp::Ordering::Less,
                (_, None) => return std::cmp::Ordering::Greater,
            }
        }
    });
    let _ = section;
    keys.into_iter()
        .filter_map(|k| {
            if k == "version" {
                return None;
            }
            let a = before.get(&k).cloned().unwrap_or_else(|| "Not set".into());
            let b = after.get(&k).cloned().unwrap_or_else(|| "Not set".into());
            (a != b).then_some((k, a, b))
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn appearance_pair_failure_restores_bytes_and_reports_failed_rollback() {
        let root = PathBuf::from("/tmp/opencode").join(format!(
            "tend-appearance-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let before = Appearance {
            theme: 1,
            close_to_panel: true,
        };
        let next = Appearance {
            theme: 2,
            close_to_panel: false,
        };
        w::private_write(&root.join("theme"), "1\n").unwrap();
        w::private_write(&root.join("background"), "true\n").unwrap();
        let mut calls = 0;
        let error = next
            .save_at_with(&root, &before, |p, value| {
                calls += 1;
                if calls == 2 {
                    return Err("controlled second write failure".into());
                }
                w::private_write(p, value.unwrap())
            })
            .unwrap_err();
        assert!(error.contains("Previous preferences restored"));
        assert_eq!(std::fs::read(root.join("theme")).unwrap(), b"1\n");
        assert_eq!(std::fs::read(root.join("background")).unwrap(), b"true\n");
        let mut calls = 0;
        let error = next
            .save_at_with(&root, &before, |p, value| {
                calls += 1;
                if calls == 2 || calls == 3 {
                    return Err("controlled storage failure".into());
                }
                w::private_write(p, value.unwrap())
            })
            .unwrap_err();
        assert!(error.contains("Some appearance preferences changed"));
        assert_eq!(Appearance::read_at(&root).unwrap().theme, 2);
    }
    #[test]
    fn imports_are_typed_inactive_and_do_not_activate_settings() {
        let mut b = Bundle::empty();
        for (k, v) in [
            (
                Section::Monitoring,
                json(health::Settings {
                    enabled: true,
                    ..Default::default()
                })
                .unwrap(),
            ),
            (
                Section::UserSchedule,
                json(rules::Schedule {
                    enabled: true,
                    ..Default::default()
                })
                .unwrap(),
            ),
            (
                Section::Maintenance,
                json(Maintenance::from(&crate::Settings::default())).unwrap(),
            ),
        ] {
            b.sections.insert(k, v);
        }
        let before = b.clone();
        let i = b.imported().unwrap();
        for v in i.sections.values() {
            assert_eq!(v["enabled"], false);
        }
        assert_eq!(b, before);
        assert_eq!(Bundle::parse(b.encode().unwrap().as_bytes()).unwrap(), b);
    }
    #[test]
    fn reject_commands_credentials_future_versions_and_unknown_nested_fields() {
        let mut b = Bundle::empty();
        b.sections.insert(
            Section::History,
            json(history::Settings::default()).unwrap(),
        );
        b.version = 2;
        assert!(b.encode().is_err());
        b.version = 1;
        b.sections.get_mut(&Section::History).unwrap()["password"] = "secret".into();
        assert!(b.encode().is_err());
        assert!(Bundle::parse(br#"{"format":"tend-settings","version":1,"exported_at":0,"sections":{"restart":{"enabled":true}}}"#).is_err());
        assert!(Bundle::parse(&vec![b' '; MAX_BYTES + 1]).is_err());
    }
    #[test]
    fn rules_can_move_between_machines_without_reading_folders_but_stay_disabled() {
        let mut b = Bundle::empty();
        b.sections.insert(Section::FolderRules,serde_json::json!({"rules":[{"name":"Cache","folder":"/missing-on-this-machine/cache","pattern":"*.cache","days":30,"recursive":true,"enabled":true}]}));
        let i = b.imported().unwrap();
        assert_eq!(
            i.sections[&Section::FolderRules]["rules"][0]["enabled"],
            false
        );
        assert!(!Path::new("/missing-on-this-machine/cache").exists());
    }
    #[test]
    fn maintenance_transport_has_no_restart_field_or_exclusion_revision() {
        let value = json(Maintenance::from(&crate::Settings {
            reboot: true,
            ..Default::default()
        }))
        .unwrap();
        assert!(value.get("reboot").is_none());
        let mut invalid = value;
        invalid["reboot"] = true.into();
        assert!(Section::Maintenance.canonical(&invalid).is_err());
        assert!(Section::Exclusions
            .canonical(&serde_json::json!({"paths":[],"revision":9}))
            .is_err());
    }
}

impl Section {
    /// User-scoped writes only. The normal writers and reviewed imports share
    /// each setting's lock and compare the expected value after acquiring it.
    pub fn save_user(self, value: &Value, expected: &Value) -> Result<(), String> {
        let value = self.canonical(value)?;
        let expected = self.canonical(expected)?;
        macro_rules! save {
            ($t:ty) => {
                serde_json::from_value::<$t>(value)
                    .map_err(|e| e.to_string())?
                    .save_checked(Some(
                        &serde_json::from_value::<$t>(expected).map_err(|e| e.to_string())?,
                    ))
            };
        }
        match self {
            Self::Monitoring => save!(health::Settings),
            Self::History => save!(history::Settings),
            Self::Workspaces => save!(workspaces::Settings),
            Self::ManualCleanup => save!(w::Cleanup),
            Self::FolderRules => save!(rules::Policy),
            Self::UserSchedule => save!(rules::Schedule).map(|_| ()),
            _ => Err("This section uses a separate reviewed system or appearance writer.".into()),
        }
    }
}
