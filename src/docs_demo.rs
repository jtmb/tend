//! Optional documentation capture: real callbacks and isolated, labeled inputs.
use adw::prelude::*;
use gtk::{glib, Widget};
use serde_json::{json, Value};
use std::{fs, path::Path, time::Duration};

fn descendants(root: &impl IsA<Widget>) -> Vec<Widget> {
    let mut result = vec![root.as_ref().clone()];
    let mut child = root.as_ref().first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        result.extend(descendants(&widget));
    }
    result
}
fn action(
    trace: &mut Vec<Value>,
    frame: usize,
    name: &str,
    target: &impl IsA<Widget>,
    outer: &gtk::Box,
) {
    let rect = target
        .as_ref()
        .compute_bounds(outer)
        .expect("Mapped action target");
    trace.push(json!({"frame":frame,"name":name,
        "x":rect.x()+rect.width()/2.,"y":rect.y()+rect.height()/2.}));
}
fn button(root: &impl IsA<Widget>, name: &str) -> gtk::Button {
    descendants(root)
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .find(|b| {
            b.label().as_deref() == Some(name)
                || (b.has_css_class("nav-item")
                    && descendants(b).iter().any(|w| {
                        w.downcast_ref::<gtk::Label>()
                            .map(|l| l.text() == name)
                            .unwrap_or(false)
                    }))
        })
        .unwrap_or_else(|| panic!("Missing demo action: {name}"))
}
fn click(
    root: &impl IsA<Widget>,
    outer: &gtk::Box,
    trace: &mut Vec<Value>,
    frame: usize,
    name: &str,
) {
    let b = button(root, name);
    assert!(b.is_sensitive(), "Demo action is still pending: {name}");
    action(trace, frame, name, &b, outer);
    b.emit_clicked();
}
fn page(stack: &gtk::Stack, name: &str) -> Widget {
    stack.child_by_name(name).expect("Demo page exists")
}
fn bottom(stack: &gtk::Stack, name: &str) {
    if let Some(scroll) = page(stack, name).downcast_ref::<gtk::ScrolledWindow>() {
        let adjustment = scroll.vadjustment();
        adjustment.set_value(adjustment.upper() - adjustment.page_size());
    }
}
fn text(root: &impl IsA<Widget>) -> String {
    descendants(root)
        .iter()
        .filter_map(|w| {
            if let Some(l) = w.downcast_ref::<gtk::Label>() {
                Some(l.text().to_string())
            } else {
                w.downcast_ref::<gtk::TextView>().map(|v| {
                    let b = v.buffer();
                    b.text(&b.start_iter(), &b.end_iter(), false).to_string()
                })
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn dropdown(stack: &gtk::Stack, tooltip: &str) -> gtk::DropDown {
    descendants(&page(stack, "system-logs"))
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::DropDown>().ok())
        .find(|w| w.tooltip_text().as_deref() == Some(tooltip))
        .expect("Demo filter exists")
}
fn performance(stack: &gtk::Stack) {
    for w in descendants(&page(stack, "task-manager")) {
        if let Some(tabs) = w.downcast_ref::<gtk::Stack>() {
            if tabs.child_by_name("performance").is_some() {
                tabs.set_visible_child_name("performance");
            }
        }
        if let Some(speed) = w.downcast_ref::<gtk::DropDown>() {
            speed.set_selected(1);
        }
    }
}
fn refresh(stack: &gtk::Stack) {
    button(&page(stack, "task-manager"), "Refresh now").emit_clicked();
}

pub fn record(
    app: &adw::Application,
    win: &adw::ApplicationWindow,
    outer: &gtk::Box,
    stack: &gtk::Stack,
    directory: &str,
) {
    let directory = Path::new(directory).to_path_buf();
    let home = tend::workstation::home();
    assert_eq!(
        home.parent(),
        directory.parent(),
        "Only sibling fixture directories are supported"
    );
    assert!(
        home.join("demo-journal.jsonl").is_file(),
        "Prepare isolated inputs first"
    );
    fs::create_dir_all(&directory).expect("Create frame directory");
    assert!(
        fs::read_dir(&directory).unwrap().next().is_none(),
        "Use a fresh frame directory"
    );
    win.set_default_size(1100, 720);
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
    stack.set_visible_child_name("task-manager");
    performance(stack);
    let (app, win, outer, stack) = (app.clone(), win.clone(), outer.clone(), stack.clone());
    let paint = gtk::WidgetPaintable::new(Some(&outer));
    let mut tick = 0usize;
    let warmup = 128;
    let mut trace = Vec::new();
    let mut evidence = Vec::new();
    glib::timeout_add_local(Duration::from_millis(125), move || {
        tick += 1;
        if tick % 64 == 0 {
            eprintln!("Demo capture tick {tick}");
        }
        if tick < warmup {
            if tick == warmup - 8 {
                stack.set_visible_child_name("overview");
            }
            if tick % 8 == 0 {
                refresh(&stack);
            }
            return glib::ControlFlow::Continue;
        }
        let frame = tick - warmup;
        match frame {
            0 | 280 => stack.set_visible_child_name("overview"),
            16 => click(&outer, &outer, &mut trace, frame, "Open disk cleanup"),
            32 => bottom(&stack, "cleanup"),
            48 => click(
                &page(&stack, "cleanup"),
                &outer,
                &mut trace,
                frame,
                "Preview cleanup",
            ),
            72 => {
                let report = text(&page(&stack, "cleanup"));
                assert!(
                    report.contains("2 files") && report.contains("3.0 MiB"),
                    "Preview did not produce the expected real file inventory: {report}"
                );
                evidence.push(json!({"frame":frame,"preview":report}));
                // Show the actual eligible filenames in the native report view.
                for w in descendants(&page(&stack, "cleanup")) {
                    if let Some(view) = w.downcast_ref::<gtk::TextView>() {
                        let buffer = view.buffer();
                        let mut iter = buffer.end_iter();
                        view.scroll_to_iter(&mut iter, 0.0, false, 0.0, 0.0);
                    }
                }
            }
            112 => {
                click(&outer, &outer, &mut trace, frame, "Task Manager");
                performance(&stack);
            }
            144 => {
                for w in descendants(&page(&stack, "task-manager")) {
                    if let Some(row) = w.downcast_ref::<gtk::ListBoxRow>() {
                        if row.widget_name() == "memory" {
                            action(&mut trace, frame, "Memory", row, &outer);
                            row.parent()
                                .and_downcast::<gtk::ListBox>()
                                .unwrap()
                                .select_row(Some(row));
                        }
                    }
                }
            }
            160 => click(&outer, &outer, &mut trace, frame, "System logs"),
            168 => click(
                &page(&stack, "system-logs"),
                &outer,
                &mut trace,
                frame,
                "Load logs",
            ),
            192 => {
                let result = text(&page(&stack, "system-logs"));
                assert!(
                    result.contains("6 matching events"),
                    "Log load failed: {result}"
                );
                evidence.push(json!({"frame":frame,"loaded":6}));
            }
            200 => {
                let filter = dropdown(&stack, "Severity filter");
                action(&mut trace, frame, "Errors and critical", &filter, &outer);
                filter.set_selected(1);
            }
            220 => {
                assert!(text(&page(&stack, "system-logs")).contains("2 matching events"));
                evidence.push(json!({"frame":frame,"errors":2}));
            }
            224 | 226 | 228 | 230 | 232 | 234 => {
                for w in descendants(&page(&stack, "system-logs")) {
                    if let Some(search) = w.downcast_ref::<gtk::SearchEntry>() {
                        if frame == 224 {
                            action(&mut trace, frame, "Search editor", search, &outer);
                            search.grab_focus();
                        }
                        search.set_text(&"editor"[..((frame - 224) / 2 + 1)]);
                    }
                }
            }
            244 => {
                assert!(text(&page(&stack, "system-logs")).contains("1 matching events"));
                evidence.push(json!({"frame":frame,"searched":1}));
            }
            248 => {
                for w in descendants(&page(&stack, "system-logs")) {
                    if let Some(list) = w.downcast_ref::<gtk::ListBox>() {
                        if let Some(row) = list.row_at_index(0) {
                            action(&mut trace, frame, "Inspect event", &row, &outer);
                            list.select_row(Some(&row));
                        }
                    }
                }
                bottom(&stack, "system-logs");
            }
            264 => {
                assert!(text(&page(&stack, "system-logs")).contains("PID: 4100"));
                evidence.push(json!({"frame":frame,"event_details":true}));
            }
            304 => {
                fs::write(
                    directory.parent().unwrap().join("actions.json"),
                    serde_json::to_vec_pretty(&trace).unwrap(),
                )
                .unwrap();
                fs::write(
                    directory.parent().unwrap().join("evidence.json"),
                    serde_json::to_vec_pretty(&evidence).unwrap(),
                )
                .unwrap();
                app.quit();
                return glib::ControlFlow::Break;
            }
            _ => (),
        }
        if (112..160).contains(&frame) && frame % 8 == 0 {
            refresh(&stack);
        }
        let snapshot = gtk::Snapshot::new();
        paint.snapshot(&snapshot, outer.width() as f64, outer.height() as f64);
        if let (Some(node), Some(renderer)) = (snapshot.to_node(), win.renderer()) {
            renderer
                .render_texture(&node, None)
                .save_to_png(directory.join(format!("{frame:04}.png")))
                .expect("Save native frame");
        } else {
            panic!("Native preview is not mapped");
        }
        glib::ControlFlow::Continue
    });
}

/// Isolated native interaction checks and a complete page contact sheet.
/// Never activates cleanup, system saves, schedules, updates or process actions.
pub fn qa(
    app: &adw::Application,
    win: &adw::ApplicationWindow,
    outer: &gtk::Box,
    stack: &gtk::Stack,
    directory: &str,
) {
    assert!(std::env::var_os("TEND_DEMO_DIR").is_some());
    let directory = Path::new(directory).to_owned();
    fs::create_dir_all(&directory).unwrap();
    let (app, win, outer, stack) = (app.clone(), win.clone(), outer.clone(), stack.clone());
    stack.set_transition_type(gtk::StackTransitionType::None);
    let root = page(&stack, "cleanup");
    let spins = descendants(&root)
        .iter()
        .filter_map(|w| w.downcast_ref::<gtk::SpinButton>().cloned())
        .collect::<Vec<_>>();
    assert_eq!(spins.len(), 5);
    let original = fs::read(tend::workstation::home().join(".config/tend/cleanup.json")).unwrap();
    let paint = gtk::WidgetPaintable::new(Some(&outer));
    let pages = [
        "overview",
        "cleanup",
        "daily-cleanup",
        "cleanup-scheduling",
        "storage",
        "task-manager",
        "system-logs",
        "startup",
        "tuning",
        "development",
        "reports",
        "recovery",
        "care",
        "schedule",
        "activity",
    ];
    let mut tick = 0usize;
    let mut evidence = vec![];
    glib::timeout_add_local(Duration::from_millis(250), move || {
        tick += 1;
        match tick {
            1 => {
                stack.set_visible_child_name("cleanup");
                spins[3].set_value(45.);
                assert!(text(&root).contains("Unsaved changes"));
            }
            2 => {
                button(&root, "Restore defaults").emit_clicked();
                assert_eq!(
                    fs::read(tend::workstation::home().join(".config/tend/cleanup.json")).unwrap(),
                    original
                );
                button(&root, "Discard changes").emit_clicked();
                assert_eq!(spins[3].value_as_int(), 30);
                evidence.push(json!({"defaults_do_not_save":true,"discard_restores":true}));
            }
            3 => {
                spins[3].set_value(45.);
                button(&root, "Save policy").emit_clicked();
                assert_eq!(tend::workstation::Cleanup::load().unwrap().npm_log_days, 45);
                evidence.push(json!({"save_reload":45}));
            }
            4 => {
                button(&root, "Preview cleanup").emit_clicked();
                spins[0].set_value(31.);
            }
            8 => {
                assert!(
                    text(&root).contains("Settings changed while the preview was running"),
                    "stale preview check failed: {}",
                    text(&root)
                );
                assert!(!button(&root, "Clean selected…").is_sensitive());
                evidence.push(json!({"stale_preview_rejected":true}));
                button(&root, "Preview cleanup").emit_clicked();
            }
            12 => {
                assert!(
                    button(&root, "Clean selected…").is_sensitive(),
                    "Fresh preview did not enable cleanup: {}",
                    text(&root)
                );
                spins[0].set_value(32.);
                assert!(!button(&root, "Clean selected…").is_sensitive());
                button(&root, "Discard changes").emit_clicked();
                evidence.push(json!({"fresh_preview_then_edit_invalidates":true}));
            }
            13 => {
                let daily = page(&stack, "daily-cleanup");
                let fields = descendants(&daily)
                    .iter()
                    .filter_map(|w| w.downcast_ref::<gtk::SpinButton>().cloned())
                    .collect::<Vec<_>>();
                fields[2].set_value(12.);
                button(&daily, "Preview daily policy").emit_clicked();
                assert!(text(&daily).contains("12GB"));
                assert_eq!(tend::workstation::Cleanup::load().unwrap().keep_gb, 5);
                evidence.push(json!({"independent_policies":true}));
                button(&daily, "Discard changes").emit_clicked();
            }
            _ => {}
        }
        if tick >= 16 {
            let frame = tick - 16;
            let index = frame / 4;
            if index < pages.len() * 2 {
                let id = pages[index % pages.len()];
                if frame % 4 == 0 {
                    assert!(stack.child_by_name(id).is_some(), "QA page missing: {id}");
                    stack.set_visible_child_name(id);
                    adw::StyleManager::default().set_color_scheme(if index < pages.len() {
                        adw::ColorScheme::ForceDark
                    } else {
                        adw::ColorScheme::ForceLight
                    });
                    win.set_default_size(if index < pages.len() { 1100 } else { 820 }, 760);
                }
                if frame % 4 == 3 {
                    let snapshot = gtk::Snapshot::new();
                    paint.snapshot(&snapshot, outer.width() as f64, outer.height() as f64);
                    let texture = win
                        .renderer()
                        .unwrap()
                        .render_texture(&snapshot.to_node().unwrap(), None);
                    texture
                        .save_to_png(directory.join(format!("{:02}-{id}.png", index)))
                        .unwrap();
                }
            } else if index < pages.len() * 2 + 4 {
                let i = index - pages.len() * 2;
                let id = if i < 2 { "cleanup" } else { "daily-cleanup" };
                if frame % 4 == 0 {
                    stack.set_visible_child_name(id);
                    bottom(&stack, id);
                    adw::StyleManager::default().set_color_scheme(if i % 2 == 0 {
                        adw::ColorScheme::ForceDark
                    } else {
                        adw::ColorScheme::ForceLight
                    });
                }
                if frame % 4 == 3 {
                    let snapshot = gtk::Snapshot::new();
                    paint.snapshot(&snapshot, outer.width() as f64, outer.height() as f64);
                    win.renderer()
                        .unwrap()
                        .render_texture(&snapshot.to_node().unwrap(), None)
                        .save_to_png(directory.join(format!("bottom-{i}-{id}.png")))
                        .unwrap();
                }
            } else if index < pages.len() * 2 + 9 {
                let scenario = index - pages.len() * 2 - 4;
                if frame % 4 == 0 {
                    match scenario {
                        0 => {
                            let config = tend::workstation::home()
                                .join(".config/tend/cleanup-schedule.json");
                            fs::write(
                                config,
                                r#"{"enabled":true,"expression":"0 10 * * *","catch_up":true}"#,
                            )
                            .unwrap();
                            stack.set_visible_child_name("cleanup");
                            button(&root, "Save policy").emit_clicked();
                        }
                        1 => {
                            button(&root, "Add rule…").emit_clicked();
                        }
                        2 => {
                            fs::write(
                                tend::workstation::home().join(".config/tend/cleanup.json"),
                                "{",
                            )
                            .unwrap();
                            stack.remove(&page(&stack, "cleanup"));
                            crate::cleanup_policy_ui::manual(&stack, &win);
                            stack.set_visible_child_name("cleanup");
                        }
                        3 => {
                            fs::write(
                                tend::workstation::home().join(".config/tend/rules.json"),
                                "{",
                            )
                            .unwrap();
                            stack.remove(&page(&stack, "cleanup"));
                            crate::cleanup_policy_ui::manual(&stack, &win);
                            stack.set_visible_child_name("cleanup");
                            bottom(&stack, "cleanup");
                        }
                        4 => {
                            fs::write(
                                tend::workstation::home()
                                    .join(".config/tend/cleanup-schedule.json"),
                                "{",
                            )
                            .unwrap();
                            stack.remove(&page(&stack, "cleanup-scheduling"));
                            crate::cleanup_ui::schedule_page(&stack);
                            stack.set_visible_child_name("cleanup-scheduling");
                        }
                        _ => unreachable!(),
                    }
                }
                if frame % 4 == 3 {
                    if scenario < 2 {
                        let dialog = gtk::Window::list_toplevels()
                            .into_iter()
                            .filter_map(|w| w.downcast::<adw::MessageDialog>().ok())
                            .find(|w| w.is_visible())
                            .expect("Expected native confirmation or rule editor");
                        let view = gtk::WidgetPaintable::new(Some(&dialog));
                        let snapshot = gtk::Snapshot::new();
                        view.snapshot(&snapshot, dialog.width() as f64, dialog.height() as f64);
                        dialog
                            .renderer()
                            .unwrap()
                            .render_texture(&snapshot.to_node().unwrap(), None)
                            .save_to_png(directory.join(format!("dialog-{scenario}.png")))
                            .unwrap();
                        dialog.response("cancel");
                        evidence.push(json!({"dialog_review":scenario,"cancelled":true}));
                    } else {
                        let snapshot = gtk::Snapshot::new();
                        paint.snapshot(&snapshot, outer.width() as f64, outer.height() as f64);
                        win.renderer()
                            .unwrap()
                            .render_texture(&snapshot.to_node().unwrap(), None)
                            .save_to_png(directory.join(format!("error-{scenario}.png")))
                            .unwrap();
                        let id = if scenario == 4 {
                            "cleanup-scheduling"
                        } else {
                            "cleanup"
                        };
                        assert!(text(&page(&stack, id)).contains("Cannot load"));
                        evidence.push(json!({"invalid_file_visible":scenario}));
                    }
                }
            } else {
                fs::write(
                    directory.join("evidence.json"),
                    serde_json::to_vec_pretty(&evidence).unwrap(),
                )
                .unwrap();
                app.activate_action("quit", None);
                return glib::ControlFlow::Break;
            }
        }
        glib::ControlFlow::Continue
    });
}
