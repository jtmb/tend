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
fn thread_cpu_ns() -> u64 {
    let mut time = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    if unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut time) } != 0 {
        return 0;
    }
    (time.tv_sec as u64).saturating_mul(1_000_000_000).saturating_add(time.tv_nsec as u64)
}
fn thread_queue_ns() -> Option<u64> {
    std::fs::read_to_string("/proc/thread-self/schedstat").ok()?
        .split_whitespace().nth(1)?.parse().ok()
}
fn queue_wait_ms(start: Option<u64>) -> Option<f64> {
    thread_queue_ns()?.checked_sub(start?).map(|n| n as f64 / 1_000_000.)
}
type PendingFeedback = Rc<RefCell<Option<(Instant, u64, Option<u64>)>>>;
fn frame_phase(name: &str, pending: &PendingFeedback, phases: &RefCell<Vec<serde_json::Value>>) {
    if let Some((start, cpu, queue)) = *pending.borrow() {
        phases.borrow_mut().push(json!({
            "phase": name,
            "elapsed_ms": start.elapsed().as_secs_f64() * 1000.,
            "main_thread_cpu_ms": thread_cpu_ns().saturating_sub(cpu) as f64 / 1_000_000.,
            "run_queue_wait_ms": queue_wait_ms(queue),
        }));
    }
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
        let pending: PendingFeedback = Rc::new(RefCell::new(None));
        let latency = Rc::new(RefCell::new(Vec::<f64>::new()));
        let phases = Rc::new(RefCell::new(Vec::<serde_json::Value>::new()));
        let feedback_details = Rc::new(RefCell::new(Vec::<serde_json::Value>::new()));
        let clock = win.frame_clock().unwrap();
        let (p, trace) = (pending.clone(), phases.clone());
        let before = clock.connect_before_paint(move |_| frame_phase("before_paint", &p, &trace));
        let (p, trace) = (pending.clone(), phases.clone());
        let layout = clock.connect_layout(move |_| frame_phase("layout", &p, &trace));
        let (p, trace) = (pending.clone(), phases.clone());
        let paint = clock.connect_paint(move |_| frame_phase("paint", &p, &trace));
        let (p, l, trace, details) = (pending.clone(), latency.clone(), phases.clone(), feedback_details.clone());
        let signal = clock.connect_after_paint(move |_| {
            if let Some((start, cpu, queue)) = p.borrow_mut().take() {
                l.borrow_mut().push(start.elapsed().as_secs_f64() * 1000.);
                details.borrow_mut().push(json!({
                    "main_thread_cpu_ms": thread_cpu_ns().saturating_sub(cpu) as f64 / 1_000_000.,
                    "run_queue_wait_ms": queue_wait_ms(queue),
                    "phases": trace.borrow_mut().drain(..).collect::<Vec<_>>(),
                }));
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
                phases.borrow_mut().clear();
                *pending.borrow_mut() = Some((Instant::now(), thread_cpu_ns(), thread_queue_ns()));
                let callback=Instant::now();
                target.emit_clicked();
                let callback_ms=callback.elapsed().as_secs_f64()*1000.;
                win.queue_draw();
                until(|| pending.borrow().is_none());
                assert_eq!(stack.visible_child_name().as_deref(), Some(id));
                observations.push(json!({"action":"navigation","page":id,"repeat":repeat,"harness_lookup_ms":lookup_ms,"callback_ms":callback_ms,"first_painted_feedback_ms":latency.borrow().last().unwrap(),"feedback_details":feedback_details.borrow().last().unwrap()}));
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
                phases.borrow_mut().clear();
                *pending.borrow_mut() = Some((Instant::now(), thread_cpu_ns(), thread_queue_ns()));
                severity.set_selected(selection);
                win.queue_draw();
                until(|| pending.borrow().is_none());
                assert!(text(&logs).contains(&format!("{count} matching event{}",if count==1{""}else{"s"})));
                observations.push(json!({"action":"severity_filter","repeat":repeat,"matches":count,"first_painted_feedback_ms":latency.borrow().last().unwrap(),"feedback_details":feedback_details.borrow().last().unwrap()}));
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
                phases.borrow_mut().clear();
                *pending.borrow_mut() = Some((Instant::now(), thread_cpu_ns(), thread_queue_ns()));
                severity.set_selected(selected);
                win.queue_draw();
                until(|| pending.borrow().is_none());
                assert!(text(&logs).contains(&format!("{count} matching event{}",if count==1{""}else{"s"})));
                observations.push(json!({"action":"large_journal_filter","repeat":repeat,"matches":count,"first_painted_feedback_ms":latency.borrow().last().unwrap(),"feedback_details":feedback_details.borrow().last().unwrap()}));
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
        for id in [before, layout, paint] { clock.disconnect(id); }
        fs::write(
            directory.join("interaction-evidence.json"),
            serde_json::to_vec_pretty(&observations).unwrap(),
        )
        .unwrap();
        let mut retention_checks = vec![];
        for id in ["cleanup", "daily-cleanup"] {
            stack.set_visible_child_name(id);
            let root = stack.child_by_name(id).unwrap();
            let expander = descendants(&root).into_iter()
                .find(|w| w.widget_name() == "cleanup-retention")
                .unwrap().downcast::<gtk::Expander>().unwrap();
            let summary = descendants(&root).into_iter()
                .find(|w| w.widget_name() == "cleanup-retention-summary")
                .unwrap().downcast::<gtk::Label>().unwrap();
            assert!(!expander.is_expanded());
            assert!(!expander.child().unwrap().is_child_visible());
            assert!(summary.is_visible() && summary.text().contains("Build cache:"));
            expander.set_expanded(true);
            assert!(expander.child().unwrap().is_child_visible());
            assert!(!summary.is_visible());
            let fields = descendants(&expander.child().unwrap()).into_iter()
                .filter_map(|w| w.downcast::<gtk::SpinButton>().ok()).collect::<Vec<_>>();
            assert_eq!(fields.len(), 5);
            let categories = descendants(&root).into_iter()
                .filter_map(|w| w.downcast::<gtk::Switch>().ok()).collect::<Vec<_>>();
            assert_eq!(categories.len(), if id == "daily-cleanup" { 5 } else { 7 });
            let owners = [1, 2, 1, 3, if id == "daily-cleanup" { 4 } else { 5 }];
            for (index, field) in fields.iter().enumerate() {
                let original = field.value_as_int();
                let before = summary.text();
                categories[owners[index]].set_active(true);
                assert!(field.is_sensitive(), "Enabled category still blocks its limit");
                field.set_value(f64::from(original + 1));
                assert_ne!(summary.text(), before, "Draft summary missed an edited limit");
                button(&root, "Discard changes").emit_clicked();
                assert_eq!(field.value_as_int(), original);
                assert_eq!(summary.text(), before);
            }
            for width in [720, 1100] {
                win.set_default_size(width, 760);
                wait(Duration::from_millis(250));
                if let Some(scroll) = expander.ancestor(gtk::ScrolledWindow::static_type()).and_downcast::<gtk::ScrolledWindow>() {
                    if let Some(bounds) = expander.compute_bounds(&scroll) {
                        let position = scroll.vadjustment();
                        position.set_value((position.value() + f64::from(bounds.y()) - 24.).max(0.));
                    }
                }
                wait(Duration::from_millis(150));
                let path = directory.join(format!("{id}-retention-{width}.png"));
                let paintable = gtk::WidgetPaintable::new(Some(&outer));
                let shot = gtk::Snapshot::new();
                let scale = outer.scale_factor() as f32;
                shot.scale(scale, scale);
                paintable.snapshot(&shot, outer.width() as f64, outer.height() as f64);
                win.renderer().unwrap().render_texture(shot.to_node().as_ref().unwrap(), None)
                    .save_to_png(&path).unwrap();
                crate::docs_demo::validate_capture(&outer, &stack, path.to_str().unwrap());
            }
            expander.set_expanded(false);
            assert!(summary.is_visible());
            assert!(!expander.child().unwrap().is_child_visible());
            assert!(!button(&root, "Save changes").is_sensitive());
            if let Some(scroll) = expander.ancestor(gtk::ScrolledWindow::static_type()).and_downcast::<gtk::ScrolledWindow>() {
                scroll.vadjustment().set_value(0.);
            }
            retention_checks.push(json!({"page":id,"all_five_limits_editable":true,"draft_summary_updates":true,"discard_restores_values":true,"collapse_preserves_focus_guard":true,"expanded_captures":2}));
        }
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
        let report = json!({"scope":"Real native navigation/filter callbacks to first painted feedback, not completion of animations or external queries; Xvfb/Cairo", "timings":observations,"worst_first_feedback_ms":worst,"interaction_target_ms":100,"interaction_target_met":worst<100.,"captures":captures,"cleanup_retention":retention_checks,"scale":outer.scale_factor(),"text_dpi":settings.property::<i32>("gtk-xft-dpi"),"high_contrast":adw::StyleManager::default().is_high_contrast(),"virtualized_journal_records":1000,"materialized_log_rows":materialized,"filtered_selection_mapping":true,"journal_page_navigation":true,"paged_export_all_matches":true,"export_preserves_later_filter_status":true,"fixture_only":true});
        fs::write(
            directory.join("release-evidence.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        app.quit();
    });
}
