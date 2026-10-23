use serde::{Deserialize, Serialize};
use std::{fs, process::Command};
pub const CONFIG: &str = "/etc/tend/settings.json";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub enabled: bool,
    pub hour: u8,
    pub minute: u8,
    pub reboot: bool,
    pub warnings: [bool; 3],
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            hour: 8,
            minute: 0,
            reboot: true,
            warnings: [true; 3],
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.hour > 23 || self.minute > 59 {
            Err("Choose a valid 24-hour time.".into())
        } else {
            Ok(())
        }
    }
    pub fn read() -> Result<Self, String> {
        match fs::read_to_string(CONFIG) {
            Ok(s) => {
                let value: Self = serde_json::from_str(&s).map_err(|e| e.to_string())?;
                value.validate()?;
                Ok(value)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.to_string()),
        }
    }
    pub fn warning_time(&self, lead: i32) -> String {
        let minutes = (self.hour as i32 * 60 + self.minute as i32 - lead).rem_euclid(1440);
        format!("{:02}:{:02}:00", minutes / 60, minutes % 60)
    }
}
pub fn output(program: &str, args: &[&str]) -> Result<String, String> {
    let result = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if result.status.success() {
        Ok(String::from_utf8_lossy(&result.stdout).trim().into())
    } else {
        Err(String::from_utf8_lossy(&result.stderr).trim().to_string())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn midnight_warnings_wrap() {
        let s = Settings {
            hour: 0,
            minute: 10,
            ..Default::default()
        };
        assert_eq!(s.warning_time(60), "23:10:00");
        assert_eq!(s.warning_time(15), "23:55:00");
    }
    #[test]
    fn invalid_schedule_rejected() {
        assert!(Settings {
            hour: 24,
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(Settings {
            minute: 60,
            ..Default::default()
        }
        .validate()
        .is_err());
    }
    #[test]
    fn unknown_fields_rejected() {
        assert!(serde_json::from_str::<Settings>(r#"{"enabled":true,"hour":8,"minute":0,"reboot":true,"warnings":[true,true,true],"command":"bad"}"#).is_err());
    }
}
pub mod cleanup_rules;
pub mod preferences;
pub mod system_logs;
pub mod task_stats;
pub mod workstation;

pub mod presentation;
