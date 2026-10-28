//! Opt-in health policy and deterministic, bounded alert state. No scheduler,
//! system changes, process enumeration or I/O in the state machine.
use crate::{observations::Observation, workstation as w};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::PathBuf,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub enabled: bool,
    pub threshold: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub version: u8,
    pub enabled: bool,
    pub cpu: Rule,
    pub memory: Rule,
    pub disk: Rule,
    pub memory_pressure: Rule,
    pub io_pressure: Rule,
    pub hold_seconds: u32,
    pub cooldown_seconds: u32,
    pub recovery: bool,
    pub crashes: bool,
}
impl Default for Settings {
    fn default() -> Self {
        let rule = |threshold| Rule {
            enabled: true,
            threshold,
        };
        Self {
            version: 1,
            enabled: false,
            cpu: rule(90.),
            memory: rule(10.),
            disk: rule(20.),
            memory_pressure: rule(10.),
            io_pressure: rule(20.),
            hold_seconds: 60,
            cooldown_seconds: 1800,
            recovery: true,
            crashes: false,
        }
    }
}
pub fn path() -> PathBuf {
    w::home().join(".config/tend/monitoring.json")
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || !(30..=3600).contains(&self.hold_seconds)
            || !(60..=86400).contains(&self.cooldown_seconds)
        {
            return Err("Monitoring version must be 1; sustained time 30–3600 seconds; cooldown 60–86400 seconds.".into());
        }
        for (name, rule, min, max) in [
            ("CPU", &self.cpu, 1., 100.),
            ("Available memory", &self.memory, 1., 90.),
            ("Free disk space", &self.disk, 1., 1024.),
            ("Memory pressure", &self.memory_pressure, 0.1, 100.),
            ("I/O pressure", &self.io_pressure, 0.1, 100.),
        ] {
            if !rule.threshold.is_finite() || !(min..=max).contains(&rule.threshold) {
                return Err(format!("{name} threshold must be {min}–{max}."));
            }
        }
        Ok(())
    }
    pub fn read() -> Result<Self, String> {
        Self::read_at(&path())
    }
    fn read_at(path: &std::path::Path) -> Result<Self, String> {
        let file = match fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
        {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(format!("Monitoring settings unavailable: {e}")),
        };
        let m = file.metadata().map_err(|e| e.to_string())?;
        if !m.is_file()
            || m.uid() != unsafe { libc::geteuid() }
            || m.mode() & 0o077 != 0
            || m.len() > 8192
        {
            return Err(
                "Monitoring settings must be a private regular file, at most 8 KiB.".into(),
            );
        }
        let mut text = String::new();
        file.take(8193)
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
        if text.len() > 8192 {
            return Err("Monitoring settings exceed 8 KiB.".into());
        }
        let value: Self =
            serde_json::from_str(&text).map_err(|e| format!("Invalid monitoring settings: {e}"))?;
        value.validate()?;
        Ok(value)
    }
    pub fn save(&self) -> Result<(), String> {
        self.save_checked(None)
    }
    pub fn save_checked(&self, expected: Option<&Self>) -> Result<(), String> {
        self.validate()?;
        let _guard = crate::config_io::lock(&path())?;
        if let Some(expected) = expected {
            if Self::read()? != *expected {
                return Err("Saved settings changed. Review again before saving.".into());
            }
        }

        self.validate()?;
        w::private_write(
            &path(),
            &serde_json::to_string_pretty(self).map_err(|e| e.to_string())?,
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Metric {
    Cpu,
    Memory,
    Disk,
    MemoryPressure,
    IoPressure,
}
impl Metric {
    pub const ALL: [Self; 5] = [
        Self::Cpu,
        Self::Memory,
        Self::Disk,
        Self::MemoryPressure,
        Self::IoPressure,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Cpu => "High CPU use",
            Self::Memory => "Low available memory",
            Self::Disk => "Low free disk space",
            Self::MemoryPressure => "Memory stalls",
            Self::IoPressure => "I/O stalls",
        }
    }
    pub fn rule(self, s: &Settings) -> &Rule {
        match self {
            Self::Cpu => &s.cpu,
            Self::Memory => &s.memory,
            Self::Disk => &s.disk,
            Self::MemoryPressure => &s.memory_pressure,
            Self::IoPressure => &s.io_pressure,
        }
    }
    fn low(self) -> bool {
        matches!(self, Self::Memory | Self::Disk)
    }
    fn value(self, o: &Observation) -> Option<f64> {
        let value = match self {
            Self::Cpu => o.cpu_percent,
            Self::Memory => (o.memory_total > 0 && o.memory_available <= o.memory_total)
                .then(|| 100. * o.memory_available as f64 / o.memory_total as f64),
            Self::Disk => o.root_free_bytes.map(|b| b as f64 / 1073741824.),
            Self::MemoryPressure => o.pressure.memory_full,
            Self::IoPressure => o.pressure.io_full,
        }?;
        (value.is_finite() && value >= 0. && (self == Self::Disk || value <= 100.)).then_some(value)
    }
    fn bad(self, value: f64, threshold: f64) -> bool {
        if self.low() {
            value < threshold
        } else {
            value >= threshold
        }
    }
    fn recovered(self, value: f64, threshold: f64) -> bool {
        if self.low() {
            value >= threshold * 1.1
        } else {
            value < threshold * 0.9
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Notice {
    pub metric: Metric,
    pub recovered: bool,
    pub value: f64,
    pub threshold: f64,
    pub time_ms: u64,
}
impl Notice {
    pub fn title(&self) -> String {
        if self.recovered {
            format!("Recovered · {}", self.metric.name())
        } else {
            self.metric.name().into()
        }
    }
    pub fn body(&self) -> String {
        let unit = if self.metric == Metric::Disk {
            "GiB free"
        } else if self.metric == Metric::Memory {
            "% available"
        } else if self.metric == Metric::Cpu {
            "% CPU"
        } else {
            "% time stalled"
        };
        format!(
            "{:.1} {unit} · threshold {:.1}. {}Open Tend to inspect the evidence.",
            self.value,
            self.threshold,
            if self.recovered {
                "The recovery threshold was sustained. "
            } else {
                "The alert threshold was sustained. "
            }
        )
    }
}
#[derive(Default)]
struct State {
    since: Option<u64>,
    recovery_since: Option<u64>,
    active: bool,
    announced: bool,
    last_notice: Option<u64>,
}
pub struct Engine {
    settings: Settings,
    states: [State; 5],
    last: Option<u64>,
    boot_id: String,
    snooze_until: Option<u64>,
}
impl Engine {
    pub fn new(settings: Settings) -> Result<Self, String> {
        settings.validate()?;
        Ok(Self {
            settings,
            states: Default::default(),
            last: None,
            boot_id: String::new(),
            snooze_until: None,
        })
    }
    pub fn snooze(&mut self, now: u64, seconds: u32) {
        self.snooze_until = Some(now.saturating_add(seconds as u64 * 1000));
        for s in &mut self.states {
            s.announced = false;
        }
    }
    pub fn resume(&mut self) {
        self.snooze_until = None;
    }
    pub fn snoozed(&self, now: u64) -> bool {
        self.snooze_until.is_some_and(|until| now < until)
    }
    pub fn active_count(&self) -> usize {
        self.states.iter().filter(|s| s.active).count()
    }
    pub fn missing(&mut self) {
        for s in &mut self.states {
            s.since = None;
            s.recovery_since = None;
        }
        self.last = None;
    }
    pub fn update(&mut self, o: &Observation, boot_ms: u64) -> Vec<Notice> {
        if !self.settings.enabled {
            return Vec::new();
        }
        if self.boot_id != o.boot_id {
            self.states = Default::default();
            self.last = None;
            if !self.boot_id.is_empty() {
                self.snooze_until = None;
            }
            self.boot_id = o.boot_id.clone();
        }
        let continuous = !o.gap
            && self
                .last
                .is_some_and(|last| boot_ms > last && boot_ms - last <= 45000);
        if !continuous {
            self.missing();
        }
        self.last = Some(boot_ms);
        let snoozed = self.snoozed(boot_ms);
        let mut notices = Vec::new();
        for (i, metric) in Metric::ALL.into_iter().enumerate() {
            let rule = metric.rule(&self.settings);
            let s = &mut self.states[i];
            if !rule.enabled {
                *s = State::default();
                continue;
            }
            let Some(value) = metric.value(o) else {
                s.since = None;
                s.recovery_since = None;
                continue;
            };
            if !s.active {
                if metric.bad(value, rule.threshold) {
                    let since = *s.since.get_or_insert(boot_ms);
                    if boot_ms.saturating_sub(since) >= self.settings.hold_seconds as u64 * 1000 {
                        s.active = true;
                    }
                } else {
                    s.since = None;
                }
            }
            if s.active && metric.recovered(value, rule.threshold) {
                let since = *s.recovery_since.get_or_insert(boot_ms);
                if boot_ms.saturating_sub(since) >= self.settings.hold_seconds as u64 * 1000 {
                    if s.announced && self.settings.recovery && !snoozed {
                        notices.push(Notice {
                            metric,
                            recovered: true,
                            value,
                            threshold: rule.threshold,
                            time_ms: o.time_unix_ms,
                        });
                    }
                    s.active = false;
                    s.announced = false;
                    s.since = None;
                    s.recovery_since = None;
                }
            } else {
                s.recovery_since = None;
            }
            let sustained_bad = if s.active && metric.bad(value, rule.threshold) {
                let since = *s.since.get_or_insert(boot_ms);
                boot_ms.saturating_sub(since) >= self.settings.hold_seconds as u64 * 1000
            } else {
                if !metric.bad(value, rule.threshold) {
                    s.since = None;
                }
                false
            };
            // Active conditions repeat only after cooldown, including after snooze.
            if s.active
                && sustained_bad
                && !snoozed
                && s.last_notice.is_none_or(|last| {
                    boot_ms.saturating_sub(last) >= self.settings.cooldown_seconds as u64 * 1000
                })
            {
                s.last_notice = Some(boot_ms);
                s.announced = true;
                notices.push(Notice {
                    metric,
                    recovered: false,
                    value,
                    threshold: rule.threshold,
                    time_ms: o.time_unix_ms,
                });
            }
        }
        notices
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn policy() -> Settings {
        let mut s = Settings {
            enabled: true,
            hold_seconds: 30,
            cooldown_seconds: 60,
            ..Default::default()
        };
        s.memory.enabled = false;
        s.disk.enabled = false;
        s.memory_pressure.enabled = false;
        s.io_pressure.enabled = false;
        s
    }
    fn sample(cpu: Option<f64>) -> Observation {
        Observation {
            boot_id: "boot-a".into(),
            cpu_percent: cpu,
            ..Default::default()
        }
    }
    #[test]
    fn settings_reads_are_private_bounded_and_preserve_bad_inputs() {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let parent = std::path::Path::new("/tmp/opencode");
        fs::create_dir_all(parent).unwrap();
        let dir = parent.join(format!(
            "tend-health-settings-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        let path = dir.join("policy.json");
        assert_eq!(Settings::read_at(&path).unwrap(), Settings::default());
        assert!(!path.exists());
        fs::write(&path, serde_json::to_vec(&policy()).unwrap()).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(Settings::read_at(&path).unwrap(), policy());
        let bytes = fs::read(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(Settings::read_at(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let link = dir.join("alias.json");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(Settings::read_at(&link).is_err());
        fs::write(&path, b"{broken").unwrap();
        assert!(Settings::read_at(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"{broken");
        fs::write(&path, vec![b' '; 8193]).unwrap();
        assert!(Settings::read_at(&path).is_err());
        assert_eq!(fs::metadata(&path).unwrap().len(), 8193);
        fs::write(&path, br#"{"version":99}"#).unwrap();
        assert!(Settings::read_at(&path).is_err());
        fs::write(&path, br#"{"unexpected":true}"#).unwrap();
        assert!(Settings::read_at(&path).is_err());
    }
    #[test]
    fn defaults_validation() {
        assert!(!Settings::default().enabled);
        assert!(!Settings::default().crashes);
        assert!(Settings::default().validate().is_ok());
        for v in [f64::NAN, f64::INFINITY, -1., 101.] {
            let mut s = policy();
            s.cpu.threshold = v;
            assert!(s.validate().is_err());
        }
    }
    #[test]
    fn hold_cooldown_hysteresis_recovery() {
        let mut e = Engine::new(policy()).unwrap();
        assert!(e.update(&sample(Some(95.)), 0).is_empty());
        assert!(e.update(&sample(Some(95.)), 15000).is_empty());
        assert_eq!(e.update(&sample(Some(95.)), 30000).len(), 1);
        assert!(e.update(&sample(Some(95.)), 45000).is_empty());
        assert!(e.update(&sample(Some(85.)), 60000).is_empty());
        assert!(e.update(&sample(Some(85.)), 75000).is_empty());
        assert!(e.update(&sample(Some(95.)), 90000).is_empty());
        assert!(e.update(&sample(Some(95.)), 105000).is_empty());
        assert_eq!(e.update(&sample(Some(95.)), 120000).len(), 1);
        for t in [135000, 150000] {
            assert!(e.update(&sample(Some(70.)), t).is_empty());
        }
        let r = e.update(&sample(Some(70.)), 165000);
        assert_eq!(r.len(), 1);
        assert!(r[0].recovered);
        assert_eq!(e.active_count(), 0);
    }
    #[test]
    fn gap_unknown_clock_and_boot_reset() {
        let mut e = Engine::new(policy()).unwrap();
        e.update(&sample(Some(95.)), 0);
        e.update(&sample(Some(95.)), 15000);
        assert!(e.update(&sample(None), 30000).is_empty());
        assert!(e.update(&sample(Some(95.)), 45000).is_empty());
        assert!(e.update(&sample(Some(95.)), 200000).is_empty());
        assert!(e.update(&sample(Some(95.)), 1000).is_empty());
        let mut o = sample(Some(95.));
        o.boot_id = "boot-b".into();
        assert!(e.update(&o, 30000).is_empty());
        e.update(&o, 45000);
        assert_eq!(e.update(&o, 60000).len(), 1);
        o.gap = true;
        assert!(e.update(&o, 500000).is_empty());
        o.cpu_percent = Some(0.);
        assert!(e.update(&o, 600000).is_empty());
    }
    #[test]
    fn snooze_recovery_and_disable() {
        let mut e = Engine::new(policy()).unwrap();
        e.update(&sample(Some(95.)), 0);
        e.update(&sample(Some(95.)), 15000);
        e.snooze(15000, 60);
        for t in [30000, 45000, 60000] {
            assert!(e.update(&sample(Some(95.)), t).is_empty());
        }
        assert_eq!(e.update(&sample(Some(95.)), 75000).len(), 1);
        e.snooze(75000, 600);
        for t in [90000, 105000, 120000] {
            assert!(e.update(&sample(Some(0.)), t).is_empty());
        }
        e.resume();
        assert_eq!(e.active_count(), 0);
        let mut disabled = Engine::new(Settings::default()).unwrap();
        for t in [0, 15000, 30000, 45000] {
            assert!(disabled.update(&sample(Some(100.)), t).is_empty());
        }
    }
    #[test]
    fn low_thresholds_and_unavailable_metrics() {
        let mut p = policy();
        p.cpu.enabled = false;
        p.memory.enabled = true;
        p.disk.enabled = true;
        p.memory_pressure.enabled = true;
        p.io_pressure.enabled = true;
        let mut e = Engine::new(p).unwrap();
        let o = Observation {
            boot_id: "a".into(),
            memory_total: 1000,
            memory_available: 1,
            root_free_bytes: Some(1),
            pressure: crate::observations::Pressure {
                memory_full: Some(15.),
                io_full: Some(25.),
                ..Default::default()
            },
            ..Default::default()
        };
        e.update(&o, 0);
        e.update(&o, 15000);
        assert_eq!(e.update(&o, 30000).len(), 4);
        let unknown = Observation {
            boot_id: "a".into(),
            ..Default::default()
        };
        for t in [45000, 60000, 75000, 90000] {
            assert!(e.update(&unknown, t).is_empty());
        }
        assert_eq!(e.active_count(), 4);
    }
}
