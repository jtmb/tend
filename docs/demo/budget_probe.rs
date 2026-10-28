//! App-owned instrumentation, compiled only for the docs-demo budget build.
//! The same workload is copied into the reference build without feature changes.
use adw::prelude::*;
use gtk::{gio, glib};
use serde_json::{json, Value};
use std::{cell::Cell, fs, path::Path, rc::Rc, time::Duration};
type Hook = Rc<dyn Fn() -> Result<(), String>>;
type Stats = Rc<dyn Fn() -> Value>;
fn phase(path: &Path, name: &str, bus: &str, stats: &Stats, extra: Value) {
    let value =
        json!({"phase":name,"pid":std::process::id(),"bus":bus,"stats":stats(),"extra":extra});
    let temporary = path.with_extension("new");
    fs::write(&temporary, serde_json::to_vec(&value).unwrap()).unwrap();
    fs::rename(temporary, path).unwrap();
}
pub fn run(
    app: &adw::Application,
    win: &adw::ApplicationWindow,
    stack: &gtk::Stack,
    directory: &str,
    enable: Option<Hook>,
    stop: Hook,
    stats: Stats,
) {
    assert!(std::env::var_os("TEND_DEMO_DIR").is_some());
    let home = std::path::PathBuf::from(std::env::var_os("TEND_DEMO_HOME").unwrap());
    let directory = Path::new(directory);
    assert_eq!(directory.parent(), home.parent());
    assert!(home.join("demo-journal.jsonl").is_file());
    fs::create_dir_all(directory).unwrap();
    let path = directory.join("stage.json");
    let connection = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>).unwrap();
    let bus = connection.unique_name().unwrap().to_string();
    let group = gio::SimpleActionGroup::new();
    let begun = Rc::new(Cell::new(false));
    if let Some(enable) = enable {
        let action = gio::SimpleAction::new("enable", None);
        let (path, bus, stats, begun) = (path.clone(), bus.clone(), stats.clone(), begun.clone());
        action.connect_activate(move |_, _| {
            if begun.replace(true) {
                return;
            }
            if let Err(error) = enable() {
                phase(&path, "failed", &bus, &stats, json!({"error":error}));
                return;
            }
            let (path, bus, stats) = (path.clone(), bus.clone(), stats.clone());
            glib::timeout_add_local_once(Duration::from_secs(35), move || {
                phase(
                    &path,
                    "enabled_ready",
                    &bus,
                    &stats,
                    json!({"warm_seconds":35}),
                )
            });
        });
        group.add_action(&action);
    }
    let action = gio::SimpleAction::new("finish", None);
    let (path2, bus2, stats2, app2, finishing) = (
        path.clone(),
        bus.clone(),
        stats.clone(),
        app.clone(),
        Rc::new(Cell::new(false)),
    );
    action.connect_activate(move |_, _| {
        if finishing.replace(true) {
            return;
        }
        if let Err(error) = stop() {
            phase(&path2, "failed", &bus2, &stats2, json!({"error":error}));
            app2.quit();
            return;
        }
        let (path, bus, stats, app) = (path2.clone(), bus2.clone(), stats2.clone(), app2.clone());
        glib::timeout_add_local_once(Duration::from_secs(2), move || {
            let settled = stats();
            let (path, bus, stats, app) = (path.clone(), bus.clone(), stats.clone(), app.clone());
            glib::timeout_add_local_once(Duration::from_secs(17), move || {
                let after = stats();
                let stopped = settled["collections"] == after["collections"]
                    && after["subscribers"].as_u64().unwrap_or(0) == 0;
                phase(
                    &path,
                    "complete",
                    &bus,
                    &stats,
                    json!({"settled_stats":settled,"stopped":stopped}),
                );
                app.quit();
            });
        });
    });
    group.add_action(&action);
    connection
        .export_action_group("/TendBudget", &group)
        .unwrap();
    let (win, stack) = (win.clone(), stack.clone());
    let pages = [
        "task-manager",
        "care",
        "storage",
        "system-logs",
        "cleanup",
        "overview",
    ];
    let mut index = 0;
    win.set_default_size(1100, 760);
    glib::timeout_add_local(Duration::from_secs(3), move || {
        if index < pages.len() {
            stack.set_visible_child_name(pages[index]);
            index += 1;
            return glib::ControlFlow::Continue;
        }
        win.set_visible(false);
        let (path, bus, stats) = (path.clone(), bus.clone(), stats.clone());
        glib::timeout_add_local_once(Duration::from_secs(5), move || {
            phase(
                &path,
                "baseline_ready",
                &bus,
                &stats,
                json!({"workload":pages,"hidden":true}),
            )
        });
        glib::ControlFlow::Break
    });
}
