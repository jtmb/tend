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
        "Off-screen demo target: {name}: {rect:?} in {}×{}",outer.width(),outer.height()
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
fn open_cleanup_retention(root: &impl IsA<Widget>) {
    let expander = descendants(root).into_iter()
        .find(|w| w.widget_name() == "cleanup-retention")
        .unwrap().downcast::<gtk::Expander>().unwrap();
    expander.set_expanded(true);
    assert!(expander.child().unwrap().is_child_visible());
}
fn open_folder_rules(root:&impl IsA<Widget>){
    let expander=descendants(root).into_iter().find(|w|w.widget_name()=="cleanup-folder-rules").unwrap().downcast::<gtk::Expander>().unwrap();
    expander.set_expanded(true);
    assert!(expander.child().unwrap().is_child_visible());
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
fn scroll_action_into_view(root: &impl IsA<Widget>, name: &str) {
    let target=button(root,name);
    if let Some(scroll)=target.ancestor(gtk::ScrolledWindow::static_type()).and_downcast::<gtk::ScrolledWindow>() {
        if let Some(bounds)=target.compute_bounds(&scroll) {
            let adjustment=scroll.vadjustment();
            eprintln!("Demo scroll {name}: y={} value={} upper={} page={}",bounds.y(),adjustment.value(),adjustment.upper(),adjustment.page_size());
            adjustment.set_value((adjustment.value()+bounds.y() as f64-32.).max(0.));
        }
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
fn wait_until(test:impl Fn()->bool,timeout:Duration)->bool {
    let started=std::time::Instant::now();
    while !test()&&started.elapsed()<timeout {wait_main_context(Duration::from_millis(50));}test()
}
fn log_selection(root:&impl IsA<Widget>)->gtk::SingleSelection{
    descendants(root).into_iter().find_map(|w|w.downcast::<gtk::ListView>().ok().filter(|v|v.widget_name()=="log-events-view")).unwrap().model().unwrap().downcast::<gtk::SingleSelection>().unwrap()
}
/// A new WidgetPaintable may not have an image until a subsequent frame.
/// Capture actual settled content with a bounded wait, including device scale.
fn capture_widget(root:&impl IsA<Widget>,path:&Path){
    // Headless frame clocks can leave spring transitions between states.
    // Static design captures use process-local reduced motion, never desktop settings.
    gtk::Settings::default().unwrap().set_property("gtk-enable-animations", false);
    wait_main_context(Duration::from_millis(1000));
    let root=root.as_ref();let paint=gtk::WidgetPaintable::new(Some(root));let node=std::cell::RefCell::new(None);root.queue_draw();
    assert!(wait_until(||{let snapshot=gtk::Snapshot::new();let scale=root.scale_factor() as f32;snapshot.scale(scale,scale);paint.snapshot(&snapshot,root.width() as f64,root.height() as f64);*node.borrow_mut()=snapshot.to_node();node.borrow().is_some()},Duration::from_secs(5)),"Capture never became ready: {}",path.display());
    root.native().and_then(|n|n.renderer()).expect("Mapped native renderer").render_texture(node.borrow().as_ref().unwrap(),None).save_to_png(path).unwrap();
    let mut footers=0;
    for button in descendants(root).into_iter().filter_map(|w|w.downcast::<gtk::Button>().ok()).filter(|b|b.is_mapped()&&matches!(b.label().as_deref(),Some("Save changes"|"Save exclusions"|"Discard changes"))){
        let b=button.compute_bounds(root).unwrap();assert!(b.x()>=0.&&b.y()>=0.&&b.x()+b.width()<=root.width() as f32&&b.y()+b.height()<=root.height() as f32,"Clipped dialog/page footer in {}",path.display());footers+=1;
    }
    let settings=gtk::Settings::default().unwrap();fs::write(path.with_extension("json"),serde_json::to_vec_pretty(&json!({"animations":settings.property::<bool>("gtk-enable-animations"),"settled_wait_ms":1000,"logical_width":root.width(),"logical_height":root.height(),"scale":root.scale_factor(),"text_dpi":settings.property::<i32>("gtk-xft-dpi"),"high_contrast":adw::StyleManager::default().is_high_contrast(),"dark":adw::StyleManager::default().is_dark(),"mapped_footer_actions":footers})).unwrap()).unwrap();
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
    win.set_default_size(1100, 760);
    stack.set_transition_type(gtk::StackTransitionType::None);
    gtk::Settings::default().unwrap().set_property("gtk-enable-animations", false);
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
    stack.set_visible_child_name("task-manager");
    performance(stack);
    let (app, win, outer, stack) = (app.clone(), win.clone(), outer.clone(), stack.clone());
    let paint = gtk::WidgetPaintable::new(Some(&outer));
    let mut tick = 0usize;
    let warmup = 128;
    let mut trace = Vec::new();
    let mut evidence = Vec::new();
    // Encoding a frame can exceed the interval on a busy dev machine. A
    // default-priority repeating source would then starve GTK's frame clock,
    // leaving scrolling and transitions unallocated between captures.
    glib::timeout_add_local_full(Duration::from_millis(125), glib::Priority::LOW, move || {
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
            32 => scroll_action_into_view(&page(&stack,"cleanup"), "Preview cleanup"),
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
                assert!(text(&page(&stack, "system-logs")).contains("1 matching event"));
                evidence.push(json!({"frame":frame,"searched":1}));
            }
            248 => {
                let logs=page(&stack,"system-logs");let target=descendants(&logs).into_iter().find(|w|w.widget_name()=="log-event-row"&&w.is_mapped()).unwrap();
                action(&mut trace,frame,"Inspect event",&target,&outer);log_selection(&logs).set_selected(0);bottom(&stack,"system-logs");
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
    open_cleanup_retention(&root);
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
        "diagnostics",
        "workspaces",
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
                open_cleanup_retention(&daily);
                let fields = descendants(&daily)
                    .iter()
                    .filter_map(|w| w.downcast_ref::<gtk::SpinButton>().cloned())
                    .collect::<Vec<_>>();
                fields[2].set_value(12.);
                let preview=button(&daily, "Preview daily policy");preview.emit_clicked();
                assert!(wait_until(||preview.is_sensitive(),Duration::from_secs(12)));
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
                let list=log_selection(&logs);
                assert_eq!(
                    list.n_items() as usize,
                    6,
                    "Expected six decoded sample journal events"
                );
                list.set_selected(0);
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
                    list.n_items() as usize,
                    0,
                    "Changing log source left old rows visible"
                );
                let period = dropdown(&stack, "Time range");
                period.set_selected(1);
                assert!(
                    list.n_items() as usize == 0 && !export.is_sensitive(),
                    "Changing log time range left old events exportable"
                );
                period.set_selected(2);
                let restored = fs::read(home.join("qa-journal-backup.jsonl")).unwrap();
                fs::write(home.join("demo-journal.jsonl"), restored).unwrap();
                source.set_selected(0);
                button(&logs, "Load logs").emit_clicked();
                evidence.push(json!({"log_load_decoded_six_and_selected_details":true,"source_and_period_changes_clear_old_events":true}));
            } else if step == 4 {
                let list=log_selection(&logs);
                assert_eq!(
                    list.n_items() as usize,
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
                let list=log_selection(&logs);
                let detail = descendants(&logs)
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::TextView>().ok())
                    .next()
                    .unwrap();
                assert_eq!(
                    list.n_items() as usize,
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
                    list.n_items() as usize == 0
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
                // The generic text/JSON report utility lives in Storage.
                // Activity is now a structured timeline with separate tests.
                let activity = page(&stack, "storage");
                stack.set_visible_child_name("storage");
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
                    wait_until(||!button(&storage, "Cancel scan").is_sensitive(),Duration::from_secs(15)),
                    "Cancel scan did not return to idle state: {}",text(&storage)
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
                // Paused/cold views now show a waiting state. Exercise Refresh
                // and await real mapped rows before checking table geometry.
                refresh(&stack);
                assert!(wait_until(||table_scroll.is_mapped()&&adjustment.upper()>adjustment.page_size(),Duration::from_secs(15)),"Process rows/scrolling did not become available: {}",text(&task));
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
                open_folder_rules(&rules);
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
                            open_folder_rules(&root);
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

/// Reproduce the recorded OpenCode-terminal OOM through actual native actions.
/// Uses historical kernel JSON; never starts a memory-consuming workload.
pub fn memory_qa(app:&adw::Application,win:&adw::ApplicationWindow,outer:&gtk::Box,stack:&gtk::Stack,directory:&str) {
    assert!(std::env::var_os("TEND_DEMO_DIR").is_some());
    let directory=std::path::PathBuf::from(directory);fs::create_dir_all(&directory).unwrap();
    let (app,win,outer,stack)=(app.clone(),win.clone(),outer.clone(),stack.clone());
    stack.set_transition_type(gtk::StackTransitionType::None);
    glib::timeout_add_local_once(Duration::from_millis(600),move|| {
        let mut trace=vec![];
        win.set_default_size(1100,820);wait_main_context(Duration::from_millis(250));
        let diagnostics=page(&stack,"diagnostics");stack.set_visible_child_name("diagnostics");
        let section=descendants(&diagnostics).into_iter().find(|w|w.widget_name()=="memory-incidents").unwrap().downcast::<gtk::Expander>().unwrap();
        assert!(!section.is_expanded());section.set_expanded(true);
        wait_main_context(Duration::from_millis(200));
        let explain=button(&section,"Explain recent memory failures");
        action(&mut trace,0,"Explain recent memory failures",&explain,&outer);
        explain.emit_clicked();assert!(wait_until(||explain.is_sensitive(),Duration::from_secs(15)));
        let report=descendants(&section).into_iter().find_map(|w|w.downcast::<gtk::TextView>().ok()).unwrap();
        let buffer=report.buffer();let full=buffer.text(&buffer.start_iter(),&buffer.end_iter(),false).to_string();
        for expected in ["1 recorded kernel OOM kill","System-wide memory exhaustion","node (PID 6557)","5.15 GiB","2.78 GiB","228.0 KiB of 4.00 GiB","does not prove a memory leak or identify its parent application"] {assert!(full.contains(expected),"Missing {expected}: {full}");}
        fs::write(directory.join("native-report.txt"),&full).unwrap();
        for name in ["Export text","Export JSON"] {
            button(&section,name).emit_clicked();
            assert!(text(&section).contains("Saved "));
        }
        let exports=fs::read_dir(tend::workstation::home().join("Documents/Tend reports")).unwrap().map(|p|p.unwrap().path()).collect::<Vec<_>>();
        assert_eq!(exports.len(),2);
        for path in &exports {assert!(fs::read_to_string(path).unwrap().contains("5.15 GiB"));}
        capture_widget(&outer,&directory.join("memory-diagnostics-1100.png"));
        win.set_default_size(720,820);wait_main_context(Duration::from_millis(250));
        capture_widget(&outer,&directory.join("memory-diagnostics-720.png"));
        win.set_default_size(1100,820);wait_main_context(Duration::from_millis(250));
        let logs=page(&stack,"system-logs");stack.set_visible_child_name("system-logs");wait_main_context(Duration::from_millis(200));
        click(&logs,&outer,&mut trace,1,"Last crashed apps");
        let selection=log_selection(&logs);
        assert!(wait_until(||button(&logs,"Last crashed apps").is_sensitive()&&selection.n_items()==1,Duration::from_secs(15)),"{}",text(&logs));
        assert_eq!(dropdown(&stack,"Log source").selected(),2);
        selection.set_selected(0);
        let details=text(&logs);for expected in ["node (PID 6557)","5.15 GiB","2.78 GiB","Event ID:"] {assert!(details.contains(expected),"{details}");}
        let search=descendants(&logs).into_iter().find_map(|w|w.downcast::<gtk::SearchEntry>().ok().filter(|e|e.placeholder_text().as_deref()==Some("Search application, service, PID or message"))).unwrap();
        action(&mut trace,2,"Filter by victim PID",&search,&outer);assert!(search.is_mapped());assert!(search.grab_focus());
        search.set_text("6557");wait_main_context(Duration::from_millis(300));assert!(text(&logs).contains("1 matching event."));
        search.set_text("no-matching-victim");assert!(wait_until(||selection.n_items()==0,Duration::from_secs(3)));
        search.set_text("6557");assert!(wait_until(||selection.n_items()==1,Duration::from_secs(3)));
        wait_main_context(Duration::from_millis(350));selection.set_selected(0);wait_main_context(Duration::from_millis(350));
        assert!(text(&logs).contains("Swap free at the kill: 228.0 KiB of 4.00 GiB"));
        capture_widget(&outer,&directory.join("memory-logs.png"));
        // All activity periods are selected explicitly: retained real fixture
        // timestamps must stay usable when this check is rerun months later.
        let activity=page(&stack,"activity");stack.set_visible_child_name("activity");
        let control=|tip:&str|descendants(&activity).into_iter().find_map(|w|w.downcast::<gtk::DropDown>().ok().filter(|w|w.tooltip_text().as_deref()==Some(tip))).unwrap();
        control("Activity source").set_selected(3);control("Activity time range").set_selected(4);
        let category=control("Activity category");category.set_selected(3);
        let table=descendants(&activity).into_iter().find_map(|w|w.downcast::<gtk::ColumnView>().ok()).unwrap();let model=table.model().unwrap();
        let load=button(&activity,"Load recent activity");
        wait_main_context(Duration::from_millis(250));
        action(&mut trace,2,"Load recent activity",&load,&outer);load.emit_clicked();
        assert!(wait_until(||load.is_sensitive()&&model.n_items()==1,Duration::from_secs(15)),"{}",text(&activity));
        model.select_item(0,true);assert!(text(&activity).contains("System-wide memory exhaustion"));
        assert!(text(&activity).contains("OOM kill"));
        capture_widget(&outer,&directory.join("memory-activity.png"));
        category.set_selected(5);load.emit_clicked();assert!(wait_until(||load.is_sensitive(),Duration::from_secs(15)));assert_eq!(model.n_items(),0);
        category.set_selected(3);control("Activity result").set_selected(2);load.emit_clicked();assert!(wait_until(||load.is_sensitive(),Duration::from_secs(15)));assert_eq!(model.n_items(),1);
        let include=descendants(&activity).into_iter().find_map(|w|w.downcast::<gtk::CheckButton>().ok().filter(|c|c.label().is_some_and(|s|s.starts_with("Include readable crashes")))).unwrap();include.set_active(false);load.emit_clicked();assert!(wait_until(||load.is_sensitive(),Duration::from_secs(15)));assert_eq!(model.n_items(),0);
        // A missing journal context is uncertainty, never a fabricated zero.
        let fixture=tend::workstation::home().join("demo-oom.jsonl");
        let retained=directory.join("complete-oom.jsonl");fs::rename(&fixture,&retained).unwrap();
        let raw=fs::read_to_string(&retained).unwrap();fs::write(&fixture,format!("{}\n",raw.lines().last().unwrap())).unwrap();
        stack.set_visible_child_name("diagnostics");explain.emit_clicked();assert!(wait_until(||explain.is_sensitive(),Duration::from_secs(15)));
        let partial=buffer.text(&buffer.start_iter(),&buffer.end_iter(),false).to_string();
        for expected in ["scope unconfirmed","Swapped memory: Not recorded","missing figures are not zero"] {assert!(partial.contains(expected),"{partial}");}
        fs::rename(&fixture,directory.join("partial-oom.jsonl")).unwrap();fs::rename(&retained,&fixture).unwrap();
        fs::write(directory.join("memory-evidence.json"),serde_json::to_vec_pretty(&json!({
            "actual_native_actions":trace,"killed_pid":6557,"anonymous_bytes":5401820u64*1024,"swap_bytes":728098u64*4096,
            "explanation_from_recorded_kernel_event":true,"crash_source_catches_no_core_dump":true,"pid_search_filters":true,"local_text_and_json_exports":exports.len(),
            "activity_pressure_and_crash_categories_distinct":true,"failed_result_includes_oom":true,"incident_reads_can_be_excluded":true,
            "missing_context_reported_unknown":true,"fixture_only":true,"stress_test_executed":false,"system_actions_executed":false
        })).unwrap()).unwrap();app.activate_action("quit",None);
    });
}

/// Phase-specific real-widget regression checks. Runs in a retained fixture and
/// exits only this NON_UNIQUE preview, never the installed/running application.
pub fn activity_qa(app:&adw::Application,win:&adw::ApplicationWindow,outer:&gtk::Box,stack:&gtk::Stack,directory:&str) {
    assert!(std::env::var_os("TEND_DEMO_DIR").is_some());
    let directory=std::path::PathBuf::from(directory);fs::create_dir_all(&directory).unwrap();
    let (app,win,outer,stack)=(app.clone(),win.clone(),outer.clone(),stack.clone());
    stack.set_transition_type(gtk::StackTransitionType::None);
    glib::timeout_add_local_once(Duration::from_millis(700),move|| {
        let home=tend::workstation::home();
        tend::workstation::audit_result("updates",Ok("Fixture successful check; no packages changed".into())).unwrap();
        assert!(tend::workstation::audit_result("updates",Err("Fixture failed check; no operation executed".into())).is_err());
        let mut session=tend::diagnostics::Session::new("Fixture diagnostic",10).unwrap();
        session.points.push(tend::diagnostics::Point {elapsed_ms:0,valid:false,cpu:None,memory_available:0,swap_used:0,cpu_pressure:None,memory_pressure:None,io_pressure:None,swap_in:None,swap_out:None});session.save().unwrap();
        let journal=home.join("demo-journal.jsonl");let retained=home.join("retained-activity-journal.jsonl");fs::rename(&journal,&retained).unwrap();
        let crash=json!({"__REALTIME_TIMESTAMP":(tend::workstation::now()*1000000).to_string(),"PRIORITY":"3","_COMM":"fixture-editor","_PID":"4100","COREDUMP_PID":"4100","COREDUMP_EXE":"/usr/bin/fixture-editor","COREDUMP_SIGNAL_NAME":"SIGSEGV","MESSAGE_ID":"fc2e22bc6ee647b6b90729ab34a250b1","MESSAGE":"Fixture editor crash; no application was closed"});
        fs::write(&journal,format!("{crash}\n")).unwrap();
        let activity=page(&stack,"activity");stack.set_visible_child_name("activity");
        let control=|tip:&str|descendants(&activity).into_iter().find_map(|w|w.downcast::<gtk::DropDown>().ok().filter(|w|w.tooltip_text().as_deref()==Some(tip))).unwrap();
        let source=control("Activity source");let kind=control("Activity category");let outcome=control("Activity result");let period=control("Activity time range");
        let search=descendants(&activity).into_iter().find_map(|w|w.downcast::<gtk::SearchEntry>().ok().filter(|e|e.placeholder_text().as_deref()==Some("Search summaries and saved reports"))).unwrap();
        let table=descendants(&activity).into_iter().find_map(|w|w.downcast::<gtk::ColumnView>().ok()).unwrap();let model=table.model().unwrap();
        let load=button(&activity,"Load recent activity");
        let read=||{load.emit_clicked();assert!(wait_until(||load.is_sensitive(),Duration::from_secs(20)),"{}",text(&activity));};
        source.set_selected(1);period.set_selected(4);read();
        assert_eq!(model.n_items(),200,"{}",text(&activity));assert!(button(&activity,"Older activity").is_sensitive());
        model.select_item(0,true);assert!(text(&activity).contains("No outcome recorded; phase may be active or interrupted"));
        button(&activity,"Older activity").emit_clicked();assert!(wait_until(||load.is_sensitive(),Duration::from_secs(20)));
        assert_eq!(model.n_items(),150);assert!(!button(&activity,"Older activity").is_sensitive());
        outcome.set_selected(4);read();assert_eq!(model.n_items(),1);model.select_item(0,true);assert!(text(&activity).contains("Awaiting result"));
        outcome.set_selected(1);kind.set_selected(2);search.set_text("Fixture apt-cache");wait_main_context(Duration::from_millis(300));read();
        assert_eq!(model.n_items(),25,"{}",text(&activity));model.select_item(0,true);
        let body=text(&activity);for expected in ["Attempt 2 of 3","Elapsed: 1.0 s","Net free-space change:","Fixture apt-cache outcome"] {assert!(body.contains(expected),"{body}");}
        source.set_selected(2);kind.set_selected(1);search.set_text("");wait_main_context(Duration::from_millis(300));read();
        assert_eq!(model.n_items(),1);model.select_item(0,true);assert!(text(&activity).contains("Fixture successful check"));
        outcome.set_selected(2);read();assert_eq!(model.n_items(),1);model.select_item(0,true);assert!(text(&activity).contains("Fixture failed check"));
        outcome.set_selected(0);kind.set_selected(3);read();assert_eq!(model.n_items(),1);model.select_item(0,true);assert!(text(&activity).contains("Fixture diagnostic"));
        source.set_selected(3);kind.set_selected(0);read();assert_eq!(model.n_items(),1);model.select_item(0,true);assert!(text(&activity).contains("SIGSEGV"));
        button(&activity,"Export this page").emit_clicked();assert!(wait_until(||text(&activity).contains("Saved "),Duration::from_secs(15)));
        assert_eq!(fs::read_dir(home.join("Documents/Tend reports")).unwrap().count(),1);
        button(&activity,"Maintenance journal").emit_clicked();assert!(wait_until(||button(&activity,"Maintenance journal").is_sensitive(),Duration::from_secs(20)));
        assert!(text(&activity).contains("Isolated maintenance journal preview"));assert_eq!(model.n_items(),0);
        source.set_selected(1);load.emit_clicked();stack.set_visible_child_name("overview");assert!(wait_until(||load.is_sensitive(),Duration::from_secs(20)));assert_eq!(model.n_items(),0);
        stack.set_visible_child_name("activity");read();assert_eq!(model.n_items(),200);model.select_item(0,true);
        for width in [720,1100] {
            win.set_default_size(width,820);wait_main_context(Duration::from_millis(250));
            capture_widget(&outer,&directory.join(format!("activity-{width}.png")));
        }
        search.set_text(&"x".repeat(241));wait_main_context(Duration::from_millis(300));read();assert_eq!(model.n_items(),0);assert!(text(&activity).contains("Invalid activity filters"));
        fs::rename(&journal,home.join("retained-fixture-crash.jsonl")).unwrap();fs::rename(&retained,&journal).unwrap();
        fs::write(directory.join("activity-evidence.json"),serde_json::to_vec_pretty(&json!({
            "root_writer_to_native_reader":true,"350_daily_outcomes_paged":true,"pending_is_not_claimed_completed":true,
            "category_result_and_text_filters":true,"successful_and_failed_local_reports":true,"diagnostic_and_crash_details":true,
            "page_export_fixture_only":true,"journal_preview_no_service_actions":true,"late_navigation_result_rejected":true,
            "unbounded_query_rejected":true,"original_journal_preserved":true,"system_actions_invoked":false
        })).unwrap()).unwrap();app.activate_action("quit",None);
    });
}

pub fn cleanup_qa(
    app: &adw::Application,
    win: &adw::ApplicationWindow,
    outer: &gtk::Box,
    stack: &gtk::Stack,
    directory: &str,
) {
    use std::os::unix::fs::PermissionsExt;
    assert!(std::env::var_os("TEND_DEMO_DIR").is_some());
    let directory = std::path::PathBuf::from(directory);
    fs::create_dir_all(&directory).unwrap();
    let (app, win, outer, stack) = (app.clone(), win.clone(), outer.clone(), stack.clone());
    stack.set_transition_type(gtk::StackTransitionType::None);
    glib::timeout_add_local_once(Duration::from_millis(700), move || {
        let home = tend::workstation::home();
        let config = home.join(".config/tend");
        fs::set_permissions(&config, fs::Permissions::from_mode(0o700)).unwrap();
        let exclusions = tend::exclusions::Policy {
            paths: vec![tend::exclusions::Entry {
                path: home.join("sample-cache/old-shader.cache"),
                enabled: true,
                note: "Keep shader".into(),
            }],
            ..Default::default()
        };
        let bytes = serde_json::to_vec_pretty(&exclusions).unwrap();
        fs::write(config.join("exclusions.json"), &bytes).unwrap();
        fs::write(config.join("exclusions.lock"), "").unwrap();
        for name in ["exclusions.json", "exclusions.lock"] {
            fs::set_permissions(config.join(name), fs::Permissions::from_mode(0o644)).unwrap();
        }
        let cleanup = page(&stack, "cleanup");
        stack.set_visible_child_name("cleanup");
        let preview = button(&cleanup, "Preview cleanup");
        preview.emit_clicked();
        assert!(wait_until(
            || preview.is_sensitive(),
            Duration::from_secs(12)
        ));
        let report = text(&cleanup);
        assert!(
            report.contains("Why:") && report.contains("Effect:"),
            "{report}"
        );
        let table = descendants(&cleanup)
            .into_iter()
            .find_map(|w| w.downcast::<gtk::ColumnView>().ok())
            .unwrap();
        let model = table.model().unwrap();
        assert_eq!(model.n_items(), 1, "{report}");
        model.select_item(0, true);
        assert!(text(&cleanup).contains("old-download.cache"));
        assert!(button(&cleanup, "Clean selected…").is_sensitive());
        // Additional retained fixture entries exercise every preview page.
        let old = std::time::SystemTime::now() - Duration::from_secs(30 * 86400);
        for i in 0..205 {
            let path = home.join(format!("sample-cache/paging-{i:03}.cache"));
            fs::write(&path, b"fixture").unwrap();
            fs::File::options()
                .write(true)
                .open(path)
                .unwrap()
                .set_times(fs::FileTimes::new().set_modified(old))
                .unwrap();
        }
        preview.emit_clicked();
        assert!(wait_until(
            || preview.is_sensitive(),
            Duration::from_secs(12)
        ));
        assert_eq!(model.n_items(), 200);
        button(&cleanup, "Next files").emit_clicked();
        assert_eq!(model.n_items(), 6);
        assert!(text(&cleanup).contains("Select a file to review"));
        button(&cleanup, "Previous files").emit_clicked();
        assert_eq!(model.n_items(), 200);
        let manage = button(&cleanup, "Manage exclusions…");
        manage.emit_clicked();
        assert!(wait_until(
            || manage.is_sensitive(),
            Duration::from_secs(12)
        ));
        let window = gtk::Window::list_toplevels()
            .into_iter()
            .find_map(|w| {
                w.downcast::<adw::Window>().ok().filter(|w| {
                    w.is_visible() && w.title().as_deref() == Some("Cleanup exclusions")
                })
            })
            .unwrap();
        assert!(!button(&window, "Save exclusions").is_sensitive());
        button(&window, "Add protected path…").emit_clicked();
        let dialog = gtk::Window::list_toplevels()
            .into_iter()
            .find_map(|w| {
                w.downcast::<adw::MessageDialog>()
                    .ok()
                    .filter(|w| w.is_visible())
            })
            .unwrap();
        let path = descendants(&dialog)
            .into_iter()
            .find_map(|w| {
                w.downcast::<gtk::Entry>()
                    .ok()
                    .filter(|w| w.placeholder_text().as_deref() == Some("Absolute protected path"))
            })
            .unwrap();
        path.set_text("relative");
        assert!(!dialog.is_response_enabled("save"));
        path.set_text(&home.join("keep-new").to_string_lossy());
        assert!(dialog.is_response_enabled("save"));
        button(&dialog, "Keep in draft").emit_clicked();
        wait_main_context(Duration::from_millis(100));
        let list = descendants(&window)
            .into_iter()
            .find_map(|w| {
                w.downcast::<gtk::ListBox>()
                    .ok()
                    .filter(|l| l.has_css_class("boxed-list"))
            })
            .unwrap();
        let rows = |list: &gtk::ListBox| {
            (0..65)
                .take_while(|i| list.row_at_index(*i).is_some())
                .count()
        };
        assert_eq!(rows(&list), 2);
        list.select_row(list.row_at_index(1).as_ref());
        button(&window, "Edit protected path…").emit_clicked();
        let edit = gtk::Window::list_toplevels()
            .into_iter()
            .find_map(|w| {
                w.downcast::<adw::MessageDialog>()
                    .ok()
                    .filter(|w| w.is_visible())
            })
            .unwrap();
        let note = descendants(&edit)
            .into_iter()
            .find_map(|w| {
                w.downcast::<gtk::Entry>()
                    .ok()
                    .filter(|e| e.placeholder_text().as_deref() == Some("Optional label"))
            })
            .unwrap();
        note.set_text("Retained draft");
        button(&edit, "Keep in draft").emit_clicked();
        wait_main_context(Duration::from_millis(100));
        assert!(text(&window).contains("Retained draft"));
        list.select_row(list.row_at_index(1).as_ref());
        button(&window, "Remove protected path").emit_clicked();
        assert_eq!(rows(&list), 1);
        list.select_row(list.row_at_index(0).as_ref());
        button(&window, "Remove protected path").emit_clicked();
        assert_eq!(rows(&list), 0);
        button(&window, "Discard changes").emit_clicked();
        assert_eq!(rows(&list), 1);
        assert_eq!(fs::read(config.join("exclusions.json")).unwrap(), bytes);
        capture_widget(&window,&directory.join("exclusions.png"));
        window.close();
        drop(window);
        drop(dialog);
        drop(edit);
        drop(path);
        drop(note);
        drop(list);
        stack.set_visible_child_name("daily-cleanup");
        let daily = page(&stack, "daily-cleanup");
        assert!(button(&daily, "Manage exclusions…").is_sensitive());
        let daily_preview = button(&daily, "Preview daily policy");
        daily_preview.emit_clicked();
        assert!(wait_until(
            || daily_preview.is_sensitive(),
            Duration::from_secs(12)
        ));
        assert!(
            text(&daily).contains("Protected · Unused Docker build cache"),
            "{}",
            text(&daily)
        );
        stack.set_visible_child_name("cleanup");
        assert!(!button(&cleanup, "Clean selected…").is_sensitive());
        assert_eq!(model.n_items(), 0);
        preview.emit_clicked();
        assert!(wait_until(
            || preview.is_sensitive(),
            Duration::from_secs(12)
        ));
        for width in [720, 1100] {
            win.set_default_size(width, 820);
            wait_main_context(Duration::from_millis(200));
            capture_widget(&outer,&directory.join(format!("cleanup-{width}.png")));
        }
        fs::write(directory.join("cleanup-evidence.json"),serde_json::to_vec_pretty(&json!({
            "real_preview_filters_protected_file":true,"why_and_effect_details":true,"all_206_candidates_paged":true,
            "draft_add_edit_remove_discard":true,"invalid_path_rejected":true,"unsupported_save_disabled":true,
            "daily_preview_discloses_bulk_skips":true,"leaving_page_releases_selection":true,
            "policy_bytes_preserved":fs::read(config.join("exclusions.json")).unwrap()==bytes,
            "deletion_actions_invoked":false,"system_actions_invoked":false
        })).unwrap()).unwrap();
        app.activate_action("quit", None);
    });
}

pub fn expansion_qa(app:&adw::Application,win:&adw::ApplicationWindow,outer:&gtk::Box,stack:&gtk::Stack,directory:&str) {
    assert!(std::env::var_os("TEND_DEMO_DIR").is_some());
    let directory=std::path::PathBuf::from(directory);fs::create_dir_all(&directory).unwrap();
    let (app,win,outer,stack)=(app.clone(),win.clone(),outer.clone(),stack.clone());
    stack.set_transition_type(gtk::StackTransitionType::None);
    glib::timeout_add_local_once(Duration::from_millis(700),move || {
        let mut evidence=vec![];
        let storage=page(&stack,"storage");stack.set_visible_child_name("storage");
        let home=tend::workstation::home();let fixture=home.join("qa-history-files");fs::create_dir_all(&fixture).unwrap();
        fs::write(fixture.join("alpha"),b"fixture first").unwrap();
        let entry=descendants(&storage).into_iter().filter_map(|w|w.downcast::<gtk::Entry>().ok()).find(|e|e.placeholder_text().as_deref()==Some("Folder to scan")).unwrap();
        entry.set_text(&fixture.to_string_lossy());button(&storage,"Scan folder").emit_clicked();
        let report=descendants(&storage).into_iter().filter_map(|w|w.downcast::<gtk::TextView>().ok()).next().unwrap();
        assert!(wait_for_report(&report,Duration::from_secs(15)));assert!(text(&storage).contains("Initial complete baseline saved"),"{}",text(&storage));
        fs::rename(fixture.join("alpha"),home.join("retained-alpha")).unwrap();fs::write(fixture.join("beta"),b"fixture second").unwrap();
        button(&storage,"Scan folder").emit_clicked();assert!(wait_for_report(&report,Duration::from_secs(15)));assert!(text(&storage).contains("Complete metadata snapshot saved"));
        button(&storage,"Load storage changes").emit_clicked();
        let deadline=std::time::Instant::now()+Duration::from_secs(15);
        while !button(&storage,"Load storage changes").is_sensitive()&&std::time::Instant::now()<deadline {wait_main_context(Duration::from_millis(50));}
        assert!(text(&storage).contains("Showing 2 changes"),"{}",text(&storage));
        let table=descendants(&storage).into_iter().filter_map(|w|w.downcast::<gtk::ColumnView>().ok()).next().unwrap();
        assert_eq!(table.model().unwrap().n_items(),2);table.model().unwrap().select_item(0,true);
        assert!(text(&storage).contains("Logical:"));
        evidence.push(json!({"storage_native_scan_baseline":true,"storage_second_scan_comparison":true,"storage_loaded_two_changes":true,"storage_selected_details":true,"files_retained":home.join("retained-alpha").exists()}));
        let workspace=page(&stack,"workspaces");stack.set_visible_child_name("workspaces");
        descendants(&workspace).into_iter().filter_map(|w|w.downcast::<gtk::Expander>().ok()).find(|e|e.label().as_deref()==Some("Add or edit a project")).unwrap().set_expanded(true);
        button(&workspace,"Load workspaces").emit_clicked();assert!(wait_until(||button(&workspace,"Add project to draft").is_sensitive(),Duration::from_secs(15)));
        let entry_row=|title:&str|descendants(&workspace).into_iter().filter_map(|w|w.downcast::<adw::EntryRow>().ok()).find(|e|e.title()==title).unwrap();
        entry_row("Project name").set_text("QA workspace");entry_row("Absolute project folder").set_text(&fixture.to_string_lossy());
        button(&workspace,"Add project to draft").emit_clicked();assert!(wait_until(||text(&workspace).contains("Project draft updated"),Duration::from_secs(15)));
        button(&workspace,"Save changes").emit_clicked();assert!(wait_until(||text(&workspace).contains("Workspace settings saved"),Duration::from_secs(15)));
        assert_eq!(tend::workspaces::Settings::read().unwrap().projects.len(),1);
        let original_cwd=std::env::current_dir().unwrap();std::env::set_current_dir(&fixture).unwrap();
        button(&workspace,"Refresh project activity").emit_clicked();assert!(wait_until(||text(&workspace).contains("Activity sampled"),Duration::from_secs(15)));std::env::set_current_dir(original_cwd).unwrap();
        let activity=descendants(&workspace).into_iter().filter_map(|w|w.downcast::<gtk::ColumnView>().ok()).next().unwrap();assert!(activity.model().unwrap().n_items()>0,"Preview process should match its fixture cwd");
        fs::write(home.join("demo-workspaces-docker.jsonl"),json!({"id":"a".repeat(64),"name":"/sample-api","state":"running","compose_directory":fixture.to_str(),"compose_project":"qa","compose_service":"api"}).to_string()).unwrap();
        button(&workspace,"Inspect Compose labels").emit_clicked();assert!(wait_until(||text(&workspace).contains("sample-api"),Duration::from_secs(15)));
        button(&workspace,"Load last scan").emit_clicked();assert!(wait_until(||text(&workspace).contains("Recorded scan for QA workspace"),Duration::from_secs(15)));
        for width in [720,1100] {
            win.set_default_size(width,760);wait_main_context(Duration::from_millis(250));
            capture_widget(&outer,&directory.join(format!("workspaces-{width}.png")));
        }
        button(&workspace,"Inspect project storage").emit_clicked();assert_eq!(stack.visible_child_name().as_deref(),Some("storage"));assert_eq!(entry.text().as_str(),fixture.to_string_lossy());stack.set_visible_child_name("workspaces");
        button(&workspace,"Edit selected").emit_clicked();entry_row("Project name").set_text("Unsaved renamed project");button(&workspace,"Update project draft").emit_clicked();assert!(wait_until(||text(&workspace).contains("Project draft updated"),Duration::from_secs(15)));
        button(&workspace,"Discard changes").emit_clicked();assert!(text(&workspace).contains("QA workspace"));assert_eq!(tend::workspaces::Settings::read().unwrap().projects[0].name,"QA workspace");
        button(&workspace,"Remove from draft").emit_clicked();button(&workspace,"Discard changes").emit_clicked();assert!(text(&workspace).contains("QA workspace"));
        button(&workspace,"Remove from draft").emit_clicked();button(&workspace,"Save changes").emit_clicked();assert!(wait_until(||text(&workspace).contains("Workspace settings saved"),Duration::from_secs(15)));assert!(tend::workspaces::Settings::read().unwrap().projects.is_empty());
        evidence.push(json!({"workspace_add_edit_save_discard_remove":true,"workspace_live_cwd_mapping":true,"workspace_compose_label_fixture":true,"workspace_cached_scan":true,"workspace_storage_navigation":true,"no_project_files_changed":fixture.join("beta").exists()}));
        let task=page(&stack,"task-manager");
        let process_state=descendants(&task).into_iter().find(|w|w.widget_name()=="process-results-state").unwrap().downcast::<gtk::Stack>().unwrap();
        let before_page=crate::monitor::budget_stats()["collections"].as_u64().unwrap();
        stack.set_visible_child_name("task-manager");
        assert_eq!(process_state.visible_child_name().as_deref(),Some("waiting"));
        assert_eq!(crate::monitor::budget_stats()["subscribers"].as_u64().unwrap(),0,"Sampling competed with the initial page paint");
        stack.set_visible_child_name("storage");
        wait_main_context(Duration::from_millis(150));
        assert_eq!(crate::monitor::budget_stats()["collections"].as_u64().unwrap(),before_page,"Leaving before paint still collected processes");
        assert_eq!(crate::monitor::budget_stats()["subscribers"].as_u64().unwrap(),0);
        let population_started=std::time::Instant::now();
        stack.set_visible_child_name("task-manager");
        let process=descendants(&task).into_iter().filter_map(|w|w.downcast::<gtk::ColumnView>().ok()).next().unwrap();
        assert!(wait_for_task_rows(&process.model().unwrap(),Duration::from_secs(15)));
        let first_rows_ms=population_started.elapsed().as_secs_f64()*1000.;
        assert_eq!(process_state.visible_child_name().as_deref(),Some("rows"));
        let tabs=descendants(&task).into_iter().filter_map(|w|w.downcast::<gtk::Stack>().ok()).find(|s|s.child_by_name("apps").is_some()).unwrap();
        assert!(wait_until(||!text(&task).contains("Loading process rows…"),Duration::from_secs(15)));
        let expected=descendants(&task).into_iter().filter_map(|w|w.downcast::<gtk::Label>().ok()).map(|l|l.text().to_string()).find(|t|t.starts_with("CPU ")&&t.contains("Memory")&&t.ends_with(" processes")).unwrap();
        let total=expected.split_whitespace().rev().nth(1).unwrap().parse::<u32>().unwrap();
        assert_eq!(process.model().unwrap().n_items(),total,"Staged population omitted sampled processes");
        let complete_rows_ms=population_started.elapsed().as_secs_f64()*1000.;
        let model=process.model().unwrap();
        let own=(0..model.n_items()).find_map(|i|{let object=model.item(i).unwrap().downcast::<glib::BoxedAnyObject>().unwrap();let own=object.borrow::<tend::task_stats::Task>().pid==std::process::id();own.then_some((i,object))}).unwrap();
        model.select_item(own.0,true);
        let before=crate::monitor::budget_stats()["collections"].as_u64().unwrap();refresh(&stack);
        assert!(wait_until(||crate::monitor::budget_stats()["collections"].as_u64().unwrap()>before,Duration::from_secs(15)));wait_main_context(Duration::from_millis(150));
        assert!(wait_until(||!text(&task).contains("Loading process rows…"),Duration::from_secs(15)));
        let current=(0..model.n_items()).find_map(|i|{let object=model.item(i).unwrap().downcast::<glib::BoxedAnyObject>().unwrap();let own=object.borrow::<tend::task_stats::Task>().pid==std::process::id();own.then_some(object)}).unwrap();
        assert_eq!(own.1,current,"A live refresh replaced a surviving process row");
        assert_eq!(model.clone().downcast::<gtk::SingleSelection>().unwrap().selected_item().unwrap(),current.upcast::<glib::Object>(),"Live refresh lost the selected process");
        evidence.push(json!({"complete_staged_process_population":total,"first_actual_rows_ms":first_rows_ms,"complete_actual_rows_ms":complete_rows_ms,"waiting_state_before_data":true,"stable_live_row_objects_and_selection":true,"automatic_sampling_after_first_paint":true,"leave_before_paint_cancels_collection":true}));
        let process_search=descendants(&tabs.child_by_name("processes").unwrap()).into_iter().find_map(|w|w.downcast::<gtk::SearchEntry>().ok().filter(|e|e.placeholder_text().as_deref()==Some("Search name, PID or user"))).unwrap();
        let info=descendants(&task).into_iter().find(|w|w.widget_name()=="process-selection-details").unwrap().downcast::<gtk::Label>().unwrap();
        assert!(info.text().contains(&format!("PID {}",std::process::id())));
        process_search.set_text("tend-no-match-clear-selected-details");assert!(wait_until(||model.n_items()==0,Duration::from_secs(5)));
        assert_eq!(info.text(),"Select a process for details. Other users’ processes are read-only.");
        assert!(!button(&task,"Request quit…").is_sensitive()&&!button(&task,"End task…").is_sensitive());
        process_search.set_text("");assert!(wait_until(||model.n_items()>0,Duration::from_secs(5)));
        evidence.push(json!({"filtering_out_selection_clears_stale_details":true,"no_selection_process_actions_disabled":true}));
        for name in ["apps","process-tree"] {
            tabs.set_visible_child_name(name);wait_main_context(Duration::from_millis(150));
            let root=tabs.child_by_name(name).unwrap();
            let table=descendants(&root).into_iter().filter_map(|w|w.downcast::<gtk::ColumnView>().ok()).next().unwrap();
            let selection=table.model().unwrap().downcast::<gtk::SingleSelection>().unwrap();
            let model=selection.model().unwrap().downcast::<gtk::TreeListModel>().unwrap();
            let original=model.n_items();assert!(original>0,"{name} has no rows");
            let mut expanded=false;
            for i in 0..model.n_items() {let row=model.row(i).unwrap();if row.is_expandable() {row.set_expanded(true);expanded=true;break}}
            assert!(model.n_items()>=original);
            let search=descendants(&root).into_iter().filter_map(|w|w.downcast::<gtk::SearchEntry>().ok()).next().unwrap();
            search.set_text("tend-no-match-qa-identity");wait_main_context(Duration::from_millis(300));assert_eq!(model.n_items(),0);
            search.set_text("");wait_main_context(Duration::from_millis(300));assert!(model.n_items()>=original);
            let check=descendants(&root).into_iter().filter_map(|w|w.downcast::<gtk::CheckButton>().ok()).next().unwrap();check.set_active(true);assert!(model.n_items()>=original);check.set_active(false);
            // A child opens the existing process details; no signal is sent.
            let mut linked=false;
            for i in 0..model.n_items().min(500) {
                let row=model.row(i).unwrap();row.set_expanded(true);selection.set_selected(i);
                let button=button(&root,"Show selected process");
                if button.is_sensitive() {
                    process_search.set_text("tend-no-match-before-process-link");wait_main_context(Duration::from_millis(300));
                    button.emit_clicked();assert_eq!(tabs.visible_child_name().as_deref(),Some("processes"));
                    assert!(process_search.text().is_empty(),"Process link retained an excluding search");
                    assert!(process.model().unwrap().clone().downcast::<gtk::SingleSelection>().unwrap().selected_item().is_some(),"Process link did not select its identity during staged loading");
                    linked=true;break
                }
            }
            assert!(linked,"No process details link in {name}");
            evidence.push(json!({"view":name,"rows":original,"expanded":expanded,"search":true,"user_filter":true,"process_link":true}));
        }
        // Charts must not rebuild hidden process tables on each live sample.
        tabs.set_visible_child_name("performance");
        assert_eq!(process.model().unwrap().n_items(),0);
        refresh(&stack);wait_main_context(Duration::from_millis(2500));
        assert_eq!(process.model().unwrap().n_items(),0,"Hidden process table was rebuilt while charting");
        for name in ["apps","process-tree"]{
            let table=descendants(&tabs.child_by_name(name).unwrap()).into_iter().find_map(|w|w.downcast::<gtk::ColumnView>().ok()).unwrap();
            assert_eq!(table.model().unwrap().n_items(),0,"Hidden group model was retained while charting");
        }
        tabs.set_visible_child_name("processes");
        assert!(wait_for_task_rows(&process.model().unwrap(),Duration::from_secs(5)),"Switching back did not stage cached process data");
        evidence.push(json!({"performance_skips_hidden_process_models":true,"return_to_processes_uses_latest_snapshot":true}));
        stack.set_visible_child_name("diagnostics");let diag=page(&stack,"diagnostics");
        assert!(!button(&diag,"Finish recording").is_sensitive());assert!(!button(&diag,"Save recording").is_sensitive());
        button(&diag,"Load saved recordings").emit_clicked();wait_main_context(Duration::from_millis(500));assert!(text(&diag).contains("No saved recordings"));
        for index in 0..2 {
            button(&diag,"Start recording").emit_clicked();assert!(!button(&diag,"Start recording").is_sensitive());
            let marker=descendants(&diag).into_iter().filter_map(|w|w.downcast::<gtk::Entry>().ok()).find(|e|e.placeholder_text().as_deref()==Some("e.g. Scrolling stalled")).unwrap();
            marker.set_text("QA scrolling interaction");button(&diag,"Add workload marker").emit_clicked();
            wait_main_context(Duration::from_millis(4500));button(&diag,"Finish recording").emit_clicked();
            assert!(button(&diag,"Save recording").is_sensitive());assert!(text(&diag).contains("QA scrolling interaction"));
            button(&diag,"Save recording").emit_clicked();wait_main_context(Duration::from_millis(1000));
            assert!(text(&diag).contains("saved privately"),"Save failed: {}",text(&diag));
            evidence.push(json!({"recording":index,"real_samples":true,"marker":true,"finished":true,"saved":true}));
        }
        button(&diag,"Load saved recordings").emit_clicked();wait_main_context(Duration::from_millis(700));assert!(button(&diag,"Compare recordings").is_sensitive());
        button(&diag,"View baseline").emit_clicked();assert!(text(&diag).contains("Observed signals"));
        button(&diag,"Compare recordings").emit_clicked();assert!(text(&diag).contains("Insufficient evidence"));
        button(&diag,"Start recording").emit_clicked();wait_main_context(Duration::from_millis(100));button(&diag,"Discard recording").emit_clicked();assert!(!button(&diag,"Save recording").is_sensitive());
        let sessions=tend::diagnostics::load().unwrap();assert_eq!(sessions.len(),2);
        evidence.push(json!({"comparison_short_run_warning":true,"cancel_kept_history_unchanged":true,"sessions":sessions.len()}));
        for width in [720,1100] {
            win.set_default_size(width,760);wait_main_context(Duration::from_millis(250));
            capture_widget(&outer,&directory.join(format!("diagnostics-{width}.png")));
        }
        stack.set_visible_child_name("task-manager");refresh(&stack);
        assert!(wait_for_task_rows(&process.model().unwrap(),Duration::from_secs(15)));
        evidence.push(json!({"quit_during_process_population":text(&task).contains("Loading process rows…")}));
        fs::write(directory.join("expansion-evidence.json"),serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
        app.activate_action("quit",None);
    });
}

/// Native monitoring editor, sampler delivery and notification protocol fixture.
pub fn health_qa(app:&adw::Application,win:&adw::ApplicationWindow,outer:&gtk::Box,stack:&gtk::Stack,directory:&str){
    let (app,win,outer,stack,directory)=(app.clone(),win.clone(),outer.clone(),stack.clone(),Path::new(directory).to_owned());
    let home=tend::workstation::home();assert_eq!(home.parent(),directory.parent());assert!(home.join("demo-journal.jsonl").is_file());
    fs::create_dir_all(&directory).unwrap();
    glib::timeout_add_local_once(Duration::from_millis(1200),move||{
        let named=|root:&gtk::Widget,name:&str|descendants(root).into_iter().find(|w|w.widget_name()==name).unwrap_or_else(||panic!("Missing monitoring control {name}"));
        let notifications=||->Vec<Value>{fs::read(directory.join("notifications.json")).ok().and_then(|b|serde_json::from_slice(&b).ok()).unwrap_or_default()};
        assert_eq!(crate::health_ui::summary(),"Monitoring off");assert!(!tend::health::path().exists());assert!(!tend::history::path().exists());
        history_limits_qa(&win,&stack,&directory);
        stack.set_visible_child_name("care");button(&page(&stack,"care"),"Configure monitoring").emit_clicked();wait_main_context(Duration::from_millis(400));
        let dialog=win.visible_dialog().unwrap();let widget=dialog.clone().upcast::<gtk::Widget>();
        let enabled=named(&widget,"monitor-enabled").downcast::<gtk::Switch>().unwrap();
        assert!(!enabled.is_active());assert!(!button(&widget,"Save changes").is_sensitive());
        enabled.set_active(true);assert!(button(&widget,"Save changes").is_sensitive());button(&widget,"Discard changes").emit_clicked();assert!(!enabled.is_active());
        enabled.set_active(true);named(&widget,"monitor-hold").downcast::<gtk::SpinButton>().unwrap().set_value(30.);named(&widget,"monitor-cooldown").downcast::<gtk::SpinButton>().unwrap().set_value(60.);
        button(&widget,"Save changes").emit_clicked();assert!(wait_until(||tend::health::Settings::read().is_ok_and(|s|s.enabled)&&crate::health_ui::enabled(),Duration::from_secs(6)));
        wait_main_context(Duration::from_millis(300));assert!(!button(&widget,"Save changes").is_sensitive());
        for width in [720,1100]{win.set_default_size(width,760);wait_main_context(Duration::from_millis(500));capture_widget(&win,&directory.join(format!("monitoring-{width}.png")));}
        let weak_save=button(&widget,"Save changes").downgrade();dialog.close();drop(widget);drop(dialog);drop(enabled);wait_main_context(Duration::from_millis(400));assert!(wait_until(||weak_save.upgrade().is_none(),Duration::from_secs(3)),"Closed monitoring editor retains its signal graph after closing settled");
        for target in ["diagnostics","system-logs","activity","storage"]{app.activate_action(&format!("open-{target}"),None);assert_eq!(stack.visible_child_name().as_deref(),Some(target));}
        stack.set_visible_child_name("care");
        let close=descendants(&outer).into_iter().find(|w|w.widget_name()=="window-close").unwrap().downcast::<gtk::Button>().unwrap();close.emit_clicked();assert!(!win.is_visible(),"Preview panel did not receive the close");
        let start=tend::observations::boot_millis().unwrap()+60000;
        for elapsed in [0,15000,30000]{crate::health_ui::qa_observe(&app,95.,start+elapsed);}
        assert!(wait_until(||notifications().len()==1,Duration::from_secs(6)));assert_eq!(notifications()[0]["title"],"High CPU use");wait_main_context(Duration::from_millis(250));
        app.activate_action("show",None);button(&page(&stack,"care"),"Configure monitoring").emit_clicked();wait_main_context(Duration::from_millis(300));let dialog=win.visible_dialog().unwrap();let widget=dialog.clone().upcast::<gtk::Widget>();
        button(&widget,"Snooze for one hour").emit_clicked();assert!(crate::health_ui::snoozed());
        for elapsed in [45000,60000,75000,90000]{crate::health_ui::qa_observe(&app,95.,start+elapsed);}wait_main_context(Duration::from_millis(300));assert_eq!(notifications().len(),1);
        button(&widget,"Resume notifications").emit_clicked();assert!(!crate::health_ui::snoozed());crate::health_ui::qa_observe(&app,95.,start+105000);
        assert!(wait_until(||notifications().len()==2,Duration::from_secs(6)));wait_main_context(Duration::from_millis(250));
        for elapsed in [120000,135000,150000]{crate::health_ui::qa_observe(&app,10.,start+elapsed);}
        assert!(wait_until(||notifications().len()==3,Duration::from_secs(6)));assert!(notifications()[2]["title"].as_str().unwrap().contains("Recovered"));wait_main_context(Duration::from_millis(250));
        // The optional crash check reads only prepared public journal data.
        let crashes=named(&widget,"monitor-crashes").downcast::<gtk::Switch>().unwrap();crashes.set_active(true);button(&widget,"Save changes").emit_clicked();assert!(wait_until(||tend::health::Settings::read().is_ok_and(|s|s.crashes),Duration::from_secs(6)));wait_main_context(Duration::from_millis(250));
        let journal=home.join("demo-journal.jsonl");let original=journal.with_extension("health-original");fs::rename(&journal,&original).unwrap();let report=json!({"__REALTIME_TIMESTAMP":((tend::workstation::now()-1)*1000000).to_string(),"PRIORITY":"3","_COMM":"fixture-app","COREDUMP_EXE":"/fixture/app","COREDUMP_PID":"42","MESSAGE":"Fixture crash; no real app was stopped"});fs::write(&journal,format!("{report}\n")).unwrap();crate::health_ui::qa_crashes(&app);
        assert!(wait_until(||notifications().len()==4,Duration::from_secs(6)));assert_eq!(notifications()[3]["title"],"Recent crashes or memory kills");crate::health_ui::qa_crashes(&app);wait_main_context(Duration::from_millis(400));assert_eq!(notifications().len(),4);fs::rename(&journal,directory.join("fixture-crash-journal.jsonl")).unwrap();fs::rename(original,&journal).unwrap();
        named(&widget,"monitor-enabled").downcast::<gtk::Switch>().unwrap().set_active(false);button(&widget,"Save changes").emit_clicked();assert!(wait_until(||!crate::health_ui::enabled(),Duration::from_secs(6)));let count=notifications().len();crate::health_ui::qa_observe(&app,100.,start+1000000);wait_main_context(Duration::from_millis(300));assert_eq!(notifications().len(),count);assert_eq!(crate::health_ui::summary(),"Monitoring off");
        dialog.close();drop(widget);drop(dialog);drop(crashes);
        let query=tend::activity::Query{source:2,category:3,..Default::default()};
        assert!(wait_until(||tend::activity::load(&query,None).is_ok_and(|events|events.events.len()==4),Duration::from_secs(6)),"Alert history is missing");
        crate::health_ui::qa_audit_burst();
        assert!(wait_until(||tend::activity::load(&query,None).is_ok_and(|events|events.events.len()==7),Duration::from_secs(6)),"Bounded alert backlog did not drain");
        let events=tend::activity::load(&query,None).unwrap();
        fs::write(directory.join("health-evidence.json"),serde_json::to_vec_pretty(&json!({"native_save_discard":true,"history_limits_native":true,"closed_editor_released":true,"hidden_monitor_delivery":true,"warning_repeat_recovery":true,"snooze_resume":true,"optional_crash_query_deduplicated":true,"disabled_no_delivery":true,"panel_shortcuts":true,"bounded_alert_history":events.events.len(),"queued_audit_burst":3,"notifications":notifications().len(),"fixture_only":true})).unwrap()).unwrap();
        app.quit();
    });
}

fn history_limits_qa(win:&adw::ApplicationWindow,stack:&gtk::Stack,directory:&Path){
    let find=|root:&gtk::Widget,name:&str|descendants(root).into_iter().find(|w|w.widget_name()==name).unwrap().downcast::<gtk::SpinButton>().unwrap();
    stack.set_visible_child_name("care");button(&page(stack,"care"),"History limits").emit_clicked();wait_main_context(Duration::from_millis(300));
    let dialog=win.visible_dialog().unwrap();let widget=dialog.clone().upcast::<gtk::Widget>();let budget=find(&widget,"history-budget");let retention=find(&widget,"history-retention");
    assert_eq!(budget.value_as_int(),512);assert_eq!(retention.value_as_int(),30);assert!(!button(&widget,"Save changes").is_sensitive());
    budget.set_value(256.);retention.set_value(14.);button(&widget,"Discard changes").emit_clicked();assert_eq!(budget.value_as_int(),512);assert_eq!(retention.value_as_int(),30);
    budget.set_value(128.);retention.set_value(45.);button(&widget,"Save changes").emit_clicked();budget.set_value(192.);
    assert!(wait_until(||tend::history::Settings::read().is_ok_and(|s|s.max_mib==128&&s.retention_days==45)&&button(&widget,"Save changes").is_sensitive(),Duration::from_secs(6)));assert!(text(&widget).contains("Unsaved changes"));button(&widget,"Discard changes").emit_clicked();assert_eq!(budget.value_as_int(),128);
    for width in [720,1100]{win.set_default_size(width,760);wait_main_context(Duration::from_millis(400));capture_widget(win,&directory.join(format!("history-limits-{width}.png")));}
    let weak=button(&widget,"Save changes").downgrade();dialog.close();drop(widget);drop(dialog);drop(budget);drop(retention);wait_main_context(Duration::from_millis(350));assert!(wait_until(||weak.upgrade().is_none(),Duration::from_secs(3)));
    for target in ["storage","activity"]{stack.set_visible_child_name(target);button(&page(stack,target),"History limits").emit_clicked();wait_main_context(Duration::from_millis(250));let dialog=win.visible_dialog().unwrap();let widget=dialog.clone().upcast::<gtk::Widget>();assert_eq!(find(&widget,"history-budget").value_as_int(),128);assert_eq!(find(&widget,"history-retention").value_as_int(),45);assert!(!button(&widget,"Save changes").is_sensitive());dialog.close();drop(widget);drop(dialog);wait_main_context(Duration::from_millis(300));}
    let path=tend::workstation::home().join(".config/tend/history.json");let saved=path.with_extension("qa-saved");fs::rename(&path,&saved).unwrap();tend::workstation::private_write(&path,"{bad fixture").unwrap();
    button(&page(stack,"activity"),"History limits").emit_clicked();wait_main_context(Duration::from_millis(300));let dialog=win.visible_dialog().unwrap();let widget=dialog.clone().upcast::<gtk::Widget>();assert!(text(&widget).contains("Saved history limits could not be read"));assert_eq!(find(&widget,"history-budget").value_as_int(),512);dialog.close();drop(widget);drop(dialog);wait_main_context(Duration::from_millis(300));assert_eq!(fs::read(&path).unwrap(),b"{bad fixture");fs::rename(&path,directory.join("invalid-history-settings.json")).unwrap();fs::rename(saved,path).unwrap();
    assert!(!tend::history::path().exists(),"Saving limits must not create or scan history");
}

/// End-to-end settings transfer using real buttons and native editors. Only
/// the file picker is supplied a path in this private, retained fixture.
pub fn bundle_qa(app:&adw::Application,win:&adw::ApplicationWindow,outer:&gtk::Box,stack:&gtk::Stack,directory:&str){
    let(app,win,outer,stack,directory)=(app.clone(),win.clone(),outer.clone(),stack.clone(),Path::new(directory).to_owned());let home=tend::workstation::home();assert_eq!(home.parent(),directory.parent());fs::create_dir_all(&directory).unwrap();
    glib::timeout_add_local_once(Duration::from_millis(1000),move||{
        use tend::settings_bundle::{Bundle,Section};use std::os::unix::fs::PermissionsExt;
        let modal=|title:&str|{assert!(wait_until(||win.visible_dialog().is_some_and(|d|d.title().as_str()==title),Duration::from_secs(10)),"Missing settings dialog {title}");wait_main_context(Duration::from_millis(150));win.visible_dialog().unwrap()};
        let named=|root:&gtk::Widget,name:&str|descendants(root).into_iter().find(|w|w.widget_name()==name).unwrap_or_else(||panic!("Missing settings control {name}"));
        let close=|dialog:adw::Dialog|{dialog.close();drop(dialog);wait_main_context(Duration::from_millis(350));};
        stack.set_visible_child_name("care");let care=page(&stack,"care");crate::settings_ui::qa_cancel();button(&care,"Export settings").emit_clicked();assert!(wait_until(||button(&care,"Export settings").is_sensitive(),Duration::from_secs(3)));let export=directory.join("exported.tend.json");crate::settings_ui::qa_pick(export.clone());button(&care,"Export settings").emit_clicked();assert!(wait_until(||export.is_file()&&button(&care,"Export settings").is_sensitive(),Duration::from_secs(10)));let mut bundle=Bundle::read(&export).unwrap();assert_eq!(bundle.sections.len(),10);assert_eq!(fs::metadata(&export).unwrap().permissions().mode()&0o777,0o600);
        let encoded=bundle.encode().unwrap();assert!(!encoded.contains("reboot"));assert!(!encoded.contains("password"));assert!(!encoded.contains("login"));
        bundle.sections.get_mut(&Section::Appearance).unwrap()["theme"]=json!(std::env::var("TEND_QA_THEME_ID").ok().and_then(|s|s.parse::<u8>().ok()).filter(|v|[1,2].contains(v)).unwrap_or(2));bundle.sections.get_mut(&Section::Appearance).unwrap()["close_to_panel"]=json!(false);bundle.sections.get_mut(&Section::Monitoring).unwrap()["enabled"]=json!(true);bundle.sections.get_mut(&Section::Monitoring).unwrap()["hold_seconds"]=json!(120);bundle.sections.get_mut(&Section::History).unwrap()["max_mib"]=json!(256);bundle.sections.get_mut(&Section::History).unwrap()["retention_days"]=json!(14);
        bundle.sections.insert(Section::Workspaces,json!({"version":1,"projects":[{"id":"fixture-project","name":"Imported project","root":home.join("sample-cache")}]}));bundle.sections.get_mut(&Section::ManualCleanup).unwrap()["age_days"]=json!(45);bundle.sections.get_mut(&Section::DailyCleanup).unwrap()["build_days"]=json!(28);
        bundle.sections.insert(Section::FolderRules,json!({"rules":[{"name":"Imported cache","folder":home.join("sample-cache"),"pattern":"*.cache","days":21,"recursive":false,"enabled":true}]}));bundle.sections.insert(Section::UserSchedule,json!({"enabled":true,"expression":"5 9 * * 1-5","catch_up":true}));bundle.sections.get_mut(&Section::Maintenance).unwrap()["hour"]=json!(9);
        bundle.sections.insert(Section::Exclusions,json!({"paths":(0..64).map(|i|json!({"path":home.join(format!("keep-{i:02}")),"note":format!("Fixture protected path {i}"),"enabled":true})).collect::<Vec<_>>()}));
        let input=directory.join("input.tend.json");bundle.save_export(&input).unwrap();let before=Section::ALL.into_iter().map(|s|(s,s.current().unwrap())).collect::<std::collections::BTreeMap<_,_>>();
        crate::settings_ui::qa_pick(input);button(&care,"Import settings").emit_clicked();let dialog=modal("Import settings");let widget=dialog.clone().upcast::<gtk::Widget>();assert!(!button(&widget,"Review selected section").is_sensitive());button(&widget,"Load as inactive drafts").emit_clicked();assert!(wait_until(||tend::settings_bundle::draft_path().is_file()&&button(&widget,"Review selected section").is_sensitive(),Duration::from_secs(10)));let drafts=Bundle::read_drafts().unwrap().unwrap();for section in [Section::Monitoring,Section::UserSchedule,Section::Maintenance]{assert_eq!(drafts.sections[&section]["enabled"],false);}assert_eq!(drafts.sections[&Section::FolderRules]["rules"][0]["enabled"],false);for(s,value)in &before{assert_eq!(&s.current().unwrap(),value,"Loading drafts changed {}",s.title());}
        // Full comparison paging includes every protected-path field.
        let choose=named(&widget,"bundle-section").downcast::<gtk::DropDown>().unwrap();choose.set_selected(9);assert!(button(&widget,"Next changes").is_sensitive());let mut pages=1;while button(&widget,"Next changes").is_sensitive(){button(&widget,"Next changes").emit_clicked();pages+=1;assert!(pages<=5);}assert_eq!(pages,4);button(&widget,"Previous changes").emit_clicked();assert!(button(&widget,"Next changes").is_sensitive());
        for width in [720,1100]{win.set_default_size(width,760);wait_main_context(Duration::from_millis(400));capture_widget(&win,&directory.join(format!("settings-import-{width}.png")));}
        drop(widget);drop(choose);close(dialog);
        // An existing page draft cannot be overwritten by an imported review.
        let manual=page(&stack,"cleanup");open_cleanup_retention(&manual);let spins=descendants(&manual).into_iter().filter_map(|w|w.downcast::<gtk::SpinButton>().ok()).collect::<Vec<_>>();spins[0].set_value(75.);button(&care,"Review imported drafts").emit_clicked();let dialog=modal("Import settings");let widget=dialog.clone().upcast::<gtk::Widget>();named(&widget,"bundle-section").downcast::<gtk::DropDown>().unwrap().set_selected(4);button(&widget,"Review selected section").emit_clicked();wait_main_context(Duration::from_millis(300));assert!(text(&widget).contains("existing unsaved changes"));assert_eq!(spins[0].value_as_int(),75);drop(widget);close(dialog);button(&manual,"Discard changes").emit_clicked();
        let mut checked=vec![];
        for section in Section::ALL{
            button(&care,"Review imported drafts").emit_clicked();let comparison=modal("Import settings");let widget=comparison.clone().upcast::<gtk::Widget>();let index=Section::ALL.into_iter().position(|s|s==section).unwrap();named(&widget,"bundle-section").downcast::<gtk::DropDown>().unwrap().set_selected(index as u32);button(&widget,"Review selected section").emit_clicked();drop(widget);drop(comparison);let dialog=modal(section.title());let widget=dialog.clone().upcast::<gtk::Widget>();let save=button(&widget,"Save changes");
            for width in [720,1100]{win.set_default_size(width,760);wait_main_context(Duration::from_millis(100));capture_widget(&win,&directory.join(format!("review-{index}-{width}.png")));}
            if matches!(section,Section::DailyCleanup|Section::Maintenance|Section::Exclusions){assert!(!save.is_sensitive(),"Root save must be blocked in preview");assert!(text(&widget).contains("System saves are disabled"));}
            else if section==Section::UserSchedule{let entry=named(&widget,"bundle-expression").downcast::<adw::EntryRow>().unwrap();entry.set_text("not a cron expression");assert!(!save.is_sensitive());entry.set_text("5 9 * * 1-5");save.emit_clicked();assert!(wait_until(||text(&widget).contains("Changes were not saved"),Duration::from_secs(5)));for e in descendants(&widget).into_iter().filter_map(|w|w.downcast::<gtk::Expander>().ok()).filter(|e|e.label().as_deref()==Some("Details")){e.set_expanded(true);}wait_main_context(Duration::from_millis(150));assert!(text(&widget).contains("Cron writes are disabled"),"{}",text(&widget));assert_eq!(section.current().unwrap(),before[&section]);}
            else{
                if section==Section::History{
                    // A changed file is detected after the shared write lock.
                    tend::workstation::private_write(&home.join(".config/tend/history.json"),&serde_json::to_string(&tend::history::Settings{version:1,max_mib:384,retention_days:28}).unwrap()).unwrap();save.emit_clicked();assert!(wait_until(||text(&widget).contains("Changes were not saved"),Duration::from_secs(5)),"{}",text(&widget));for e in descendants(&widget).into_iter().filter_map(|w|w.downcast::<gtk::Expander>().ok()).filter(|e|e.label().as_deref()==Some("Details")){e.set_expanded(true);}wait_main_context(Duration::from_millis(150));assert!(text(&widget).contains("Saved settings changed"),"{}",text(&widget));assert_eq!(tend::history::Settings::read().unwrap().max_mib,384);
                    drop(save);drop(widget);close(dialog);button(&care,"Review imported drafts").emit_clicked();let comparison=modal("Import settings");let widget=comparison.clone().upcast::<gtk::Widget>();named(&widget,"bundle-section").downcast::<gtk::DropDown>().unwrap().set_selected(index as u32);button(&widget,"Review selected section").emit_clicked();drop(widget);drop(comparison);let dialog=modal(section.title());let widget=dialog.clone().upcast::<gtk::Widget>();button(&widget,"Save changes").emit_clicked();assert!(wait_until(||tend::history::Settings::read().is_ok_and(|s|s.max_mib==256&&s.retention_days==14)&&text(&widget).contains("Saved · no unsaved changes"),Duration::from_secs(8)));drop(widget);close(dialog);checked.push(section.title());continue;
                }
                if section==Section::Monitoring{named(&widget,"bundle-hold_seconds").downcast::<gtk::SpinButton>().unwrap().set_value(180.);button(&widget,"Discard changes").emit_clicked();assert_eq!(named(&widget,"bundle-hold_seconds").downcast::<gtk::SpinButton>().unwrap().value_as_int(),60);assert!(!save.is_sensitive());assert_eq!(section.current().unwrap(),before[&section]);named(&widget,"bundle-hold_seconds").downcast::<gtk::SpinButton>().unwrap().set_value(120.);}
                if section==Section::Workspaces{button(&widget,"Add item").emit_clicked();assert!(!save.is_sensitive());named(&widget,"bundle-projects-1-name").downcast::<adw::EntryRow>().unwrap().set_text("Temporary project draft");named(&widget,"bundle-projects-1-root").downcast::<adw::EntryRow>().unwrap().set_text(home.to_str().unwrap());assert!(save.is_sensitive());let stale=named(&widget,"bundle-projects-1-name").downcast::<adw::EntryRow>().unwrap();button(&widget,"Remove from draft").emit_clicked();stale.set_text("Stale widget must not edit replacement data");drop(stale);}
                if section==Section::FolderRules{assert!(!named(&widget,"bundle-rules-0-enabled").downcast::<gtk::Switch>().unwrap().is_active());}
                assert!(save.is_sensitive(),"No Save for {}",section.title());save.emit_clicked();assert!(wait_until(||text(&widget).contains("Saved · no unsaved changes"),Duration::from_secs(8)),"Save did not finish: {}",text(&widget));assert_eq!(section.current().unwrap(),drafts.sections[&section]);
            }
            let weak=save.downgrade();drop(save);drop(widget);close(dialog);assert!(wait_until(||weak.upgrade().is_none(),Duration::from_secs(3)),"Review retained its Save widget after closing settled: {}",section.title());checked.push(section.title());
        }
        assert_eq!(checked.len(),10);assert_eq!(spins[0].value_as_int(),45,"Existing manual editor was not refreshed");
        // Closing an in-flight review and leaving Preferences both invalidate
        // the callback rather than opening a cancelled modal later.
        button(&care,"Review imported drafts").emit_clicked();let dialog=modal("Import settings");let widget=dialog.clone().upcast::<gtk::Widget>();button(&widget,"Review selected section").emit_clicked();drop(widget);close(dialog);wait_main_context(Duration::from_millis(350));assert!(win.visible_dialog().is_none(),"Cancelled review reopened itself");
        button(&care,"Review imported drafts").emit_clicked();stack.set_visible_child_name("storage");wait_main_context(Duration::from_millis(500));assert!(win.visible_dialog().is_none(),"A late settings load opened after navigation");stack.set_visible_child_name("care");
        let invalid=directory.join("invalid.json");fs::write(&invalid,b"{bad bundle").unwrap();crate::settings_ui::qa_pick(invalid);button(&care,"Import settings").emit_clicked();assert!(wait_until(||text(&care).contains("Settings were not imported"),Duration::from_secs(5)));assert_eq!(Bundle::read_drafts().unwrap().unwrap(),drafts);
        crate::settings_ui::qa_pick(directory.clone());button(&care,"Export settings").emit_clicked();assert!(wait_until(||text(&care).contains("Settings were not exported")&&button(&care,"Export settings").is_sensitive(),Duration::from_secs(5)));
        let retry_export=directory.join("roundtrip.tend.json");crate::settings_ui::qa_pick(retry_export.clone());button(&care,"Export settings").emit_clicked();assert!(wait_until(||retry_export.is_file(),Duration::from_secs(8)));let exported=Bundle::read(&retry_export).unwrap();assert_eq!(exported.sections[&Section::History]["max_mib"],256);assert_eq!(exported.sections[&Section::UserSchedule],before[&Section::UserSchedule]);
        fs::write(directory.join("bundle-evidence.json"),serde_json::to_vec_pretty(&json!({"all_ten_native_sections":checked,"native_export_import_buttons":true,"fixture_picker":true,"complete_diff_pagination":pages,"inactive_drafts_preserve_active_settings":true,"pending_edits_preserved":true,"concurrent_save_refused":true,"cancelled_and_stale_callbacks_ignored":true,"native_validation_add_remove":true,"stale_array_controls_ignored":true,"section_save_discard_and_lifetime":true,"root_and_cron_saves_blocked_in_preview":true,"invalid_input_preserves_drafts":true,"export_private_mode":true,"cancel_and_export_failure_recover":true,"roundtrip_export":true,"no_cleanup_or_restart":true})).unwrap()).unwrap();app.quit();let _=outer;
    });
}
