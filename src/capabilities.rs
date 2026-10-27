//! Read-only compatibility negotiation. Never infer a capability from a
//! software version or silently enable an integration that is not installed.
use serde::Serialize;
#[derive(Serialize)]
pub struct Capabilities {
    pub protocol: u32,
    pub version: &'static str,
    pub role: &'static str,
    pub structured_observations: bool,
    pub bounded_history: bool,
    pub cleanup_exclusions: bool,
    pub explained_cleanup: bool,
    pub maintenance_events: bool,
}
pub fn for_role(role: &'static str) -> Capabilities {
    Capabilities {
        protocol: 4,
        version: env!("CARGO_PKG_VERSION"),
        role,
        structured_observations: role == "worker",
        bounded_history: role == "worker",
        cleanup_exclusions: role == "worker" || role == "administrator",
        explained_cleanup: role == "worker" || role == "administrator",
        maintenance_events: role=="worker"||role=="administrator",
    }
}
pub fn json(role: &'static str) -> String {
    serde_json::to_string(&for_role(role)).unwrap()
}
