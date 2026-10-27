//! Explained, immutable cleanup selections. File execution revalidates the
//! previewed inode and timestamps; bulk engines retain their own eligibility.
use crate::{cleanup_rules as rules, exclusions, workstation as w};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};
const CANDIDATE_BUDGET: usize = 16 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stamp {
    pub dev: u64,
    pub ino: u64,
    pub size: u64,
    pub allocated: u64,
    pub mtime: i64,
    pub mtime_ns: i64,
    pub ctime: i64,
    pub ctime_ns: i64,
    pub uid: u32,
    pub links: u64,
}
impl Stamp {
    fn from(m: &fs::Metadata) -> Self {
        Self {
            dev: m.dev(),
            ino: m.ino(),
            size: m.len(),
            allocated: m.blocks().saturating_mul(512),
            mtime: m.mtime(),
            mtime_ns: m.mtime_nsec(),
            ctime: m.ctime(),
            ctime_ns: m.ctime_nsec(),
            uid: m.uid(),
            links: m.nlink(),
        }
    }
    pub fn matches(&self, s: &libc::stat) -> bool {
        self.dev == s.st_dev
            && self.ino == s.st_ino
            && self.size == s.st_size.max(0) as u64
            && self.mtime == s.st_mtime
            && self.mtime_ns == s.st_mtime_nsec
            && self.ctime == s.st_ctime
            && self.ctime_ns == s.st_ctime_nsec
            && self.uid == s.st_uid
            && self.links == s.st_nlink
    }
}
#[derive(Clone, Debug)]
pub struct Candidate {
    pub category: usize,
    pub path: PathBuf,
    pub stamp: Stamp,
}
#[derive(Clone, Debug)]
pub enum Action {
    Files { root: PathBuf, days: u32 },
    Apt,
    Docker(Vec<String>),
    Pip(PathBuf),
    Trash,
}
#[derive(Clone, Debug)]
pub struct Category {
    pub title: String,
    pub action: Action,
    pub selected: bool,
    pub reason: String,
    pub consequence: String,
    pub skip: Option<String>,
    pub files: u64,
    pub allocated: u64,
    pub protected: u64,
}
#[derive(Clone)]
pub struct Plan {
    pub created: u64,
    pub exclusions: exclusions::Policy,
    pub cleanup: w::Cleanup,
    pub custom: rules::Policy,
    pub categories: Vec<Category>,
    pub candidates: Vec<Candidate>,
    pub inventory: String,
}
impl Plan {
    pub fn build(
        cleanup: &w::Cleanup,
        custom: &rules::Policy,
        inventory: bool,
    ) -> Result<Self, String> {
        cleanup.validate()?;
        if custom.rules.len() > 64 {
            return Err("At most 64 folder rules are supported.".into());
        }
        let guard = exclusions::Guard::acquire()?;
        let protected = &guard.compiled;
        let mut plan = Self {
            created: w::now(),
            exclusions: protected.policy.clone(),
            cleanup: cleanup.clone(),
            custom: custom.clone(),
            categories: vec![],
            candidates: vec![],
            inventory: String::new(),
        };
        let apt_root = if cleanup.apt && protected.policy.active() {
            exclusions::apt_root()
        } else {
            Some(PathBuf::from("/var/cache/apt/archives"))
        };
        if cleanup.apt {
            plan.categories.push(Category {title:"Obsolete APT downloads".into(),action:Action::Apt,selected:true,reason:"Packages no longer downloadable from configured repositories; APT decides eligibility at execution.".into(),consequence:"A later reinstall may require downloading packages again.".into(),skip:protected.bulk_reason("apt",apt_root.as_deref()),files:0,allocated:0,protected:0});
        }
        for (title, args) in cleanup.docker_commands() {
            plan.categories.push(Category {title:title.into(),action:Action::Docker(args),selected:true,reason:format!("Unused build cache older than {} days with {} GB retained, or dangling images older than {} days. Docker rechecks eligibility.",cleanup.age_days,cleanup.keep_gb,cleanup.image_days()),consequence:"Builds or image retrieval may take longer. Containers, volumes and tagged images are preserved.".into(),skip:protected.bulk_reason("docker",None),files:0,allocated:0,protected:0});
        }
        let mut roots = Vec::new();
        for (title, root, days) in w::cache_roots(cleanup) {
            roots.push((title.to_owned(), root, days, None));
        }
        for rule in &custom.rules {
            if rule.enabled {
                rule.validate()?;
                roots.push((
                    rule.name.clone(),
                    rule.folder.canonicalize().map_err(|e| e.to_string())?,
                    rule.days,
                    Some(rule.clone()),
                ));
            }
        }
        let mut seen = HashSet::new();
        let mut weight = 0usize;
        for (title, root, days, rule) in roots {
            let index = plan.categories.len();
            let mut category=Category {title,action:Action::Files {root:root.clone(),days},selected:true,reason:format!("Regular files with modification time older than {days} days{}.",rule.as_ref().map(|r|format!(", matching {}",r.pattern)).unwrap_or_default()),consequence:if rule.is_some() {"Files matching this saved rule are permanently removed; review its purpose before cleaning."}else{"Diagnostic history or generated previews are removed; installed dependencies remain in place."}.into(),skip:None,files:0,allocated:0,protected:0};
            if protected.protects(&root) {
                category.skip = Some("The entire folder is protected by an exclusion.".into());
                plan.categories.push(category);
                continue;
            }
            let files = if let Some(rule) = &rule {
                rules::candidates(rule)?
            } else {
                w::old_files(&root, days, protected)?
            };
            for path in files {
                if protected.protects(&path) {
                    category.protected += 1;
                    continue;
                }
                if !seen.insert(path.clone()) {
                    continue;
                }
                let Ok(meta) = fs::symlink_metadata(&path) else {
                    continue;
                };
                if !meta.is_file()
                    || meta.file_type().is_symlink()
                    || meta.uid() != unsafe { libc::geteuid() }
                {
                    continue;
                }
                use std::os::unix::ffi::OsStrExt;
                weight = weight.saturating_add(
                    path.as_os_str().as_bytes().len() + std::mem::size_of::<Candidate>() + 64,
                );
                if weight > CANDIDATE_BUDGET {
                    return Err("Cleanup candidates exceed the 16 MiB preview budget. Increase the minimum age or choose more specific folders before previewing again.".into());
                }
                let stamp = Stamp::from(&meta);
                category.files += 1;
                category.allocated = category.allocated.saturating_add(stamp.allocated);
                plan.candidates.push(Candidate {
                    category: index,
                    path,
                    stamp,
                });
            }
            plan.categories.push(category);
        }
        if cleanup.pip {
            let root = exclusions::resolve_missing(&w::home().join(".cache/pip"))?;
            plan.categories.push(Category {title:"Pip download/wheel cache".into(),action:Action::Pip(root.clone()),selected:true,reason:format!("All entries in the explicitly selected pip cache: {}.", root.display()),consequence:"Future installs may download and rebuild wheels again; virtual environments and installed packages remain in place.".into(),skip:protected.bulk_reason("pip",Some(&root)),files:0,allocated:0,protected:0});
        }
        if cleanup.trash {
            plan.categories.push(Category {title:"Desktop Trash".into(),action:Action::Trash,selected:true,reason:"All desktop Trash contents, including mounted-volume trash stores supported by GIO.".into(),consequence:"Permanently deletes trashed files; this cannot be undone.".into(),skip:protected.bulk_reason("trash",None),files:0,allocated:0,protected:0});
        }
        if inventory {
            if cleanup.apt
                && plan
                    .categories
                    .iter()
                    .any(|c| matches!(c.action, Action::Apt) && c.skip.is_none())
            {
                plan.inventory.push_str(&format!(
                    "Obsolete APT downloads (simulation):\n{}\n",
                    w::command("/usr/bin/apt-get", &["--simulate", "autoclean"])
                ));
            }
            if plan
                .categories
                .iter()
                .any(|c| matches!(c.action, Action::Docker(_)) && c.skip.is_none())
            {
                plan.inventory.push_str(&format!(
                    "Docker inventory (usage is not a deletion estimate):\n{}\n",
                    w::command("/usr/bin/docker", &["system", "df"])
                ));
            }
        }
        Ok(plan)
    }
    pub fn valid_for(&self, policy: &exclusions::Policy) -> Result<(), String> {
        if policy != &self.exclusions {
            return Err(
                "Exclusions changed after this preview. Preview again before cleaning.".into(),
            );
        }
        if w::now().saturating_sub(self.created) > 900 || self.created > w::now() + 5 {
            return Err("The cleanup preview expired or the clock changed. Preview again.".into());
        }
        Ok(())
    }
    pub fn render(&self) -> String {
        let mut lines = vec![format!(
            "Cleanup preview · {}\nExclusions revision {} · {} active protected paths\n",
            crate::presentation::when(self.created as i64),
            self.exclusions.revision,
            self.exclusions.paths.iter().filter(|p| p.enabled).count()
        )];
        for (index, c) in self.categories.iter().enumerate() {
            lines.push(format!(
                "{}{}",
                c.title,
                if c.skip.is_some() { " · Skipped" } else { "" }
            ));
            if let Some(reason) = &c.skip {
                lines.push(reason.clone());
                continue;
            }
            if matches!(c.action, Action::Files { .. }) {
                lines.push(format!(
                    "{}: {} files, {} allocated · {} protected paths skipped",
                    c.title,
                    c.files,
                    w::bytes(c.allocated),
                    c.protected
                ));
            }
            lines.push(format!("Why: {}\nEffect: {}", c.reason, c.consequence));
            for candidate in self
                .candidates
                .iter()
                .filter(|p| p.category == index)
                .take(20)
            {
                lines.push(format!(
                    "  {} · {}{}",
                    candidate.path.display(),
                    w::bytes(candidate.stamp.allocated),
                    if candidate.stamp.links > 1 {
                        " · hard-linked; blocks may remain allocated"
                    } else {
                        ""
                    }
                ));
            }
            if c.files > 20 {
                lines.push("  More candidates are available in the item list.".into());
            }
            lines.push(String::new());
        }
        lines.push(self.inventory.clone());
        lines.push("File sizes describe selected allocations, not guaranteed reclaimed space. Changed files are skipped during execution. Review the categories and item details before confirming.".into());
        lines.join("\n")
    }
    pub fn execute_user(&self) -> Result<String, String> {
        #[cfg(feature = "docs-demo")]
        if std::env::var_os("TEND_DEMO_DIR").is_some() {
            return Err("Deletion is disabled in documentation recordings.".into());
        }
        let guard = exclusions::Guard::acquire()?;
        self.valid_for(&guard.compiled.policy)?;
        for rule in self.custom.rules.iter().filter(|r| r.enabled) {
            rule.validate()?;
        }
        if crate::output("/usr/bin/id", &["-u"])? == "0" {
            return Err("User cleanup cannot run as root".into());
        }
        let before = crate::observations::filesystem_free(Path::new("/")).ok();
        let mut lines = Vec::new();
        let mut failed = false;
        for (index, category) in self.categories.iter().enumerate() {
            if let Some(reason) = &category.skip {
                lines.push(format!("{}: skipped · {reason}", category.title));
                continue;
            }
            let result = match &category.action {
                Action::Apt => {
                    lines.push(
                        "APT runs only through the system helper or daily maintenance.".into(),
                    );
                    continue;
                }
                Action::Docker(args) => crate::output(
                    "/usr/bin/docker",
                    &args.iter().map(String::as_str).collect::<Vec<_>>(),
                ),
                Action::Pip(root) => {
                    let current = exclusions::resolve_missing(root)?;
                    if &current != root {
                        return Err("The pip cache location changed after preview. Preview again before cleaning.".into());
                    }
                    if let Some(reason) = guard.compiled.bulk_reason("pip", Some(&current)) {
                        lines.push(format!("{}: skipped · {reason}", category.title));
                        continue;
                    }
                    crate::output(
                        "/usr/bin/python3",
                        &[
                            "-m",
                            "pip",
                            "--cache-dir",
                            &root.to_string_lossy(),
                            "cache",
                            "purge",
                        ],
                    )
                }
                Action::Trash => crate::output("/usr/bin/gio", &["trash", "--empty"]),
                Action::Files { root, days } => {
                    let mut removed = 0;
                    let mut skipped = 0;
                    let mut errors = 0;
                    for candidate in self.candidates.iter().filter(|c| c.category == index) {
                        match w::remove_old_file_guarded(
                            root,
                            &candidate.path,
                            *days,
                            &guard.compiled,
                            Some(&candidate.stamp),
                        ) {
                            Ok(true) => removed += 1,
                            Ok(false) => skipped += 1,
                            Err(_) => errors += 1,
                        }
                    }
                    let text=format!("removed {removed} files; {skipped} changed or protected items skipped; {errors} failures");
                    if errors > 0 {
                        Err(text)
                    } else {
                        Ok(text)
                    }
                }
            };
            match result {
                Ok(text) => lines.push(format!("{}: {text}", category.title)),
                Err(error) => {
                    failed = true;
                    lines.push(format!("{}: failed · {error}", category.title));
                }
            }
        }
        if let (Some(before), Ok(after)) =
            (before, crate::observations::filesystem_free(Path::new("/")))
        {
            lines.push(format!(
                "Root free-space change: {:.1} MiB (concurrent writes included)",
                (after as i128 - before as i128) as f64 / 1048576.
            ));
        }
        let mut report = lines.join("\n");
        if let Err(error) = w::record("cleanup", &report) {
            report.push_str(&format!(
                "\nCleanup results are above; history could not be saved: {error}"
            ));
        }
        if failed {
            Err(report)
        } else {
            Ok(report)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn previews_are_invalidated_by_exclusion_changes_and_expiry() {
        let mut c = w::Cleanup::default();
        c.apt = false;
        c.docker_build = false;
        c.docker_images = false;
        c.npm_logs = false;
        let mut plan = Plan::build(&c, &rules::Policy::default(), false).unwrap();
        let old = plan.exclusions.clone();
        let mut changed = old.clone();
        changed.revision += 1;
        assert!(plan.valid_for(&old).is_ok());
        assert!(plan.valid_for(&changed).is_err());
        plan.created = w::now().saturating_sub(901);
        assert!(plan.valid_for(&old).is_err());
    }
    #[test]
    fn replacements_after_preview_do_not_match_the_saved_identity() {
        let root = PathBuf::from("/tmp/opencode").join(format!(
            "tend-plan-stamp-{}-{}",
            std::process::id(),
            w::now()
        ));
        fs::create_dir_all(&root).unwrap();
        let file = root.join("cache.log");
        fs::write(&file, "old").unwrap();
        let stamp = Stamp::from(&fs::metadata(&file).unwrap());
        fs::rename(&file, root.join("retained-old.log")).unwrap();
        fs::write(&file, "new").unwrap();
        let c = std::ffi::CString::new(file.as_os_str().as_encoded_bytes()).unwrap();
        let mut s = unsafe { std::mem::zeroed() };
        assert_eq!(unsafe { libc::lstat(c.as_ptr(), &mut s) }, 0);
        assert!(!stamp.matches(&s));
        assert_eq!(fs::read_to_string(&file).unwrap(), "new");
    }
}
