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
                    let npm = w::home().join(".opencode/bin/npm");
                    println!("{}", tend::output(npm.to_str().ok_or("Invalid npm path")?, &["cache", "verify"])?);
                }
                "npm-logs" => {
                    policy.user_phase(&phase)?;
                    println!("{}", w::daily_npm_logs(policy.npm_log_days)?);
                }
                _ => println!("{}", w::cleanup_user(&policy.user_phase(&phase)?)?),
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
