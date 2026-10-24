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
    assert!(
        rect.x() >= 0.
            && rect.y() >= 0.
            && rect.x() + rect.width() <= outer.width() as f32
            && rect.y() + rect.height() <= outer.height() as f32,
        "Off-screen demo target: {name}"
    );
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
    if let Some(scroll) = descendants(&page(stack, name))
        .into_iter()
        .find_map(|w| w.downcast::<gtk::ScrolledWindow>().ok())
    {
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
                bottom(&stack, "cleanup");
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

fn design_initial_checks(outer: &gtk::Box, stack: &gtk::Stack) {
    for id in ["cleanup", "daily-cleanup", "cleanup-scheduling", "schedule"] {
        let root = page(stack, id);
        assert_eq!(root.widget_name(), "editor-page");
        let save = button(&root, "Save changes");
        assert!(!save.is_sensitive(), "Unchanged {id} can save");
        assert!(!button(&root, "Discard changes").is_sensitive());
        assert!(save.ancestor(adw::HeaderBar::static_type()).is_none());
        assert!(descendants(&root)
            .iter()
            .any(|w| w.widget_name() == "editor-footer"));
    }
    for w in descendants(outer) {
        if let Some(header) = w.downcast_ref::<adw::HeaderBar>() {
            assert!(!text(header).contains("Save changes"));
        }
    }
    let schedule = page(stack, "cleanup-scheduling");
    let mode = descendants(&schedule)
        .into_iter()
        .find_map(|w| w.downcast::<gtk::DropDown>().ok())
        .unwrap();
    assert_eq!(mode.selected(), 0, "Default cron must display as Daily");
    assert!(text(&page(stack, "daily-cleanup")).contains("System integration update required"));
    assert!(text(&schedule).contains("Using effective defaults"));
    // Render saved weekly and complex expressions through the real editor.
    // These are fixture JSON files only, never installed cron jobs.
    let config = tend::workstation::home().join(".config/tend/cleanup-schedule.json");
    for (i, expression, mode_expected, h, m, weekday) in [
        (0, "05 08 * * 4", 1, 8, 5, 4),
        (1, "*/15 8-18 * * 1-5", 2, 10, 0, 0),
        (2, "00  09 * * *", 0, 9, 0, 0),
    ] {
        fs::write(
            &config,
            serde_json::to_string(&tend::cleanup_rules::Schedule {
                expression: expression.into(),
                ..Default::default()
            })
            .unwrap(),
        )
        .unwrap();
        stack.remove(&page(stack, "cleanup-scheduling"));
        crate::cleanup_ui::schedule_page(stack);
        let cron = page(stack, "cleanup-scheduling");
        let drops = descendants(&cron)
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::DropDown>().ok())
            .collect::<Vec<_>>();
        let spins = descendants(&cron)
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::SpinButton>().ok())
            .collect::<Vec<_>>();
        assert_eq!(drops[0].selected(), mode_expected);
        assert_eq!(drops[1].selected(), weekday);
        assert_eq!((spins[0].value_as_int(), spins[1].value_as_int()), (h, m));
        let toggle = descendants(&cron)
            .into_iter()
            .find_map(|w| w.downcast::<gtk::Switch>().ok())
            .unwrap();
        toggle.set_active(true);
        assert!(button(&cron, "Save changes").is_sensitive());
        button(&cron, "Discard changes").emit_clicked();
        assert!(!button(&cron, "Save changes").is_sensitive());
        assert_eq!(
            tend::cleanup_rules::Schedule::load().unwrap().expression,
            expression
        );
        fs::rename(&config, config.with_extension(format!("qa-case-{i}"))).unwrap();
    }
    stack.remove(&page(stack, "cleanup-scheduling"));
    crate::cleanup_ui::schedule_page(stack);
    // The exact footer used by all four editors, with simulated completion. No
    // storage adapter, cron command or system helper is involved in these states.
    let footer = crate::editor::Footer::new(true, true, None);
    assert!(!footer.save.is_sensitive());
    footer.sync(true, true);
    assert!(footer.save.is_sensitive());
    assert!(footer.discard.is_sensitive());
    footer.saving();
    assert!(!footer.save.is_sensitive());
    assert!(!footer.discard.is_sensitive());
    footer.error("Injected permission failure");
    assert!(footer.save.is_sensitive());
    assert!(text(&footer.root).contains("draft is preserved"));
    footer.success();
    assert!(!footer.save.is_sensitive());
    assert!(text(&footer.root).contains("Saved"));
    footer.sync(true, false);
    assert!(!footer.save.is_sensitive());
    let unavailable = crate::editor::Footer::new(false, true, Some("Test helper unavailable"));
    unavailable.sync(true, true);
    assert!(!unavailable.save.is_sensitive());
    assert!(unavailable.discard.is_sensitive());
    // Appearance: force rename failures only inside the fresh fixture home.
    let care = page(stack, "care");
    let theme = descendants(&care)
        .into_iter()
        .find_map(|w| w.downcast::<gtk::DropDown>().ok())
        .unwrap();
    let path = gtk::glib::user_config_dir().join("tend/theme");
    let previous = theme.selected();
    if path.exists() {
        fs::rename(&path, path.with_extension("qa-backup")).unwrap();
    }
    fs::create_dir(&path).unwrap();
    theme.set_selected((previous + 1) % 3);
    assert_eq!(
        theme.selected(),
        previous,
        "Theme failure must restore previous control"
    );
    assert!(text(&care).contains("previous preference was restored"));
    fs::rename(&path, path.with_extension("qa-failed-target")).unwrap();
    if path.with_extension("qa-backup").exists() {
        fs::rename(path.with_extension("qa-backup"), &path).unwrap();
    }
    theme.set_selected((previous + 1) % 3);
    theme.set_selected(previous);
    let toggle = descendants(&care)
        .into_iter()
        .find_map(|w| w.downcast::<gtk::Switch>().ok())
        .unwrap();
    let path = gtk::glib::user_config_dir().join("tend/background");
    let previous = toggle.is_active();
    if path.exists() {
        fs::rename(&path, path.with_extension("qa-backup")).unwrap();
    }
    fs::create_dir(&path).unwrap();
    toggle.set_active(!previous);
    assert_eq!(
        toggle.is_active(),
        previous,
        "Close preference failure must restore previous value"
    );
    fs::rename(&path, path.with_extension("qa-failed-target")).unwrap();
    if path.with_extension("qa-backup").exists() {
        fs::rename(path.with_extension("qa-backup"), &path).unwrap();
    }
    toggle.set_active(!previous);
    toggle.set_active(previous);
    // A draft survives navigating away, and Discard writes neither policy.
    let m = page(stack, "schedule");
    let hour = descendants(&m)
        .into_iter()
        .find_map(|w| w.downcast::<gtk::SpinButton>().ok())
        .unwrap();
    let before = hour.value_as_int();
    hour.set_value(((before + 1) % 24) as f64);
    stack.set_visible_child_name("schedule");
    stack.set_visible_child_name("care");
    stack.set_visible_child_name("schedule");
    assert_eq!(hour.value_as_int(), (before + 1) % 24);
    button(&m, "Discard changes").emit_clicked();
    assert_eq!(hour.value_as_int(), before);
    // Empty/report/loading/error use the actual shared report state widget.
    let parent = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let report = crate::workbench::text_panel(&parent);
    let states = report
        .ancestor(gtk::Stack::static_type())
        .and_downcast::<gtk::Stack>()
        .unwrap();
    assert_eq!(states.visible_child_name().as_deref(), Some("empty"));
    report.buffer().set_text("Working…");
    assert_eq!(states.visible_child_name().as_deref(), Some("loading"));
    report.buffer().set_text("Collected diagnostic output");
    assert_eq!(states.visible_child_name().as_deref(), Some("report"));
    report
        .buffer()
        .set_text("Could not complete operation: injected error");
    assert_eq!(states.visible_child_name().as_deref(), Some("error"));
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
    design_initial_checks(&outer, &stack);
    let matrix_count = if std::env::var_os("TEND_QA_STATE_ONLY").is_some() {
        0
    } else {
        pages.len() * 6
    };
    let mut tick = 0usize;
    let mut evidence = vec![
        json!({"shared_footer_states":true,"appearance_failure_rollback":true,"cron_daily_initialization":true,"navigation_preserves_drafts":true,"report_states":true}),
    ];
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
                let save = button(&root, "Save changes");
                let mut reached = false;
                for _ in 0..80 {
                    root.child_focus(gtk::DirectionType::TabForward);
                    if save.has_focus() {
                        reached = true;
                        break;
                    }
                }
                assert!(reached, "Save footer cannot be reached with Tab");
                save.emit_clicked();
                evidence.push(json!({"keyboard_save_reachable":true}));
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
                assert!(
                    !text(&daily).contains("12GB"),
                    "Discard left a stale daily preview"
                );
                let cron = page(&stack, "cleanup-scheduling");
                button(&cron, "Preview schedule").emit_clicked();
                descendants(&cron)
                    .into_iter()
                    .find_map(|w| w.downcast::<gtk::SpinButton>().ok())
                    .unwrap()
                    .set_value(11.);
            }
            14 => {
                // Force a local policy-save failure without touching any original
                // fixture input. Restore the pathname afterward; keep artifacts.
                assert!(text(&page(&stack, "cleanup-scheduling"))
                    .contains("Schedule changed while the preview was running"));
                evidence.push(json!({"stale_daily_and_cron_previews_rejected":true}));
                let path = tend::workstation::home().join(".config/tend/cleanup.json");
                fs::rename(&path, path.with_extension("qa-backup")).unwrap();
                fs::create_dir(&path).unwrap();
                spins[3].set_value(47.);
                button(&root, "Save changes").emit_clicked();
                assert!(text(&root).contains("draft is preserved"));
                assert_eq!(spins[3].value_as_int(), 47);
                button(&root, "Discard changes").emit_clicked();
                assert_eq!(spins[3].value_as_int(), 45);
                fs::rename(&path, path.with_extension("qa-failed-target")).unwrap();
                fs::rename(path.with_extension("qa-backup"), &path).unwrap();
                let cron = page(&stack, "cleanup-scheduling");
                let enabled = descendants(&cron)
                    .into_iter()
                    .find_map(|w| w.downcast::<gtk::Switch>().ok())
                    .unwrap();
                enabled.set_active(true);
                button(&cron, "Save changes").emit_clicked();
                evidence.push(json!({"manual_failure_keeps_draft":true}));
            }
            15 => {
                let cron = page(&stack, "cleanup-scheduling");
                assert!(text(&cron).contains("draft is preserved"));
                button(&cron, "Discard changes").emit_clicked();
                assert!(!button(&cron, "Save changes").is_sensitive());
                assert!(!tend::workstation::home()
                    .join(".config/tend/cleanup-schedule.json")
                    .exists());
                evidence.push(
                    json!({"cron_failure_keeps_draft":true,"cron_discard_does_not_write":true}),
                );
            }
            _ => {}
        }
        if tick >= 16 {
            let frame = tick - 16;
            let index = frame / 4;
            if index < matrix_count {
                let id = pages[index % pages.len()];
                if frame % 4 == 0 {
                    assert!(stack.child_by_name(id).is_some(), "QA page missing: {id}");
                    stack.set_visible_child_name(id);
                    adw::StyleManager::default().set_color_scheme(
                        if (index / pages.len()) % 2 == 0 {
                            adw::ColorScheme::ForceDark
                        } else {
                            adw::ColorScheme::ForceLight
                        },
                    );
                    win.set_default_size([720, 960, 1280][index / (pages.len() * 2)], 760);
                }
                if frame % 4 == 3 {
                    let navigation = descendants(&outer)
                        .into_iter()
                        .find_map(|w| w.downcast::<adw::OverlaySplitView>().ok())
                        .unwrap();
                    assert_eq!(navigation.is_collapsed(), outer.width() < 860);
                    assert_eq!(
                        navigation.shows_sidebar(),
                        outer.width() >= 860,
                        "Desktop navigation must remain visible"
                    );
                    let root = page(&stack, id);
                    if root.widget_name() == "editor-page" {
                        let save = button(&root, "Save changes");
                        let bounds = save.compute_bounds(&outer).unwrap();
                        assert!(
                            bounds.y() >= 0.
                                && bounds.y() + bounds.height() <= outer.height() as f32,
                            "Unreachable footer on {id}"
                        );
                        assert!(
                            bounds.x() >= 0. && bounds.x() + bounds.width() <= outer.width() as f32,
                            "Clipped footer on {id}"
                        );
                        assert!(save.is_mapped());
                        evidence.push(json!({"page":id,"footer_in_view":true}));
                    }
                    evidence.push(json!({"page":id,"width":outer.width(),"height":outer.height(),"theme":if (index/pages.len())%2==0 {"dark"} else {"light"}}));
                    let snapshot = gtk::Snapshot::new();
                    paint.snapshot(&snapshot, outer.width() as f64, outer.height() as f64);
                    let texture = win
                        .renderer()
                        .unwrap()
                        .render_texture(&snapshot.to_node().unwrap(), None);
                    texture
                        .save_to_png(directory.join(format!("{:02}-{id}.png", index)))
                        .unwrap();
                    if index == 0 {
                        let toggle = descendants(&outer)
                            .into_iter()
                            .find(|w| w.widget_name() == "navigation-toggle")
                            .unwrap()
                            .downcast::<gtk::ToggleButton>()
                            .unwrap();
                        toggle.set_active(true);
                        assert!(navigation.shows_sidebar());
                        button(&outer, "Maintenance").emit_clicked();
                        assert_eq!(stack.visible_child_name().as_deref(), Some("schedule"));
                        assert!(!navigation.shows_sidebar());
                        stack.set_visible_child_name(id);
                        evidence.push(json!({"compact_navigation_opens_selects_and_closes":true}));
                    }
                }
            } else if index < matrix_count + 4 {
                let i = index - matrix_count;
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
            } else if index < matrix_count + 9 {
                let scenario = index - matrix_count - 4;
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
                            spins[3].set_value(46.);
                            button(&root, "Save changes").emit_clicked();
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
                        if scenario == 2 {
                            let message = descendants(&page(&stack, id))
                                .into_iter()
                                .filter_map(|w| w.downcast::<gtk::Label>().ok())
                                .find(|l| l.text().starts_with("Cannot load policy."))
                                .unwrap();
                            let bounds = message.compute_bounds(&outer).unwrap();
                            assert!(
                                bounds.y() >= 0.
                                    && bounds.y() + bounds.height() < outer.height() as f32,
                                "Load error is buried below the form"
                            );
                            assert!(message.has_css_class("error"));
                            assert!(!message.has_css_class("caption"));
                            evidence.push(json!({"policy_error_visible_above_form":true}));
                        }
                        assert!(
                            text(&page(&stack, id)).contains("Cannot load")
                                || text(&page(&stack, id)).contains("need attention")
                        );
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

/// Controlled by a fake panel host on a private dbus-run-session bus only.
pub fn panel_qa(
    win: &adw::ApplicationWindow,
    outer: &gtk::Box,
    ready: std::rc::Rc<std::cell::Cell<bool>>,
    path: &str,
) {
    let (win, outer, path) = (win.clone(), outer.clone(), Path::new(path).to_owned());
    let mut stage = 0;
    let mut ticks = 0;
    glib::timeout_add_local(Duration::from_millis(100), move || {
        ticks += 1;
        assert!(ticks < 250, "Panel host did not complete its smoke test");
        if stage == 0 && ready.get() {
            let close = descendants(&outer)
                .into_iter()
                .find(|w| w.widget_name() == "window-close")
                .unwrap()
                .downcast::<gtk::Button>()
                .unwrap();
            close.emit_clicked();
            assert!(
                !win.is_visible(),
                "Panel close must hide instead of minimize"
            );
            fs::write(&path, r#"{"closed_to_panel":true}"#).unwrap();
            stage = 1;
        } else if stage == 1 && win.is_visible() {
            fs::write(
                &path,
                r#"{"closed_to_panel":true,"reopened_from_menu":true}"#,
            )
            .unwrap();
            stage = 2;
        } else if stage == 2 {
            let close = descendants(&outer)
                .into_iter()
                .find(|w| w.widget_name() == "window-close")
                .unwrap()
                .downcast::<gtk::Button>()
                .unwrap();
            close.emit_clicked();
            assert!(!win.is_visible());
            fs::write(
                &path,
                r#"{"closed_to_panel":true,"reopened_from_menu":true,"closed_again":true}"#,
            )
            .unwrap();
            stage = 3;
        } else if stage == 3 && win.is_visible() {
            fs::write(&path, r#"{"closed_to_panel":true,"reopened_from_menu":true,"closed_again":true,"recovered_host_loss":true}"#).unwrap();
            stage = 4;
        }
        glib::ControlFlow::Continue
    });
}

pub fn validate_capture(outer: &gtk::Box, stack: &gtk::Stack, path: &str) {
    let root = stack.visible_child().unwrap();
    let mut footer_visible = None;
    if root.widget_name() == "editor-page" {
        for name in ["Save changes", "Discard changes"] {
            let action = button(&root, name);
            let bounds = action.compute_bounds(outer).unwrap();
            assert!(
                bounds.x() >= 0.
                    && bounds.y() >= 0.
                    && bounds.x() + bounds.width() <= outer.width() as f32
                    && bounds.y() + bounds.height() <= outer.height() as f32,
                "Clipped {name} in {path}"
            );
        }
        footer_visible = Some(true);
    }
    let toggle = descendants(outer)
        .into_iter()
        .find(|w| w.widget_name() == "navigation-toggle")
        .unwrap();
    assert_eq!(
        toggle.is_visible(),
        outer.width() < 860,
        "Navigation breakpoint mismatch"
    );
    let navigation = descendants(outer)
        .into_iter()
        .find_map(|w| w.downcast::<adw::OverlaySplitView>().ok())
        .unwrap();
    assert_eq!(navigation.is_collapsed(), outer.width() < 860);
    assert_eq!(
        navigation.shows_sidebar(),
        outer.width() >= 860,
        "Desktop navigation must remain visible"
    );
    let settings = gtk::Settings::default().unwrap();
    let report = json!({"page":stack.visible_child_name().map(|s|s.to_string()),"logical_width":outer.width(),"logical_height":outer.height(),"scale":outer.scale_factor(),"text_dpi":settings.property::<i32>("gtk-xft-dpi"),"high_contrast":adw::StyleManager::default().is_high_contrast(),"footer_in_view":footer_visible});
    fs::write(
        Path::new(path).with_extension("json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}
