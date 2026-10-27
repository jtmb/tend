"""Bounded maintenance events and durable phase receipts (Python stdlib only).

This module never schedules or starts maintenance. The existing engine supplies
one phase action while holding its run lock. Receipts prevent a history/state
failure from replaying a phase whose outcome is completed or uncertain.
"""
import datetime as dt
import json
import os
from pathlib import Path
import shutil
import sqlite3
import stat
import time
from urllib.parse import quote
import uuid

PHASES = ('updates', 'apt-cache', 'docker-build', 'docker-images',
          'npm-logs', 'npm-cache', 'health')
FILENAME = 'tend-history.sqlite3'
MAX_BYTES = 4 * 1024 * 1024  # DELETE journal is bounded by this database size.
APPLICATION_ID = 0x54454E4D
MAX_EVENTS = 1024
RETENTION_DAYS = 30
MESSAGE_BYTES = 1800


def _message(value):
    raw = str(value).encode('utf-8', errors='replace')
    result = raw[:MESSAGE_BYTES].decode('utf-8', errors='ignore')
    return result + (' [truncated]' if len(raw) > MESSAGE_BYTES else '')


def _json(value):
    return json.dumps(value, ensure_ascii=False, separators=(',', ':'))


def _boottime_ms():
    return time.clock_gettime_ns(time.CLOCK_BOOTTIME) // 1_000_000


def _boot_id():
    return Path('/proc/sys/kernel/random/boot_id').read_text().strip()[:128]


def scheduled_ms(window):
    """Use the existing configured Toronto schedule; never change it."""
    from zoneinfo import ZoneInfo
    path = Path('/etc/tend/settings.json')
    try:
        with path.open('rb') as stream:
            raw = stream.read(32 * 1024 + 1)
        if len(raw) > 32 * 1024:
            raise ValueError('Maintenance settings exceed 32 KiB')
        settings = json.loads(raw)
    except FileNotFoundError:
        settings = {}
    hour, minute = settings.get('hour', 8), settings.get('minute', 0)
    if type(hour) is not int or type(minute) is not int or not 0 <= hour <= 23 or not 0 <= minute <= 59:
        raise ValueError('Invalid maintenance start time')
    day = dt.date.fromisoformat(window)
    return int(dt.datetime.combine(day, dt.time(hour, minute), ZoneInfo('America/Toronto')).timestamp() * 1000)


class PhaseHistory:
    def __init__(self, state_dir, owner=0, wall_ms=None, boot_ms=None, boot_id=None, free=None):
        self.path = Path(state_dir) / FILENAME
        self.wall_ms = wall_ms or (lambda: time.time_ns() // 1_000_000)
        self.boot_ms = boot_ms or _boottime_ms
        self.boot_id = boot_id or _boot_id
        self.free = free or (lambda: shutil.disk_usage('/').free)
        self.db = None
        self.cached_result = None
        for parent in self.path.parents:
            m = parent.lstat()
            if not stat.S_ISDIR(m.st_mode) or stat.S_ISLNK(m.st_mode) or m.st_uid != owner or m.st_mode & 0o022:
                raise ValueError('Maintenance history requires protected administrator-owned directories')
            if owner != 0:
                break
        try:
            fd = os.open(self.path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o644)
        except FileExistsError:
            fd = os.open(self.path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        try:
            m = os.fstat(fd)
            if not stat.S_ISREG(m.st_mode) or m.st_uid != owner or m.st_mode & 0o022 or m.st_size > MAX_BYTES:
                raise ValueError('Invalid maintenance history file')
        finally:
            os.close(fd)
        for suffix in ('-journal', '-wal', '-shm'):
            sidecar = Path(str(self.path) + suffix)
            if sidecar.exists() or sidecar.is_symlink():
                m = sidecar.lstat()
                if not stat.S_ISREG(m.st_mode) or m.st_uid != owner or m.st_mode & 0o022 or m.st_size > MAX_BYTES:
                    raise ValueError('Invalid maintenance history sidecar')
        db = sqlite3.connect('file:' + quote(str(self.path.absolute()), safe='/') + '?mode=rw', uri=True, timeout=0.25)
        self.db = db
        try:
            version = db.execute('PRAGMA user_version').fetchone()[0]
            application = db.execute('PRAGMA application_id').fetchone()[0]
            if version not in (0, 1):
                raise ValueError('Unsupported maintenance history version; existing file preserved')
            tables={row[0] for row in db.execute("SELECT name FROM sqlite_master WHERE type='table'")}
            if (version==0 and (tables or application not in (0,APPLICATION_ID))) or (version==1 and (application!=APPLICATION_ID or tables!={'metadata','receipts','events','sqlite_sequence'})):
                raise ValueError('Unrecognized maintenance history; existing file preserved')
            db.execute('PRAGMA trusted_schema=OFF')
            db.execute('PRAGMA journal_mode=DELETE')
            db.execute('PRAGMA synchronous=FULL')
            db.execute('PRAGMA temp_store=MEMORY')
            db.execute('PRAGMA cache_size=-512')
            pages = MAX_BYTES // db.execute('PRAGMA page_size').fetchone()[0]
            db.execute('PRAGMA max_page_count=' + str(pages))
            if version==0:
                db.executescript('''BEGIN IMMEDIATE;
                    CREATE TABLE IF NOT EXISTS metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);
                    CREATE TABLE IF NOT EXISTS receipts(
                        window TEXT NOT NULL,phase TEXT NOT NULL,attempt INTEGER NOT NULL,
                        token TEXT NOT NULL,status TEXT NOT NULL,payload TEXT NOT NULL,
                        PRIMARY KEY(window,phase));
                    CREATE TABLE IF NOT EXISTS events(
                        row INTEGER PRIMARY KEY AUTOINCREMENT,id TEXT NOT NULL UNIQUE,
                        time_ms INTEGER NOT NULL,phase TEXT NOT NULL,window TEXT NOT NULL,
                        attempt INTEGER NOT NULL,kind TEXT NOT NULL,payload TEXT NOT NULL);
                    CREATE INDEX IF NOT EXISTS event_time ON events(time_ms DESC,row DESC);
                    CREATE INDEX IF NOT EXISTS event_phase ON events(phase,time_ms DESC,row DESC);
                    PRAGMA user_version=1;
                '''+'PRAGMA application_id='+str(APPLICATION_ID)+'; COMMIT;')
        except BaseException:
            db.close()
            self.db = None
            raise

    def close(self):
        if self.db is not None:
            self.db.close()
            self.db = None

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()

    def _event(self, ticket, kind):
        public = {key: value for key, value in ticket.items() if not key.startswith('_')}
        self.db.execute('INSERT INTO events(id,time_ms,phase,window,attempt,kind,payload) VALUES(?,?,?,?,?,?,?)',
                        (ticket['id'] + ':' + kind, ticket.get('finished_ms') or ticket['started_ms'],
                         ticket['phase'], ticket['window'], ticket['attempt'], kind,
                         _json(public)))

    def begin(self, phase, window, attempt, scheduled=None):
        if phase not in PHASES or type(attempt) is not int or not 1 <= attempt <= 3:
            raise ValueError('Invalid phase or maintenance attempt')
        if dt.date.fromisoformat(window).isoformat() != window:
            raise ValueError('Invalid maintenance date')
        now = self.wall_ms()
        with self.db:
            self.db.execute('BEGIN IMMEDIATE')
            latest = self.db.execute("SELECT value FROM metadata WHERE key='latest_window'").fetchone()
            if latest and window < latest[0]:
                return None, ('deferred', 'Clock moved to an earlier maintenance date; no action repeated')
            receipt = self.db.execute('SELECT attempt,status,payload FROM receipts WHERE window=? AND phase=?', (window, phase)).fetchone()
            if receipt:
                previous = json.loads(receipt[2])
                if receipt[1] == 'running':
                    return None, ('deferred', 'Previous phase outcome is uncertain; review its history before rerunning')
                if receipt[1] == 'success' or attempt <= receipt[0]:
                    self.cached_result = previous
                    return None, (receipt[1], 'Recorded result; action not repeated: ' + previous['message'])
            scheduled = scheduled_ms(window) if scheduled is None else scheduled
            ticket = dict(version=1, id=uuid.uuid4().hex, phase=phase, window=window,
                          attempt=attempt, started_ms=now, finished_ms=None, duration_ms=None,
                          status='running', message='Phase started', boot_id=self.boot_id(),
                          scheduled_ms=scheduled,
                          catch_up_seconds=max(0, (now - scheduled) // 1000),
                          free_before=self.free(), free_after=None, free_delta_bytes=None,
                          clock_changed=False, _boot_ms=self.boot_ms())
            cutoff = (dt.date.fromisoformat(window) - dt.timedelta(days=RETENTION_DAYS-1)).isoformat()
            self.db.execute('DELETE FROM receipts WHERE window<?', (cutoff,))
            self.db.execute('DELETE FROM events WHERE time_ms<?', (now - RETENTION_DAYS * 86400 * 1000,))
            self.db.execute('DELETE FROM events WHERE row IN (SELECT row FROM events ORDER BY row DESC LIMIT -1 OFFSET ?)', (MAX_EVENTS - 2,))
            self.db.execute("INSERT OR REPLACE INTO metadata(key,value) VALUES('latest_window',?)", (window,))
            self.db.execute('INSERT OR REPLACE INTO receipts VALUES(?,?,?,?,?,?)',
                            (window, phase, attempt, ticket['id'], 'running', _json(ticket)))
            self._event(ticket, 'started')
        return ticket, None

    def finish(self, ticket, status, message):
        if status not in ('success', 'failed', 'deferred') or ticket['boot_id'] != self.boot_id():
            raise ValueError('Invalid or interrupted phase outcome')
        value = dict(ticket)
        value.update(status=status, message=_message(message), finished_ms=self.wall_ms(),
                     duration_ms=max(0, self.boot_ms() - ticket['_boot_ms']), free_after=self.free())
        value['free_delta_bytes'] = value['free_after'] - value['free_before']
        value['clock_changed'] = abs(value['finished_ms'] - value['started_ms'] - value['duration_ms']) > 1000
        with self.db:
            self.db.execute('BEGIN IMMEDIATE')
            row = self.db.execute('SELECT token,status FROM receipts WHERE window=? AND phase=?', (ticket['window'], ticket['phase'])).fetchone()
            if row != (ticket['id'], 'running'):
                raise ValueError('Phase receipt changed; outcome cannot be replaced')
            self.db.execute('UPDATE receipts SET status=?,payload=? WHERE window=? AND phase=?',
                            (status, _json(value), ticket['window'], ticket['phase']))
            self._event(value, 'finished')
        return value


def run_phase(state_dir, phase, window, attempt, action, entry=None):
    """The action is called at most once per successful/uncertain phase date."""
    try:
        history = PhaseHistory(state_dir)
        ticket, previous = history.begin(phase, window, attempt)
    except Exception as exc:
        if 'history' in locals():
            history.close()
        return 'deferred', 'No action started: maintenance receipt unavailable: ' + _message(exc)
    with history:
        if previous is not None:
            if entry is not None and previous[0]=='success' and history.cached_result is not None:
                entry['tend_recorded_success_unix']=history.cached_result['finished_ms']/1000
            return previous
        try:
            status, message = action()
        except Exception as exc:
            status, message = 'failed', type(exc).__name__ + ': ' + str(exc)
        if status not in ('success', 'failed', 'deferred'):
            return 'failed', 'Phase returned an unknown outcome; receipt retained for review'
        message = _message(message)
        try:
            history.finish(ticket, status, message)
        except Exception as exc:
            # Preserve the operation's result. Its durable running receipt blocks
            # replay if the legacy state file also fails to save the outcome.
            return status, _message(message) + '; history unavailable: ' + _message(exc)
        return status, message


def check_rollback(state_dir, owner=0):
    """Removing receipts must not allow the legacy engine to replay a phase."""
    path = Path(state_dir)/FILENAME
    if not path.exists() and not path.is_symlink():
        return
    for parent in path.parents:
        m=parent.lstat()
        if not stat.S_ISDIR(m.st_mode) or stat.S_ISLNK(m.st_mode) or m.st_uid!=owner or m.st_mode&0o022:
            raise ValueError('Unprotected maintenance state directory')
        if owner!=0:break
    for file in (path,Path(state_dir)/'state.json'):
        if file.exists() or file.is_symlink():
            m=file.lstat()
            if not stat.S_ISREG(m.st_mode) or m.st_uid!=owner or m.st_mode&0o022 or m.st_size>MAX_BYTES:
                raise ValueError('Invalid maintenance state file; rollback stopped')
    with sqlite3.connect('file:'+quote(str(path.absolute()),safe='/')+'?mode=ro',uri=True,timeout=0.25) as db:
        if db.execute('PRAGMA user_version').fetchone()[0]!=1 or db.execute('PRAGMA application_id').fetchone()[0]!=APPLICATION_ID:
            raise ValueError('Unsupported maintenance receipt version; rollback stopped')
        state_path=Path(state_dir)/'state.json'
        state=json.loads(state_path.read_text()) if state_path.exists() else {}
        phases=state.get('phases',{})
        for window,phase in db.execute("SELECT window,phase FROM receipts WHERE status='running'"):
            entry=phases.get(phase,{})
            if entry.get('window')!=window or entry.get('success') is not True:
                raise ValueError('An uncertain phase receipt needs review before removing its replay protection')
        for window,phase in db.execute("SELECT window,phase FROM receipts WHERE status='success'"):
            entry=phases.get(phase,{})
            recorded=entry.get('window','')
            if recorded<window or (recorded==window and entry.get('success') is not True):
                raise ValueError('Reconcile recorded successful phase outcomes before removing replay protection')


if __name__ == '__main__':
    import sys
    if sys.argv[1:] == ['--capabilities']:
        print(json.dumps(dict(protocol=4, role='maintenance-history', maintenance_events=True)))
    else:
        raise SystemExit('Only read-only --capabilities is supported directly')
