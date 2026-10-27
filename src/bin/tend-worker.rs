use std::io::Read;
use tend::workstation as w;
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    match std::env::args().nth(1).as_deref() {
        Some("capabilities" | "--capabilities") => println!("{}", tend::capabilities::json("worker")),
        Some("activity") => {
            let input=std::env::args().nth(2).unwrap_or_else(||"{}".into());
            if input.len()>4096 {return Err("Activity query exceeds 4 KiB".into());}
            let query:tend::activity::Query=serde_json::from_str(&input).map_err(|e|e.to_string())?;
            println!("{}",serde_json::to_string(&tend::activity::load(&query,None)?).map_err(|e|e.to_string())?);
        }
        Some("observe") => {
            let hub=tend::sampler::Hub::new();
            let sample=hub.request(tend::sampler::Detail::Basic).wait(std::time::Duration::from_secs(10))
                .ok_or("Observation timed out")??;
            println!("{}",serde_json::to_string(&sample.observation).map_err(|e|e.to_string())?);
        }
        Some("history") => {
            let events=match tend::history::History::existing()? {
                Some(store)=>store.events(40,None,None)?.events, None=>vec![],
            };
            println!("{}",serde_json::to_string(&events).map_err(|e|e.to_string())?);
        }
        Some("cleanup") => {
            let mut s = String::new();
            std::io::stdin()
                .take(8192)
                .read_to_string(&mut s)
                .map_err(|e| e.to_string())?;
            let c: w::Cleanup = serde_json::from_str(&s).map_err(|e| e.to_string())?;
            println!("{}", w::cleanup_user(&c)?);
        }
        Some("daily-phase") => {
            #[cfg(feature = "docs-demo")]
            if std::env::var_os("TEND_DEMO_DIR").is_some() {
                return Err("Daily execution is disabled in documentation builds".into());
            }
            let phase = std::env::args().nth(2).ok_or("Missing phase")?;
            let policy: tend::daily_policy::Policy = serde_json::from_str(
                &std::env::args().nth(3).ok_or("Missing policy")?,
            ).map_err(|e| e.to_string())?;
            policy.validate()?;
            let _lock = tend::cleanup_rules::cleanup_lock()?;
            match phase.as_str() {
                "npm-cache" if policy.npm_verify => {
                    println!("{}", w::verify_npm()?);
                }
                "npm-logs" => {
                    policy.user_phase(&phase)?;
                    println!("{}", w::daily_npm_logs(policy.npm_log_days)?);
                }
                _ => println!("{}", w::cleanup_daily(&policy.user_phase(&phase)?)?),
            }
        }
        Some("scheduled") => {
            let runtime = format!("/run/user/{}", unsafe { libc::geteuid() });
            if std::path::Path::new(&runtime).is_dir() {
                std::env::set_var("XDG_RUNTIME_DIR", &runtime);
                std::env::set_var(
                    "DBUS_SESSION_BUS_ADDRESS",
                    format!("unix:path={runtime}/bus"),
                );
            }
            let apply = std::env::args().any(|a| a == "--apply");
            println!("{}", tend::cleanup_rules::scheduled(apply)?);
        }
        Some("report") => println!("{}", w::system_report()?),
        _ => return Err("Supported operations: report, cleanup (policy on stdin), scheduled [--apply], daily-phase (internal)".into()),
    }
    Ok(())
}
