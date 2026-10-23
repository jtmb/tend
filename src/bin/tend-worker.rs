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
        _ => return Err("Supported operations: report, cleanup (selected policy on stdin)".into()),
    }
    Ok(())
}
