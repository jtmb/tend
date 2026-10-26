//! Bounded, serializable observations. Collection is read-only and local.
use crate::workstation::Sample;
use serde::{Deserialize, Serialize};
use std::{ffi::CString, os::unix::ffi::OsStrExt, path::Path, sync::OnceLock, time::Duration};

pub fn boot_id() -> &'static str {
    static BOOT: OnceLock<String> = OnceLock::new();
    BOOT.get_or_init(|| {
        std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
            .unwrap_or_else(|_| "unknown".into())
            .trim()
            .to_owned()
    })
}
/// CLOCK_BOOTTIME includes suspend, unlike Instant's Linux monotonic clock.
pub fn boot_millis() -> Option<u64> {
    let mut time = unsafe { std::mem::zeroed::<libc::timespec>() };
    if unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut time) } != 0 || time.tv_sec < 0 {
        return None;
    }
    Some(
        (time.tv_sec as u64)
            .saturating_mul(1000)
            .saturating_add(time.tv_nsec.max(0) as u64 / 1_000_000),
    )
}
pub fn sample_interval(
    before: Option<u64>,
    after: Option<u64>,
    monotonic: Duration,
) -> (Duration, bool) {
    let (Some(before), Some(after)) = (before, after) else {
        return (monotonic, true);
    };
    let Some(delta) = after.checked_sub(before) else {
        return (Duration::ZERO, true);
    };
    let elapsed = Duration::from_millis(delta);
    let gap = elapsed.is_zero()
        || elapsed > Duration::from_secs(60)
        || elapsed.abs_diff(monotonic) > Duration::from_millis(250);
    (elapsed, gap)
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProcessIdentity {
    pub boot_id: String,
    pub pid: u32,
    pub start_ticks: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Pressure {
    pub cpu_some: Option<f64>,
    pub memory_some: Option<f64>,
    pub memory_full: Option<f64>,
    pub io_some: Option<f64>,
    pub io_full: Option<f64>,
}
impl Pressure {
    pub fn parse(text: &str) -> Self {
        let value = |kind: &str, mode: &str| {
            text.lines().find_map(|line| {
                let rest = line
                    .strip_prefix(kind)?
                    .trim_start_matches(':')
                    .trim_start();
                let rest = rest.strip_prefix(mode)?;
                rest.split_whitespace().find_map(|part| {
                    part.strip_prefix("avg10=")?
                        .parse::<f64>()
                        .ok()
                        .filter(|n| n.is_finite() && (0. ..=100.).contains(n))
                })
            })
        };
        let continuation = |kind: &str, mode: &str| {
            let mut inside = false;
            for line in text.lines() {
                if line.starts_with(&format!("{kind}:")) {
                    inside = true;
                } else if ["cpu:", "memory:", "io:"]
                    .iter()
                    .any(|k| line.starts_with(k))
                {
                    inside = false;
                }
                if inside {
                    let stripped = line.strip_prefix(&format!("{kind}: ")).unwrap_or(line);
                    if let Some(rest) = stripped.strip_prefix(mode) {
                        return rest.split_whitespace().find_map(|p| {
                            p.strip_prefix("avg10=")?
                                .parse::<f64>()
                                .ok()
                                .filter(|n| n.is_finite() && (0. ..=100.).contains(n))
                        });
                    }
                }
            }
            None
        };
        Self {
            cpu_some: value("cpu", "some"),
            memory_some: value("memory", "some"),
            memory_full: continuation("memory", "full"),
            io_some: value("io", "some"),
            io_full: continuation("io", "full"),
        }
    }
}

pub fn filesystem_free(path: &Path) -> Result<u64, String> {
    let path = CString::new(path.as_os_str().as_bytes()).map_err(|_| "Invalid filesystem path")?;
    let mut v = unsafe { std::mem::zeroed::<libc::statvfs>() };
    if unsafe { libc::statvfs(path.as_ptr(), &mut v) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(v.f_bavail.saturating_mul(v.f_frsize))
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Observation {
    pub time_unix_ms: u64,
    pub boot_id: String,
    pub interval_ms: u64,
    pub cpu_percent: Option<f64>,
    pub memory_total: u64,
    pub memory_available: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    pub swap_in_bytes_per_second: Option<f64>,
    pub swap_out_bytes_per_second: Option<f64>,
    pub pressure: Pressure,
    pub root_free_bytes: Option<u64>,
    pub gap: bool,
}
impl Observation {
    pub fn from_samples(current: &Sample, previous: Option<&Sample>, interval: Duration) -> Self {
        let valid = !interval.is_zero() && interval <= Duration::from_secs(60);
        let previous = previous.filter(|p| valid && current.total > p.total);
        let seconds = interval.as_secs_f64().max(0.001);
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(1) as f64;
        Self {
            time_unix_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .min(u64::MAX as u128) as u64,
            boot_id: boot_id().into(),
            interval_ms: interval.as_millis().min(u64::MAX as u128) as u64,
            cpu_percent: previous.filter(|p| current.idle >= p.idle).map(|p| {
                (100. * (1. - (current.idle - p.idle) as f64 / (current.total - p.total) as f64))
                    .clamp(0., 100.)
            }),
            memory_total: current.memory_total,
            memory_available: current.memory_available,
            swap_total: current.swap_total,
            swap_used: current.swap_total.saturating_sub(current.swap_free),
            swap_in_bytes_per_second: previous
                .filter(|p| current.page_in >= p.page_in)
                .map(|p| (current.page_in - p.page_in) as f64 * page_size / seconds),
            swap_out_bytes_per_second: previous
                .filter(|p| current.page_out >= p.page_out)
                .map(|p| (current.page_out - p.page_out) as f64 * page_size / seconds),
            pressure: Pressure::parse(&current.pressure),
            root_free_bytes: filesystem_free(Path::new("/")).ok(),
            gap: !valid,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn suspend_and_clock_gaps_cannot_be_reported_as_continuous_sampling() {
        assert!(!sample_interval(Some(1000), Some(3000), Duration::from_secs(2)).1);
        assert!(sample_interval(Some(1000), Some(13000), Duration::from_secs(2)).1);
        assert!(sample_interval(Some(1000), Some(63000), Duration::from_secs(62)).1);
        assert!(sample_interval(Some(3000), Some(1000), Duration::from_secs(2)).1);
        assert!(sample_interval(None, Some(1000), Duration::from_secs(2)).1);
    }
    #[test]
    fn pressure_distinguishes_missing_and_zero_and_reads_full_lines() {
        let p = Pressure::parse("cpu: some avg10=20.50 avg60=0.0 total=20\nmemory: some avg10=0.00 total=1\nfull avg10=4.00 total=2\nio: Unavailable\n");
        assert_eq!(p.cpu_some, Some(20.5));
        assert_eq!(p.memory_some, Some(0.));
        assert_eq!(p.memory_full, Some(4.));
        assert_eq!(p.io_full, None);
    }
    #[test]
    fn counter_reset_and_suspend_do_not_invent_rates() {
        let old = Sample {
            total: 1000,
            idle: 500,
            page_in: 100,
            ..Default::default()
        };
        let new = Sample {
            total: 1100,
            idle: 550,
            page_in: 90,
            ..Default::default()
        };
        let s = Observation::from_samples(&new, Some(&old), Duration::from_secs(2));
        assert_eq!(s.cpu_percent, Some(50.));
        assert!(s.swap_in_bytes_per_second.is_none());
        let s = Observation::from_samples(&new, Some(&old), Duration::from_secs(120));
        assert!(s.gap && s.cpu_percent.is_none() && s.swap_out_bytes_per_second.is_none());
    }
}
