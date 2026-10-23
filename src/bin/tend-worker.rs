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
        Some("report") => println!("{}", w::system_report()?),
        _ => return Err("Supported operations: report, cleanup (selected policy on stdin)".into()),
    }
    Ok(())
}
