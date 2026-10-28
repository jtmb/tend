//! Native page, keyboard and first-painted-feedback checks. Documentation only.
use adw::prelude::*;
use gtk::glib;
use serde_json::json;
use std::{
    cell::RefCell,
    fs,
    path::Path,
    rc::Rc,
    time::{Duration, Instant},
};
fn descendants(root: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
    let mut result = vec![root.as_ref().clone()];
    let mut index = 0;
    while index < result.len() {
        let mut child = result[index].first_child();
        while let Some(w) = child {
            child = w.next_sibling();
            result.push(w);
        }
        index += 1;
    }
    result
}
fn button(root: &impl IsA<gtk::Widget>, name: &str) -> gtk::Button {
    descendants(root)
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .find(|b| {
            b.label().as_deref() == Some(name)
                || (b.has_css_class("nav-item")
                    && descendants(b).iter().any(|w| {
                        w.downcast_ref::<gtk::Label>()
                            .is_some_and(|l| l.text() == name)
                    }))
        })
        .unwrap_or_else(|| panic!("Missing native action {name}"))
}
fn wait(duration: Duration) {
    let loop_ = glib::MainLoop::new(None, false);
    let quit = loop_.clone();
    glib::timeout_add_local_once(duration, move || quit.quit());
    loop_.run();
}
fn until(test: impl Fn() -> bool) {
    let start = Instant::now();
    while !test() {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "Native release check timed out"
        );
        wait(Duration::from_millis(10));
    }
}
fn text(root: &impl IsA<gtk::Widget>) -> String {
    descendants(root)
        .iter()
        .filter_map(|w| w.downcast_ref::<gtk::Label>().map(|l| l.text().to_string()))
        .collect::<Vec<_>>()
        .join("\n")
}
pub fn run(
    app: &adw::Application,
    win: &adw::ApplicationWindow,
    outer: &gtk::Box,
    stack: &gtk::Stack,
    directory: &str,
) {
    assert!(std::env::var_os("TEND_DEMO_DIR").is_some());
    let home = tend::workstation::home();
    assert!(home.join("demo-journal.jsonl").is_file());
    let directory = Path::new(directory).to_owned();
    assert_eq!(directory.parent(), home.parent());
    fs::create_dir_all(&directory).unwrap();
    let (app, win, outer, stack) = (app.clone(), win.clone(), outer.clone(), stack.clone());
    glib::timeout_add_local_once(Duration::from_millis(600), move || {
        let pages = [
            ("overview", "Overview"),
            ("task-manager", "Task Manager"),
            ("diagnostics", "Diagnostics"),
            ("workspaces", "Workspaces"),
            ("storage", "Storage"),
            ("cleanup", "Disk cleanup"),
            ("daily-cleanup", "Daily cleanup"),
            ("cleanup-scheduling", "Cleanup schedule"),
            ("system-logs", "System logs"),
            ("startup", "Startup & services"),
            ("tuning", "Performance"),
            ("development", "Development tools"),
            ("reports", "Updates & reports"),
            ("recovery", "Recovery"),
            ("care", "Preferences"),
            ("schedule", "Maintenance"),
            ("activity", "Activity"),
        ];
        let pending: Rc<RefCell<Option<Instant>>> = Rc::new(RefCell::new(None));
        let latency = Rc::new(RefCell::new(Vec::<f64>::new()));
        let clock = win.frame_clock().unwrap();
        let (p, l) = (pending.clone(), latency.clone());
        let signal = clock.connect_after_paint(move |_| {
            if let Some(start) = p.borrow_mut().take() {
                l.borrow_mut().push(start.elapsed().as_secs_f64() * 1000.);
            }
        });
        let mut observations = vec![];
        win.set_default_size(1100, 760);
        wait(Duration::from_millis(300));
        for repeat in 0..3 {
            for (id, title) in pages {
                // Locating a widget through the retained test tree is harness
                // work, not part of a human clicking that already visible button.
                let lookup=Instant::now();
                let target=button(&outer,title);
                let lookup_ms=lookup.elapsed().as_secs_f64()*1000.;
                *pending.borrow_mut() = Some(Instant::now());
                let callback=Instant::now();
                target.emit_clicked();
                let callback_ms=callback.elapsed().as_secs_f64()*1000.;
                win.queue_draw();
                until(|| pending.borrow().is_none());
                assert_eq!(stack.visible_child_name().as_deref(), Some(id));
                observations.push(json!({"action":"navigation","page":id,"repeat":repeat,"harness_lookup_ms":lookup_ms,"callback_ms":callback_ms,"first_painted_feedback_ms":latency.borrow().last().unwrap()}));
                wait(Duration::from_millis(80));
            }
        }
        // Local filtering is measured after a real asynchronous journal load.
        button(&outer, "System logs").emit_clicked();
        let logs = stack.child_by_name("system-logs").unwrap();
        button(&logs, "Load logs").emit_clicked();
        until(|| text(&logs).contains("6 matching events"));
        let severity = descendants(&logs)
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::DropDown>().ok())
            .find(|w| w.tooltip_text().as_deref() == Some("Severity filter"))
            .unwrap();
        for repeat in 0..3 {
            for (selection, count) in [(1, 2), (0, 6)] {
                *pending.borrow_mut() = Some(Instant::now());
                severity.set_selected(selection);
                win.queue_draw();
                until(|| pending.borrow().is_none());
                assert!(text(&logs).contains(&format!("{count} matching events")));
                observations.push(json!({"action":"severity_filter","repeat":repeat,"matches":count,"first_painted_feedback_ms":latency.borrow().last().unwrap()}));
                wait(Duration::from_millis(80));
            }
        }
        // Full-size journal fixture proves viewport virtualization and index
        // mapping after filtering. Original sample bytes/mtime are restored.
        let journal = home.join("demo-journal.jsonl");
        let original = directory.join("original-journal.jsonl");
        fs::rename(&journal, &original).unwrap();
        let now = tend::workstation::now();
        let mut rows = String::new();
        for index in 0..1000u64 {
            let row = json!({"__REALTIME_TIMESTAMP":((now-index)*1000000).to_string(),"PRIORITY":if index%2==0{"3"}else{"6"},"_COMM":"demo-journal","_PID":(50000+index).to_string(),"MESSAGE":format!("Virtualized journal fixture {index}; no system operation.")});
            rows.push_str(&format!("{row}\n"));
        }
        fs::write(&journal, rows).unwrap();
        severity.set_selected(0);
        button(&logs, "Load logs").emit_clicked();
        until(|| text(&logs).contains("1000 matching events"));
        let view = descendants(&logs)
            .into_iter()
            .find_map(|w| w.downcast::<gtk::ListView>().ok())
            .unwrap();
        let selection = view
            .model()
            .unwrap()
            .downcast::<gtk::SingleSelection>()
            .unwrap();
        assert_eq!(selection.n_items(), 25);
        wait(Duration::from_millis(100));
        let materialized = descendants(&view)
            .iter()
            .filter(|w| w.widget_name() == "log-event-row")
            .count();
        assert!(
            materialized > 0 && materialized <= 25,
            "Log list materialized {materialized} rows"
        );
        for repeat in 0..3 {
            for (selected, count) in [(1, 500), (0, 1000)] {
                *pending.borrow_mut() = Some(Instant::now());
                severity.set_selected(selected);
                win.queue_draw();
                until(|| pending.borrow().is_none());
                assert!(text(&logs).contains(&format!("{count} matching events")));
                observations.push(json!({"action":"large_journal_filter","repeat":repeat,"matches":count,"first_painted_feedback_ms":latency.borrow().last().unwrap()}));
                wait(Duration::from_millis(80));
            }
        }
        severity.set_selected(1);
        assert!(!button(&logs, "Previous events").is_sensitive());
        assert!(button(&logs, "Next events").is_sensitive());
        button(&logs, "Next events").emit_clicked();
        assert!(button(&logs, "Previous events").is_sensitive());
        assert!(text(&logs).contains("26–50 of 500"));
        selection.set_selected(24);
        let detail = descendants(&logs)
            .into_iter()
            .find_map(|w| w.downcast::<gtk::TextView>().ok())
            .unwrap();
        let buffer = detail.buffer();
        assert!(
            buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), false)
                .contains("PID: 50098"),
            "Filtered event selection used a view index as a data index"
        );
        button(&logs, "Export filtered logs").emit_clicked();
        // A completed export must not replace the status of a later filter.
        severity.set_selected(0);
        assert!(text(&logs).contains("1000 matching events"));
        let reports = home.join("Documents/Tend reports");
        until(|| {
            reports.is_dir()
                && fs::read_dir(&reports).unwrap().any(|entry| {
                    entry
                        .unwrap()
                        .path()
                        .extension()
                        .is_some_and(|v| v == "txt")
                })
                && button(&logs, "Export filtered logs").is_sensitive()
        });
        let exported = fs::read_dir(&reports)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.extension().is_some_and(|v| v == "txt"))
            .unwrap();
        assert_eq!(
            fs::read_to_string(exported)
                .unwrap()
                .matches("Virtualized journal fixture")
                .count(),
            500,
            "Export omitted other result pages"
        );
        assert!(
            text(&logs).contains("1000 matching events"),
            "A stale export callback overwrote the current filter status"
        );
        fs::rename(&journal, directory.join("large-journal-fixture.jsonl")).unwrap();
        fs::rename(&original, &journal).unwrap();
        severity.set_selected(0);
        button(&logs, "Load logs").emit_clicked();
        until(|| text(&logs).contains("6 matching events"));
        clock.disconnect(signal);
        fs::write(
            directory.join("interaction-evidence.json"),
            serde_json::to_vec_pretty(&observations).unwrap(),
        )
        .unwrap();
        let mut captures = vec![];
        for width in [720, 1100] {
            win.set_default_size(width, 760);
            wait(Duration::from_millis(250));
            for (id, _) in pages {
                stack.set_visible_child_name(id);
                wait(Duration::from_millis(250));
                let path = directory.join(format!("{id}-{width}.png"));
                let paint = gtk::WidgetPaintable::new(Some(&outer));
                let node = RefCell::new(None);
                win.queue_draw();
                until(|| {
                    let shot = gtk::Snapshot::new();
                    let scale = outer.scale_factor() as f32;
                    shot.scale(scale, scale);
                    paint.snapshot(&shot, outer.width() as f64, outer.height() as f64);
                    *node.borrow_mut() = shot.to_node();
                    node.borrow().is_some()
                });
                win.renderer()
                    .unwrap()
                    .render_texture(node.borrow().as_ref().unwrap(), None)
                    .save_to_png(&path)
                    .unwrap();
                crate::docs_demo::validate_capture(&outer, &stack, path.to_str().unwrap());
                let root = stack.visible_child().unwrap();
                for e in descendants(&root)
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::Expander>().ok())
                {
                    if let Some(child) = e.child() {
                        assert_eq!(
                            child.is_child_visible(),
                            e.is_expanded(),
                            "Collapsed content remains a focus candidate"
                        );
                    }
                }
                let mut focused = std::collections::BTreeSet::new();
                gtk::prelude::GtkWindowExt::set_focus(&win, None::<&gtk::Widget>);
                for _ in 0..64 {
                    if !win.child_focus(gtk::DirectionType::TabForward) {
                        continue;
                    }
                    if let Some(f) = gtk::prelude::RootExt::focus(&win)
                        .filter(|f| f.is_ancestor(&root) || f == &root)
                    {
                        focused.insert(format!("{}:{}", f.type_().name(), f.widget_name()));
                    }
                }
                assert!(!focused.is_empty(), "No keyboard focus on {id}");
                captures.push(json!({"page":id,"width":outer.width(),"keyboard_targets":focused.len(),"image":path}));
            }
        }
        let settings = gtk::Settings::default().unwrap();
        let worst = latency.borrow().iter().copied().fold(0., f64::max);
        let report = json!({"scope":"Real native navigation/filter callbacks to first painted feedback, not completion of animations or external queries; Xvfb/Cairo", "timings":observations,"worst_first_feedback_ms":worst,"interaction_target_ms":100,"interaction_target_met":worst<100.,"captures":captures,"scale":outer.scale_factor(),"text_dpi":settings.property::<i32>("gtk-xft-dpi"),"high_contrast":adw::StyleManager::default().is_high_contrast(),"virtualized_journal_records":1000,"materialized_log_rows":materialized,"filtered_selection_mapping":true,"journal_page_navigation":true,"paged_export_all_matches":true,"export_preserves_later_filter_status":true,"fixture_only":true});
        fs::write(
            directory.join("release-evidence.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        app.quit();
    });
}
