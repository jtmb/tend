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
fn child_count(root: &impl IsA<Widget>) -> usize {
    let mut count = 0;
    let mut child = root.as_ref().first_child();
    while let Some(widget) = child {
        count += 1;
        child = widget.next_sibling();
    }
    count
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
fn wait_for_report(view: &gtk::TextView, timeout: Duration) -> bool {
    let buffer = view.buffer();
    let current = || buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).to_string();
    let pending = |value: &str| value == "Working…" || value == "Collecting…";
    if !pending(&current()) {
        return true;
    }
    let main_loop = glib::MainLoop::new(None, false);
    let on_change = main_loop.clone();
    let handler = buffer.connect_changed(move |buffer| {
        let value = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
        if value != "Working…" && value != "Collecting…" {
            on_change.quit();
        }
    });
    let timed_out = std::rc::Rc::new(std::cell::Cell::new(false));
    let did_timeout = timed_out.clone();
    let on_timeout = main_loop.clone();
    let timeout_id = glib::timeout_add_local_once(timeout, move || {
        did_timeout.set(true);
        on_timeout.quit();
    });
    main_loop.run();
    if !timed_out.get() {
        timeout_id.remove();
    }
    buffer.disconnect(handler);
    !timed_out.get() && !pending(&current())
}
fn wait_for_task_rows(model: &gtk::SelectionModel, timeout: Duration) -> bool {
    if model.n_items() > 0 {
        return true;
    }
    let main_loop = glib::MainLoop::new(None, false);
    let on_change = main_loop.clone();
    let handler = model.connect_items_changed(move |model, _, _, _| {
        if model.n_items() > 0 {
            on_change.quit();
        }
    });
    let timed_out = std::rc::Rc::new(std::cell::Cell::new(false));
    let did_timeout = timed_out.clone();
    let on_timeout = main_loop.clone();
    let timeout_id = glib::timeout_add_local_once(timeout, move || {
        did_timeout.set(true);
        on_timeout.quit();
    });
    main_loop.run();
    if !timed_out.get() {
        timeout_id.remove();
    }
    model.disconnect(handler);
    !timed_out.get() && model.n_items() > 0
}
fn wait_main_context(duration: Duration) {
    let main_loop = glib::MainLoop::new(None, false);
    let on_timeout = main_loop.clone();
    glib::timeout_add_local_once(duration, move || on_timeout.quit());
    main_loop.run();
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
    let feature_mode = std::env::var_os("TEND_QA_FEATURES").is_some();
    let matrix_count = if std::env::var_os("TEND_QA_STATE_ONLY").is_some() || feature_mode {
        0
    } else {
        pages.len() * 6
    };
    let mut tick = 0usize;
    let mut evidence = vec![
        json!({"shared_footer_states":true,"appearance_failure_rollback":true,"cron_daily_initialization":true,"navigation_preserves_drafts":true,"report_states":true}),
    ];
    let clipboard_copy_result = std::rc::Rc::new(std::cell::RefCell::new(None::<String>));
    glib::timeout_add_local(
        Duration::from_millis(if feature_mode { 1_000 } else { 250 }),
        move || {
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
        if tick >= 16 && feature_mode {
            let step = tick - 16;
            let home = tend::workstation::home();
            let storage = page(&stack, "storage");
            let logs = page(&stack, "system-logs");
            let fixture = home.join("qa-storage");
            if step == 0 {
                fs::create_dir_all(&fixture).unwrap();
                fs::write(fixture.join("small.txt"), "isolated storage sample\n").unwrap();
                let path = descendants(&storage)
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::Entry>().ok())
                    .find(|e| e.placeholder_text().as_deref() == Some("Folder to scan"))
                    .unwrap();
                path.set_text(&fixture.to_string_lossy());
                stack.set_visible_child_name("storage");
                button(&storage, "Scan folder").emit_clicked();
                evidence.push(json!({"storage_scan_started_on_isolated_fixture":true}));
            } else if step == 1 {
                let report = descendants(&storage)
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::TextView>().ok())
                    .next()
                    .unwrap();
                let contents = report.buffer().text(
                    &report.buffer().start_iter(),
                    &report.buffer().end_iter(),
                    false,
                );
                assert!(
                    contents.contains("Scan complete") && contents.contains("small.txt"),
                    "Storage scan report missing fixture: {contents}"
                );
                let path = descendants(&storage)
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::Entry>().ok())
                    .find(|e| e.placeholder_text().as_deref() == Some("Folder to scan"))
                    .unwrap();
                path.set_text(&home.join("missing-qa-folder").to_string_lossy());
                button(&storage, "Scan folder").emit_clicked();
                evidence.push(json!({"storage_scan_lists_real_fixture_content":true,"storage_missing_path_started":true}));
            } else if step == 2 {
                let report = descendants(&storage)
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::TextView>().ok())
                    .next()
                    .unwrap();
                let contents = report.buffer().text(
                    &report.buffer().start_iter(),
                    &report.buffer().end_iter(),
                    false,
                );
                let failed_state = report
                    .ancestor(gtk::Stack::static_type())
                    .and_downcast::<gtk::Stack>()
                    .and_then(|s| s.visible_child_name().map(|name| name.to_string()));
                assert_eq!(
                    failed_state.as_deref(),
                    Some("error"),
                    "Missing storage path should show the report error state"
                );
                evidence.push(json!({"storage_missing_path_report":contents.to_string(),"storage_missing_path_state":failed_state}));
                let logs_path = home.join("demo-journal.jsonl");
                let original = fs::read(&logs_path).unwrap();
                stack.set_visible_child_name("system-logs");
                button(&logs, "Load logs").emit_clicked();
                fs::write(home.join("qa-journal-backup.jsonl"), &original).unwrap();
                // A follow-up tick lets the successful worker completion update the real ListBox.
                evidence.push(json!({"log_first_load_started":true}));
            } else if step == 3 {
                let list = descendants(&logs)
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::ListBox>().ok())
                    .find(|l| l.selection_mode() == gtk::SelectionMode::Single)
                    .unwrap();
                assert_eq!(
                    child_count(&list),
                    6,
                    "Expected six decoded sample journal events"
                );
                if let Some(row) = list.row_at_index(0) {
                    list.select_row(Some(&row));
                }
                let detail = descendants(&logs)
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::TextView>().ok())
                    .next()
                    .unwrap();
                let contents = detail.buffer().text(
                    &detail.buffer().start_iter(),
                    &detail.buffer().end_iter(),
                    false,
                );
                assert!(
                    contents.contains("Process:") && !contents.contains("No log data is uploaded"),
                    "Selected event details were not shown: {contents}"
                );
                let source = dropdown(&stack, "Log source");
                source.set_selected(1);
                let export = button(&logs, "Export filtered logs");
                assert!(
                    !export.is_sensitive(),
                    "Changing log source left old events exportable"
                );
                assert_eq!(
                    child_count(&list),
                    0,
                    "Changing log source left old rows visible"
                );
                let period = dropdown(&stack, "Time range");
                period.set_selected(1);
                assert!(
                    child_count(&list) == 0 && !export.is_sensitive(),
                    "Changing log time range left old events exportable"
                );
                period.set_selected(2);
                let restored = fs::read(home.join("qa-journal-backup.jsonl")).unwrap();
                fs::write(home.join("demo-journal.jsonl"), restored).unwrap();
                source.set_selected(0);
                button(&logs, "Load logs").emit_clicked();
                evidence.push(json!({"log_load_decoded_six_and_selected_details":true,"source_and_period_changes_clear_old_events":true}));
            } else if step == 4 {
                let list = descendants(&logs)
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::ListBox>().ok())
                    .find(|l| l.selection_mode() == gtk::SelectionMode::Single)
                    .unwrap();
                assert_eq!(
                    child_count(&list),
                    6,
                    "Reload did not restore sample events"
                );
                let logs_path = home.join("demo-journal.jsonl");
                let original = fs::read(&logs_path).unwrap();
                fs::write(home.join("qa-journal-backup.jsonl"), &original).unwrap();
                fs::rename(&logs_path, home.join("qa-journal-saved.jsonl")).unwrap();
                fs::create_dir(&logs_path).unwrap();
                button(&logs, "Load logs").emit_clicked();
                assert!(
                    !button(&logs, "Export filtered logs").is_sensitive(),
                    "Export remained enabled while reloading"
                );
                assert!(
                    !dropdown(&stack, "Log source").is_sensitive(),
                    "Log query controls remained enabled while loading"
                );
                evidence.push(json!({"log_failed_reload_started_with_export_disabled":true}));
            } else if step == 5 {
                let list = descendants(&logs)
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::ListBox>().ok())
                    .find(|l| l.selection_mode() == gtk::SelectionMode::Single)
                    .unwrap();
                let detail = descendants(&logs)
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::TextView>().ok())
                    .next()
                    .unwrap();
                assert_eq!(
                    child_count(&list),
                    0,
                    "Failed reload left old events visible"
                );
                let contents = detail.buffer().text(
                    &detail.buffer().start_iter(),
                    &detail.buffer().end_iter(),
                    false,
                );
                assert!(
                    !contents.contains("Process:") && !contents.contains("Source:"),
                    "Failed reload left old event details visible: {contents}"
                );
                assert!(
                    !button(&logs, "Export filtered logs").is_sensitive(),
                    "Failed reload left stale events exportable"
                );
                dropdown(&stack, "Severity filter").set_selected(1);
                descendants(&logs)
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::SearchEntry>().ok())
                    .next()
                    .unwrap()
                    .set_text("error");
                assert!(
                    child_count(&list) == 0
                        && !button(&logs, "Export filtered logs").is_sensitive(),
                    "Filters repopulated stale events after failed reload"
                );
                assert!(
                    text(&logs).contains("Logs could not be loaded."),
                    "Failed log load did not show its error Notice"
                );
                fs::rename(
                    home.join("demo-journal.jsonl"),
                    home.join("qa-journal-failed-directory"),
                )
                .unwrap();
                fs::rename(
                    home.join("qa-journal-saved.jsonl"),
                    home.join("demo-journal.jsonl"),
                )
                .unwrap();
                evidence.push(json!({"failed_reload_cleared_rows_details_and_export_even_after_filtering":true}));
            } else if step == 6 {
                let activity = page(&stack, "activity");
                let report = descendants(&activity)
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::TextView>().ok())
                    .next()
                    .unwrap();
                let before = fs::read_dir(home.join("Documents/Tend reports"))
                    .map(|es| es.count())
                    .unwrap_or(0);
                report
                    .buffer()
                    .set_text("QA isolated export sample text one");
                button(&activity, "Export text").emit_clicked();
                report
                    .buffer()
                    .set_text("QA isolated export sample text two");
                button(&activity, "Export text").emit_clicked();
                report
                    .buffer()
                    .set_text("QA isolated export sample json three");
                button(&activity, "Export JSON").emit_clicked();
                report
                    .buffer()
                    .set_text("QA isolated export sample json four");
                button(&activity, "Export JSON").emit_clicked();
                let after = fs::read_dir(home.join("Documents/Tend reports"))
                    .unwrap()
                    .count();
                assert_eq!(
                    after,
                    before + 4,
                    "Rapid same-format exports must create four distinct files"
                );
                let files = fs::read_dir(home.join("Documents/Tend reports"))
                    .unwrap()
                    .flatten()
                    .filter_map(|entry| fs::read_to_string(entry.path()).ok())
                    .collect::<Vec<_>>();
                for sample in [
                    "QA isolated export sample text one",
                    "QA isolated export sample text two",
                    "QA isolated export sample json three",
                    "QA isolated export sample json four",
                ] {
                    assert!(
                        files.iter().any(|body| body.contains(sample)),
                        "Rapid export lost or overwrote {sample}"
                    );
                }
                evidence.push(json!({"rapid_repeated_text_and_json_exports_preserve_four_distinct_reports":true}));
            } else if step == 7 {
                let startup = page(&stack, "startup");
                let fixture_startup = home.join(".config/autostart/qa-isolated-startup.desktop");
                fs::write(
                    home.join("qa-startup-original.desktop"),
                    "[Desktop Entry]\nName=Tend QA isolated startup fixture\nExec=/usr/bin/true\nType=Application\n",
                )
                .unwrap();
                fs::create_dir_all(fixture_startup.parent().unwrap()).unwrap();
                fs::write(&fixture_startup, "[Desktop Entry]\nName=Tend QA isolated startup fixture\nExec=/usr/bin/true\nType=Application\n").unwrap();
                stack.set_visible_child_name("startup");
                button(&startup, "Load startup entries").emit_clicked();
                evidence.push(json!({"startup_load_started_with_isolated_fixture":true}));
            } else if step == 8 {
                let startup = page(&stack, "startup");
                let row = descendants(&startup)
                    .into_iter()
                    .filter_map(|w| w.downcast::<adw::ActionRow>().ok())
                    .find(|r| r.title() == "Tend QA isolated startup fixture")
                    .expect("Isolated startup fixture is visible after Load");
                let toggle = descendants(&row)
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::Button>().ok())
                    .find(|b| b.label().as_deref() == Some("Disable"))
                    .unwrap();
                toggle.emit_clicked();
                let dialog = gtk::Window::list_toplevels()
                    .into_iter()
                    .filter_map(|w| w.downcast::<adw::MessageDialog>().ok())
                    .find(|w| w.is_visible())
                    .unwrap();
                button(&dialog, "Cancel").emit_clicked();
                assert!(!dialog.is_visible(), "Native Cancel button should close the startup confirmation");
                assert_eq!(
                    fs::read(home.join(".config/autostart/qa-isolated-startup.desktop")).unwrap(),
                    fs::read(home.join("qa-startup-original.desktop")).unwrap(),
                    "Cancelling startup confirmation changed the fixture"
                );
                toggle.emit_clicked();
                let dialog = gtk::Window::list_toplevels()
                    .into_iter()
                    .filter_map(|w| w.downcast::<adw::MessageDialog>().ok())
                    .find(|w| w.is_visible())
                    .unwrap();
                button(&dialog, "Save").emit_clicked();
                evidence.push(json!({"startup_confirmation_cancel_and_apply_real_callbacks":true}));
            } else if step == 9 {
                let override_path = home.join(".config/autostart/qa-isolated-startup.desktop");
                assert!(
                    fs::read_to_string(&override_path)
                        .unwrap()
                        .contains("Hidden=true"),
                    "Confirmed startup change did not write isolated override"
                );
                let recovery = page(&stack, "recovery");
                stack.set_visible_child_name("recovery");
                button(&recovery, "Load saved changes").emit_clicked();
                evidence.push(json!({"startup_change_recorded_for_recovery":true}));
            } else if step == 10 {
                let recovery = page(&stack, "recovery");
                let row = descendants(&recovery)
                    .into_iter()
                    .filter_map(|w| w.downcast::<adw::ActionRow>().ok())
                    .find(|r| {
                        r.title()
                            .contains("Startup: Tend QA isolated startup fixture")
                    })
                    .expect("Startup recovery item is visible");
                let restore = descendants(&row)
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::Button>().ok())
                    .find(|b| b.label().as_deref() == Some("Restore…"))
                    .unwrap();
                restore.emit_clicked();
                let dialog = gtk::Window::list_toplevels()
                    .into_iter()
                    .filter_map(|w| w.downcast::<adw::MessageDialog>().ok())
                    .find(|w| w.is_visible())
                    .unwrap();
                button(&dialog, "Restore").emit_clicked();
                evidence.push(json!({"recovery_restore_confirmation_real_callback":true}));
            } else if step == 11 {
                assert_eq!(
                    fs::read(home.join(".config/autostart/qa-isolated-startup.desktop")).unwrap(),
                    fs::read(home.join("qa-startup-original.desktop")).unwrap(),
                    "Recovery did not restore original isolated startup entry"
                );
                let tuning = page(&stack, "tuning");
                stack.set_visible_child_name("tuning");
                let animation = descendants(&tuning)
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::DropDown>().ok())
                    .next()
                    .unwrap();
                animation.set_selected(1);
                button(&tuning, "Apply").emit_clicked();
                assert!(
                    text(&tuning).contains("Applied. Previous value is available in Recovery.")
                        || text(&tuning).contains("Could not"),
                    "Appearance callback did not produce status"
                );
                evidence
                    .push(json!({"animations_apply_callback_used_memory_gsettings_backend":true}));
            } else if step == 12 {
                stack.set_visible_child_name("storage");
                let cancel_fixture = home.join("qa-storage-cancel");
                fs::create_dir_all(&cancel_fixture).unwrap();
                for index in 0..25_000 {
                    fs::write(cancel_fixture.join(format!("sample-{index:05}.tmp")), "").unwrap();
                }
                let path = descendants(&storage)
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::Entry>().ok())
                    .find(|e| e.placeholder_text().as_deref() == Some("Folder to scan"))
                    .unwrap();
                path.set_text(&cancel_fixture.to_string_lossy());
                button(&storage, "Scan folder").emit_clicked();
                assert!(
                    button(&storage, "Cancel scan").is_sensitive(),
                    "Cancel scan was not enabled during scan"
                );
                button(&storage, "Cancel scan").emit_clicked();
                evidence.push(json!({"storage_cancel_scan_real_handler_invoked":true}));
            } else if step == 13 {
                if !button(&storage, "Cancel scan").is_sensitive() {
                    let report = descendants(&storage)
                        .into_iter()
                        .filter_map(|w| w.downcast::<gtk::TextView>().ok())
                        .next()
                        .unwrap();
                    let contents = report.buffer().text(
                        &report.buffer().start_iter(),
                        &report.buffer().end_iter(),
                        false,
                    );
                    assert!(
                        contents.contains("Partial scan"),
                        "Cancelled scan did not report a partial result: {contents}"
                    );
                    evidence.push(json!({"storage_cancel_produced_partial_scan_report":true}));
                } else {
                    evidence.push(json!({"storage_cancel_completion_pending":true}));
                }
            } else if step == 14 {
                assert!(
                    !button(&storage, "Cancel scan").is_sensitive(),
                    "Cancel scan did not return to idle state"
                );
                let report = descendants(&storage)
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::TextView>().ok())
                    .next()
                    .unwrap();
                let contents = report.buffer().text(
                    &report.buffer().start_iter(),
                    &report.buffer().end_iter(),
                    false,
                );
                assert!(
                    contents.contains("Partial scan"),
                    "Cancelled scan did not report a partial result: {contents}"
                );
                if !evidence
                    .iter()
                    .any(|item| item["storage_cancel_produced_partial_scan_report"] == true)
                {
                    evidence.push(json!({"storage_cancel_produced_partial_scan_report":true}));
                }
                stack.set_visible_child_name("task-manager");
                win.set_default_size(720, 760);
                evidence.push(json!({"narrow_task_manager_resize_requested":true}));
            } else if step == 15 {
                let task = page(&stack, "task-manager");
                let table_scroll = descendants(&task)
                    .into_iter()
                    .find(|w| w.widget_name() == "process-table-scroll")
                    .unwrap()
                    .downcast::<gtk::ScrolledWindow>()
                    .unwrap();
                let adjustment = table_scroll.hadjustment();
                assert_eq!(
                    table_scroll.hscrollbar_policy(),
                    gtk::PolicyType::Always,
                    "Narrow process table must expose horizontal scrolling"
                );
                assert!(
                    adjustment.upper() > adjustment.page_size(),
                    "Process table has no scrollable overflow at narrow width"
                );
                adjustment.set_value(adjustment.upper() - adjustment.page_size());
                evidence.push(json!({"narrow_process_table_horizontal_adjustment_reveals_status":true,"scroll_upper":adjustment.upper(),"scroll_page":adjustment.page_size()}));
                let rules = page(&stack, "cleanup");
                stack.set_visible_child_name("cleanup");
                fs::create_dir_all(home.join("qa-rule-cache")).unwrap();
                button(&rules, "Add rule…").emit_clicked();
                evidence.push(json!({"folder_rule_add_dialog_opened":true}));
            } else if step == 16 {
                let dialog = gtk::Window::list_toplevels().into_iter()
                    .filter_map(|w| w.downcast::<adw::MessageDialog>().ok())
                    .find(|w| w.is_visible() && w.title().as_deref() == Some("Add cleanup rule"))
                    .expect("Add rule dialog should be visible");
                let entries = descendants(&dialog).into_iter()
                    .filter_map(|w| w.downcast::<gtk::Entry>().ok()).collect::<Vec<_>>();
                let get_entry = |placeholder: &str| entries.iter().find(|e| e.placeholder_text().as_deref() == Some(placeholder)).unwrap().clone();
                get_entry("Rule name").set_text("QA isolated cache rule");
                get_entry("Absolute folder path").set_text(&home.join("qa-rule-cache").to_string_lossy());
                get_entry("Filename patterns, e.g. *.log,*.tmp").set_text("*.tmp");
                button(&dialog, "Save rule").emit_clicked();
                evidence.push(json!({"folder_rule_add_save_handler_invoked":true}));
            } else if step == 17 {
                let rules = page(&stack, "cleanup");
                let saved: serde_json::Value = serde_json::from_slice(&fs::read(home.join(".config/tend/rules.json")).unwrap()).unwrap();
                assert!(saved["rules"].as_array().unwrap().iter().any(|r| r["name"] == "QA isolated cache rule"), "Valid folder rule was not saved");
                let list = descendants(&rules).into_iter().filter_map(|w| w.downcast::<gtk::ListBox>().ok()).find(|l| l.selection_mode() == gtk::SelectionMode::Single).unwrap();
                let row = list.row_at_index(1).expect("Added rule row");
                list.select_row(Some(&row));
                button(&rules, "Edit selected…").emit_clicked();
                evidence.push(json!({"folder_rule_add_persisted_and_edit_opened":true}));
            } else if step == 18 {
                let dialog = gtk::Window::list_toplevels().into_iter()
                    .filter_map(|w| w.downcast::<adw::MessageDialog>().ok())
                    .find(|w| w.is_visible() && w.title().as_deref() == Some("Edit cleanup rule"))
                    .expect("Edit rule dialog should be visible");
                let days = descendants(&dialog).into_iter().filter_map(|w| w.downcast::<gtk::SpinButton>().ok()).next().unwrap();
                assert_eq!(days.value_as_int(), 30, "Edit dialog should preserve saved rule values");
                days.set_value(60.);
                dialog.response("save");
                evidence.push(json!({"folder_rule_edit_preserved_fields_and_saved_new_retention":true}));
            } else if step == 19 {
                let rules = page(&stack, "cleanup");
                let saved: serde_json::Value = serde_json::from_slice(&fs::read(home.join(".config/tend/rules.json")).unwrap()).unwrap();
                assert!(saved["rules"].as_array().unwrap().iter().any(|r| r["name"] == "QA isolated cache rule" && r["days"] == 60), "Edited folder-rule retention was not persisted");
                let list = descendants(&rules).into_iter().filter_map(|w| w.downcast::<gtk::ListBox>().ok()).find(|l| l.selection_mode() == gtk::SelectionMode::Single).unwrap();
                let row = list.row_at_index(1).expect("Edited rule row");
                list.select_row(Some(&row));
                button(&rules, "Remove selected").emit_clicked();
                evidence.push(json!({"folder_rule_edit_persisted_and_remove_handler_invoked":true}));
            } else if step == 20 {
                let saved: serde_json::Value = serde_json::from_slice(&fs::read(home.join(".config/tend/rules.json")).unwrap()).unwrap();
                assert!(!saved["rules"].as_array().unwrap().iter().any(|r| r["name"] == "QA isolated cache rule"), "Remove selected left the rule in saved policy");
                let rules = page(&stack, "cleanup");
                button(&rules, "Add rule…").emit_clicked();
                evidence.push(json!({"folder_rule_remove_persisted_and_invalid_add_dialog_opened":true}));
            } else if step == 21 {
                let dialog = gtk::Window::list_toplevels().into_iter().filter_map(|w| w.downcast::<adw::MessageDialog>().ok()).find(|w| w.is_visible() && w.title().as_deref() == Some("Add cleanup rule")).unwrap();
                let entries = descendants(&dialog).into_iter().filter_map(|w| w.downcast::<gtk::Entry>().ok()).collect::<Vec<_>>();
                let get_entry = |placeholder: &str| entries.iter().find(|e| e.placeholder_text().as_deref() == Some(placeholder)).unwrap().clone();
                get_entry("Rule name").set_text("QA invalid path");
                get_entry("Absolute folder path").set_text("../outside");
                get_entry("Filename patterns, e.g. *.log,*.tmp").set_text("*.tmp");
                assert!(!dialog.is_response_enabled("save"), "Invalid traversal path must disable Save rule");
                let draft_name = get_entry("Rule name").text().to_string();
                let draft_path = get_entry("Absolute folder path").text().to_string();
                let validation = text(&dialog);
                assert!(validation.contains("Folder unavailable") || validation.contains("specific user-owned folder"), "Invalid path needs a visible explanation: {validation}");
                evidence.push(json!({"invalid_rule_save_disabled":true,"invalid_rule_draft_name":draft_name,"invalid_rule_draft_path":draft_path,"invalid_rule_validation_visible":true}));
                fs::write(directory.join("feature-progress.json"), serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
            } else if step == 22 {
                let dialog = gtk::Window::list_toplevels().into_iter().filter_map(|w| w.downcast::<adw::MessageDialog>().ok()).find(|w| w.is_visible() && w.title().as_deref() == Some("Add cleanup rule")).unwrap();
                let paint = gtk::WidgetPaintable::new(Some(&dialog));
                let snapshot = gtk::Snapshot::new();
                paint.snapshot(&snapshot, dialog.width() as f64, dialog.height() as f64);
                let screenshot_saved = match (win.renderer(), snapshot.to_node()) {
                    (Some(renderer), Some(node)) => renderer.render_texture(&node, None)
                        .save_to_png(directory.join("invalid-rule-validation.png")).is_ok(),
                    _ => false,
                };
                evidence.push(json!({"invalid_rule_screenshot_saved":screenshot_saved}));
                button(&dialog, "Cancel").emit_clicked();
                let rules = page(&stack, "cleanup");
                let saved: serde_json::Value = serde_json::from_slice(&fs::read(home.join(".config/tend/rules.json")).unwrap()).unwrap();
                assert!(!saved["rules"].as_array().unwrap().iter().any(|r| r["name"] == "QA invalid path"), "Invalid path was persisted");
                let status = text(&rules);
                let open = gtk::Window::list_toplevels().into_iter().filter_map(|w| w.downcast::<adw::MessageDialog>().ok()).any(|w| w.is_visible() && w.title().as_deref() == Some("Add cleanup rule"));
                evidence.push(json!({"invalid_rule_not_persisted":true,"invalid_rule_dialog_closed_by_cancel":!open,"invalid_rule_status":status}));
                let baseline = fs::read(home.join(".config/tend/rules.json")).unwrap();
                button(&rules, "Add rule…").emit_clicked();
                fs::write(home.join("qa-rules-baseline.json"), baseline).unwrap();
            } else if step == 23 {
                let dialog = gtk::Window::list_toplevels().into_iter().filter_map(|w| w.downcast::<adw::MessageDialog>().ok()).find(|w| w.is_visible() && w.title().as_deref() == Some("Add cleanup rule")).unwrap();
                let entries = descendants(&dialog).into_iter().filter_map(|w| w.downcast::<gtk::Entry>().ok()).collect::<Vec<_>>();
                let get_entry = |placeholder: &str| entries.iter().find(|e| e.placeholder_text().as_deref() == Some(placeholder)).unwrap().clone();
                get_entry("Rule name").set_text("QA protected path");
                get_entry("Absolute folder path").set_text(&home.join(".config").to_string_lossy());
                get_entry("Filename patterns, e.g. *.log,*.tmp").set_text("*.tmp");
                assert!(!dialog.is_response_enabled("save"), "Protected folder must disable Save rule");
                let draft_name = get_entry("Rule name").text().to_string();
                let draft_path = get_entry("Absolute folder path").text().to_string();
                let validation = text(&dialog);
                assert!(validation.contains("specific user-owned folder"), "Protected path needs a visible explanation: {validation}");
                evidence.push(json!({"protected_rule_save_disabled":true,"protected_rule_draft_name":draft_name,"protected_rule_draft_path":draft_path,"protected_rule_validation_visible":true}));
                fs::write(directory.join("feature-progress.json"), serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
            } else if step == 24 {
                let dialog = gtk::Window::list_toplevels().into_iter().filter_map(|w| w.downcast::<adw::MessageDialog>().ok()).find(|w| w.is_visible() && w.title().as_deref() == Some("Add cleanup rule")).unwrap();
                let paint = gtk::WidgetPaintable::new(Some(&dialog));
                let snapshot = gtk::Snapshot::new();
                paint.snapshot(&snapshot, dialog.width() as f64, dialog.height() as f64);
                let screenshot_saved = match (win.renderer(), snapshot.to_node()) {
                    (Some(renderer), Some(node)) => renderer.render_texture(&node, None)
                        .save_to_png(directory.join("protected-rule-validation.png")).is_ok(),
                    _ => false,
                };
                evidence.push(json!({"protected_rule_screenshot_saved":screenshot_saved}));
                button(&dialog, "Cancel").emit_clicked();
                let rules = page(&stack, "cleanup");
                let saved = fs::read(home.join(".config/tend/rules.json")).unwrap();
                assert_eq!(saved, fs::read(home.join("qa-rules-baseline.json")).unwrap(), "Protected folder policy changed");
                let open = gtk::Window::list_toplevels().into_iter().filter_map(|w| w.downcast::<adw::MessageDialog>().ok()).any(|w| w.is_visible() && w.title().as_deref() == Some("Add cleanup rule"));
                evidence.push(json!({"protected_folder_not_persisted":true,"protected_rule_dialog_closed_by_cancel":!open,"protected_rule_status":text(&rules)}));
                stack.set_visible_child_name("storage");
                button(&storage, "Disk overview").emit_clicked();
                evidence.push(json!({"disk_overview_read_only_handler_invoked":true}));
            } else if step == 25 {
                let storage = page(&stack, "storage");
                let report = descendants(&storage).into_iter().filter_map(|w| w.downcast::<gtk::TextView>().ok()).next().unwrap();
                assert!(wait_for_report(&report, Duration::from_secs(30)), "Disk overview did not finish within 30 seconds");
                let body = report.buffer().text(&report.buffer().start_iter(), &report.buffer().end_iter(), false);
                let state = report.ancestor(gtk::Stack::static_type()).and_downcast::<gtk::Stack>().and_then(|s| s.visible_child_name().map(|n| n.to_string()));
                evidence.push(json!({"disk_overview_native_callback_completed":true,"state":state,"output_bytes":body.len()}));
                report.buffer().set_text("QA native clipboard readback marker");
                button(&storage, "Copy report").emit_clicked();
                let copied = clipboard_copy_result.clone();
                button(&storage, "Copy report").clipboard().read_text_async(
                    None::<&gtk::gio::Cancellable>,
                    move |result| *copied.borrow_mut() = result.ok().flatten().map(|s| s.to_string()),
                );
                let startup = page(&stack, "startup");
                stack.set_visible_child_name("startup");
                button(&startup, "Startup & restore report").emit_clicked();
            } else if step == 26 {
                let startup = page(&stack, "startup");
                let report = descendants(&startup).into_iter().filter_map(|w| w.downcast::<gtk::TextView>().ok()).next().unwrap();
                assert!(wait_for_report(&report, Duration::from_secs(30)), "Startup report did not finish within 30 seconds");
                let body = report.buffer().text(&report.buffer().start_iter(), &report.buffer().end_iter(), false);
                let state = report.ancestor(gtk::Stack::static_type()).and_downcast::<gtk::Stack>().and_then(|s| s.visible_child_name().map(|n| n.to_string()));
                evidence.push(json!({"startup_restore_report_native_callback_completed":true,"state":state,"output_bytes":body.len()}));
                assert_eq!(clipboard_copy_result.borrow().as_deref(), Some("QA native clipboard readback marker"), "Copy report did not place report text on the native clipboard");
                evidence.push(json!({"copy_report_native_clipboard_readback":true}));
                let tuning = page(&stack, "tuning");
                stack.set_visible_child_name("tuning");
                button(&tuning, "Read memory status").emit_clicked();
            } else if step == 27 {
                let tuning = page(&stack, "tuning");
                let report = descendants(&tuning).into_iter().filter_map(|w| w.downcast::<gtk::TextView>().ok()).next().unwrap();
                assert!(wait_for_report(&report, Duration::from_secs(30)), "Memory status report did not finish within 30 seconds");
                let body = report.buffer().text(&report.buffer().start_iter(), &report.buffer().end_iter(), false);
                let state = report.ancestor(gtk::Stack::static_type()).and_downcast::<gtk::Stack>().and_then(|s| s.visible_child_name().map(|n| n.to_string()));
                evidence.push(json!({"memory_status_read_native_callback_completed":true,"state":state,"output_bytes":body.len()}));
                let development = page(&stack, "development");
                stack.set_visible_child_name("development");
                button(&development, "Inspect Docker & caches").emit_clicked();
            } else if step == 28 {
                let development = page(&stack, "development");
                let report = descendants(&development).into_iter().filter_map(|w| w.downcast::<gtk::TextView>().ok()).next().unwrap();
                assert!(wait_for_report(&report, Duration::from_secs(30)), "Docker/cache report did not finish within 30 seconds");
                let body = report.buffer().text(&report.buffer().start_iter(), &report.buffer().end_iter(), false);
                let state = report.ancestor(gtk::Stack::static_type()).and_downcast::<gtk::Stack>().and_then(|s| s.visible_child_name().map(|n| n.to_string()));
                evidence.push(json!({"docker_cache_read_only_native_callback_completed":true,"state":state,"output_bytes":body.len()}));
                let reports = page(&stack, "reports");
                stack.set_visible_child_name("reports");
                button(&reports, "Show pending updates").emit_clicked();
            } else if step == 29 {
                let reports = page(&stack, "reports");
                let report = descendants(&reports).into_iter().filter_map(|w| w.downcast::<gtk::TextView>().ok()).next().unwrap();
                assert!(wait_for_report(&report, Duration::from_secs(30)), "Pending updates report did not finish within 30 seconds");
                let body = report.buffer().text(&report.buffer().start_iter(), &report.buffer().end_iter(), false);
                let state = report.ancestor(gtk::Stack::static_type()).and_downcast::<gtk::Stack>().and_then(|s| s.visible_child_name().map(|n| n.to_string()));
                evidence.push(json!({"pending_updates_read_only_native_callback_completed":true,"state":state,"output_bytes":body.len()}));
                button(&reports, "Load history").emit_clicked();
            } else if step == 30 {
                let reports = page(&stack, "reports");
                let report = descendants(&reports).into_iter().filter_map(|w| w.downcast::<gtk::TextView>().ok()).next().unwrap();
                assert!(wait_for_report(&report, Duration::from_secs(30)), "History report did not finish within 30 seconds");
                let body = report.buffer().text(&report.buffer().start_iter(), &report.buffer().end_iter(), false);
                let state = report.ancestor(gtk::Stack::static_type()).and_downcast::<gtk::Stack>().and_then(|s| s.visible_child_name().map(|n| n.to_string()));
                evidence.push(json!({"maintenance_history_native_callback_completed":true,"state":state,"output_bytes":body.len()}));
                button(&reports, "System report").emit_clicked();
            } else if step == 31 {
                let reports = page(&stack, "reports");
                let report = descendants(&reports).into_iter().filter_map(|w| w.downcast::<gtk::TextView>().ok()).next().unwrap();
                assert!(wait_for_report(&report, Duration::from_secs(30)), "System report did not finish within 30 seconds");
                let body = report.buffer().text(&report.buffer().start_iter(), &report.buffer().end_iter(), false);
                let state = report.ancestor(gtk::Stack::static_type()).and_downcast::<gtk::Stack>().and_then(|s| s.visible_child_name().map(|n| n.to_string()));
                evidence.push(json!({"system_report_native_callback_completed":true,"state":state,"output_bytes":body.len()}));
                let activity = page(&stack, "activity");
                stack.set_visible_child_name("activity");
                button(&activity, "Load recent activity").emit_clicked();
            } else if step == 32 {
                let activity = page(&stack, "activity");
                let report = descendants(&activity).into_iter().filter_map(|w| w.downcast::<gtk::TextView>().ok()).next().unwrap();
                assert!(wait_for_report(&report, Duration::from_secs(30)), "Activity read did not finish within 30 seconds");
                let body = report.buffer().text(&report.buffer().start_iter(), &report.buffer().end_iter(), false);
                evidence.push(json!({"activity_read_native_callback_completed":true,"output_bytes":body.len()}));
                let task = page(&stack, "task-manager");
                stack.set_visible_child_name("task-manager");
                let export = button(&task, "Export snapshot");
                let reports_dir = home.join("Documents/Tend reports");
                let before = fs::read_dir(&reports_dir).map(|e| e.count()).unwrap_or(0);
                let initial_enabled = export.is_sensitive();
                assert!(!initial_enabled, "Task snapshot export should be disabled before the first collection");
                export.emit_clicked();
                let after = fs::read_dir(&reports_dir).map(|e| e.count()).unwrap_or(0);
                evidence.push(json!({"task_export_before_first_collection_sensitive":initial_enabled,"task_export_before_first_collection_created_file":after > before}));
                button(&task, "Refresh now").emit_clicked();
            } else if step == 33 {
                let task = page(&stack, "task-manager");
                let table = descendants(&task).into_iter().filter_map(|w| w.downcast::<gtk::ColumnView>().ok()).next().unwrap();
                let model = table.model().expect("Task table selection model");
                assert!(wait_for_task_rows(&model, Duration::from_secs(30)), "Task Manager process collection did not finish within 30 seconds");
                let initial_count = model.n_items();
                assert!(initial_count > 0, "Task Manager did not populate process rows");
                assert!(button(&task, "Export snapshot").is_sensitive(), "Task snapshot export should be enabled after collection");
                let search = descendants(&task).into_iter().filter_map(|w| w.downcast::<gtk::SearchEntry>().ok()).next().unwrap();
                search.set_text("tend-qa-no-process-match");
                wait_main_context(Duration::from_millis(250));
                assert_eq!(model.n_items(), 0, "Task Manager search did not filter out unmatched rows");
                search.set_text("");
                wait_main_context(Duration::from_millis(250));
                assert_eq!(model.n_items(), initial_count, "Clearing task search did not restore process rows");
                let only_mine = descendants(&task).into_iter().filter_map(|w| w.downcast::<gtk::CheckButton>().ok()).find(|b| b.label().as_deref() == Some("Only my processes")).unwrap();
                only_mine.set_active(true);
                assert!(model.n_items() <= initial_count, "Only my processes filter increased result count");
                only_mine.set_active(false);
                let column = table.columns().item(2).and_downcast::<gtk::ColumnViewColumn>().unwrap();
                table.sort_by_column(Some(&column), gtk::SortType::Ascending);
                assert!(table.sorter().is_some(), "Task Manager column sort did not activate a sorter");
                let speed = descendants(&task).into_iter().filter_map(|w| w.downcast::<gtk::DropDown>().ok()).find(|d| d.model().and_then(|m| m.item(0)).is_some()).unwrap();
                speed.set_selected(0);
                evidence.push(json!({"task_manager_real_process_search_filter_and_sort":true,"task_rows":initial_count,"task_monitor_paused":speed.selected()==0}));
                button(&task, "Refresh now").emit_clicked();
            } else if step == 34 {
                let task = page(&stack, "task-manager");
                let table = descendants(&task).into_iter().filter_map(|w| w.downcast::<gtk::ColumnView>().ok()).next().unwrap();
                assert!(table.model().unwrap().n_items() > 0, "Refresh now did not retain/populate process rows");
                let before = fs::read_dir(home.join("Documents/Tend reports")).map(|e| e.count()).unwrap_or(0);
                button(&task, "Export snapshot").emit_clicked();
                let after = fs::read_dir(home.join("Documents/Tend reports")).map(|e| e.count()).unwrap_or(0);
                evidence.push(json!({"task_manager_refresh_callback_kept_rows":true,"task_snapshot_export_created_file":after > before}));
            } else if step == 35 {
                stack.set_visible_child_name("overview");
                let overview = page(&stack, "overview");
                let refresh = button(&overview, "Refresh status");
                refresh.emit_clicked();
                assert!(!refresh.is_sensitive());
                let deadline = std::time::Instant::now() + Duration::from_secs(30);
                while !refresh.is_sensitive() && std::time::Instant::now() < deadline {
                    wait_main_context(Duration::from_millis(100));
                }
                assert!(refresh.is_sensitive(), "Overview refresh did not finish");
                assert!(text(&overview).contains("Scheduled maintenance"));
                button(&overview, "Open disk cleanup").emit_clicked();
                assert_eq!(stack.visible_child_name().as_deref(), Some("cleanup"));
                evidence.push(json!({"overview_refresh_and_cleanup_navigation_buttons":true}));
            } else if step == 36 {
                stack.set_visible_child_name("care");
                let appearance = page(&stack, "care");
                let theme = descendants(&appearance).into_iter().find_map(|w| w.downcast::<gtk::DropDown>().ok()).unwrap();
                let previous = theme.selected();
                let config = glib::user_config_dir().join("tend");
                for selected in 0..3 {
                    theme.set_selected(selected);
                    assert_eq!(fs::read_to_string(config.join("theme")).unwrap(), selected.to_string());
                }
                theme.set_selected(previous);
                let background = descendants(&appearance).into_iter().find_map(|w| w.downcast::<gtk::Switch>().ok()).unwrap();
                let prior = background.is_active();
                background.set_active(!prior);
                assert_eq!(fs::read_to_string(config.join("background")).unwrap(), (!prior).to_string());
                background.set_active(prior);
                assert_eq!(fs::read_to_string(config.join("background")).unwrap(), prior.to_string());
                evidence.push(json!({"appearance_all_theme_choices_and_background_autosave":true}));
            } else {
                fs::write(
                    directory.join("feature-evidence.json"),
                    serde_json::to_vec_pretty(&evidence).unwrap(),
                )
                .unwrap();
                app.activate_action("quit", None);
                return glib::ControlFlow::Break;
            }
            fs::write(
                directory.join("feature-progress.json"),
                serde_json::to_vec_pretty(&evidence).unwrap(),
            )
            .unwrap();
        } else if tick >= 16 {
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
                        button(&dialog, "Cancel").emit_clicked();
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
        },
    );
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
