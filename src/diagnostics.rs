//! Bounded, explicitly requested recordings. Evidence describes correlation,
//! never assigns a cause or changes running applications.
use crate::{history::History, sampler::Capture, workstation as w};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_MS: u64 = 15 * 60 * 1000;
pub const MAX_POINTS: usize = 451;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Point {
    pub elapsed_ms: u64,
    pub valid: bool,
    pub cpu: Option<f64>,
    pub memory_available: u64,
    pub swap_used: u64,
    pub cpu_pressure: Option<f64>,
    pub memory_pressure: Option<f64>,
    pub io_pressure: Option<f64>,
    pub swap_in: Option<f64>,
    pub swap_out: Option<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Marker {
    pub elapsed_ms: u64,
    pub label: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Contributor {
    pub id: String,
    pub name: String,
    pub cpu_sum: f64,
    pub samples: u32,
    pub peak_memory: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub version: u32,
    pub id: String,
    pub started_ms: u64,
    pub boot_id: String,
    pub workload: String,
    pub kernel: String,
    pub tend_version: String,
    pub memory_total: u64,
    pub logical_cpus: u32,
    pub requested_ms: u64,
    pub duration_ms: u64,
    pub points: Vec<Point>,
    pub markers: Vec<Marker>,
    pub contributors: Vec<Contributor>,
    pub errors: u32,
    #[serde(default)]
    pub gap_samples: u32,
    #[serde(skip)]
    last_sequence: u64,
}
fn text(value: &str, max: usize) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .take(max)
        .collect()
}
impl Session {
    pub fn new(workload: &str, seconds: u64) -> Result<Self, String> {
        if !(10..=900).contains(&seconds) {
            return Err("Choose a recording duration from 10 seconds to 15 minutes.".into());
        }
        let started_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        Ok(Self {
            version: 1,
            id: format!(
                "diagnostic:{}:{started_ms}:{}",
                crate::observations::boot_id(),
                std::process::id()
            ),
            started_ms,
            boot_id: crate::observations::boot_id().into(),
            workload: text(workload, 240),
            kernel: std::fs::read_to_string("/proc/sys/kernel/osrelease")
                .unwrap_or_else(|_| "Unavailable".into())
                .trim()
                .into(),
            tend_version: env!("CARGO_PKG_VERSION").into(),
            memory_total: 0,
            logical_cpus: 0,
            requested_ms: seconds * 1000,
            duration_ms: 0,
            points: vec![],
            markers: vec![],
            contributors: vec![],
            errors: 0,
            gap_samples: 0,
            last_sequence: 0,
        })
    }
    pub fn push(&mut self, elapsed_ms: u64, capture: &Capture) {
        if elapsed_ms > self.requested_ms + 5000 {
            self.gap_samples += 1;
            return;
        }
        let elapsed_ms = elapsed_ms.min(self.requested_ms);
        if self.points.len() >= MAX_POINTS
            || elapsed_ms > MAX_MS
            || capture.sequence == self.last_sequence
        {
            return;
        }
        self.last_sequence = capture.sequence;
        self.duration_ms = elapsed_ms;
        let o = &capture.observation;
        if o.gap {
            self.gap_samples += 1;
        }
        self.memory_total = o.memory_total;
        let valid = !o.gap && o.interval_ms > 0 && o.cpu_percent.is_some();
        self.points.push(Point {
            elapsed_ms,
            valid,
            cpu: o.cpu_percent,
            memory_available: o.memory_available,
            swap_used: o.swap_used,
            cpu_pressure: o.pressure.cpu_some,
            memory_pressure: o.pressure.memory_some,
            io_pressure: o.pressure.io_some,
            swap_in: o.swap_in_bytes_per_second,
            swap_out: o.swap_out_bytes_per_second,
        });
        if let Some(full) = &capture.full {
            self.logical_cpus = full.cores as u32;
            // Only retain a bounded aggregate of measured contributors, no argv,
            // environment, other users' app names, or per-process sample history.
            let mut candidates: Vec<_> = full
                .applications
                .iter()
                .filter(|g| g.uid == unsafe { libc::geteuid() } && g.cpu_percent.is_some())
                .collect();
            candidates.sort_by(|a, b| b.cpu_percent.unwrap().total_cmp(&a.cpu_percent.unwrap()));
            for g in candidates.into_iter().take(10) {
                if let Some(c) = self.contributors.iter_mut().find(|c| c.id == g.id) {
                    c.cpu_sum += g.cpu_percent.unwrap();
                    c.samples += 1;
                    c.peak_memory = c.peak_memory.max(g.memory_bytes);
                } else if self.contributors.len() < 64 {
                    self.contributors.push(Contributor {
                        id: g.id.clone(),
                        name: g.name.clone(),
                        cpu_sum: g.cpu_percent.unwrap(),
                        samples: 1,
                        peak_memory: g.memory_bytes,
                    });
                }
            }
        }
    }
    pub fn mark(&mut self, elapsed_ms: u64, label: &str) -> Result<(), String> {
        let label = text(label, 80);
        if label.trim().is_empty() {
            return Err("Add a short marker label.".into());
        }
        if self.markers.len() >= 32 {
            return Err("This recording already has 32 markers.".into());
        }
        self.markers.push(Marker {
            elapsed_ms: elapsed_ms.min(self.requested_ms),
            label,
        });
        Ok(())
    }
    pub fn finish(&mut self, elapsed_ms: u64) {
        self.duration_ms = elapsed_ms.min(self.requested_ms);
    }
    pub fn save(&self) -> Result<String, String> {
        if self.points.is_empty() {
            return Err("Record at least one sample before saving.".into());
        }
        let mut history = History::open()?;
        history.insert(
            &self.id,
            "diagnostic",
            self.started_ms as i64,
            &self.boot_id,
            &serde_json::to_value(self).map_err(|e| e.to_string())?,
        )?;
        Ok("Recording saved privately on this computer.".into())
    }
    pub fn summary(&self) -> String {
        let metrics = self.metrics();
        let mut lines = vec![format!(
            "{}\n{} · {} seconds · {} samples\nKernel {} · {} logical CPUs · {} RAM\n",
            if self.workload.is_empty() {
                "Untitled workload"
            } else {
                &self.workload
            },
            crate::presentation::when((self.started_ms / 1000) as i64),
            self.duration_ms / 1000,
            self.points.len(),
            self.kernel,
            self.logical_cpus,
            w::bytes(self.memory_total)
        )];
        for (key, m) in &metrics {
            lines.push(format!(
                "{key}: median {:.1} · 95th percentile {:.1} ({} valid samples)",
                m.median, m.p95, m.samples
            ));
        }
        if metrics.is_empty() {
            lines.push(
                "No valid rate samples yet. Missing counters cannot establish a bottleneck.".into(),
            );
        }
        let sustained = |metric: fn(&Point) -> Option<f64>, threshold: f64| {
            let mut run = 0;
            let mut longest = 0;
            let mut previous = None;
            for p in &self.points {
                let delta = previous
                    .map(|last: u64| p.elapsed_ms.saturating_sub(last))
                    .unwrap_or(0);
                previous = Some(p.elapsed_ms);
                if p.valid && delta <= 5000 && metric(p).is_some_and(|v| v >= threshold) {
                    run += delta;
                    longest = longest.max(run);
                } else {
                    run = 0;
                }
            }
            longest / 1000
        };
        let mut observations = Vec::new();
        for (name, metric, threshold) in [
            (
                "CPU over 90%",
                (|p: &Point| p.cpu) as fn(&Point) -> Option<f64>,
                90.,
            ),
            ("Memory pressure over 10%", |p| p.memory_pressure, 10.),
            ("I/O pressure over 10%", |p| p.io_pressure, 10.),
        ] {
            let seconds = sustained(metric, threshold);
            if seconds >= 10 {
                observations.push(format!(
                    "• {name} observed for at least {seconds} continuous seconds."
                ));
            }
        }
        lines.push("\nObserved signals".into());
        if observations.is_empty() {
            lines.push("No sustained threshold signal recorded. Short or quiet recordings can miss intermittent stalls.".into());
        } else {
            lines.extend(observations);
        }
        let mut contributors = self.contributors.clone();
        contributors.sort_by(|a, b| b.cpu_sum.total_cmp(&a.cpu_sum));
        if !contributors.is_empty() {
            lines.push("\nApp CPU observed in top-contributor samples".into());
        }
        for c in contributors.iter().take(5) {
            lines.push(format!(
                "• {}: {:.1}% average across {} contributing samples; peak RSS {}",
                c.name,
                c.cpu_sum / c.samples.max(1) as f64,
                c.samples,
                w::bytes(c.peak_memory)
            ));
        }
        if !self.markers.is_empty() {
            lines.push("\nWorkload markers".into());
        }
        for m in &self.markers {
            lines.push(format!(
                "• {}:{:02} · {}",
                m.elapsed_ms / 60000,
                (m.elapsed_ms / 1000) % 60,
                m.label
            ));
        }
        lines.push(format!("\n{} sample errors; {} warm-up or discontinuous rate observations. Pressure uses the kernel’s 10-second moving average. RSS can count shared memory more than once. These measurements show coincident activity, not a proven cause. Application versions and project state are not verified; keep them constant for comparisons.",self.errors,self.gap_samples));
        lines.join("\n")
    }
    pub fn metrics(&self) -> BTreeMap<String, Metric> {
        let mut result = BTreeMap::new();
        for (name, get) in [
            ("CPU (%)", (|p: &Point| p.cpu) as fn(&Point) -> Option<f64>),
            ("Memory pressure (%)", |p| p.memory_pressure),
            ("I/O pressure (%)", |p| p.io_pressure),
            ("Swap in (KiB/s)", |p| p.swap_in.map(|v| v / 1024.)),
            ("Swap out (KiB/s)", |p| p.swap_out.map(|v| v / 1024.)),
        ] {
            let mut values: Vec<_> = self
                .points
                .iter()
                .filter(|p| p.valid)
                .filter_map(get)
                .filter(|v| v.is_finite())
                .collect();
            if values.is_empty() {
                continue;
            }
            values.sort_by(f64::total_cmp);
            result.insert(
                name.into(),
                Metric {
                    median: values[(values.len() - 1) / 2],
                    p95: values[((values.len() as f64 * 0.95).ceil() as usize).saturating_sub(1)],
                    samples: values.len(),
                },
            );
        }
        result
    }
}
#[derive(Clone, Debug)]
pub struct Metric {
    pub median: f64,
    pub p95: f64,
    pub samples: usize,
}
pub fn load() -> Result<Vec<Session>, String> {
    let Some(history) = History::existing()? else {
        return Ok(vec![]);
    };
    history
        .events(20, None, Some("diagnostic"))?
        .events
        .into_iter()
        .map(|e| {
            serde_json::from_value(e.payload)
                .map_err(|e| format!("A saved recording could not be read: {e}"))
        })
        .collect()
}
pub fn compare(a: &Session, b: &Session) -> String {
    let left = a.metrics();
    let right = b.metrics();
    let mut lines = vec![format!("{} → {}\n", a.workload, b.workload)];
    if a.kernel != b.kernel || a.memory_total != b.memory_total || a.logical_cpus != b.logical_cpus
    {
        lines.push(
            "Different kernel or hardware context: the sessions are not directly comparable."
                .into(),
        );
    }
    if a.duration_ms < 60000
        || b.duration_ms < 60000
        || a.points.iter().filter(|p| p.valid).count() < 30
        || b.points.iter().filter(|p| p.valid).count() < 30
    {
        lines.push(
            "Insufficient evidence: record at least one minute and 30 valid samples per session."
                .into(),
        );
    }
    if a.workload != b.workload {
        lines.push(
            "Workload labels differ. Repeat the same interactions before drawing conclusions."
                .into(),
        );
    }
    for (key, l) in left {
        if let Some(r) = right.get(&key) {
            lines.push(format!(
                "{key}: median {:.1} → {:.1}; 95th percentile {:.1} → {:.1}",
                l.median, r.median, l.p95, r.p95
            ));
        }
    }
    lines.push("\nObserved differences do not prove an improvement in responsiveness. Repeat the same workload three times, note how interactions feel, and check for regressions. Lower CPU or swap occupancy alone is insufficient.".into());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observations::Observation;
    fn capture(n: u64) -> Capture {
        Capture {
            sequence: n,
            full: None,
            observation: Observation {
                cpu_percent: Some(95.),
                memory_total: 8000,
                interval_ms: 2000,
                ..Default::default()
            },
        }
    }
    #[test]
    fn recordings_are_bounded_and_duplicates_do_not_skew_metrics() {
        let mut s = Session::new("test", 900).unwrap();
        s.push(0, &capture(1));
        s.push(0, &capture(1));
        assert_eq!(s.points.len(), 1);
        for n in 2..600 {
            s.push(n * 1500, &capture(n));
        }
        assert_eq!(s.points.len(), MAX_POINTS);
        for _ in 0..32 {
            s.mark(0, "scroll").unwrap();
        }
        assert!(s.mark(0, "extra").is_err());
        assert!(serde_json::to_vec(&s).unwrap().len() < 256 * 1024);
        assert!(s.summary().contains("CPU over 90%"));
    }
    #[test]
    fn unavailable_or_gap_samples_never_become_zero_evidence() {
        let mut s = Session::new("test", 180).unwrap();
        let mut c = capture(1);
        c.observation.gap = true;
        s.push(0, &c);
        assert!(s.metrics().is_empty());
        assert!(compare(&s, &s).contains("Insufficient evidence"));
        c.sequence = 2;
        c.observation.gap = false;
        c.observation.cpu_percent = None;
        s.push(2000, &c);
        assert!(s.metrics().is_empty());
    }
    #[test]
    fn comparisons_disclose_context_and_short_runs() {
        let a = Session::new("a", 180).unwrap();
        let mut b = a.clone();
        b.kernel = "different".into();
        b.workload = "b".into();
        let report = compare(&a, &b);
        assert!(report.contains("not directly comparable"));
        assert!(report.contains("labels differ"));
        assert!(report.contains("Insufficient"));
        assert!(Session::new("", 901).is_err());
    }
}
