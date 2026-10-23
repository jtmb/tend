//! Read-only Task Manager counters. No service, tracing, or privileged collection.
use crate::workstation::{self as w, Sample};
use std::{collections::HashMap, fs, os::unix::fs::MetadataExt, time::Instant};
#[derive(Clone, Default)]
pub struct Task {
    pub pid: u32,
    pub start: u64,
    pub ticks: u64,
    pub name: String,
    pub user: String,
    pub uid: u32,
    pub memory: u64,
    pub cpu: Option<f64>,
    pub read: Option<u64>,
    pub written: Option<u64>,
    pub read_rate: Option<f64>,
    pub write_rate: Option<f64>,
    pub status: String,
    pub executable: String,
}
#[derive(Clone, Default)]
pub struct Disk {
    pub name: String,
    pub read: u64,
    pub written: u64,
    pub busy_ms: u64,
    pub read_rate: f64,
    pub write_rate: f64,
    pub busy: f64,
}
#[derive(Clone, Default)]
pub struct Adapter {
    pub name: String,
    pub received: u64,
    pub sent: u64,
    pub receive_rate: f64,
    pub send_rate: f64,
}
#[derive(Clone)]
pub struct Snapshot {
    pub at: Instant,
    pub system: Sample,
    pub tasks: Vec<Task>,
    pub disks: Vec<Disk>,
    pub adapters: Vec<Adapter>,
    pub cpu: Option<f64>,
    pub cores: usize,
    pub uptime: String,
    pub gpu: Vec<(String, f64)>,
}
pub fn rate(current: u64, previous: u64, seconds: f64) -> f64 {
    current.saturating_sub(previous) as f64 / seconds.max(0.001)
}
pub fn parse_net(text: &str) -> Vec<Adapter> {
    text.lines()
        .filter_map(|line| {
            let (name, data) = line.split_once(':')?;
            let fields: Vec<_> = data.split_whitespace().collect();
            let name = name.trim();
            if name == "lo" {
                return None;
            }
            Some(Adapter {
                name: name.into(),
                received: fields.first()?.parse().ok()?,
                sent: fields.get(8)?.parse().ok()?,
                ..Default::default()
            })
        })
        .collect()
}
pub fn collect(previous: Option<&Snapshot>) -> Snapshot {
    let at = Instant::now();
    let elapsed = previous
        .map(|p| at.duration_since(p.at).as_secs_f64())
        .unwrap_or(1.);
    let cores = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);
    let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f64;
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(1) as u64;
    let names: HashMap<u32, String> = fs::read_to_string("/etc/passwd")
        .unwrap_or_default()
        .lines()
        .filter_map(|s| {
            let f: Vec<_> = s.split(':').collect();
            Some((f.get(2)?.parse().ok()?, f.first()?.to_string()))
        })
        .collect();
    let prior: HashMap<(u32, u64), &Task> = previous
        .map(|p| p.tasks.iter().map(|t| ((t.pid, t.start), t)).collect())
        .unwrap_or_default();
    let mut tasks = vec![];
    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|s| s.parse::<u32>().ok())
            else {
                continue;
            };
            let Ok(meta) = entry.metadata() else { continue };
            let uid = meta.uid();
            let Ok(stat) = fs::read_to_string(entry.path().join("stat")) else {
                continue;
            };
            let Some(end) = stat.rfind(')') else { continue };
            let f: Vec<_> = stat[end + 1..].split_whitespace().collect();
            let num = |i: usize| f.get(i).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
            let start = num(19);
            let ticks = num(11) + num(12);
            let old = prior.get(&(pid, start));
            let cpu = old.map(|p| {
                (rate(ticks, p.ticks, elapsed) / hz / cores as f64 * 100.).clamp(0., 100.)
            });
            let io = fs::read_to_string(entry.path().join("io")).ok();
            let io_num = |key: &str| {
                io.as_ref()?.lines().find_map(|s| {
                    s.strip_prefix(key)
                        .and_then(|s| s.trim().parse::<u64>().ok())
                })
            };
            let read = io_num("read_bytes:");
            let written = io_num("write_bytes:");
            let read_rate = read
                .zip(old.and_then(|p| p.read))
                .map(|(a, b)| rate(a, b, elapsed));
            let write_rate = written
                .zip(old.and_then(|p| p.written))
                .map(|(a, b)| rate(a, b, elapsed));
            let status = match f.first().copied().unwrap_or("?") {
                "R" => "Running",
                "S" => "Sleeping",
                "D" => "Waiting for I/O",
                "T" | "t" => "Stopped",
                "Z" => "Zombie",
                "I" => "Idle",
                _ => "Other",
            }
            .into();
            let name = stat
                .find('(')
                .map(|i| stat[i + 1..end].to_owned())
                .unwrap_or_default();
            tasks.push(Task {
                pid,
                start,
                ticks,
                name,
                user: names.get(&uid).cloned().unwrap_or_else(|| uid.to_string()),
                uid,
                memory: num(21) * page,
                cpu,
                read,
                written,
                read_rate,
                write_rate,
                status,
                executable: fs::read_link(entry.path().join("exe"))
                    .map(|p| p.to_string_lossy().into())
                    .unwrap_or_else(|_| "Not accessible".into()),
            });
        }
    }
    let mut disks = vec![];
    if let Ok(entries) = fs::read_dir("/sys/block") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if ["loop", "ram", "zram", "dm-"]
                .iter()
                .any(|p| name.starts_with(p))
            {
                continue;
            }
            let Ok(raw) = fs::read_to_string(entry.path().join("stat")) else {
                continue;
            };
            let f: Vec<u64> = raw
                .split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();
            if f.len() < 10 {
                continue;
            }
            let mut disk = Disk {
                name,
                read: f[2] * 512,
                written: f[6] * 512,
                busy_ms: f[9],
                ..Default::default()
            };
            if let Some(old) = previous.and_then(|p| p.disks.iter().find(|d| d.name == disk.name)) {
                disk.read_rate = rate(disk.read, old.read, elapsed);
                disk.write_rate = rate(disk.written, old.written, elapsed);
                disk.busy = (rate(disk.busy_ms, old.busy_ms, elapsed) / 10.).clamp(0., 100.);
            }
            disks.push(disk);
        }
    }
    disks.sort_by(|a, b| a.name.cmp(&b.name));
    let mut adapters = parse_net(&fs::read_to_string("/proc/net/dev").unwrap_or_default());
    for adapter in &mut adapters {
        if let Some(old) = previous.and_then(|p| p.adapters.iter().find(|a| a.name == adapter.name))
        {
            adapter.receive_rate = rate(adapter.received, old.received, elapsed);
            adapter.send_rate = rate(adapter.sent, old.sent, elapsed);
        }
    }
    adapters.sort_by(|a, b| a.name.cmp(&b.name));
    let system = w::sample();
    let cpu = previous.filter(|p| system.total > p.system.total).map(|p| {
        (100.
            * (1.
                - system.idle.saturating_sub(p.system.idle) as f64
                    / (system.total - p.system.total) as f64))
            .clamp(0., 100.)
    });
    let seconds = fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok())
        .unwrap_or(0.) as u64;
    let uptime = format!(
        "{}d {:02}:{:02}:{:02}",
        seconds / 86400,
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    );
    let mut gpu = vec![];
    if let Ok(entries) = fs::read_dir("/sys/class/drm") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("card") || !name[4..].bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            if let Some(busy) = fs::read_to_string(entry.path().join("device/gpu_busy_percent"))
                .ok()
                .and_then(|s| s.trim().parse::<f64>().ok())
            {
                gpu.push((name, busy.clamp(0., 100.)));
            }
        }
    }
    Snapshot {
        at,
        system,
        tasks,
        disks,
        adapters,
        cpu,
        cores,
        uptime,
        gpu,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn network_columns_count_wire_bytes_and_skip_loopback() {
        let a=parse_net("Inter-| Receive | Transmit\nlo: 999 1 0 0 0 0 0 0 999 1 0 0 0 0 0 0\nenp0s1: 2048 2 0 0 0 0 0 0 4096 3 0 0 0 0 0 0\n");
        assert_eq!(a.len(), 1);
        assert_eq!((a[0].received, a[0].sent), (2048, 4096));
    }
    #[test]
    fn reset_counters_do_not_create_negative_or_huge_rates() {
        assert_eq!(rate(100, 200, 2.), 0.);
        assert_eq!(rate(4096, 2048, 2.), 1024.);
    }
    #[test]
    fn live_snapshot_has_current_process_and_valid_system_counters() {
        let first = collect(None);
        assert!(first.system.memory_total > 0);
        let me = first
            .tasks
            .iter()
            .find(|p| p.pid == std::process::id())
            .expect("current process");
        assert!(me.start > 0 && me.memory > 0);
        assert!(me.cpu.is_none());
        std::thread::sleep(std::time::Duration::from_millis(30));
        let second = collect(Some(&first));
        assert!(second.cpu.is_some_and(|v| (0.0..=100.0).contains(&v)));
        assert!(second
            .tasks
            .iter()
            .filter_map(|t| t.cpu)
            .all(|v| v.is_finite() && (0.0..=100.0).contains(&v)));
        assert!(second
            .disks
            .iter()
            .all(|d| d.busy.is_finite() && (0.0..=100.0).contains(&d.busy)));
    }
}
