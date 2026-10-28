//! Read-only collector budget measurement; no desktop/session operations.
use std::time::{Duration, Instant};
use tend::sampler::{Detail, Hub};
fn cpu() -> f64 {
    let mut r = unsafe { std::mem::zeroed::<libc::rusage>() };
    unsafe {
        libc::getrusage(libc::RUSAGE_SELF, &mut r);
    }
    (r.ru_utime.tv_sec + r.ru_stime.tv_sec) as f64
        + (r.ru_utime.tv_usec + r.ru_stime.tv_usec) as f64 / 1e6
}
fn rss() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|s| {
            s.strip_prefix("VmRSS:")?
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()
        })
        .unwrap_or(0)
        * 1024
}
fn main() -> Result<(), String> {
    let seconds = std::env::args()
        .nth(1)
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(600)
        .clamp(60, 1800);
    let baseline = rss();
    let hub = Hub::new();
    std::thread::sleep(Duration::from_secs(2));
    let idle_collections = hub.collection_count();
    let full = hub
        .request(Detail::Full)
        .wait(Duration::from_secs(20))
        .ok_or("Full sample timed out")??;
    let processes = full.full.as_ref().map(|s| s.tasks.len());
    drop(full);
    std::thread::sleep(Duration::from_secs(1));
    let sub = hub.subscribe(Detail::Basic, Duration::from_secs(15));
    let started = Instant::now();
    let initial_cpu = cpu();
    let mut peak = rss();
    let mut samples = 0;
    while started.elapsed() < Duration::from_secs(seconds) {
        if let Some(value) = sub.wait(Duration::from_secs(1)) {
            value?;
            samples += 1;
        }
        peak = peak.max(rss());
    }
    drop(sub);
    let wall = started.elapsed().as_secs_f64();
    let cpu_percent = (cpu() - initial_cpu) / wall * 100.;
    std::thread::sleep(Duration::from_secs(2));
    // A read already in flight can finish after its last consumer drops.
    // Check a complete subsequent interval after allowing that read to settle.
    let count = hub.collection_count();
    std::thread::sleep(Duration::from_secs(17));
    let stopped = count == hub.collection_count();
    hub.shutdown();
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({"scope":"collector only; not full application budget certification","build":if cfg!(debug_assertions){"development"}else{"optimized"},
        "seconds":wall,"samples":samples,"processes_in_single_full_sample":processes,"cpu_percent_one_core":cpu_percent,
        "baseline_rss_bytes":baseline,"peak_rss_bytes":peak,"incremental_rss_bytes":peak.saturating_sub(baseline),
        "idle_collections":idle_collections,"stopped_after_last_consumer":stopped,
        "collector_cpu_budget_pass":cpu_percent<0.5,"collector_memory_budget_pass":peak.saturating_sub(baseline)<=32*1024*1024})).unwrap());
    Ok(())
}
