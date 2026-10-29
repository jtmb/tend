//! Requested kernel OOM postmortems. No allocation stress, process signals or
//! background watcher; context is bounded and never joined to a live reused PID.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProcessMemory {
    pub pid: u32,
    pub name: String,
    pub rss_bytes: Option<u64>,
    pub swap_bytes: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Incident {
    pub time: i64,
    pub time_us: u64,
    pub boot_id: String,
    pub pid: u32,
    pub name: String,
    pub kind: String,
    pub anonymous_bytes: Option<u64>,
    pub swap_bytes: Option<u64>,
    pub swap_free_bytes: Option<u64>,
    pub swap_total_bytes: Option<u64>,
    pub cgroup: Option<String>,
    pub processes: Vec<ProcessMemory>,
    pub evidence: String,
    pub context_available: bool,
}
fn number_bytes(message: &str, key: &str) -> Option<u64> {
    message
        .split_whitespace()
        .find_map(|s| s.strip_prefix(key))?
        .trim_end_matches(',')
        .strip_suffix("kB")?
        .parse::<u64>()
        .ok()?
        .checked_mul(1024)
}
fn swap_line(message: &str) -> Option<u64> {
    message
        .split_once('=')?
        .1
        .trim()
        .strip_suffix("kB")?
        .trim()
        .parse::<u64>()
        .ok()?
        .checked_mul(1024)
}
fn victim(message: &str) -> Option<(u32, String)> {
    let rest = message
        .strip_prefix("Out of memory: Killed process ")
        .or_else(|| message.strip_prefix("Memory cgroup out of memory: Killed process "))?;
    let (pid, rest) = rest.split_once(" (")?;
    let name = rest.split_once(") total-vm:")?.0;
    let pid = pid.parse::<u32>().ok()?;
    if pid == 0 || name.is_empty() {
        return None;
    }
    Some((pid, name.chars().take(80).collect()))
}
fn field(message: &str, key: &str) -> Option<String> {
    message
        .split(',')
        .find_map(|s| s.strip_prefix(key))
        .map(|s| s.chars().take(1024).collect())
}
#[derive(Default)]
struct Context {
    boot: String,
    start: u64,
    started: bool,
    free: Option<u64>,
    total: Option<u64>,
    pid: Option<u32>,
    scope: Option<String>,
    global: bool,
    memcg: bool,
    columns: Option<(usize, usize, usize)>,
    tasks: BTreeMap<u32, ProcessMemory>,
}
/// Input must be journal JSON in journal order. Only trusted kernel-transport
/// records contribute; a program printing an OOM-looking message is not proof.
pub fn parse(input: &str, page_size: u64) -> Vec<Incident> {
    let mut context = Context::default();
    let mut output = Vec::new();
    let valid_page = page_size.is_power_of_two() && (1024..=65536).contains(&page_size);
    for line in input.lines().take(4096) {
        if line.len() > 64 * 1024 {
            continue;
        }
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if row["_TRANSPORT"].as_str() != Some("kernel") {
            continue;
        }
        let Some(us) = row["__REALTIME_TIMESTAMP"]
            .as_str()
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|n| *n <= i64::MAX as u64)
        else {
            continue;
        };
        let Some(message) = row["MESSAGE"].as_str() else {
            continue;
        };
        let boot = row["_BOOT_ID"]
            .as_str()
            .filter(|s| s.len() == 32 && s.bytes().all(|c| c.is_ascii_hexdigit()))
            .unwrap_or("");
        if context.boot != boot
            || us < context.start
            || us.saturating_sub(context.start) > 5_000_000
            || message.contains("invoked oom-killer:")
        {
            context = Context {
                boot: boot.into(),
                start: us,
                started: message.contains("invoked oom-killer:"),
                ..Default::default()
            };
        }
        if message.starts_with("Free swap") {
            context.free = swap_line(message);
        }
        if message.starts_with("Total swap") {
            context.total = swap_line(message);
        }
        if message.starts_with("oom-kill:") {
            context.pid = field(message, "pid=").and_then(|s| s.parse().ok());
            context.scope = field(message, "task_memcg=");
            // global_oom means no memcg; cpuset/memory-policy constraints can
            // still make an allocation fail while other host RAM is free.
            context.global = message.starts_with("oom-kill:constraint=CONSTRAINT_NONE,")
                && message.split(',').any(|s| s == "global_oom");
            context.memcg = message.starts_with("oom-kill:constraint=CONSTRAINT_MEMCG");
        }
        if let Some((_, rest)) = message.split_once(']') {
            let columns = rest.split_whitespace().collect::<Vec<_>>();
            if message.trim_start().starts_with("[  pid")
                || message.trim_start().starts_with("[ pid")
            {
                context.columns = columns
                    .iter()
                    .position(|s| *s == "rss")
                    .zip(columns.iter().position(|s| *s == "swapents"))
                    .zip(columns.iter().position(|s| *s == "name"))
                    .map(|((r, s), n)| (r, s, n));
            } else if let Some((r, s, n)) = context.columns {
                let pid = message
                    .split_once('[')
                    .and_then(|(_, s)| s.split_once(']'))
                    .and_then(|(s, _)| s.trim().parse::<u32>().ok());
                if let Some(pid) = pid.filter(|_| context.tasks.len() < 2048) {
                    let bytes = |index: usize| {
                        if valid_page {
                            columns
                                .get(index)
                                .and_then(|v| v.parse::<u64>().ok())
                                .and_then(|v| v.checked_mul(page_size))
                        } else {
                            None
                        }
                    };
                    if columns.len() > n {
                        context.tasks.insert(
                            pid,
                            ProcessMemory {
                                pid,
                                name: columns[n..].join(" ").chars().take(80).collect(),
                                rss_bytes: bytes(r),
                                swap_bytes: bytes(s),
                            },
                        );
                    }
                }
            }
        }
        let Some((pid, name)) = victim(message) else {
            continue;
        };
        let correlated = !boot.is_empty() && context.pid == Some(pid);
        let kind = if message.starts_with("Memory cgroup out of memory:")
            || (correlated && context.memcg)
        {
            "Cgroup memory limit"
        } else if correlated && context.global {
            "System-wide memory exhaustion"
        } else {
            "Kernel out-of-memory kill; scope unconfirmed"
        };
        let mut processes = if correlated {
            context.tasks.values().cloned().collect::<Vec<_>>()
        } else {
            vec![]
        };
        let swap = processes
            .iter()
            .find(|p| p.pid == pid)
            .and_then(|p| p.swap_bytes);
        processes.sort_by_key(|p| std::cmp::Reverse(p.rss_bytes.unwrap_or(0)));
        processes.truncate(5);
        let swap_valid = matches!((context.free,context.total),(Some(f),Some(t))if f<=t);
        output.push(Incident {
            time: (us / 1_000_000) as i64,
            time_us: us,
            boot_id: boot.into(),
            pid,
            name,
            kind: kind.into(),
            anonymous_bytes: number_bytes(message, "anon-rss:"),
            swap_bytes: swap,
            swap_free_bytes: if correlated && swap_valid {
                context.free
            } else {
                None
            },
            swap_total_bytes: if correlated && swap_valid {
                context.total
            } else {
                None
            },
            cgroup: if correlated {
                context.scope.clone()
            } else {
                None
            },
            processes,
            evidence: message.chars().take(4096).collect(),
            context_available: correlated && context.started,
        });
        context = Context {
            boot: boot.into(),
            start: us,
            ..Default::default()
        };
    }
    output
}
fn command(period: u32) -> Command {
    let mut command = Command::new("/usr/bin/journalctl");
    command.args([
        "--quiet",
        "--no-pager",
        "--output=json",
        "--output-fields=__REALTIME_TIMESTAMP,_BOOT_ID,_TRANSPORT,MESSAGE",
    ]);
    if period == 0 {
        command.arg("--boot");
    } else {
        command.args([
            "--since",
            match period {
                1 => "-1h",
                2 => "-24h",
                _ => "-7d",
            },
        ]);
    }
    command
}
pub fn load(period: u32, cancelled: &AtomicBool) -> Result<Vec<Incident>, String> {
    if cancelled.load(Ordering::Relaxed) {
        return Err("Memory incident query cancelled.".into());
    }
    #[cfg(feature = "docs-demo")]
    if std::env::var_os("TEND_DEMO_DIR").is_some() {
        let path = crate::workstation::home().join("demo-oom.jsonl");
        return match std::fs::read_to_string(path) {
            Ok(raw) => Ok(parse(&raw, 4096)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
            Err(e) => Err(e.to_string()),
        };
    }
    let mut query = command(period);
    query.args([
        "--lines=5",
        "--grep=(Out of memory|Memory cgroup out of memory): Killed process",
        "_TRANSPORT=kernel",
    ]);
    let raw = crate::queries::run_cancellable(
        &mut query,
        Duration::from_secs(10),
        512 * 1024,
        cancelled,
    )?;
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    let page = u64::try_from(page).unwrap_or(0);
    let markers = parse(&raw, page);
    let mut incidents = vec![];
    for marker in markers {
        if cancelled.load(Ordering::Relaxed) {
            return Err("Memory incident query cancelled.".into());
        }
        if marker.boot_id.is_empty() {
            incidents.push(marker);
            continue;
        }
        let mut context = Command::new("/usr/bin/journalctl");
        context.args([
            "--quiet",
            "--no-pager",
            "--output=json",
            "--output-fields=__REALTIME_TIMESTAMP,_BOOT_ID,_TRANSPORT,MESSAGE",
            "--lines=1024",
            "_TRANSPORT=kernel",
        ]);
        context.arg(format!("_BOOT_ID={}", marker.boot_id));
        context.arg(format!("--since=@{}", marker.time.saturating_sub(2)));
        context.arg(format!("--until=@{}", marker.time.saturating_add(1)));
        let found = crate::queries::run_cancellable(
            &mut context,
            Duration::from_secs(10),
            2 * 1024 * 1024,
            cancelled,
        )
        .ok()
        .and_then(|raw| {
            parse(&raw, page).into_iter().find(|e| {
                e.boot_id == marker.boot_id && e.time_us == marker.time_us && e.pid == marker.pid
            })
        });
        if cancelled.load(Ordering::Relaxed) {
            return Err("Memory incident query cancelled.".into());
        }
        incidents.push(found.unwrap_or(marker));
    }
    incidents.sort_by_key(|e| std::cmp::Reverse(e.time_us));
    Ok(incidents)
}
fn amount(bytes: Option<u64>) -> String {
    bytes
        .map(|n| {
            if n >= 1024 * 1024 * 1024 {
                format!("{:.2} GiB", n as f64 / 1073741824.)
            } else {
                crate::workstation::bytes(n)
            }
        })
        .unwrap_or_else(|| "Not recorded".into())
}
impl Incident {
    pub fn details(&self) -> String {
        let mut text=format!("{}\n{}\n\nKilled process: {} (PID {})\nAnonymous resident memory: {}\nSwapped memory: {}\nSwap free at the kill: {} of {}\nCgroup: {}\nBoot ID: {}\nEvent ID: {}:{}:{}\n",self.kind,crate::presentation::exact(self.time),self.name,self.pid,amount(self.anonymous_bytes),amount(self.swap_bytes),amount(self.swap_free_bytes),amount(self.swap_total_bytes),self.cgroup.as_deref().unwrap_or("Not recorded"),if self.boot_id.is_empty(){"Not recorded"}else{&self.boot_id},self.boot_id,self.time_us,self.pid);
        if self.kind == "Cgroup memory limit" {
            text.push_str("\nA cgroup OOM can occur while the host still has free RAM. The numeric historical limit is not supplied by this event.\n");
        }
        if matches!((self.swap_free_bytes,self.swap_total_bytes),(Some(f),Some(t))if t>0&&(f as f64)/(t as f64)<0.01)
        {
            text.push_str("\nSwap was almost completely exhausted at this event.\n");
        }
        if !self.processes.is_empty() {
            text.push_str(
                "\nLargest recorded resident sets at the event (shared pages can overlap):\n",
            );
            for p in &self.processes {
                text.push_str(&format!(
                    "{} · PID {} · RAM {} · swap {}\n",
                    p.name,
                    p.pid,
                    amount(p.rss_bytes),
                    amount(p.swap_bytes)
                ));
            }
        }
        if !self.context_available {
            text.push_str("\nKernel context is incomplete; missing figures are not zero.\n");
        }
        text.push_str("\nThe kernel selected this victim. This does not prove a memory leak or identify its parent application. A subprocess can exhaust memory and take down its terminal/session. No live process is identified by reusing this exited PID. Current free memory can recover after a kill and does not describe memory at the event.\n\nKernel evidence:\n");
        text.push_str(&self.evidence);
        text
    }
    pub fn entry(&self) -> crate::system_logs::Entry {
        crate::system_logs::Entry {
            time: self.time,
            source: format!("Out of memory · {} (PID {})", self.name, self.pid),
            priority: 3,
            message: format!(
                "{} · anonymous RAM {} · swap {}",
                self.kind,
                amount(self.anonymous_bytes),
                amount(self.swap_bytes)
            ),
            crash: true,
            details: self.details(),
        }
    }
}
pub fn report(period: u32) -> Result<String, String> {
    let incidents = load(period, &AtomicBool::new(false))?;
    let mut out=format!("Memory incident report · {} recorded kernel OOM kill{}\nRequested history only; at most five kills, bounded context. No stress test or settings change.\n",incidents.len(),if incidents.len()==1{""}else{"s"});
    if incidents.is_empty() {
        out.push_str("\nNo readable matching kernel OOM record was found. Rotated/protected logs, userspace OOM killers and application-internal heap errors may need other evidence; this is not proof that memory was healthy.\n");
    }
    for e in incidents {
        out.push_str("\n────────────────────────\n");
        out.push_str(&e.details());
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIXTURE: &str = include_str!("../tests/fixtures/opencode-kernel-oom.jsonl");

    fn changed(mut f: impl FnMut(&mut Value)) -> String {
        FIXTURE
            .lines()
            .map(|line| {
                let mut row: Value = serde_json::from_str(line).unwrap();
                f(&mut row);
                row.to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    fn only_kill() -> String {
        FIXTURE.lines().last().unwrap().into()
    }

    #[test]
    fn explains_recorded_opencode_terminal_oom_without_inventing_parentage() {
        let all = parse(FIXTURE, 4096);
        assert_eq!(all.len(), 1);
        let event = &all[0];
        assert_eq!((event.pid, event.name.as_str()), (6557, "node"));
        assert_eq!(event.kind, "System-wide memory exhaustion");
        assert_eq!(event.anonymous_bytes, Some(5_401_820 * 1024));
        assert_eq!(event.swap_bytes, Some(728_098 * 4096));
        assert_eq!(event.swap_free_bytes, Some(228 * 1024));
        assert_eq!(event.swap_total_bytes, Some(4_194_300 * 1024));
        assert!(event
            .cgroup
            .as_deref()
            .unwrap()
            .ends_with("/test-terminal.scope"));
        assert!(event.context_available);
        assert_eq!(event.processes.len(), 3);
        assert_eq!(event.processes[0].pid, 6557);
        let report = event.details();
        for text in [
            "5.15 GiB",
            "2.78 GiB",
            "Swap was almost completely exhausted",
            "does not prove a memory leak or identify its parent application",
            "Current free memory can recover",
        ] {
            assert!(report.contains(text), "Missing {text}");
        }
        let entry = event.entry();
        assert!(entry.crash);
        assert!(entry.source.contains("node (PID 6557)"));
        assert!(!entry.source.contains("gmain"));
    }

    #[test]
    fn repeated_real_event_retains_its_own_victim_and_exhausted_swap() {
        let repeated = include_str!("../tests/fixtures/opencode-kernel-oom-repeated.jsonl");
        let events = parse(&format!("{FIXTURE}\n{repeated}"), 4096);
        assert_eq!(events.len(), 2);
        let event = &events[1];
        assert_eq!(event.time_us, 1_793_238_294_994_549);
        assert_eq!((event.pid, event.name.as_str()), (46694, "node"));
        assert_eq!(event.kind, "System-wide memory exhaustion");
        assert_eq!(event.anonymous_bytes, Some(3_597_644 * 1024));
        assert_eq!(event.swap_bytes, Some(131_002 * 4096));
        assert_eq!(event.swap_free_bytes, Some(24 * 1024));
        assert_eq!(event.swap_total_bytes, Some(4_194_300 * 1024));
        assert_eq!(event.processes[0].pid, 46694);
        assert!(event.processes.iter().any(|p| p.name == "rustc"));
        assert!(!event.processes.iter().any(|p| p.pid == 6557));
        assert!(event.context_available);
        let details = event.details();
        assert!(details.contains("3.43 GiB"));
        assert!(details.contains("Swap was almost completely exhausted"));
        assert!(details.contains("does not prove a memory leak"));
        assert_ne!(
            crate::system_logs::identity(&events[0].entry()),
            crate::system_logs::identity(&event.entry())
        );
    }

    #[test]
    fn marker_without_context_reports_unknown_not_zero_or_healthy() {
        let event = parse(&only_kill(), 4096).remove(0);
        assert_eq!(event.anonymous_bytes, Some(5_401_820 * 1024));
        assert_eq!(event.swap_bytes, None);
        assert_eq!(event.swap_free_bytes, None);
        assert_eq!(event.cgroup, None);
        assert!(event.processes.is_empty());
        assert!(!event.context_available);
        assert!(event.kind.contains("scope unconfirmed"));
        assert!(event.details().contains("missing figures are not zero"));
    }

    #[test]
    fn accepts_only_trusted_kernel_victim_records() {
        assert!(parse(&changed(|r| r["_TRANSPORT"] = "stdout".into()), 4096).is_empty());
        assert!(parse(
            r#"{"MESSAGE":"Out of memory: Killed process 3 (node) total-vm:1kB"}"#,
            4096
        )
        .is_empty());
        let ordinary =
            changed(|r| r["MESSAGE"] = "node allocation failed: heap out of memory".into());
        assert!(parse(&ordinary, 4096).is_empty());
    }

    #[test]
    fn never_joins_another_pid_boot_or_distant_event() {
        for raw in [
            FIXTURE.replace("task=node,pid=6557,", "task=node,pid=9999,"),
            changed(|r| {
                if r["MESSAGE"].as_str().unwrap().starts_with("Out of memory:") {
                    r["_BOOT_ID"] = "22222222222222222222222222222222".into();
                }
            }),
            changed(|r| {
                if r["MESSAGE"].as_str().unwrap().starts_with("Out of memory:") {
                    r["__REALTIME_TIMESTAMP"] = "1793232055669642".into();
                }
            }),
            changed(|r| r["_BOOT_ID"] = "invalid".into()),
        ] {
            let event = parse(&raw, 4096).remove(0);
            assert_eq!(event.swap_bytes, None);
            assert_eq!(event.swap_free_bytes, None);
            assert_eq!(event.cgroup, None);
            assert!(event.processes.is_empty());
            assert!(!event.context_available);
        }
    }

    #[test]
    fn cgroup_exhaustion_is_distinct_from_host_exhaustion() {
        let raw = FIXTURE
            .replace("CONSTRAINT_NONE", "CONSTRAINT_MEMCG")
            .replace(",global_oom,", ",");
        let event = parse(&raw, 4096).remove(0);
        assert_eq!(event.kind, "Cgroup memory limit");
        assert!(event.details().contains("host still has free RAM"));
        let raw = only_kill().replace("Out of memory:", "Memory cgroup out of memory:");
        assert_eq!(parse(&raw, 4096)[0].kind, "Cgroup memory limit");
    }

    #[test]
    fn allocation_constraint_is_not_reported_as_whole_host_exhaustion() {
        let raw = FIXTURE.replace("CONSTRAINT_NONE", "CONSTRAINT_CPUSET");
        assert_ne!(parse(&raw, 4096)[0].kind, "System-wide memory exhaustion");
    }

    #[test]
    fn cancelled_request_never_starts_a_journal_query() {
        assert!(load(2, &AtomicBool::new(true))
            .unwrap_err()
            .contains("cancelled"));
    }

    #[test]
    fn invalid_page_size_and_overflow_do_not_invent_bytes() {
        for size in [0, 3, u64::MAX] {
            let event = parse(FIXTURE, size).remove(0);
            assert_eq!(event.swap_bytes, None);
            assert!(event.processes.iter().all(|p| p.rss_bytes.is_none()));
            assert_eq!(event.anonymous_bytes, Some(5_401_820 * 1024));
        }
        let raw = FIXTURE
            .replace("anon-rss:5401820kB", "anon-rss:18446744073709551615kB")
            .replace("728098", "18446744073709551615");
        let event = parse(&raw, 4096).remove(0);
        assert_eq!(event.anonymous_bytes, None);
        assert_eq!(event.swap_bytes, None);
        let raw = FIXTURE.replace("228kB", "999999999kB");
        assert_eq!(parse(&raw, 4096)[0].swap_free_bytes, None);
    }

    #[test]
    fn same_pid_is_not_enough_to_reuse_previous_kill_context() {
        let raw = format!("{FIXTURE}\n{}", only_kill());
        let events = parse(&raw, 4096);
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].swap_bytes, None);
        assert_eq!(events[1].cgroup, None);
    }

    #[test]
    fn event_identity_survives_context_completion_but_distinguishes_time_and_boot() {
        let full = parse(FIXTURE, 4096).remove(0).entry();
        let partial = parse(&only_kill(), 4096).remove(0).entry();
        assert_eq!(
            crate::system_logs::identity(&full),
            crate::system_logs::identity(&partial)
        );
        for raw in [
            only_kill().replace("1793232045669642", "1793232045669643"),
            only_kill().replace(
                "11111111111111111111111111111111",
                "22222222222222222222222222222222",
            ),
        ] {
            let other = parse(&raw, 4096).remove(0).entry();
            assert_ne!(
                crate::system_logs::identity(&full),
                crate::system_logs::identity(&other)
            );
        }
    }
}
