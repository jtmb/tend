//! Bounded, read-only command queries. Callers supply fixed programs/arguments;
//! mutation commands retain their existing execution and cancellation policies.
use std::{
    io::Read,
    os::{fd::AsRawFd, unix::process::CommandExt},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct QueryProcess {
    child: Child,
    completed: bool,
}
impl Drop for QueryProcess {
    fn drop(&mut self) {
        if !self.completed {
            // Only descendants of the query just created here belong to this
            // process group. Existing applications/services are never targeted.
            unsafe {
                libc::kill(-(self.child.id() as i32), libc::SIGKILL);
            }
        }
        let _ = self.child.wait();
    }
}
fn nonblocking(reader: &impl AsRawFd) -> Result<(), String> {
    let fd = reader.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}
fn start(command: &mut Command) -> Result<Child, String> {
    let parent = unsafe { libc::getpid() };
    unsafe {
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::getppid() != parent {
                libc::_exit(125);
            }
            Ok(())
        });
    }
    command
        .process_group(0)
        .stdin(Stdio::null())
        .env("SSH_ASKPASS_REQUIRE", "never")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())
}
fn drain(
    reader: &mut impl Read,
    data: &mut Vec<u8>,
    limit: usize,
    eof: &mut bool,
    deadline: Instant,
) -> Result<(), String> {
    let mut bytes = [0; 4096];
    loop {
        if Instant::now() >= deadline {
            return Err("Read-only query timed out; its query process was stopped.".into());
        }
        match reader.read(&mut bytes) {
            Ok(0) => {
                *eof = true;
                return Ok(());
            }
            Ok(n) => {
                if data.len() + n > limit {
                    return Err("Read-only query exceeded its output limit; no partial report was accepted.".into());
                }
                data.extend_from_slice(&bytes[..n]);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
}
pub fn run(command: &mut Command, timeout: Duration, max_bytes: usize) -> Result<String, String> {
    if timeout.is_zero()
        || timeout > Duration::from_secs(30)
        || max_bytes == 0
        || max_bytes > 8 * 1024 * 1024
    {
        return Err("Invalid read-only query budget.".into());
    }
    let mut process = QueryProcess {
        child: start(command)?,
        completed: false,
    };
    let mut stdout = process.child.stdout.take().ok_or("Missing query output")?;
    let mut stderr = process
        .child
        .stderr
        .take()
        .ok_or("Missing query error output")?;
    nonblocking(&stdout)?;
    nonblocking(&stderr)?;
    let (mut out, mut err, mut out_eof, mut err_eof) = (Vec::new(), Vec::new(), false, false);
    let started = Instant::now();
    let mut status = None;
    loop {
        drain(
            &mut stdout,
            &mut out,
            max_bytes,
            &mut out_eof,
            started + timeout,
        )?;
        drain(
            &mut stderr,
            &mut err,
            64 * 1024,
            &mut err_eof,
            started + timeout,
        )?;
        if status.is_none() {
            status = process.child.try_wait().map_err(|e| e.to_string())?;
        }
        if let Some(status) = status {
            if out_eof && err_eof {
                process.completed = true;
                return if status.success() {
                    Ok(String::from_utf8_lossy(&out).into_owned())
                } else {
                    let detail = if err.is_empty() {
                        String::from_utf8_lossy(&out)
                    } else {
                        String::from_utf8_lossy(&err)
                    };
                    Err(format!(
                        "Read-only query failed ({status}): {}",
                        detail.trim()
                    ))
                };
            }
        }
        if started.elapsed() >= timeout {
            return Err("Read-only query timed out; its query process was stopped.".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queries_preserve_literal_arguments_and_bound_lifetime_and_output() {
        assert_eq!(
            run(
                Command::new("/usr/bin/printf").args(["%s", "$(literal)"]),
                Duration::from_secs(1),
                100
            )
            .unwrap(),
            "$(literal)"
        );
        assert!(run(
            Command::new("/usr/bin/head").args(["-c", "10000", "/dev/zero"]),
            Duration::from_secs(1),
            100
        )
        .unwrap_err()
        .contains("output limit"));
        let start = Instant::now();
        assert!(run(
            Command::new("/usr/bin/sleep").arg("3"),
            Duration::from_millis(100),
            100
        )
        .unwrap_err()
        .contains("timed out"));
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(run(
            &mut Command::new("/usr/bin/false"),
            Duration::from_secs(1),
            100
        )
        .unwrap_err()
        .contains("failed"));
    }
    #[test]
    fn query_exits_when_its_owner_thread_stops() {
        use std::os::unix::process::ExitStatusExt;
        let mut child =
            std::thread::spawn(|| start(Command::new("/usr/bin/sleep").arg("10")).unwrap())
                .join()
                .unwrap();
        assert_eq!(child.wait().unwrap().signal(), Some(libc::SIGKILL));
    }
}
