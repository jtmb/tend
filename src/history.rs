//! Private, quota-bounded local history. No file contents, credentials or
//! process environments are collected. Opening the store is explicit/lazy.
use crate::{observations::filesystem_free, workstation as w};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};
pub mod snapshots;
const MIB: u64 = 1024 * 1024;
const RESERVE: u64 = 8 * MIB;
const MAX_PAYLOAD: usize = 256 * 1024;
const PAGE_BYTES: usize = 2 * 1024 * 1024;
// The shared maintenance database and its DELETE journal together use at most
// 8 MiB. Reserve that within the configured total, even before integration.
pub const SYSTEM_HISTORY_RESERVE_MIB: u32 = 8;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub version: u32,
    pub max_mib: u32,
    pub retention_days: u32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            max_mib: 512,
            retention_days: 30,
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || !(128..=2048).contains(&self.max_mib)
            || !(1..=365).contains(&self.retention_days)
        {
            return Err(
                "History budget: 128–2048 MiB; retention: 1–365 days; settings version must be 1."
                    .into(),
            );
        }
        Ok(())
    }
    pub fn read() -> Result<Self, String> {
        let path = w::home().join(".config/tend/history.json");
        let value: Self = match fs::File::open(path) {
            Ok(file) => {
                let mut text = String::new();
                file.take(4097)
                    .read_to_string(&mut text)
                    .map_err(|e| e.to_string())?;
                if text.len() > 4096 {
                    return Err("History settings exceed 4 KiB.".into());
                }
                serde_json::from_str(&text).map_err(|e| format!("Invalid history settings: {e}"))?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => return Err(e.to_string()),
        };
        value.validate()?;
        Ok(value)
    }
    pub fn save(&self) -> Result<(), String> {
        self.validate()?;
        w::private_write(
            &w::home().join(".config/tend/history.json"),
            &serde_json::to_string_pretty(self).unwrap(),
        )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cursor {
    pub time_ms: i64,
    pub row: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub row: i64,
    pub id: String,
    pub kind: String,
    pub time_ms: i64,
    pub boot_id: String,
    pub payload: serde_json::Value,
}
pub struct Page {
    pub events: Vec<Event>,
    pub next: Option<Cursor>,
}
#[derive(Clone,Default)]
pub struct EventFilter {
    pub minimum_ms: i64,
    pub category: u8,
    pub outcome: u8,
    pub search: String,
}
pub struct History {
    db: Connection,
    path: PathBuf,
    max_bytes: u64,
    event_bytes: u64,
    retention_days: u32,
    min_free_bytes: u64,
}
fn error(e: impl std::fmt::Display) -> String {
    format!("History unavailable: {e}")
}
fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    value.into()
}
pub fn path() -> PathBuf {
    w::state_dir().join("history.sqlite3")
}
impl History {
    pub fn existing() -> Result<Option<Self>, String> {
        Self::existing_at(&path(), &Settings::read()?)
    }
    pub fn existing_at(path: &Path, settings: &Settings) -> Result<Option<Self>, String> {
        settings.validate()?;
        match fs::symlink_metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(error(e)),
            Ok(_) => {}
        }
        Self::check_file(path)?;
        let db = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .map_err(error)?;
        db.busy_timeout(Duration::from_millis(500)).map_err(error)?;
        let version: u32 = db
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(error)?;
        if !(1..=6).contains(&version) {
            return Err("Unsupported history version; existing data was not changed.".into());
        }
        Ok(Some(Self {
            db,
            path: path.into(),
            max_bytes: (settings.max_mib - SYSTEM_HISTORY_RESERVE_MIB) as u64 * MIB,
            event_bytes: 32 * MIB,
            retention_days: settings.retention_days,
            min_free_bytes: 2 * 1024 * MIB,
        }))
    }
    pub fn open() -> Result<Self, String> {
        let settings = Settings::read()?;
        Self::open_at(&path(), &settings)
    }
    pub fn open_at(path: &Path, settings: &Settings) -> Result<Self, String> {
        settings.validate()?;
        Self::open_limits(
            path,
            (settings.max_mib - SYSTEM_HISTORY_RESERVE_MIB) as u64 * MIB,
            32 * MIB,
            settings.retention_days,
            2 * 1024 * MIB,
        )
    }
    fn open_limits(
        path: &Path,
        max_bytes: u64,
        event_bytes: u64,
        retention_days: u32,
        min_free_bytes: u64,
    ) -> Result<Self, String> {
        if max_bytes <= RESERVE {
            return Err("History budget is too small.".into());
        }
        let parent = path.parent().ok_or("Missing history directory")?;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .map_err(error)?;
        let parent_meta = fs::symlink_metadata(parent).map_err(error)?;
        if parent_meta.file_type().is_symlink() || parent_meta.uid() != unsafe { libc::geteuid() } {
            return Err("History needs a user-owned, non-symlink directory.".into());
        }
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).map_err(error)?;
        if filesystem_free(parent)? < min_free_bytes {
            return Err("History not saved: less than 2 GiB free on its filesystem.".into());
        }
        let created = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
        {
            Ok(_) => true,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => false,
            Err(e) => return Err(error(e)),
        };
        Self::check_file(path)?;
        for suffix in ["-wal", "-shm"] {
            let p = sidecar(path, suffix);
            if p.exists() || p.is_symlink() {
                Self::check_file(&p)?;
            }
        }
        let db = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .map_err(error)?;
        db.busy_timeout(Duration::from_millis(500)).map_err(error)?;
        let version: u32 = db
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(error)?;
        if version > 6 {
            return Err("History was created by a newer Tend version; it was not changed.".into());
        }
        if version == 0 && !created {
            let count: i64 = db
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table'",
                    [],
                    |r| r.get(0),
                )
                .map_err(error)?;
            if count != 0 {
                return Err("Unrecognized history database; it was not replaced.".into());
            }
        }
        db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA mmap_size=0; PRAGMA cache_size=-4096; PRAGMA temp_store=FILE;").map_err(error)?;
        let page_size: i64 = db
            .pragma_query_value(None, "page_size", |r| r.get(0))
            .map_err(error)?;
        db.pragma_update(
            None,
            "max_page_count",
            ((max_bytes - RESERVE) / page_size.max(1) as u64) as i64,
        )
        .map_err(error)?;
        db.pragma_update(None, "journal_mode", "WAL")
            .map_err(error)?;
        db.pragma_update(None, "wal_autocheckpoint", 256)
            .map_err(error)?;
        db.pragma_update(None, "journal_size_limit", (4 * MIB) as i64)
            .map_err(error)?;
        if version == 0 {
            db.execute_batch("BEGIN IMMEDIATE;
                CREATE TABLE events(row INTEGER PRIMARY KEY, id TEXT NOT NULL UNIQUE, kind TEXT NOT NULL,
                    time_ms INTEGER NOT NULL, boot_id TEXT NOT NULL, payload TEXT NOT NULL);
                CREATE INDEX events_time ON events(time_ms DESC,row DESC);
                CREATE INDEX events_kind ON events(kind,time_ms DESC,row DESC);
                PRAGMA user_version=1; COMMIT;").map_err(error)?;
        }
        if version < 2 {
            db.execute_batch("BEGIN IMMEDIATE;
                CREATE TABLE snapshots(id INTEGER PRIMARY KEY AUTOINCREMENT,root BLOB NOT NULL,dev BLOB NOT NULL,ino BLOB NOT NULL,
                    started_ms INTEGER NOT NULL,complete INTEGER NOT NULL DEFAULT 0,files INTEGER NOT NULL DEFAULT 0,
                    skipped INTEGER NOT NULL DEFAULT 0,allocated INTEGER NOT NULL DEFAULT 0);
                CREATE INDEX snapshots_root ON snapshots(root,complete,started_ms DESC,id DESC);
                CREATE TABLE file_metadata(snapshot INTEGER NOT NULL,path BLOB NOT NULL,bucket BLOB NOT NULL,build BLOB,
                    dev BLOB NOT NULL,ino BLOB NOT NULL,size INTEGER NOT NULL,allocated INTEGER NOT NULL,
                    mtime INTEGER NOT NULL,nanos INTEGER NOT NULL,mode INTEGER NOT NULL,
                    PRIMARY KEY(snapshot,path)) WITHOUT ROWID;
                CREATE TABLE file_inodes(snapshot INTEGER NOT NULL,dev BLOB NOT NULL,ino BLOB NOT NULL,path BLOB NOT NULL,
                    allocated INTEGER NOT NULL,PRIMARY KEY(snapshot,dev,ino)) WITHOUT ROWID;
                CREATE TABLE file_changes(snapshot INTEGER NOT NULL,root BLOB NOT NULL,path BLOB NOT NULL,kind TEXT NOT NULL,
                    before_size INTEGER,after_size INTEGER,before_allocated INTEGER,after_allocated INTEGER,time_ms INTEGER NOT NULL,
                    PRIMARY KEY(snapshot,path)) WITHOUT ROWID;
                CREATE INDEX changes_root_time ON file_changes(root,time_ms DESC,snapshot DESC,path);
                PRAGMA user_version=2; COMMIT;").map_err(error)?;
        }
        if version < 3 {
            db.execute_batch(
                "BEGIN IMMEDIATE;
                CREATE INDEX IF NOT EXISTS metadata_bucket ON file_metadata(snapshot,bucket);
                CREATE INDEX IF NOT EXISTS metadata_build ON file_metadata(snapshot,build);
                PRAGMA user_version=3; COMMIT;",
            )
            .map_err(error)?;
        }
        if version < 4 {
            db.execute_batch(
                "BEGIN IMMEDIATE;
                CREATE UNIQUE INDEX IF NOT EXISTS inodes_path ON file_inodes(snapshot,path);
                PRAGMA user_version=4; COMMIT;",
            )
            .map_err(error)?;
        }
        if version < 5 {
            db.execute_batch(
                "BEGIN IMMEDIATE;
                ALTER TABLE snapshots ADD COLUMN filesystem_identity TEXT;
                PRAGMA user_version=5; COMMIT;",
            )
            .map_err(error)?;
        }
        if version < 6 {
            // complete=2 marks a published snapshot retired by an atomic
            // promotion; its physical rows can then be reclaimed in batches.
            db.pragma_update(None, "user_version", 6).map_err(error)?;
        }
        let store = Self {
            db,
            path: path.into(),
            max_bytes,
            event_bytes,
            retention_days,
            min_free_bytes,
        };
        store.ensure_space()?;
        Ok(store)
    }
    fn check_file(path: &Path) -> Result<(), String> {
        let m = fs::symlink_metadata(path).map_err(error)?;
        if !m.is_file()
            || m.file_type().is_symlink()
            || m.uid() != unsafe { libc::geteuid() }
            || m.permissions().mode() & 0o077 != 0
        {
            return Err(format!(
                "History file must be private, regular and user-owned: {}",
                path.display()
            ));
        }
        Ok(())
    }
    pub fn size_bytes(&self) -> u64 {
        ["", "-wal", "-shm"]
            .iter()
            .filter_map(|suffix| {
                fs::symlink_metadata(sidecar(&self.path, suffix))
                    .ok()
                    .filter(|m| m.file_type().is_file())
                    .map(|m| m.len())
            })
            .sum()
    }
    fn ensure_space(&self) -> Result<(), String> {
        if self.size_bytes() >= self.max_bytes.saturating_sub(RESERVE) {
            return Err("History budget reached; existing data was preserved.".into());
        }
        if filesystem_free(self.path.parent().unwrap())? < self.min_free_bytes {
            return Err("History not saved: less than 2 GiB free on its filesystem.".into());
        }
        Ok(())
    }
    pub fn insert(
        &mut self,
        id: &str,
        kind: &str,
        time_ms: i64,
        boot: &str,
        payload: &serde_json::Value,
    ) -> Result<bool, String> {
        if id.is_empty()
            || id.len() > 256
            || kind.is_empty()
            || kind.len() > 64
            || !kind
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_/".contains(&b))
            || boot.len() > 128
        {
            return Err("Invalid history event identity.".into());
        }
        let payload = serde_json::to_string(payload).map_err(error)?;
        let weight = (payload.len() + id.len() + kind.len() + boot.len() + 128) as u64;
        if payload.len() > MAX_PAYLOAD || weight > self.event_bytes {
            return Err("History event exceeds the bounded record size.".into());
        }
        self.ensure_space()?;
        let cutoff = (w::now().saturating_sub(self.retention_days as u64 * 86400) * 1000) as i64;
        if time_ms < cutoff {
            return Ok(false);
        }
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(error)?;
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM events WHERE id=?1)",
                [id],
                |r| r.get(0),
            )
            .map_err(error)?;
        if exists {
            return Ok(false);
        }
        tx.execute("DELETE FROM events WHERE time_ms < ?1", [cutoff])
            .map_err(error)?;
        let mut used: i64 = tx.query_row("SELECT COALESCE(SUM(length(CAST(payload AS BLOB))+length(CAST(id AS BLOB))+length(CAST(kind AS BLOB))+length(CAST(boot_id AS BLOB))+128),0) FROM events", [], |r| r.get(0)).map_err(error)?;
        while used.max(0) as u64 + weight > self.event_bytes {
            let old: Option<(i64,i64)> = tx.query_row("SELECT row,length(CAST(payload AS BLOB))+length(CAST(id AS BLOB))+length(CAST(kind AS BLOB))+length(CAST(boot_id AS BLOB))+128 FROM events ORDER BY time_ms,row LIMIT 1", [], |r| Ok((r.get(0)?,r.get(1)?))).optional().map_err(error)?;
            let Some((row, bytes)) = old else { break };
            tx.execute("DELETE FROM events WHERE row=?1", [row])
                .map_err(error)?;
            used = used.saturating_sub(bytes);
        }
        tx.execute(
            "INSERT INTO events(id,kind,time_ms,boot_id,payload) VALUES(?1,?2,?3,?4,?5)",
            params![id, kind, time_ms, boot, payload],
        )
        .map_err(error)?;
        tx.commit().map_err(error)?;
        self.db
            .execute_batch("PRAGMA wal_checkpoint(PASSIVE)")
            .map_err(error)?;
        Ok(true)
    }
    pub fn events(
        &self,
        limit: usize,
        before: Option<&Cursor>,
        kind: Option<&str>,
    ) -> Result<Page, String> {
        self.event_page(limit,before,kind,None)
    }
    pub fn reports(&self, limit: usize, before: Option<&Cursor>) -> Result<Page,String> {
        self.event_page(limit,before,None,Some("report/"))
    }
    pub fn filtered_events(&self, filter: &EventFilter, limit: usize, before: Option<&Cursor>) -> Result<Page,String> {
        if filter.category>4||filter.outcome>4||filter.search.chars().count()>240 {return Err("Invalid activity filter.".into());}
        let limit=limit.clamp(1,200);
        let mut statement=self.db.prepare("SELECT row,id,kind,time_ms,boot_id,payload FROM events
            WHERE time_ms>=?1 AND (?2 IS NULL OR time_ms<?2 OR(time_ms=?2 AND row<?3))
            AND (?4=0 OR (?4=1 AND kind='report/updates') OR (?4=2 AND kind IN('report/cleanup','report/custom-cleanup','report/scheduled-cleanup','report/daily-cleanup','report/npm-verify'))
                OR (?4=3 AND kind IN('diagnostic','health-alert','report/health-alert')) OR (?4=4 AND kind IN('report/settings','report/startup','report/service','report/graphics','report/memory','report/memory-restore','report/undo')))
            AND (?5=0 OR (?5=1 AND json_extract(payload,'$.success')=1) OR (?5=2 AND json_extract(payload,'$.success')=0) OR (?5=3 AND json_extract(payload,'$.status')='deferred') OR (?5=4 AND json_extract(payload,'$.status')='running'))
            AND (?6='' OR instr(lower(kind||' '||COALESCE(json_extract(payload,'$.report'),json_extract(payload,'$.workload'),'')),lower(?6))>0)
            ORDER BY time_ms DESC,row DESC LIMIT ?7").map_err(error)?;
        let rows=statement.query(params![filter.minimum_ms,before.map(|c|c.time_ms),before.map(|c|c.row),filter.category,filter.outcome,filter.search,(limit+1) as i64]).map_err(error)?;
        Self::read_event_page(rows,limit)
    }
    fn event_page(&self, limit: usize, before: Option<&Cursor>, kind: Option<&str>, prefix: Option<&str>) -> Result<Page,String> {
        let limit = limit.clamp(1, 200);
        let mut query = self.db.prepare("SELECT row,id,kind,time_ms,boot_id,payload FROM events
            WHERE (?1 IS NULL OR kind=?1) AND (?2 IS NULL OR time_ms < ?2 OR (time_ms=?2 AND row < ?3)) AND (?5 IS NULL OR kind GLOB (?5||'*'))
            ORDER BY time_ms DESC,row DESC LIMIT ?4").map_err(error)?;
        let rows = query
            .query(params![
                kind,
                before.map(|c| c.time_ms),
                before.map(|c| c.row),
                (limit + 1) as i64,
                prefix
            ])
            .map_err(error)?;
        Self::read_event_page(rows,limit)
    }
    fn read_event_page(mut rows: rusqlite::Rows<'_>,limit:usize)->Result<Page,String> {
        let mut events: Vec<Event> = Vec::new();
        let mut total = 0;
        let mut more = false;
        while let Some(row) = rows.next().map_err(error)? {
            let raw: String = row.get(5).map_err(error)?;
            if events.len() >= limit || (!events.is_empty() && total + raw.len() > PAGE_BYTES) {
                more = true;
                break;
            }
            total += raw.len();
            events.push(Event {
                row: row.get(0).map_err(error)?,
                id: row.get(1).map_err(error)?,
                kind: row.get(2).map_err(error)?,
                time_ms: row.get(3).map_err(error)?,
                boot_id: row.get(4).map_err(error)?,
                payload: serde_json::from_str(&raw).map_err(error)?,
            });
        }
        let next = if more {
            events.last().map(|e| Cursor {
                time_ms: e.time_ms,
                row: e.row,
            })
        } else {
            None
        };
        Ok(Page { events, next })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn activity_filters_preserve_outcomes_and_bound_search_parameters() {
        let path=fixture();
        let mut store=History::open_limits(&path,16*MIB,2*MIB,30,0).unwrap();
        let now=(w::now()*1000) as i64;
        for (i,kind,success,text) in [(0,"report/updates",true,"Fixture completed"),(1,"report/updates",false,"Fixture failed"),(2,"report/cleanup",true,"Éditeur cache")]
        {store.insert(&format!("fixture:{i}"),kind,now+i,"fixture",&serde_json::json!({"success":success,"report":text})).unwrap();}
        let page=store.filtered_events(&EventFilter {category:1,outcome:2,..Default::default()},200,None).unwrap();
        assert_eq!(page.events.len(),1);assert_eq!(page.events[0].id,"fixture:1");
        let page=store.filtered_events(&EventFilter {search:"Éditeur".into(),..Default::default()},200,None).unwrap();assert_eq!(page.events.len(),1);
        let page=store.filtered_events(&EventFilter {search:"' OR 1=1 --".into(),..Default::default()},200,None).unwrap();assert!(page.events.is_empty());
    }
    fn fixture() -> PathBuf {
        let path = PathBuf::from("/tmp/opencode").join(format!(
            "tend-history-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path.join("history.sqlite3")
    }
    #[test]
    fn private_quota_store_deduplicates_and_pages_stably() {
        let p = fixture();
        let mut store = History::open_limits(&p, 16 * MIB, 4096, 30, 0).unwrap();
        let time = (w::now() * 1000) as i64;
        for n in 0..12 {
            assert!(store
                .insert(
                    &format!("e{n}"),
                    "test",
                    time,
                    "boot",
                    &serde_json::json!({"n":n,"body":"x".repeat(800)})
                )
                .unwrap());
        }
        let page = store.events(2, None, None).unwrap();
        assert_eq!(page.events.len(), 2);
        assert!(page.next.is_some());
        let next = store.events(2, page.next.as_ref(), None).unwrap();
        assert_ne!(page.events[0].id, next.events[0].id);
        assert!(!store
            .insert("e11", "test", time, "boot", &serde_json::json!({}))
            .unwrap());
        assert!(store.events(200, None, None).unwrap().events.len() <= 4);
        assert_eq!(fs::metadata(p).unwrap().permissions().mode() & 0o777, 0o600);
        assert!(store.size_bytes() < 16 * MIB);
    }
    #[test]
    fn corrupt_or_symlink_store_is_not_overwritten() {
        let p = fixture();
        fs::write(&p, b"not a database").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(History::open_limits(&p, 16 * MIB, 4096, 30, 0).is_err());
        assert_eq!(fs::read(&p).unwrap(), b"not a database");
        let link = p.with_extension("link");
        std::os::unix::fs::symlink(&p, &link).unwrap();
        assert!(History::open_limits(&link, 16 * MIB, 4096, 30, 0).is_err());
    }
    #[test]
    fn expired_and_oversized_records_are_not_saved() {
        let p = fixture();
        let mut store = History::open_limits(&p, 16 * MIB, 4096, 30, 0).unwrap();
        assert!(!store
            .insert("old", "test", 0, "boot", &serde_json::json!({}))
            .unwrap());
        assert!(store
            .insert(
                "big",
                "test",
                (w::now() * 1000) as i64,
                "boot",
                &serde_json::json!({"s":"x".repeat(5000)})
            )
            .is_err());
        assert!(store.events(10, None, None).unwrap().events.is_empty());
    }
    #[test]
    fn settings_reject_unknown_fields_and_unbounded_values() {
        assert!(serde_json::from_str::<Settings>(r#"{"anything":true}"#).is_err());
        assert!(Settings {
            max_mib: 4096,
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(Settings::default().validate().is_ok());
    }
    #[test]
    fn inspecting_missing_history_creates_nothing() {
        let p = fixture();
        let absent = p.parent().unwrap().join("missing/history.sqlite3");
        assert!(History::existing_at(&absent, &Settings::default())
            .unwrap()
            .is_none());
        assert!(!absent.parent().unwrap().exists());
    }
}
