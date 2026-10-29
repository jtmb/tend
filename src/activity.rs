//! Requested, bounded incident/maintenance history. Read failures stay visible;
//! proximity in this timeline is never presented as proof of a cause.
use crate::{
    history::{self, EventFilter, History},
    workstation as w,
};
use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::Duration,
};
const PAGE: usize = 200;
const ROOT_DATABASE: &str = "/var/lib/dev-maintenance/tend-history.sqlite3";
const ROOT_BYTES: u64 = 4 * 1024 * 1024;
const APPLICATION_ID: i64 = 0x54454e4d;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Query {
    pub source: u8,   // all, daily, Tend, crashes
    pub category: u8, // all, updates, cleanup, pressure, settings, crashes
    pub outcome: u8,  // all, success, failed, deferred, awaiting
    pub seconds: u64,
    pub search: String,
    pub include_crashes: bool,
}
impl Query {
    pub fn validate(&self) -> Result<(), String> {
        if self.source > 3
            || self.category > 5
            || self.outcome > 4
            || self.seconds > 366 * 86400
            || self.search.chars().count() > 240
            || self.search.chars().any(char::is_control)
        {
            return Err(
                "Invalid activity filters; search may contain up to 240 characters.".into(),
            );
        }
        Ok(())
    }
    fn minimum(&self) -> i64 {
        if self.seconds == 0 {
            0
        } else {
            (w::now().saturating_sub(self.seconds) * 1000) as i64
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Cursor {
    pub time_ms: i64,
    pub source: u8,
    pub row: i64,
}
#[derive(Clone, Debug, Serialize)]
pub struct Event {
    pub cursor: Cursor,
    pub id: String,
    pub category: String,
    pub source: String,
    pub title: String,
    pub outcome: String,
    pub summary: String,
    pub details: String,
}
#[derive(Default, Serialize)]
pub struct Page {
    pub events: Vec<Event>,
    pub next: Option<Cursor>,
    pub warnings: Vec<String>,
}
fn bounded(text: &str, max: usize) -> String {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}{}",
        &text[..end],
        if end < text.len() { "…" } else { "" }
    )
}
fn source_cursor(before: Option<&Cursor>, source: u8) -> Option<history::Cursor> {
    before.map(|c| history::Cursor {
        time_ms: c.time_ms,
        row: if source > c.source {
            i64::MIN
        } else if source < c.source {
            i64::MAX
        } else {
            c.row
        },
    })
}
fn title(kind: &str) -> &str {
    match kind {
        "updates" => "Stable updates",
        "apt-cache" => "APT download cache",
        "docker-build" => "Docker build cache",
        "docker-images" => "Dangling Docker images",
        "npm-logs" => "npm diagnostic logs",
        "npm-cache" | "npm-verify" => "npm cache verification",
        "health" => "Health report",
        "cleanup" | "custom-cleanup" => "Manual cleanup",
        "scheduled-cleanup" => "Scheduled user cleanup",
        "daily-cleanup" => "Daily cleanup worker",
        "memory" => "Memory settings",
        "memory-restore" => "Memory settings restored",
        "settings" => "Desktop settings",
        "startup" => "Login application settings",
        "graphics" => "Graphics launcher settings",
        "service" => "User service settings",
        "undo" => "Setting restored",
        "storage" => "Storage scan",
        "diagnostic" => "Diagnostic recording",
        "health-alert" => "Health alert",
        _ => "Saved Tend report",
    }
}
fn category(kind: &str) -> &str {
    match kind {
        "updates" => "Updates",
        "apt-cache" | "docker-build" | "docker-images" | "npm-logs" | "npm-cache"
        | "npm-verify" | "cleanup" | "custom-cleanup" | "scheduled-cleanup" | "daily-cleanup" => {
            "Cleanup"
        }
        "diagnostic" | "health-alert" => "Pressure",
        "settings" | "startup" | "graphics" | "service" | "memory" | "memory-restore" | "undo" => {
            "Settings"
        }
        _ => "Maintenance",
    }
}
fn private_event(event: history::Event) -> Event {
    let kind = event.kind.strip_prefix("report/").unwrap_or(&event.kind);
    let outcome = match event.payload["success"].as_bool() {
        Some(true) => "Success",
        Some(false) => "Failed",
        None => "Recorded",
    };
    let report = if kind == "diagnostic" {
        serde_json::from_value::<crate::diagnostics::Session>(event.payload.clone())
            .map(|s| s.summary())
            .unwrap_or_else(|_| "Diagnostic details could not be decoded.".into())
    } else {
        event.payload["report"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| bounded(&event.payload.to_string(), 32 * 1024))
    };
    let summary = if kind == "diagnostic" {
        event.payload["workload"]
            .as_str()
            .unwrap_or("Recorded workload")
            .to_owned()
    } else {
        report.lines().next().unwrap_or("Saved report").to_owned()
    };
    let mut details = format!(
        "{}\n{}\nSource: Tend\nResult: {outcome}\n\n{}",
        title(kind),
        crate::presentation::exact(event.time_ms / 1000),
        bounded(&report, 32 * 1024)
    );
    if event.payload["truncated"] == true {
        details.push_str("\n\nReport was truncated to the saved-history limit. Explicit exports can contain the full original report.");
    }
    Event {
        cursor: Cursor {
            time_ms: event.time_ms,
            source: 1,
            row: event.row,
        },
        id: event.id,
        category: category(kind).into(),
        source: "Tend".into(),
        title: title(kind).into(),
        outcome: outcome.into(),
        summary: bounded(&summary, 240),
        details,
    }
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Maintenance {
    version: u32,
    id: String,
    phase: String,
    window: String,
    attempt: u8,
    started_ms: i64,
    finished_ms: Option<i64>,
    duration_ms: Option<u64>,
    status: String,
    message: String,
    boot_id: String,
    scheduled_ms: i64,
    catch_up_seconds: u64,
    free_before: u64,
    free_after: Option<u64>,
    free_delta_bytes: Option<i64>,
    clock_changed: bool,
}
fn maintenance_event(row: i64, time_ms: i64, raw: &str, pending: bool) -> Result<Event, String> {
    if raw.len() > 8192 {
        return Err("Maintenance record exceeds 8 KiB.".into());
    }
    let record: Maintenance =
        serde_json::from_str(raw).map_err(|e| format!("Invalid maintenance record: {e}"))?;
    if record.version != 1
        || record.id.len() != 32
        || !record.id.bytes().all(|b| b.is_ascii_hexdigit())
        || !(1..=3).contains(&record.attempt)
        || record.message.len() > 2048
        || record.boot_id.len() > 128
        || !matches!(
            record.phase.as_str(),
            "updates"
                | "apt-cache"
                | "docker-build"
                | "docker-images"
                | "npm-logs"
                | "npm-cache"
                | "health"
        )
        || !matches!(
            record.status.as_str(),
            "success" | "failed" | "deferred" | "running"
        )
        || record.window.len() != 10
        || crate::presentation::date(&record.window) == "Unavailable"
    {
        return Err("Unsupported maintenance record; it was preserved.".into());
    }
    let outcome = if pending {
        "Awaiting result"
    } else {
        match record.status.as_str() {
            "success" => "Success",
            "failed" => "Failed",
            "deferred" => "Deferred",
            _ => "Started",
        }
    };
    let duration = record
        .duration_ms
        .map(|n| format!("{:.1} s", n as f64 / 1000.))
        .unwrap_or_else(|| "Not recorded".into());
    let net = record
        .free_delta_bytes
        .map(|n| {
            format!(
                "{}{}",
                if n < 0 { "−" } else { "+" },
                w::bytes(n.unsigned_abs())
            )
        })
        .unwrap_or_else(|| "Not recorded".into());
    let details=format!("{}\nSource: Daily maintenance\nMaintenance date: {}\nResult: {outcome}\nAttempt {} of 3\nScheduled: {}\nStarted: {}\nFinished: {}\nElapsed: {duration}\nDelay from schedule: {:.1} minutes\nRoot free before: {}\nRoot free after: {}\nNet free-space change: {net}\n\n{}\n\n{}Net free-space changes include other processes. Timing and nearby incidents do not establish a cause.",
        title(&record.phase),crate::presentation::date(&record.window),record.attempt,crate::presentation::exact(record.scheduled_ms/1000),crate::presentation::exact(record.started_ms/1000),record.finished_ms.map(|n|crate::presentation::exact(n/1000)).unwrap_or_else(||"No outcome recorded; phase may be active or interrupted".into()),record.catch_up_seconds as f64/60.,w::bytes(record.free_before),record.free_after.map(w::bytes).unwrap_or_else(||"Not recorded".into()),record.message,if record.clock_changed {"Clock changed during this phase; elapsed time uses the boot clock.\n"}else{""});
    Ok(Event {
        cursor: Cursor {
            time_ms,
            source: 2,
            row,
        },
        id: format!("maintenance:{}:{}", record.id, record.status),
        category: category(&record.phase).into(),
        source: "Daily maintenance".into(),
        title: title(&record.phase).into(),
        outcome: outcome.into(),
        summary: format!(
            "Attempt {} · {duration} · {}",
            record.attempt,
            bounded(&record.message, 160)
        ),
        details,
    })
}
fn root_location() -> (PathBuf, u32) {
    #[cfg(feature = "docs-demo")]
    if std::env::var_os("TEND_DEMO_DIR").is_some() {
        return (w::state_dir().join("demo-maintenance.sqlite3"), unsafe {
            libc::geteuid()
        });
    }
    (PathBuf::from(ROOT_DATABASE), 0)
}
fn root_events(
    path: &Path,
    owner: u32,
    query: &Query,
    before: Option<&Cursor>,
) -> Result<(Vec<Event>, bool), String> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((vec![], false)),
        Err(e) => return Err(e.to_string()),
        Ok(m) => {
            if !m.is_file()
                || m.file_type().is_symlink()
                || m.uid() != owner
                || m.mode() & 0o022 != 0
                || m.len() > ROOT_BYTES
            {
                return Err(
                    "Maintenance history file is not protected or exceeds its budget.".into(),
                );
            }
        }
    }
    for parent in path.parent().into_iter().flat_map(Path::ancestors) {
        let m = fs::symlink_metadata(parent).map_err(|e| e.to_string())?;
        if !m.is_dir() || m.file_type().is_symlink() || m.uid() != owner || m.mode() & 0o022 != 0 {
            return Err("Maintenance history directory is not protected.".into());
        }
        if owner != 0 {
            break;
        }
    }
    let db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(|e| e.to_string())?;
    db.busy_timeout(Duration::from_millis(250))
        .map_err(|e| e.to_string())?;
    db.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA mmap_size=0; PRAGMA cache_size=-512;")
        .map_err(|e| e.to_string())?;
    if db
        .pragma_query_value::<i64, _>(None, "application_id", |r| r.get(0))
        .map_err(|e| e.to_string())?
        != APPLICATION_ID
        || db
            .pragma_query_value::<u32, _>(None, "user_version", |r| r.get(0))
            .map_err(|e| e.to_string())?
            != 1
    {
        return Err("Unsupported maintenance history; it was not changed.".into());
    }
    let cursor = source_cursor(before, 2);
    let mut statement=db.prepare("SELECT e.row,e.time_ms,e.payload,(e.kind='started' AND e.id=r.token||':started' AND r.status='running') pending
        FROM events e LEFT JOIN receipts r ON e.window=r.window AND e.phase=r.phase
        WHERE e.time_ms>=?1 AND (?2 IS NULL OR e.time_ms<?2 OR(e.time_ms=?2 AND e.row<?3))
        AND (e.kind='finished' OR (e.kind='started' AND e.id=r.token||':started' AND r.status='running'))
        AND (?4=0 OR (?4=1 AND e.phase='updates') OR (?4=2 AND e.phase IN('apt-cache','docker-build','docker-images','npm-logs','npm-cache')))
        AND (?5=0 OR (?5=1 AND e.kind='finished' AND json_extract(e.payload,'$.status')='success') OR (?5=2 AND e.kind='finished' AND json_extract(e.payload,'$.status')='failed') OR (?5=3 AND e.kind='finished' AND json_extract(e.payload,'$.status')='deferred') OR (?5=4 AND e.kind='started' AND e.id=r.token||':started' AND r.status='running'))
        AND (?6='' OR instr(lower(e.phase||' '||json_extract(e.payload,'$.message')),lower(?6))>0)
        ORDER BY e.time_ms DESC,e.row DESC LIMIT 201").map_err(|e|e.to_string())?;
    let minimum = query
        .minimum()
        .max((w::now().saturating_sub(30 * 86400) * 1000) as i64);
    let mut rows = statement
        .query(params![
            minimum,
            cursor.as_ref().map(|c| c.time_ms),
            cursor.as_ref().map(|c| c.row),
            query.category,
            query.outcome,
            query.search
        ])
        .map_err(|e| e.to_string())?;
    let mut events = Vec::new();
    let mut more = false;
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        if events.len() == PAGE {
            more = true;
            break;
        }
        events.push(maintenance_event(
            row.get(0).map_err(|e| e.to_string())?,
            row.get(1).map_err(|e| e.to_string())?,
            &row.get::<_, String>(2).map_err(|e| e.to_string())?,
            row.get(3).map_err(|e| e.to_string())?,
        )?);
    }
    Ok((events, more))
}
fn merge(mut sources: Vec<(Vec<Event>, bool)>, warnings: Vec<String>) -> Page {
    // A byte-limited source may return fewer than 200 rows. Do not emit older
    // rows from other sources until that source has advanced beyond them.
    let frontier = sources
        .iter()
        .filter(|(_, more)| *more)
        .filter_map(|(items, _)| items.last().map(|e| e.cursor))
        .max();
    let mut more = sources.iter().any(|(_, more)| *more);
    let mut events: Vec<_> = sources.drain(..).flat_map(|(items, _)| items).collect();
    events.sort_by_key(|e| std::cmp::Reverse(e.cursor));
    if let Some(frontier) = frontier {
        events.retain(|e| e.cursor >= frontier);
    }
    more |= events.len() > PAGE;
    events.truncate(PAGE);
    let next = if more {
        events.last().map(|e| e.cursor)
    } else {
        None
    };
    Page {
        events,
        next,
        warnings,
    }
}
pub fn load(query: &Query, before: Option<&Cursor>) -> Result<Page, String> {
    query.validate()?;
    let mut sources = vec![];
    let mut warnings = vec![];
    if matches!(query.source, 0 | 2) && query.category != 5 {
        let result = (|| {
            let Some(store) = History::existing()? else {
                return Ok((vec![], false));
            };
            let retention = history::Settings::read()?.retention_days as u64;
            let filter = EventFilter {
                minimum_ms: query
                    .minimum()
                    .max((w::now().saturating_sub(retention * 86400) * 1000) as i64),
                category: query.category,
                outcome: query.outcome,
                search: query.search.clone(),
            };
            let page = store.filtered_events(&filter, PAGE, source_cursor(before, 1).as_ref())?;
            Ok::<_, String>((
                page.events.into_iter().map(private_event).collect(),
                page.next.is_some(),
            ))
        })();
        match result {
            Ok(page) => sources.push(page),
            Err(e) => warnings.push(format!("Tend history unavailable: {e}")),
        }
    }
    if matches!(query.source, 0 | 1) && matches!(query.category, 0 | 1 | 2) {
        let (path, owner) = root_location();
        if !path.exists() {
            warnings.push("Detailed daily history has not been recorded here yet. The maintenance journal can contain earlier runs.".into());
        }
        match root_events(&path, owner, query, before) {
            Ok(page) => sources.push(page),
            Err(e) => warnings.push(format!("Daily maintenance history unavailable: {e}")),
        }
    }
    if matches!(query.source, 0 | 3)
        && matches!(query.category, 0 | 3 | 5)
        && matches!(query.outcome, 0 | 2)
        && query.include_crashes
    {
        let period = if query.seconds > 0 && query.seconds <= 3600 {
            1
        } else if query.seconds > 0 && query.seconds <= 86400 {
            2
        } else {
            3
        };
        let crash = (|| {
            let raw = crate::system_logs::load(2, period, 1000)?;
            let entries: Vec<crate::system_logs::Entry> =
                serde_json::from_str(&raw).map_err(|e| e.to_string())?;
            let mut events = vec![];
            let search = query.search.to_lowercase();
            let mut seen = std::collections::HashSet::new();
            for entry in entries {
                let oom=entry.source.starts_with("Out of memory · ");
                if (query.category==3&&!oom)||(query.category==5&&oom){continue;}
                if entry.time.saturating_mul(1000) < query.minimum()
                    || (!search.is_empty()
                        && !format!("{} {} {}", entry.source, entry.message, entry.details)
                            .to_lowercase()
                            .contains(&search))
                {
                    continue;
                }
                let identity = crate::system_logs::identity(&entry);
                let mut hash = 14695981039346656037u64;
                for byte in identity.bytes() {
                    hash = (hash ^ byte as u64).wrapping_mul(1099511628211);
                }
                let cursor = Cursor {
                    time_ms: entry.time.saturating_mul(1000),
                    source: 0,
                    row: (hash & i64::MAX as u64) as i64,
                };
                if before.is_some_and(|c| cursor >= *c) || !seen.insert(identity) {
                    continue;
                }
                events.push(Event {cursor,id:format!("crash:{hash:x}"),category:if oom{"Memory pressure"}else{"Crashes"}.into(),source:"System logs".into(),title:bounded(&entry.source,240),outcome:if oom{"OOM kill"}else{"Crash report"}.into(),summary:bounded(&entry.message,240),details:format!("{}\n{}\n\n{}\n\n{}",entry.source,crate::presentation::exact(entry.time),bounded(&entry.details,16*1024),if oom{"Recorded kernel memory kill; this alone does not identify an application leak."}else{"A crash report is evidence of a crash, not proof of the cause of other slowdowns. Report file timestamps can describe an updated report."})});
            }
            events.sort_by_key(|e| std::cmp::Reverse(e.cursor));
            let more = events.len() > PAGE;
            events.truncate(PAGE);
            Ok::<_, String>((events, more))
        })();
        match crash {
            Ok(page) => sources.push(page),
            Err(e) => warnings.push(format!("Crash reports unavailable: {e}")),
        }
        warnings.push("Incident queries cover up to 1,000 crash journal entries, 100 readable report headers and five kernel OOM kills from the last seven days, with bounded context. Protected or rotated records may be absent.".into());
    }
    Ok(merge(sources, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(time_ms: i64, source: u8, row: i64) -> Event {
        Event {
            cursor: Cursor {
                time_ms,
                source,
                row,
            },
            id: row.to_string(),
            category: "Test".into(),
            source: "Fixture".into(),
            title: "Fixture".into(),
            outcome: "Recorded".into(),
            summary: String::new(),
            details: String::new(),
        }
    }
    #[test]
    fn merged_pages_do_not_skip_unseen_records_from_a_byte_limited_source() {
        let page = merge(
            vec![
                (vec![event(500, 1, 3), event(400, 1, 2)], true),
                (vec![event(450, 2, 2), event(100, 2, 1)], false),
            ],
            vec![],
        );
        assert_eq!(
            page.events
                .iter()
                .map(|e| e.cursor.time_ms)
                .collect::<Vec<_>>(),
            [500, 450, 400]
        );
        assert_eq!(page.next.unwrap().time_ms, 400);
        let next = merge(
            vec![
                (vec![event(300, 1, 1)], false),
                (vec![event(100, 2, 1)], false),
            ],
            vec![],
        );
        assert_eq!(next.events.len(), 2);
        assert!(next.next.is_none());
    }
    #[test]
    fn source_cursors_preserve_order_for_equal_timestamps() {
        let cursor = Cursor {
            time_ms: 1000,
            source: 1,
            row: 8,
        };
        assert_eq!(source_cursor(Some(&cursor), 2).unwrap().row, i64::MIN);
        assert_eq!(source_cursor(Some(&cursor), 1).unwrap().row, 8);
        assert_eq!(source_cursor(Some(&cursor), 0).unwrap().row, i64::MAX);
    }
    #[test]
    fn filters_reject_unbounded_or_unknown_inputs() {
        assert!(Query {
            search: "x".repeat(241),
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(Query {
            source: 4,
            ..Default::default()
        }
        .validate()
        .is_err());
    }
    #[test]
    fn root_history_filters_and_pages_without_writes_or_false_running_results() {
        use std::os::unix::fs::PermissionsExt;
        let root = PathBuf::from("/tmp/opencode").join(format!(
            "tend-activity-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let path = root.join("maintenance.sqlite3");
        let db = Connection::open(&path).unwrap();
        db.execute_batch(&format!("CREATE TABLE events(row INTEGER PRIMARY KEY,id TEXT,time_ms INTEGER,phase TEXT,window TEXT,attempt INTEGER,kind TEXT,payload TEXT);CREATE TABLE receipts(window TEXT,phase TEXT,token TEXT,status TEXT,PRIMARY KEY(window,phase)); PRAGMA application_id={APPLICATION_ID}; PRAGMA user_version=1;")).unwrap();
        let now = (w::now() * 1000) as i64;
        let phases = [
            "updates",
            "apt-cache",
            "docker-build",
            "docker-images",
            "npm-logs",
            "npm-cache",
            "health",
        ];
        for i in 0..410i64 {
            let time = now - (i / 14) * 86400000 - i;
            let window = gtk::glib::DateTime::from_unix_utc(time / 1000)
                .unwrap()
                .format("%F")
                .unwrap()
                .to_string();
            let phase = phases[((i / 2) % 7) as usize];
            let attempt = i % 2 + 1;
            let status = if attempt == 1 { "failed" } else { "success" };
            let raw = serde_json::json!({"version":1,"id":format!("{i:032x}"),"phase":phase,"window":window,"attempt":attempt,"started_ms":time-1000,"finished_ms":time,"duration_ms":1000,"status":status,"message":format!("Fixture outcome {i}"),"boot_id":"fixture","scheduled_ms":time-2000,"catch_up_seconds":1,"free_before":9000000000u64,"free_after":9000001024u64,"free_delta_bytes":1024,"clock_changed":false});
            db.execute(
                "INSERT INTO events VALUES(?1,?2,?3,?4,?5,?6,'finished',?7)",
                params![
                    i + 1,
                    format!("{i:032x}:finished"),
                    time,
                    phase,
                    window,
                    attempt,
                    raw.to_string()
                ],
            )
            .unwrap();
        }
        drop(db);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        let original = fs::read(&path).unwrap();
        let uid = unsafe { libc::geteuid() };
        let query = Query {
            source: 1,
            ..Default::default()
        };
        let mut before = None;
        let mut ids = std::collections::HashSet::new();
        loop {
            let (page, more) = root_events(&path, uid, &query, before.as_ref()).unwrap();
            assert!(!page.is_empty());
            for e in &page {
                assert!(ids.insert(e.id.clone()));
                assert!(e.details.contains("Net free-space change"));
                assert_ne!(e.outcome, "Awaiting result");
            }
            if !more {
                break;
            }
            before = page.last().map(|e| e.cursor);
        }
        assert_eq!(ids.len(), 410);
        let filtered = Query {
            category: 1,
            outcome: 1,
            ..query.clone()
        };
        let (page, more) = root_events(&path, uid, &filtered, None).unwrap();
        assert!(!more);
        assert_eq!(page.len(), 30);
        assert!(page
            .iter()
            .all(|e| e.title == "Stable updates" && e.outcome == "Success"));
        let (page, _) = root_events(
            &path,
            uid,
            &Query {
                search: "' OR 1=1 --".into(),
                ..query
            },
            None,
        )
        .unwrap();
        assert!(page.is_empty());
        assert_eq!(fs::read(&path).unwrap(), original);
    }
}
