//! Read-only access to the same requested postmortem used by the native UI.
fn main() -> Result<(), String> {
    println!("{}", tend::memory_incidents::report(2)?);
    Ok(())
}
