//! Requested metadata-only snapshots. Directory descriptors anchor traversal;
//! symlinks are recorded but never followed. Partial scans never replace a
//! complete baseline. Batches and query pages have fixed memory bounds.
use super::{error, History};
use crate::workstation::{self as w, FileSize, Scan};
use fs2::FileExt;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    ffi::{CString, OsString},
    fs::{self, File, OpenOptions},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::{OsStrExt, OsStringExt},
            fs::{MetadataExt, OpenOptionsExt},
        },
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
};
const BATCH: usize = 64;
const MAX_ENTRIES: u64 = 1_000_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Outcome {
    pub scan: Scan,
    pub snapshot_id: Option<i64>,
    pub baseline_id: Option<i64>,
    pub note: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Change {
    pub snapshot: i64,
    pub time_ms: i64,
    pub path: PathBuf,
    pub kind: String,
    pub before_size: Option<u64>,
    pub after_size: Option<u64>,
    pub before_allocated: Option<u64>,
    pub after_allocated: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChangeCursor {
    pub snapshot: i64,
    pub time_ms: i64,
    pub relative: Vec<u8>,
}
pub struct ChangePage {
    pub changes: Vec<Change>,
    pub next: Option<ChangeCursor>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Summary {
    pub time_ms: i64,
    pub files: u64,
    pub allocated: u64,
}
pub fn latest(root: &Path) -> Result<Option<Summary>, String> {
    let Some(store) = History::existing()? else {
        return Ok(None);
    };
    latest_with(&store,root)
}
fn latest_with(store:&History,root:&Path)->Result<Option<Summary>,String>{
    let version: u32 = store
        .db
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(error)?;
    if version < 2 {
        return Ok(None);
    }
    store.db.query_row("SELECT started_ms,files,allocated FROM snapshots WHERE root=?1 AND complete=1 ORDER BY id DESC LIMIT 1",[root.as_os_str().as_bytes()],|r|Ok(Summary {time_ms:r.get(0)?,files:r.get::<_,i64>(1)?.max(0) as u64,allocated:r.get::<_,i64>(2)?.max(0) as u64})).optional().map_err(error)
}
struct Entry {
    path: Vec<u8>,
    bucket: Vec<u8>,
    build: Option<Vec<u8>>,
    dev: Vec<u8>,
    ino: Vec<u8>,
    size: i64,
    allocated: i64,
    mtime: i64,
    nanos: i64,
    mode: u32,
}
fn integer(n: u64) -> Result<i64, String> {
    i64::try_from(n).map_err(|_| "Filesystem metadata exceeds the supported integer range.".into())
}
fn stat(fd: i32) -> Result<libc::stat, String> {
    let mut value = unsafe { std::mem::zeroed::<libc::stat>() };
    if unsafe { libc::fstat(fd, &mut value) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(value)
}
fn anchored_stat(fd: i32, name: &CString) -> Result<libc::stat, String> {
    let mut value = unsafe { std::mem::zeroed::<libc::stat>() };
    if unsafe { libc::fstatat(fd, name.as_ptr(), &mut value, libc::AT_SYMLINK_NOFOLLOW) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(value)
}
fn directory(fd: i32, name: &CString) -> Result<File, String> {
    let value = unsafe {
        libc::openat(
            fd,
            name.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        )
    };
    if value < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(unsafe { File::from_raw_fd(value) })
}
fn root_directory(path: &CString) -> Result<File, String> {
    #[repr(C)]
    struct How {
        flags: u64,
        mode: u64,
        resolve: u64,
    }
    // RESOLVE_NO_SYMLINKS also protects ancestor components against replacement
    // between canonicalization and opening. Linux 5.6+ supports openat2.
    let how = How {
        flags: (libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY) as u64,
        mode: 0,
        resolve: 0x04,
    };
    let fd = unsafe {
        libc::syscall(
            libc::SYS_openat2,
            libc::AT_FDCWD,
            path.as_ptr(),
            &how,
            std::mem::size_of::<How>(),
        )
    };
    if fd < 0 {
        return Err(format!(
            "Cannot safely open scan root: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(unsafe { File::from_raw_fd(fd as i32) })
}
fn filesystem_identity(dir: &File, dev: u64) -> Result<String, String> {
    use std::os::unix::fs::FileTypeExt;
    if let Ok(entries) = fs::read_dir("/dev/disk/by-uuid") {
        for entry in entries.flatten().take(4096) {
            if fs::metadata(entry.path())
                .is_ok_and(|m| m.file_type().is_block_device() && m.rdev() == dev)
            {
                return Ok(format!("uuid:{}", entry.file_name().to_string_lossy()));
            }
        }
    }
    let mut info = unsafe { std::mem::zeroed::<libc::statfs>() };
    if unsafe { libc::fstatfs(dir.as_raw_fd(), &mut info) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    // Without a verified persistent UUID, establish a fresh baseline on each
    // boot. This prevents recycled device/inode numbers implying deletions.
    let fsid = unsafe {
        std::slice::from_raw_parts(
            std::ptr::addr_of!(info.f_fsid).cast::<u8>(),
            std::mem::size_of_val(&info.f_fsid),
        )
    };
    if crate::observations::boot_id() == "unknown" {
        return Err("A reliable filesystem or boot identity is unavailable; the previous baseline was preserved.".into());
    }
    Ok(format!(
        "boot:{}:device:{dev}:fsid:{}",
        crate::observations::boot_id(),
        fsid.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}
struct ScanLock(File);
impl Drop for ScanLock {
    fn drop(&mut self) {
        // A concurrent command launch may briefly inherit the descriptor before
        // exec closes it. Release explicitly, rather than waiting for every
        // inherited copy to close after this scan has already finished.
        let _ = FileExt::unlock(&self.0);
    }
}
fn lock(store: &History) -> Result<ScanLock, String> {
    let path = store.path.with_file_name("scan.lock");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
        .map_err(error)?;
    let meta = file.metadata().map_err(error)?;
    if !meta.is_file() || meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o077 != 0 {
        return Err("Scan lock must be a private, user-owned regular file.".into());
    }
    file.try_lock_exclusive().map_err(|_| {
        "Another storage-history scan is running. Wait or cancel it first.".to_string()
    })?;
    Ok(ScanLock(file))
}

struct Writer<'a> {
    store: &'a mut History,
    id: i64,
    baseline: Option<i64>,
    root: PathBuf,
    dev: u64,
    ino: u64,
    batch: Vec<Entry>,
    batch_weight: usize,
    visited: u64,
    skipped: u64,
    cancelled: bool,
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicU64>,
    published: bool,
}
impl Writer<'_> {
    fn scan_space(&self) -> Result<(), String> {
        self.store.ensure_space()?;
        let pages: u64 = self
            .store
            .db
            .pragma_query_value(None, "page_count", |r| r.get::<_, i64>(0))
            .map_err(error)? as u64;
        let free: u64 = self
            .store
            .db
            .pragma_query_value(None, "freelist_count", |r| r.get::<_, i64>(0))
            .map_err(error)? as u64;
        let size: u64 = self
            .store
            .db
            .pragma_query_value(None, "page_size", |r| r.get::<_, i64>(0))
            .map_err(error)? as u64;
        if pages.saturating_sub(free).saturating_mul(size)
            >= self
                .store
                .max_bytes
                .saturating_sub(super::RESERVE + self.store.event_bytes)
        {
            return Err("Storage history budget reached. Space reserved for observations was preserved, along with the previous complete baseline.".into());
        }
        Ok(())
    }
    fn checkpoint(&self) -> Result<(), String> {
        self.store
            .db
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .map_err(error)?;
        self.scan_space()
    }
    fn flush(&mut self) -> Result<(), String> {
        if self.batch.is_empty() {
            return Ok(());
        }
        self.scan_space()?;
        let tx = self.store.db.transaction().map_err(error)?;
        {
            let mut insert = tx
                .prepare("INSERT INTO file_metadata VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)")
                .map_err(error)?;
            let mut inode = tx
                .prepare("INSERT OR IGNORE INTO file_inodes VALUES(?1,?2,?3,?4,?5)")
                .map_err(error)?;
            for entry in &self.batch {
                insert
                    .execute(params![
                        self.id,
                        entry.path,
                        entry.bucket,
                        entry.build,
                        entry.dev,
                        entry.ino,
                        entry.size,
                        entry.allocated,
                        entry.mtime,
                        entry.nanos,
                        entry.mode
                    ])
                    .map_err(error)?;
                if entry.mode & libc::S_IFMT == libc::S_IFREG {
                    inode
                        .execute(params![
                            self.id,
                            entry.dev,
                            entry.ino,
                            entry.path,
                            entry.allocated
                        ])
                        .map_err(error)?;
                }
            }
        }
        tx.commit().map_err(error)?;
        self.batch.clear();
        self.batch_weight = 0;
        self.checkpoint()
    }
    fn add(
        &mut self,
        path: &Path,
        metadata: &libc::stat,
        build: Option<&Path>,
    ) -> Result<(), String> {
        let bucket = path
            .components()
            .next()
            .map(|p| p.as_os_str().as_bytes().to_vec())
            .unwrap_or_default();
        let weight = (path.as_os_str().as_bytes().len()
            + bucket.len()
            + build.map(|p| p.as_os_str().as_bytes().len()).unwrap_or(0)
            + 128)
            * 6;
        if !self.batch.is_empty() && self.batch_weight + weight > 512 * 1024 {
            self.flush()?;
        }
        self.batch_weight += weight;
        self.batch.push(Entry {
            path: path.as_os_str().as_bytes().to_vec(),
            bucket,
            build: build.map(|p| p.as_os_str().as_bytes().to_vec()),
            dev: metadata.st_dev.to_le_bytes().to_vec(),
            ino: metadata.st_ino.to_le_bytes().to_vec(),
            size: integer(metadata.st_size.max(0) as u64)?,
            allocated: integer((metadata.st_blocks.max(0) as u64).saturating_mul(512))?,
            mtime: metadata.st_mtime,
            nanos: metadata.st_mtime_nsec,
            mode: metadata.st_mode,
        });
        self.visited += 1;
        self.progress.store(self.visited, Ordering::Relaxed);
        if self.batch.len() >= BATCH {
            self.flush()?
        }
        Ok(())
    }
    fn walk(
        &mut self,
        dir: &File,
        relative: &Path,
        build: Option<&Path>,
        depth: usize,
    ) -> Result<(), String> {
        if self.cancel.load(Ordering::Relaxed) {
            self.cancelled = true;
            return Ok(());
        }
        if depth >= 256 {
            self.skipped += 1;
            return Ok(());
        }
        let entries = match fs::read_dir(format!("/proc/self/fd/{}", dir.as_raw_fd())) {
            Ok(v) => v,
            Err(_) => {
                self.skipped += 1;
                return Ok(());
            }
        };
        for entry in entries {
            if self.cancel.load(Ordering::Relaxed) || self.visited >= MAX_ENTRIES {
                self.cancelled = true;
                break;
            }
            let entry = match entry {
                Ok(e) => e,
                Err(_) => {
                    self.skipped += 1;
                    continue;
                }
            };
            let name = entry.file_name();
            let name_c = CString::new(name.as_bytes()).map_err(error)?;
            let metadata = match anchored_stat(dir.as_raw_fd(), &name_c) {
                Ok(m) => m,
                Err(_) => {
                    self.skipped += 1;
                    continue;
                }
            };
            let path = relative.join(&name);
            let selected = build.map(Path::to_owned).or_else(|| {
                name.to_str()
                    .filter(|n| {
                        ["target", "node_modules", ".venv", "venv", "build", "dist"].contains(n)
                    })
                    .map(|_| path.clone())
            });
            self.add(&path, &metadata, selected.as_deref())?;
            if metadata.st_dev != self.dev {
                self.skipped += 1;
                continue;
            }
            if metadata.st_mode & libc::S_IFMT == libc::S_IFDIR {
                match directory(dir.as_raw_fd(), &name_c) {
                    Ok(child) => {
                        let current = stat(child.as_raw_fd())?;
                        if current.st_dev == metadata.st_dev && current.st_ino == metadata.st_ino {
                            self.walk(&child, &path, selected.as_deref(), depth + 1)?
                        } else {
                            self.skipped += 1;
                        }
                    }
                    Err(_) => self.skipped += 1,
                }
            }
            if self.cancelled {
                break;
            }
        }
        Ok(())
    }
    fn report(&self) -> Result<Scan, String> {
        let (files, allocated): (i64, i64) = self
            .store
            .db
            .query_row(
                "SELECT count(*),COALESCE(sum(allocated),0) FROM file_inodes WHERE snapshot=?1",
                [self.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(error)?;
        let groups = |field: &str| -> Result<Vec<FileSize>, String> {
            // field is selected only by the two constant calls below.
            let sql=format!("SELECT f.{field},SUM(i.allocated) total FROM file_metadata f INDEXED BY metadata_{field} JOIN file_inodes i INDEXED BY inodes_path ON i.snapshot=f.snapshot AND i.path=f.path WHERE f.snapshot=?1 AND f.{field} IS NOT NULL GROUP BY f.{field} ORDER BY total DESC LIMIT 50");
            let mut q = self.store.db.prepare(&sql).map_err(error)?;
            let result = q
                .query_map([self.id], |r| {
                    let path: Vec<u8> = r.get(0)?;
                    let size: i64 = r.get(1)?;
                    Ok(FileSize {
                        path: self.root.join(OsString::from_vec(path)),
                        size: size.max(0) as u64,
                    })
                })
                .map_err(error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(error);
            result
        };
        let mut q=self.store.db.prepare("SELECT path,allocated FROM file_inodes WHERE snapshot=?1 ORDER BY allocated DESC LIMIT 50").map_err(error)?;
        let largest = q
            .query_map([self.id], |r| {
                let path: Vec<u8> = r.get(0)?;
                let size: i64 = r.get(1)?;
                Ok(FileSize {
                    path: self.root.join(OsString::from_vec(path)),
                    size: size.max(0) as u64,
                })
            })
            .map_err(error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(error)?;
        Ok(Scan {
            root: self.root.clone(),
            allocated: allocated.max(0) as u64,
            files: files.max(0) as u64,
            skipped: self.skipped,
            cancelled: self.cancelled || self.skipped > 0,
            entries: groups("bucket")?,
            builds: groups("build")?,
            largest,
        })
    }
    fn diff(&mut self) -> Result<(), String> {
        let Some(baseline) = self.baseline else {
            return Ok(());
        };
        for removed in [false, true] {
            let mut cursor = Vec::<u8>::new();
            loop {
                if self.cancel.load(Ordering::Relaxed) {
                    self.cancelled = true;
                    return Ok(());
                }
                let (source, other) = if removed {
                    (baseline, self.id)
                } else {
                    (self.id, baseline)
                };
                let condition = if removed {
                    "o.path IS NULL"
                } else {
                    "o.path IS NULL OR n.size!=o.size OR n.allocated!=o.allocated OR n.ino!=o.ino OR n.mtime!=o.mtime OR n.nanos!=o.nanos OR n.mode!=o.mode"
                };
                let sql=format!("SELECT n.path,n.size,n.allocated,o.size,o.allocated FROM file_metadata n LEFT JOIN file_metadata o ON o.snapshot=?2 AND o.path=n.path WHERE n.snapshot=?1 AND n.path>?3 AND ({condition}) ORDER BY n.path LIMIT {BATCH}");
                let rows = {
                    let mut q = self.store.db.prepare(&sql).map_err(error)?;
                    let values = q
                        .query_map(params![source, other, cursor], |r| {
                            Ok((
                                r.get::<_, Vec<u8>>(0)?,
                                r.get::<_, i64>(1)?,
                                r.get::<_, i64>(2)?,
                                r.get::<_, Option<i64>>(3)?,
                                r.get::<_, Option<i64>>(4)?,
                            ))
                        })
                        .map_err(error)?
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(error)?;
                    values
                };
                if rows.is_empty() {
                    break;
                }
                cursor = rows.last().unwrap().0.clone();
                self.store.ensure_space()?;
                let tx = self.store.db.transaction().map_err(error)?;
                for (path, size, allocated, old_size, old_allocated) in rows {
                    let kind = if removed {
                        "removed"
                    } else if old_size.is_none() {
                        "added"
                    } else {
                        "metadata-changed"
                    };
                    let (before, after, before_a, after_a) = if removed {
                        (Some(size), None, Some(allocated), None)
                    } else {
                        (old_size, Some(size), old_allocated, Some(allocated))
                    };
                    tx.execute("INSERT INTO file_changes VALUES(?1,?2,?3,?4,?5,?6,?7,?8,(SELECT started_ms FROM snapshots WHERE id=?1))",params![self.id,self.root.as_os_str().as_bytes(),path,kind,before,after,before_a,after_a]).map_err(error)?;
                }
                tx.commit().map_err(error)?;
                self.checkpoint()?;
            }
        }
        Ok(())
    }
    fn finish(&mut self) -> Result<Outcome, String> {
        self.flush()?;
        if !fs::symlink_metadata(&self.root).is_ok_and(|m| {
            m.is_dir() && !m.file_type().is_symlink() && m.dev() == self.dev && m.ino() == self.ino
        }) {
            self.skipped += 1;
        }
        if self.skipped == 0 && !self.cancelled {
            self.diff()?;
        }
        let report = self.report()?;
        if report.cancelled {
            return Ok(Outcome {scan:report,snapshot_id:None,baseline_id:self.baseline,note:"Partial scan was not saved. The previous complete baseline is preserved; removals are not reported.".into()});
        }
        self.scan_space()?;
        let tx = self.store.db.transaction().map_err(error)?;
        tx.execute("UPDATE snapshots SET complete=2 WHERE id IN (SELECT id FROM snapshots WHERE root=?1 AND complete=1 ORDER BY id DESC LIMIT -1 OFFSET 1)",[self.root.as_os_str().as_bytes()]).map_err(error)?;
        tx.execute(
            "UPDATE snapshots SET complete=1,files=?2,allocated=?3 WHERE id=?1",
            params![self.id, integer(report.files)?, integer(report.allocated)?],
        )
        .map_err(error)?;
        tx.commit().map_err(error)?;
        self.published = true;
        let mut note=if self.baseline.is_some() {"Complete metadata snapshot saved. Changes are metadata observations, not content verification."}else{"Initial complete baseline saved for this folder and filesystem. Scan it again to compare changes."}.to_owned();
        // Publication is already durable. A history compaction failure must
        // never report that the completed scan itself failed or lose its data.
        let compaction = (|| {
            self.checkpoint()?;
            compact_retired(self.store)
        })();
        if let Err(error) = compaction {
            note.push_str(&format!(" Snapshot saved; history compaction will retry on the next requested scan: {error}"));
        }
        Ok(Outcome {
            scan: report,
            snapshot_id: Some(self.id),
            baseline_id: self.baseline,
            note,
        })
    }
}
impl Drop for Writer<'_> {
    fn drop(&mut self) {
        if !self.published {
            let _ = remove_snapshot(self.store, self.id, false);
        }
    }
}

fn remove_snapshot(store: &History, id: i64, keep_changes: bool) -> Result<(), String> {
    if !keep_changes {
        loop {
            let n=store.db.execute("DELETE FROM file_changes WHERE (snapshot,path) IN (SELECT snapshot,path FROM file_changes WHERE snapshot=?1 LIMIT 64)",[id]).map_err(error)?;
            store
                .db
                .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
                .map_err(error)?;
            if n == 0 {
                break;
            }
        }
    }
    for table in ["file_metadata", "file_inodes"] {
        let key = if table == "file_metadata" {
            "snapshot,path"
        } else {
            "snapshot,dev,ino"
        };
        let sql=format!("DELETE FROM {table} WHERE ({key}) IN (SELECT {key} FROM {table} WHERE snapshot=?1 LIMIT 64)");
        loop {
            let n = store.db.execute(&sql, [id]).map_err(error)?;
            store
                .db
                .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
                .map_err(error)?;
            if n == 0 {
                break;
            }
        }
    }
    store
        .db
        .execute("DELETE FROM snapshots WHERE id=?1", [id])
        .map_err(error)?;
    Ok(())
}
fn compact_retired(store: &History) -> Result<(), String> {
    loop {
        let id = store
            .db
            .query_row(
                "SELECT id FROM snapshots WHERE complete=2 LIMIT 1",
                [],
                |r| r.get::<_, i64>(0),
            )
            .optional()
            .map_err(error)?;
        let Some(id) = id else { break };
        remove_snapshot(store, id, true)?;
    }
    Ok(())
}
fn prepare(store: &History) -> Result<(), String> {
    loop {
        let id = store
            .db
            .query_row(
                "SELECT id FROM snapshots WHERE complete=0 LIMIT 1",
                [],
                |r| r.get::<_, i64>(0),
            )
            .optional()
            .map_err(error)?;
        let Some(id) = id else { break };
        remove_snapshot(store, id, false)?;
    }
    compact_retired(store)?;
    let cutoff = (w::now().saturating_sub(store.retention_days as u64 * 86400) * 1000) as i64;
    loop {
        let n=store.db.execute("DELETE FROM file_changes WHERE (snapshot,path) IN (SELECT snapshot,path FROM file_changes WHERE time_ms<?1 LIMIT 64)",[cutoff]).map_err(error)?;
        store
            .db
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .map_err(error)?;
        if n == 0 {
            break;
        }
    }
    Ok(())
}
pub fn scan(
    root: &Path,
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicU64>,
) -> Result<Outcome, String> {
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, 10);
    }
    let mut store = History::open()?;
    scan_with(&mut store, root, cancel, progress)
}
fn scan_with(
    store: &mut History,
    root: &Path,
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicU64>,
) -> Result<Outcome, String> {
    let _lock = lock(store)?;
    prepare(store)?;
    let root = root.canonicalize().map_err(error)?;
    let c = CString::new(root.as_os_str().as_bytes()).map_err(error)?;
    let dir = root_directory(&c)?;
    let metadata = stat(dir.as_raw_fd())?;
    let (dev, ino) = (
        metadata.st_dev.to_le_bytes().to_vec(),
        metadata.st_ino.to_le_bytes().to_vec(),
    );
    let filesystem = filesystem_identity(&dir, metadata.st_dev)?;
    // Snapshot IDs are scan order. Wall time can move backwards and must not
    // select an older baseline or retire a newer completed scan.
    let baseline_row=store.db.query_row("SELECT id,started_ms FROM snapshots WHERE root=?1 AND ino=?2 AND filesystem_identity=?3 AND complete=1 ORDER BY id DESC LIMIT 1",params![root.as_os_str().as_bytes(),ino,filesystem],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?))).optional().map_err(error)?;
    let started_ms=(w::now()*1000) as i64;
    let clock_backwards=baseline_row.is_some_and(|(_,time)|time>started_ms);
    let baseline=baseline_row.map(|(id,_)|id);
    store
        .db
        .execute(
            "INSERT INTO snapshots(root,dev,ino,started_ms,filesystem_identity) VALUES(?1,?2,?3,?4,?5)",
            params![
                root.as_os_str().as_bytes(),
                dev,
                ino,
                started_ms,
                filesystem
            ],
        )
        .map_err(error)?;
    let id = store.db.last_insert_rowid();
    let mut writer = Writer {
        store,
        id,
        baseline,
        root,
        dev: metadata.st_dev,
        ino: metadata.st_ino,
        batch: Vec::with_capacity(BATCH),
        batch_weight: 0,
        visited: 0,
        skipped: 0,
        cancelled: false,
        cancel,
        progress,
        published: false,
    };
    writer.add(Path::new(""), &metadata, None)?;
    writer.walk(&dir, Path::new(""), None, 0)?;
    let unchanged = root_directory(&c)
        .and_then(|current| {
            let now = stat(current.as_raw_fd())?;
            Ok(now.st_dev == metadata.st_dev
                && now.st_ino == metadata.st_ino
                && filesystem_identity(&current, now.st_dev)? == filesystem)
        })
        .unwrap_or(false);
    if !unchanged {
        writer.skipped += 1;
    }
    let mut outcome = writer.finish()?;
    if clock_backwards {outcome.note.push_str(" The system clock is earlier than the preceding scan. Comparison and snapshot retention use scan order; change-list dates retain the recorded clock and can appear out of execution order.");}
    if filesystem.starts_with("boot:") {
        outcome.note.push_str(" Persistent filesystem identity is unavailable here; comparisons conservatively reset after reboot.");
    }
    Ok(outcome)
}

pub fn changes(
    root: &Path,
    before: Option<&ChangeCursor>,
    limit: usize,
) -> Result<ChangePage, String> {
    let Some(store) = History::existing()? else {
        return Ok(ChangePage {
            changes: vec![],
            next: None,
        });
    };
    changes_with(&store, root, before, limit)
}
fn changes_with(
    store: &History,
    root: &Path,
    before: Option<&ChangeCursor>,
    limit: usize,
) -> Result<ChangePage, String> {
    let root = root.canonicalize().map_err(error)?;
    let version: u32 = store
        .db
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(error)?;
    if version < 2 {
        return Ok(ChangePage {
            changes: vec![],
            next: None,
        });
    }
    let limit = limit.clamp(1, 200);
    let mut q=store.db.prepare("SELECT c.snapshot,c.time_ms,c.path,c.kind,c.before_size,c.after_size,c.before_allocated,c.after_allocated FROM file_changes c LEFT JOIN snapshots s ON s.id=c.snapshot WHERE c.root=?1 AND (s.id IS NULL OR s.complete!=0) AND (?2 IS NULL OR c.time_ms<?2 OR(c.time_ms=?2 AND c.snapshot<?3) OR(c.time_ms=?2 AND c.snapshot=?3 AND c.path>?4)) ORDER BY c.time_ms DESC,c.snapshot DESC,c.path LIMIT ?5").map_err(error)?;
    let mut values = q
        .query_map(
            params![
                root.as_os_str().as_bytes(),
                before.map(|c| c.time_ms),
                before.map(|c| c.snapshot),
                before.map(|c| c.relative.as_slice()),
                (limit + 1) as i64
            ],
            |r| {
                let relative: Vec<u8> = r.get(2)?;
                Ok((
                    Change {
                        snapshot: r.get(0)?,
                        time_ms: r.get(1)?,
                        path: root.join(OsString::from_vec(relative.clone())),
                        kind: r.get(3)?,
                        before_size: r.get::<_, Option<i64>>(4)?.map(|n| n.max(0) as u64),
                        after_size: r.get::<_, Option<i64>>(5)?.map(|n| n.max(0) as u64),
                        before_allocated: r.get::<_, Option<i64>>(6)?.map(|n| n.max(0) as u64),
                        after_allocated: r.get::<_, Option<i64>>(7)?.map(|n| n.max(0) as u64),
                    },
                    relative,
                ))
            },
        )
        .map_err(error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(error)?;
    let mut count = 0;
    let mut bytes = 0;
    for (_, relative) in values.iter().take(limit) {
        if count > 0 && bytes + relative.len() > 2 * 1024 * 1024 {
            break;
        }
        bytes += relative.len();
        count += 1;
    }
    let more = values.len() > count;
    values.truncate(count);
    let next = if more {
        values.last().map(|(c, path)| ChangeCursor {
            snapshot: c.snapshot,
            time_ms: c.time_ms,
            relative: path.clone(),
        })
    } else {
        None
    };
    Ok(ChangePage {
        changes: values.into_iter().map(|v| v.0).collect(),
        next,
    })
}

/// Generated metadata load test, compiled only into authoring/QA builds.
/// No filesystem files are fabricated, touched or deleted by the indexer.
#[cfg(feature = "docs-demo")]
pub fn generated_metadata_probe(path: &Path, count: u64) -> Result<u64, String> {
    if !(1..=1_000_000).contains(&count)
        || !path.starts_with("/tmp/opencode")
        || !path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("tend-index-probe-"))
    {
        return Err("Index probes require a fresh /tmp/opencode/tend-index-probe-* directory and at most one million entries.".into());
    }
    if path.exists() {
        return Err("Index probe fixtures must be fresh.".into());
    }
    fs::create_dir_all(path).map_err(error)?;
    let root = path.join("synthetic-metadata-only");
    fs::create_dir(&root).map_err(error)?;
    let mut store = History::open_at(
        &path.join("state/history.sqlite3"),
        &super::Settings {
            max_mib: 2048,
            ..Default::default()
        },
    )?;
    let _lock = lock(&store)?;
    let c = CString::new(root.as_os_str().as_bytes()).map_err(error)?;
    let dir = root_directory(&c)?;
    let mut metadata = stat(dir.as_raw_fd())?;
    store
        .db
        .execute(
            "INSERT INTO snapshots(root,dev,ino,started_ms) VALUES(?1,?2,?3,?4)",
            params![
                root.as_os_str().as_bytes(),
                metadata.st_dev.to_le_bytes(),
                metadata.st_ino.to_le_bytes(),
                (w::now() * 1000) as i64
            ],
        )
        .map_err(error)?;
    let id = store.db.last_insert_rowid();
    {
        let mut writer = Writer {
            store: &mut store,
            id,
            baseline: None,
            root,
            dev: metadata.st_dev,
            ino: metadata.st_ino,
            batch: Vec::with_capacity(BATCH),
            batch_weight: 0,
            visited: 0,
            skipped: 0,
            cancelled: false,
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(AtomicU64::new(0)),
            published: false,
        };
        metadata.st_mode = libc::S_IFREG | 0o600;
        metadata.st_blocks = 0;
        for number in 0..count {
            metadata.st_ino = number + 1;
            metadata.st_size = (number % 16384) as i64;
            let relative = PathBuf::from(format!(
                "project-{:04}/target/artifact-{number:07}.bin",
                number / 1024
            ));
            let build = relative.parent().unwrap();
            writer.add(&relative, &metadata, Some(build))?;
        }
        let outcome = writer.finish()?;
        if outcome.scan.files != count {
            return Err("Generated metadata count differs from the completed snapshot.".into());
        }
    }
    Ok(store.size_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn fixture() -> (History, PathBuf) {
        let parent = PathBuf::from("/tmp/opencode").join(format!(
            "tend-snapshots-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let root = parent.join("files");
        fs::create_dir_all(&root).unwrap();
        let store = History::open_limits(
            &parent.join("state/history.sqlite3"),
            16 * 1024 * 1024,
            4096,
            30,
            0,
        )
        .unwrap();
        (store, root)
    }
    fn run(store: &mut History, root: &Path) -> Outcome {
        scan_with(
            store,
            root,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicU64::new(0)),
        )
        .unwrap()
    }
    #[test]
    fn finished_scan_releases_lock_with_an_inherited_descriptor_still_open() {
        let (store,_root)=fixture();let held=lock(&store).unwrap();
        // dup and fork retain the same open-file description. A retained dup
        // reproduces the ownership issue without starting another process.
        let inherited=held.0.try_clone().unwrap();
        drop(held);
        assert!(lock(&store).is_ok());
        drop(inherited);
    }
    #[test]
    fn complete_snapshots_retain_lossless_names_hardlinks_and_change_history() {
        let (mut store, root) = fixture();
        fs::write(root.join("alpha"), b"first").unwrap();
        fs::write(root.join("beta"), b"second").unwrap();
        fs::hard_link(root.join("beta"), root.join("linked-beta")).unwrap();
        let unusual = root.join(OsString::from_vec(b"name-\xff".to_vec()));
        fs::write(&unusual, b"byte name").unwrap();
        let outside = root.parent().unwrap().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("private"), b"not indexed").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
        let first = run(&mut store, &root);
        assert!(first.snapshot_id.is_some());
        assert!(first.baseline_id.is_none());
        assert_eq!(first.scan.files, 3);
        assert_eq!(
            first.scan.allocated,
            fs::metadata(root.join("alpha")).unwrap().blocks() * 512
                + fs::metadata(root.join("beta")).unwrap().blocks() * 512
                + fs::metadata(&unusual).unwrap().blocks() * 512
        );
        assert!(changes_with(&store, &root, None, 20)
            .unwrap()
            .changes
            .is_empty());
        // Moving a fixture out of the scanned root models removal without
        // deleting any file. All fixture inputs remain available for inspection.
        fs::rename(root.join("alpha"), outside.join("retained-alpha")).unwrap();
        fs::write(root.join("beta"), b"changed fixture").unwrap();
        fs::write(root.join("gamma"), b"new").unwrap();
        let second = run(&mut store, &root);
        assert_eq!(second.baseline_id, first.snapshot_id);
        let page = changes_with(&store, &root, None, 2).unwrap();
        assert_eq!(page.changes.len(), 2);
        assert!(page.next.is_some());
        let next = changes_with(&store, &root, page.next.as_ref(), 100).unwrap();
        let mut changes = page.changes;
        changes.extend(next.changes);
        assert!(changes
            .iter()
            .any(|c| c.path == root.join("alpha") && c.kind == "removed"));
        assert!(changes
            .iter()
            .any(|c| c.path == root.join("gamma") && c.kind == "added"));
        assert!(changes
            .iter()
            .any(|c| c.path == root.join("beta") && c.kind == "metadata-changed"));
        assert!(!changes
            .iter()
            .any(|c| c.path.starts_with(root.join("link/private"))));
        fs::write(&unusual, b"updated byte name").unwrap();
        let third = run(&mut store, &root);
        assert_eq!(third.baseline_id, second.snapshot_id);
        let count: i64 = store
            .db
            .query_row("SELECT count(*) FROM snapshots WHERE complete=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 2);
        let changes = changes_with(&store, &root, None, 200).unwrap();
        assert!(changes.changes.iter().any(|c| c.path == unusual));
        assert!(changes
            .changes
            .iter()
            .any(|c| c.snapshot == second.snapshot_id.unwrap() && c.kind == "removed"));
    }
    #[test]
    fn backwards_wall_clock_keeps_latest_baseline_and_both_newest_snapshots() {
        let (mut store,root)=fixture();let now=(w::now()*1000) as i64;
        fs::write(root.join("item"),b"first").unwrap();let first=run(&mut store,&root);
        store.db.execute("UPDATE snapshots SET started_ms=?1 WHERE id=?2",params![now+172800000,first.snapshot_id]).unwrap();
        fs::write(root.join("item"),b"second version").unwrap();let second=run(&mut store,&root);
        assert_eq!(second.baseline_id,first.snapshot_id);assert!(second.note.contains("clock is earlier"));
        store.db.execute("UPDATE snapshots SET started_ms=?1 WHERE id=?2",params![now+86400000,second.snapshot_id]).unwrap();
        store.db.execute("UPDATE file_changes SET time_ms=?1 WHERE snapshot=?2",params![now+86400000,second.snapshot_id]).unwrap();
        fs::write(root.join("item"),b"third and latest version").unwrap();let third=run(&mut store,&root);
        assert_eq!(third.baseline_id,second.snapshot_id);assert!(third.note.contains("clock is earlier"));
        let ids=store.db.prepare("SELECT id FROM snapshots WHERE complete=1 ORDER BY id").unwrap().query_map([],|r|r.get::<_,i64>(0)).unwrap().collect::<Result<Vec<_>,_>>().unwrap();
        assert_eq!(ids,vec![second.snapshot_id.unwrap(),third.snapshot_id.unwrap()]);
        let expected_time:i64=store.db.query_row("SELECT started_ms FROM snapshots WHERE id=?1",[third.snapshot_id.unwrap()],|r|r.get(0)).unwrap();
        assert_eq!(latest_with(&store,&root).unwrap().unwrap().time_ms,expected_time);
        let mut cursor=None;let mut changes=Vec::new();loop{let page=changes_with(&store,&root,cursor.as_ref(),1).unwrap();changes.extend(page.changes);cursor=page.next;if cursor.is_none(){break;}}
        assert!(changes.iter().any(|c|Some(c.snapshot)==second.snapshot_id));assert!(changes.iter().any(|c|Some(c.snapshot)==third.snapshot_id));
        let unique=changes.iter().map(|c|(c.snapshot,c.path.clone())).collect::<std::collections::BTreeSet<_>>();assert_eq!(unique.len(),changes.len());
        let latest=changes.iter().find(|c|Some(c.snapshot)==third.snapshot_id&&c.path==root.join("item")).unwrap();assert_eq!(latest.before_size,Some(14));assert_eq!(latest.after_size,Some(24));
    }
    #[test]
    fn cancellation_and_permission_gaps_preserve_the_complete_baseline() {
        let (mut store, root) = fixture();
        let nested = root.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("kept"), b"fixture").unwrap();
        let first = run(&mut store, &root);
        let partial = scan_with(
            &mut store,
            &root,
            Arc::new(AtomicBool::new(true)),
            Arc::new(AtomicU64::new(0)),
        )
        .unwrap();
        assert!(partial.scan.cancelled);
        assert!(partial.snapshot_id.is_none());
        fs::set_permissions(&nested, fs::Permissions::from_mode(0)).unwrap();
        let partial = run(&mut store, &root);
        fs::set_permissions(&nested, fs::Permissions::from_mode(0o700)).unwrap();
        if unsafe { libc::geteuid() } != 0 {
            assert!(partial.snapshot_id.is_none());
            assert!(partial.scan.skipped > 0);
        }
        let ids: Vec<i64> = store
            .db
            .prepare("SELECT id FROM snapshots WHERE complete=1 ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(ids.contains(&first.snapshot_id.unwrap()));
        assert!(changes_with(&store, &root, None, 200)
            .unwrap()
            .changes
            .is_empty());
    }
    #[test]
    fn compaction_failure_keeps_two_published_baselines_and_retries_without_losing_changes() {
        let (mut store, root) = fixture();
        fs::write(root.join("first"), b"fixture").unwrap();
        let first = run(&mut store, &root);
        let _second = run(&mut store, &root);
        store.db.execute_batch(&format!("CREATE TRIGGER fixture_compaction_failure BEFORE DELETE ON file_metadata WHEN OLD.snapshot={} BEGIN SELECT RAISE(FAIL,'controlled compaction failure'); END;",first.snapshot_id.unwrap())).unwrap();
        let third = run(&mut store, &root);
        assert!(third.snapshot_id.is_some());
        assert!(third.note.contains("compaction will retry"));
        let active: i64 = store
            .db
            .query_row("SELECT count(*) FROM snapshots WHERE complete=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(active, 2);
        let retired: i64 = store
            .db
            .query_row("SELECT count(*) FROM snapshots WHERE complete=2", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(retired, 1);
        store
            .db
            .execute_batch("DROP TRIGGER fixture_compaction_failure")
            .unwrap();
        prepare(&store).unwrap();
        let retired: i64 = store
            .db
            .query_row("SELECT count(*) FROM snapshots WHERE complete=2", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(retired, 0);
    }
    #[test]
    fn changed_filesystem_identity_resets_the_baseline_without_reporting_removals() {
        let (mut store, root) = fixture();
        fs::write(root.join("kept"), b"fixture").unwrap();
        let first = run(&mut store, &root);
        store
            .db
            .execute(
                "UPDATE snapshots SET filesystem_identity='different-filesystem' WHERE id=?1",
                [first.snapshot_id.unwrap()],
            )
            .unwrap();
        fs::rename(
            root.join("kept"),
            root.parent().unwrap().join("retained-from-previous-fs"),
        )
        .unwrap();
        let second = run(&mut store, &root);
        assert!(second.baseline_id.is_none());
        assert!(changes_with(&store, &root, None, 200)
            .unwrap()
            .changes
            .is_empty());
    }
    #[test]
    fn lock_and_budget_failures_do_not_publish_or_destroy_a_baseline() {
        let (mut store, root) = fixture();
        fs::write(root.join("kept"), b"fixture").unwrap();
        let first = run(&mut store, &root);
        let held = lock(&store).unwrap();
        assert!(scan_with(
            &mut store,
            &root,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicU64::new(0))
        )
        .is_err());
        drop(held);
        store.max_bytes = store.size_bytes() + 2 * 1024 * 1024;
        assert!(scan_with(
            &mut store,
            &root,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicU64::new(0))
        )
        .is_err());
        let id: i64 = store
            .db
            .query_row("SELECT id FROM snapshots WHERE complete=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(Some(id), first.snapshot_id);
        let pending: i64 = store
            .db
            .query_row("SELECT count(*) FROM snapshots WHERE complete=0", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(pending, 0);
    }
    #[test]
    fn sqlite_full_preserves_the_complete_baseline_and_can_recover() {
        let (mut store, root)=fixture();
        fs::write(root.join("kept"),b"fixture").unwrap();
        let first=run(&mut store,&root);
        let pages:i64=store.db.pragma_query_value(None,"page_count",|r|r.get(0)).unwrap();
        // SQLite's pager returns SQLITE_FULL without filling the host disk.
        store.db.pragma_update(None,"max_page_count",pages).unwrap();
        for n in 0..256 {fs::write(root.join(format!("{n:04}-{}","q".repeat(160))),b"").unwrap();}
        let failure=scan_with(&mut store,&root,Arc::new(AtomicBool::new(false)),Arc::new(AtomicU64::new(0))).unwrap_err();
        assert!(failure.to_lowercase().contains("full"),"Expected SQLITE_FULL, got {failure}");
        let latest:i64=store.db.query_row("SELECT id FROM snapshots WHERE complete=1 ORDER BY id DESC LIMIT 1",[],|r|r.get(0)).unwrap();
        assert_eq!(Some(latest),first.snapshot_id);
        assert_eq!(latest_with(&store,&root).unwrap().unwrap().files,1);
        assert!(changes_with(&store,&root,None,200).unwrap().changes.is_empty());
        store.db.pragma_update(None,"max_page_count",100000).unwrap();
        let recovered=run(&mut store,&root);
        assert_eq!(recovered.baseline_id,first.snapshot_id);
        assert_eq!(latest_with(&store,&root).unwrap().unwrap().files,257);
        assert_eq!(fs::read(root.join("kept")).unwrap(),b"fixture");
    }
}
