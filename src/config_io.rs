//! Small configuration reads never follow a leaf symlink, block on a FIFO or
//! allocate from an untrusted file size. Missing files are distinct from errors.
use std::{
    fs,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};
pub fn read(
    path: &Path,
    max: usize,
    owner: Option<u32>,
    private: bool,
) -> Result<Option<Vec<u8>>, String> {
    if max == 0 || max > 1024 * 1024 {
        return Err("Invalid configuration read budget.".into());
    }
    let file = match fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Cannot read {}: {e}", path.display())),
    };
    let m = file.metadata().map_err(|e| e.to_string())?;
    if !m.is_file()
        || m.len() > max as u64
        || owner.is_some_and(|uid| m.uid() != uid)
        || (private && m.mode() & 0o077 != 0)
        || (!private && owner == Some(0) && m.mode() & 0o022 != 0)
    {
        return Err(format!(
            "{} must be a protected regular file, at most {} KiB.",
            path.display(),
            max.div_ceil(1024)
        ));
    }
    let mut bytes = Vec::new();
    file.take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > max {
        return Err("Configuration exceeds its size limit.".into());
    }
    Ok(Some(bytes))
}

pub struct Guard(std::fs::File);
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.0);
    }
}
pub fn lock(path: &Path) -> Result<Guard, String> {
    use std::os::unix::fs::DirBuilderExt;
    let parent = path.parent().ok_or("Missing settings directory")?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .map_err(|e| e.to_string())?;
    let m = fs::symlink_metadata(parent).map_err(|e| e.to_string())?;
    if !m.is_dir() || m.file_type().is_symlink() || m.uid() != unsafe { libc::geteuid() } {
        return Err("Settings need a user-owned directory.".into());
    }
    let name = path.with_extension("json.lock");
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(name)
        .map_err(|e| e.to_string())?;
    let m = file.metadata().map_err(|e| e.to_string())?;
    if !m.is_file() || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o077 != 0 {
        return Err("Settings lock is not a private regular file.".into());
    }
    fs2::FileExt::try_lock_exclusive(&file)
        .map_err(|_| "Another settings save is in progress. Try again.".to_string())?;
    Ok(Guard(file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::{ffi::OsStrExt, fs::PermissionsExt};
    #[test]
    fn bounded_private_reads_and_explicit_unlock() {
        let dir = std::path::PathBuf::from("/tmp/opencode").join(format!(
            "tend-config-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        let uid = unsafe { libc::geteuid() };
        assert!(read(&path, 64, Some(uid), true).unwrap().is_none());
        fs::write(&path, b"{}").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(read(&path, 64, Some(uid), true).unwrap().unwrap(), b"{}");
        let symlink = dir.join("alias");
        std::os::unix::fs::symlink(&path, &symlink).unwrap();
        assert!(read(&symlink, 64, None, false).is_err());
        let fifo = dir.join("fifo");
        let c = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        assert!(read(&fifo, 64, None, false).is_err());
        fs::write(&path, [b'x'; 65]).unwrap();
        assert!(read(&path, 64, None, false).is_err());
        let guard = lock(&path).unwrap();
        let inherited = guard.0.try_clone().unwrap();
        assert!(lock(&path).is_err());
        drop(guard);
        assert!(lock(&path).is_ok());
        drop(inherited);
    }
}
