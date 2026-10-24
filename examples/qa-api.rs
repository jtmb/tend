//! Test driver for real library calls inside the QA mount namespace only.
//! Never installed. No implementation is mocked here; external commands are
//! replaced by the sandbox runner, whose only writable mounts are fixtures.
use tend::{cleanup_rules::Schedule, preferences as p, workstation as w};
fn signal_fixture(mode: &str) -> Result<String, String> {
    use std::os::unix::process::ExitStatusExt;
    // This is a newly created child in the private PID namespace, never a
    // process selected from the host session.
    let mut child = std::process::Command::new("/usr/bin/sleep")
        .arg("30").spawn().map_err(|e| e.to_string())?;
    let mut process = w::processes(&[], 1.).into_iter()
        .find(|p| p.pid == child.id()).ok_or("Fixture child missing")?;
    if mode == "stale" {
        process.start = process.start.wrapping_add(1);
        assert!(w::signal_process(&process, false).is_err());
        assert!(child.try_wait().unwrap().is_none());
        process.start = process.start.wrapping_sub(1);
    }
    let force = mode == "force";
    let report = w::signal_process(&process, force)?;
    assert_eq!(child.wait().unwrap().signal(), Some(if force { 9 } else { 15 }));
    let own = w::processes(&[], 1.).into_iter()
        .find(|p| p.pid == std::process::id()).unwrap();
    assert!(w::signal_process(&own, true).is_err());
    Ok(report)
}
fn main() {
    assert_eq!(std::env::var("TEND_QA_SANDBOX").as_deref(), Ok("1"));
    assert_eq!(std::env::var("HOME").as_deref(), Ok("/home/james"));
    assert!(std::path::Path::new("/qa/mock-command").is_file());
    let mapping = std::fs::read_to_string("/proc/self/uid_map").unwrap();
    assert_eq!(mapping.split_whitespace().collect::<Vec<_>>().get(2), Some(&"1"));
    let args: Vec<_> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("schedule") => serde_json::from_str::<Schedule>(&args[1])
            .map_err(|e| e.to_string()).and_then(|s| s.save()),
        Some("startup") => p::set_startup(&args[1], args[2] == "true"),
        Some("unit") => p::set_unit(&args[1], args[2] == "true"),
        Some("wayland") => p::wayland(&args[1]),
        Some("animations") => p::animations(args[1] == "true"),
        Some("restore") => p::restore(args[1].parse().unwrap()),
        Some("signal") => signal_fixture(&args[1]),
        _ => Err("Unsupported QA action".into()),
    };
    match result {
        Ok(report) => println!("{report}"),
        Err(error) => { eprintln!("{error}"); std::process::exit(1); }
    }
}
