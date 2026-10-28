//! Measures generated metadata indexing, not real filesystem scan speed.
use std::{path::PathBuf,time::Instant};
fn peak()->u64 {let mut r=unsafe {std::mem::zeroed::<libc::rusage>()};unsafe {libc::getrusage(libc::RUSAGE_SELF,&mut r);}r.ru_maxrss.max(0) as u64*1024}
fn main()->Result<(),String> {
    let count=std::env::args().nth(1).and_then(|s|s.parse::<u64>().ok()).unwrap_or(1_000_000);
    let path=PathBuf::from("/tmp/opencode").join(format!("tend-index-probe-{}-{}",std::process::id(),tend::workstation::now()));
    let baseline=peak();let started=Instant::now();
    let size=tend::history::snapshots::generated_metadata_probe(&path,count)?;
    let peak=peak();let extra=peak.saturating_sub(baseline);
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({"scope":"generated metadata indexing; not filesystem traversal speed","build":if cfg!(debug_assertions){"development"}else{"optimized"},"entries":count,"seconds":started.elapsed().as_secs_f64(),"baseline_peak_rss_bytes":baseline,"peak_rss_bytes":peak,"additional_peak_rss_bytes":extra,"memory_budget_pass":extra<=128*1024*1024,"database_with_sidecars_bytes":size,"retained_fixture":path})).unwrap());Ok(())
}
