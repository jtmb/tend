//! Typed daily cleanup policy. Does not control maintenance timing or updates.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
};
pub const CONFIG: &str = "/etc/tend/cleanup-policy.json";
pub const READY: &str = "/etc/tend/cleanup-policy-ready";
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    pub version: u32,
    pub apt: bool,
    pub docker_build: bool,
    pub docker_images: bool,
    pub npm_logs: bool,
    pub npm_verify: bool,
    pub build_days: u32,
    pub image_days: u32,
    pub keep_gb: u32,
    pub npm_log_days: u32,
    pub npm_verify_days: u32,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            version: 1,
            apt: true,
            docker_build: true,
            docker_images: true,
            npm_logs: true,
            npm_verify: true,
            build_days: 14,
            image_days: 14,
            keep_gb: 5,
            npm_log_days: 30,
            npm_verify_days: 7,
        }
    }
}
impl Policy {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || !(7..=3650).contains(&self.build_days)
            || !(7..=3650).contains(&self.image_days)
            || !(1..=100).contains(&self.keep_gb)
            || !(1..=3650).contains(&self.npm_log_days)
            || !(1..=30).contains(&self.npm_verify_days)
        {
            return Err("Docker ages: 7–3650 days; cache allowance: 1–100 GB; npm log age: 1–3650 days; npm verification: 1–30 days.".into());
        }
        Ok(())
    }
    pub fn user_phase(&self, name: &str) -> Result<crate::workstation::Cleanup, String> {
        self.validate()?;
        let mut c = crate::workstation::Cleanup {
            apt: false,
            docker_build: false,
            docker_images: false,
            npm_logs: false,
            age_days: self.build_days,
            image_days: Some(self.image_days),
            keep_gb: self.keep_gb,
            npm_log_days: self.npm_log_days,
            ..Default::default()
        };
        match name {
            "docker-build" if self.docker_build => c.docker_build = true,
            "docker-images" if self.docker_images => c.docker_images = true,
            "npm-logs" if self.npm_logs => c.npm_logs = true,
            _ => return Err("Unknown or disabled daily phase".into()),
        }
        Ok(c)
    }
    pub fn read() -> Result<Self, String> {
        let p = match fs::read_to_string(CONFIG) {
            Ok(s) => serde_json::from_str::<Self>(&s)
                .map_err(|e| format!("Invalid daily policy: {e}"))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => return Err(e.to_string()),
        };
        p.validate()?;
        Ok(p)
    }
    pub fn ready() -> bool {
        fs::symlink_metadata(READY)
            .map(|m| {
                m.is_file()
                    && !m.file_type().is_symlink()
                    && m.uid() == 0
                    && m.permissions().mode() & 0o022 == 0
            })
            .unwrap_or(false)
            && fs::read_to_string(READY)
                .map(|s| s.trim() == "1")
                .unwrap_or(false)
    }
    pub fn plan(&self) -> Result<String, String> {
        self.validate()?;
        let guard = crate::exclusions::Guard::acquire()?;
        let apt_root = if self.apt && guard.compiled.policy.active() {
            crate::exclusions::apt_root()
        } else {
            None
        };
        self.plan_with(&guard.compiled, apt_root.as_deref())
    }
    fn plan_with(
        &self,
        protected: &crate::exclusions::Compiled,
        apt_root: Option<&std::path::Path>,
    ) -> Result<String, String> {
        self.validate()?;
        let mut s = String::from("Daily cleanup actions — review only\n\n");
        let mut add = |on: bool, title: &str, cmd: String, skip: Option<String>| {
            s.push_str(&format!(
                "{} · {title}\n{}\n\n",
                if !on {
                    "Disabled"
                } else if skip.is_some() {
                    "Protected"
                } else {
                    "Enabled"
                },
                if !on {
                    "Skipped by policy".into()
                } else {
                    skip.unwrap_or(cmd)
                }
            ));
        };
        add(
            self.apt,
            "Obsolete APT downloads",
            "apt-get autoclean".into(),
            protected.bulk_reason("apt", apt_root),
        );
        add(
            self.docker_build,
            "Unused Docker build cache",
            format!(
                "docker builder prune --all --force --filter until={}h --keep-storage {}GB",
                self.build_days * 24,
                self.keep_gb
            ),
            protected.bulk_reason("docker", None),
        );
        add(
            self.docker_images,
            "Dangling Docker images",
            format!(
                "docker image prune --force --filter until={}h",
                self.image_days * 24
            ),
            protected.bulk_reason("docker", None),
        );
        add(
            self.npm_logs,
            "npm diagnostic logs",
            format!(
                "Regular *.log files older than {} days in ~/.npm/_logs; protected paths are omitted",
                self.npm_log_days
            ),
            protected.protects(&crate::workstation::home().join(".npm/_logs")).then(|| "The npm log folder is protected; category skipped.".into()),
        );
        add(
            self.npm_verify,
            "npm cache verification",
            format!(
                "npm cache verify · at most once every {} days",
                self.npm_verify_days
            ),
            protected.bulk_reason("npm-verify", None),
        );
        s.push_str("Containers, tagged images, networks and volumes are preserved.\nManual/cron folder rules are separate. Ubuntu owns temporary-file and journal cleanup.\nThis policy does not change updates, timer schedules, login or restart behavior.");
        Ok(s)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_policy_preserves_existing_daily_behavior() {
        let p = Policy::default();
        assert_eq!(p.build_days, 14);
        assert_eq!(p.npm_log_days, 30);
        assert_eq!(p.npm_verify_days, 7);
        assert!(p.plan().unwrap().contains("until=336h"));
    }
    #[test]
    fn invalid_daily_values_and_unknown_fields_are_rejected() {
        for s in [
            r#"{"keep_gb":0}"#,
            r#"{"npm_verify_days":31}"#,
            r#"{"build_days":6}"#,
            r#"{"command":"reboot"}"#,
            r#"{"apt":1}"#,
        ] {
            assert!(serde_json::from_str::<Policy>(s)
                .and_then(|p| p.validate().map_err(serde::de::Error::custom))
                .is_err());
        }
    }
    #[test]
    fn disabled_categories_are_explicit() {
        let p = Policy {
            docker_build: false,
            ..Policy::default()
        };
        let s = p.plan().unwrap();
        assert!(s.contains("Disabled · Unused Docker build cache\nSkipped by policy"));
        assert!(!s.contains("docker builder prune"));
    }
    #[test]
    fn explained_daily_preview_discloses_global_bulk_skips() {
        let protected = crate::exclusions::Policy {
            paths: vec![crate::exclusions::Entry {
                path: "/".into(),
                enabled: true,
                note: "Everything".into(),
            }],
            ..Default::default()
        }
        .compile()
        .unwrap();
        let report = Policy::default()
            .plan_with(
                &protected,
                Some(std::path::Path::new("/var/cache/apt/archives")),
            )
            .unwrap();
        for title in [
            "Obsolete APT downloads",
            "Unused Docker build cache",
            "Dangling Docker images",
            "npm cache verification",
        ] {
            assert!(report.contains(&format!("Protected · {title}")), "{report}");
        }
        assert!(report.contains("npm log folder is protected"));
        assert!(!report.contains("docker builder prune"));
        assert!(!report.contains("apt-get autoclean"));
    }
}

#[cfg(test)]
mod execution_tests {
    use super::*;
    #[test]
    fn shared_schema_contract_matches_python_engine() {
        let cases: serde_json::Value =
            serde_json::from_str(include_str!("../tests/policy-cases.json")).unwrap();
        for case in cases.as_array().unwrap() {
            let valid = serde_json::from_value::<Policy>(case["policy"].clone())
                .map(|p| p.validate().is_ok())
                .unwrap_or(false);
            assert_eq!(valid, case["valid"].as_bool().unwrap(), "{}", case["name"]);
        }
    }
    #[test]
    fn each_daily_user_phase_has_exactly_one_category_and_uses_daily_values() {
        let p = Policy {
            build_days: 30,
            image_days: 90,
            npm_log_days: 61,
            keep_gb: 8,
            ..Default::default()
        };
        for name in ["docker-build", "docker-images", "npm-logs"] {
            let c = p.user_phase(name).unwrap();
            assert_eq!(
                [c.docker_build, c.docker_images, c.npm_logs]
                    .iter()
                    .filter(|b| **b)
                    .count(),
                1
            );
            assert!(!c.apt && !c.pip && !c.trash && !c.thumbnails);
            assert_eq!(
                (c.age_days, c.image_days(), c.npm_log_days, c.keep_gb),
                (30, 90, 61, 8)
            );
        }
        assert!(p.user_phase("updates").is_err());
        assert!(Policy {
            npm_logs: false,
            ..p
        }
        .user_phase("npm-logs")
        .is_err());
    }
}
