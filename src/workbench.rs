use crate::{job, label, page, privileged, row, scroll};
use adw::prelude::*;
use gtk::{glib, Orientation};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    path::PathBuf,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tend::{preferences as pref, workstation as w};

fn text_panel(parent: &gtk::Box) -> gtk::TextView {
    let view = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(12)
        .bottom_margin(12)
        .left_margin(12)
        .right_margin(12)
        .build();
    let sc = gtk::ScrolledWindow::builder()
        .child(&view)
        .min_content_height(240)
        .vexpand(true)
        .build();
    sc.add_css_class("card");
    parent.append(&sc);
    view
}
fn buttons(parent: &gtk::Box) -> gtk::Box {
    let b = gtk::Box::new(Orientation::Horizontal, 8);
    parent.append(&b);
    b
}
fn button(parent: &gtk::Box, title: &str) -> gtk::Button {
    let b = gtk::Button::with_label(title);
    parent.append(&b);
    b
}
fn confirm<F: Fn() + 'static>(
    win: &adw::ApplicationWindow,
    title: &str,
    body: &str,
    action: &str,
    danger: bool,
    f: F,
) {
    let d = adw::MessageDialog::new(Some(win), Some(title), Some(body));
    d.add_responses(&[("cancel", "Cancel"), ("apply", action)]);
    d.set_default_response(Some("cancel"));
    d.set_close_response("cancel");
    d.set_response_appearance(
        "apply",
        if danger {
            adw::ResponseAppearance::Destructive
        } else {
            adw::ResponseAppearance::Suggested
        },
    );
    d.connect_response(None, move |_, r| {
        if r == "apply" {
            f();
        }
    });
    d.present();
}
fn exports(parent: &gtk::Box, view: &gtk::TextView) {
    let bar = buttons(parent);
    let status = label(
        "Reports are saved locally. Review their contents before sharing.",
        "caption",
    );
    for (name, json) in [("Export text", false), ("Export JSON", true)] {
        let b = button(&bar, name);
        let buffer = view.buffer();
        let status = status.clone();
        b.connect_clicked(move |_| {
            let text = buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), false)
                .to_string();
            status.set_text(&w::export_report(&text, json).unwrap_or_else(|e| e));
        });
    }
    let copy = button(&bar, "Copy report");
    let buffer = view.buffer();
    copy.connect_clicked(move |b| {
        b.clipboard()
            .set_text(&buffer.text(&buffer.start_iter(), &buffer.end_iter(), false));
    });
    parent.append(&status);
}
fn report_job<F: FnOnce() -> Result<String, String> + Send + 'static>(
    b: &gtk::Button,
    v: &gtk::TextView,
    work: F,
) {
    b.set_sensitive(false);
    v.buffer().set_text("Working…");
    let b = b.clone();
    let buffer = v.buffer();
    job(work, move |r| {
        b.set_sensitive(true);
        buffer.set_text(&r.unwrap_or_else(|e| format!("Could not complete operation:\n{e}")));
    });
}
fn open(path: PathBuf) {
    std::thread::spawn(move || {
        let _ = std::process::Command::new("/usr/bin/gio")
            .arg("open")
            .arg(path)
            .spawn();
    });
}
fn choose_folder(win: &adw::ApplicationWindow, entry: &gtk::Entry) {
    let dialog = gtk::FileDialog::builder()
        .title("Choose a folder to scan")
        .build();
    let entry = entry.clone();
    dialog.select_folder(Some(win), None::<&gtk::gio::Cancellable>, move |result| {
        if let Ok(file) = result {
            if let Some(path) = file.path() {
                entry.set_text(&path.to_string_lossy());
            }
        }
    });
}

pub fn add(stack: &gtk::Stack, win: &adw::ApplicationWindow) {
    storage(stack, win);
    cleanup(stack, win);
    monitor(stack, win);
    processes(stack, win);
    startup(stack, win);
    tuning(stack, win);
    development(stack, win);
    updates(stack, win);
    recovery(stack, win);
}
fn storage(stack: &gtk::Stack, win: &adw::ApplicationWindow) {
    let p=page("Storage","Scan a folder to find large files and development directories. Scans stay on this filesystem and skip symbolic links.");
    let controls = buttons(&p);
    let path = gtk::Entry::builder()
        .hexpand(true)
        .text(w::home().to_string_lossy())
        .placeholder_text("Folder to scan")
        .build();
    controls.append(&path);
    let choose = button(&controls, "Choose…");
    let (parent, entry) = (win.clone(), path.clone());
    choose.connect_clicked(move |_| choose_folder(&parent, &entry));
    let bar = buttons(&p);
    let start = button(&bar, "Scan folder");
    start.add_css_class("suggested-action");
    let cancel = button(&bar, "Cancel scan");
    cancel.set_sensitive(false);
    let reveal = button(&bar, "Open folder");
    let entry = path.clone();
    reveal.connect_clicked(move |_| open(PathBuf::from(entry.text().as_str())));
    let mounts = button(&bar, "Disk overview");
    let status = label("Ready. No files are changed by a scan.", "caption");
    p.append(&status);
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    list.add_css_class("boxed-list");
    p.append(&list);
    let view = text_panel(&p);
    view.buffer()
        .set_text("Choose a folder, then Scan folder. Large entries will have an Open button.");
    exports(&p, &view);
    let v = view.clone();
    mounts.connect_clicked(move |b| report_job(b, &v, w::system_report));
    let cancelled = Arc::new(AtomicBool::new(false));
    let c = cancelled.clone();
    cancel.connect_clicked(move |_| c.store(true, Ordering::Relaxed));
    let c = cancelled.clone();
    win.connect_close_request(move |_| {
        c.store(true, Ordering::Relaxed);
        glib::Propagation::Proceed
    });
    start.connect_clicked(move |b| {
        cancelled.store(false, Ordering::Relaxed);
        let progress = Arc::new(AtomicU64::new(0));
        let finished = Rc::new(Cell::new(false));
        let done = finished.clone();
        let text = status.clone();
        let count = progress.clone();
        glib::timeout_add_local(Duration::from_millis(500), move || {
            if done.get() {
                return glib::ControlFlow::Break;
            }
            text.set_text(&format!(
                "Scanning… {} files counted",
                count.load(Ordering::Relaxed)
            ));
            glib::ControlFlow::Continue
        });
        b.set_sensitive(false);
        cancel.set_sensitive(true);
        path.set_sensitive(false);
        choose.set_sensitive(false);
        mounts.set_sensitive(false);
        let (b, cancel, path, choose, mounts, status, view, list) = (
            b.clone(),
            cancel.clone(),
            path.clone(),
            choose.clone(),
            mounts.clone(),
            status.clone(),
            view.clone(),
            list.clone(),
        );
        let root = PathBuf::from(path.text().as_str());
        let c = cancelled.clone();
        job(
            move || {
                w::scan(&root, c, progress)
                    .and_then(|s| serde_json::to_string(&s).map_err(|e| e.to_string()))
            },
            move |r| {
                finished.set(true);
                b.set_sensitive(true);
                cancel.set_sensitive(false);
                path.set_sensitive(true);
                choose.set_sensitive(true);
                mounts.set_sensitive(true);
                while let Some(child) = list.first_child() {
                    list.remove(&child);
                }
                match r.and_then(|s| serde_json::from_str::<w::Scan>(&s).map_err(|e| e.to_string()))
                {
                    Ok(s) => {
                        let report = w::scan_text(&s);
                        status.set_text(&format!(
                            "{} • {} files{}",
                            w::bytes(s.allocated),
                            s.files,
                            if s.cancelled { " • partial scan" } else { "" }
                        ));
                        view.buffer().set_text(&report);
                        let _ = w::record("storage", &report);
                        for item in s.entries.iter().take(12) {
                            let r = adw::ActionRow::builder()
                                .use_markup(false)
                                .title(item.path.file_name().unwrap_or_default().to_string_lossy())
                                .subtitle(w::bytes(item.size))
                                .build();
                            let b = gtk::Button::with_label("Open");
                            b.set_valign(gtk::Align::Center);
                            let path = item.path.clone();
                            b.connect_clicked(move |_| {
                                open(if path.is_dir() {
                                    path.clone()
                                } else {
                                    path.parent().unwrap().into()
                                })
                            });
                            r.add_suffix(&b);
                            list.append(&r);
                        }
                    }
                    Err(e) => {
                        status.set_text("Scan failed");
                        view.buffer().set_text(&e);
                    }
                }
            },
        );
    });
    stack.add_titled(&scroll(&p), Some("storage"), "Storage");
}
#[derive(Clone)]
struct CleanupWidgets {
    checks: Vec<gtk::CheckButton>,
    age: gtk::SpinButton,
    keep: gtk::SpinButton,
}
impl CleanupWidgets {
    fn value(&self) -> w::Cleanup {
        w::Cleanup {
            apt: self.checks[0].is_active(),
            docker_build: self.checks[1].is_active(),
            docker_images: self.checks[2].is_active(),
            npm_logs: self.checks[3].is_active(),
            pip: self.checks[4].is_active(),
            thumbnails: self.checks[5].is_active(),
            trash: self.checks[6].is_active(),
            age_days: self.age.value_as_int() as u32,
            keep_gb: self.keep.value_as_int() as u32,
        }
    }
    fn sensitive(&self, on: bool) {
        for c in &self.checks {
            c.set_sensitive(on);
        }
        self.age.set_sensitive(on);
        self.keep.set_sensitive(on);
    }
}
fn cleanup(stack: &gtk::Stack, win: &adw::ApplicationWindow) {
    let p=page("Disk cleanup","Preview selected categories before deleting. These manual cleanup preferences do not change the existing daily maintenance policy.");
    let config = w::Cleanup::load();
    let group = adw::PreferencesGroup::new();
    let mut checks = vec![];
    for (title, desc, on) in [
        (
            "Obsolete APT downloads",
            "Packages no longer available from configured repositories",
            config.apt,
        ),
        (
            "Unused Docker build cache",
            "Older than the selected age, with a cache allowance",
            config.docker_build,
        ),
        (
            "Dangling Docker images",
            "Older than the selected age; tagged images and volumes are preserved",
            config.docker_images,
        ),
        (
            "npm diagnostic logs",
            "Only logs older than 30 days; cached packages are preserved",
            config.npm_logs,
        ),
        (
            "Pip download cache",
            "Remove all downloaded packages and built wheels; installed environments are preserved",
            config.pip,
        ),
        (
            "Old thumbnails",
            "Older than the selected age; regenerated when needed",
            config.thumbnails,
        ),
        (
            "Empty Trash",
            "Permanently delete all items in the desktop Trash",
            config.trash,
        ),
    ] {
        let c = gtk::CheckButton::builder().active(on).build();
        row(&group, title, desc, &c);
        checks.push(c);
    }
    p.append(&group);
    let policy = adw::PreferencesGroup::builder()
        .title("Manual cleanup preferences")
        .build();
    let age = gtk::SpinButton::with_range(7., 365., 1.);
    age.set_value(config.age_days as f64);
    let keep = gtk::SpinButton::with_range(1., 100., 1.);
    keep.set_value(config.keep_gb as f64);
    row(
        &policy,
        "Minimum age (days)",
        "Docker cache, dangling images and thumbnails",
        &age,
    );
    row(
        &policy,
        "Docker build-cache allowance (GB)",
        "Docker applies this allowance to unused build cache",
        &keep,
    );
    p.append(&policy);
    let widgets = CleanupWidgets { checks, age, keep };
    let bar = buttons(&p);
    let save = button(&bar, "Save preferences");
    let preview = button(&bar, "Preview cleanup");
    preview.add_css_class("suggested-action");
    let apply = button(&bar, "Clean selected…");
    apply.add_css_class("destructive-action");
    apply.set_sensitive(false);
    let view = text_panel(&p);
    view.buffer()
        .set_text("Preview is required. Cleanup does not install updates or restart the computer.");
    exports(&p, &view);
    let approved = Rc::new(RefCell::new(None::<String>));
    for c in &widgets.checks {
        let a = apply.clone();
        let plan = approved.clone();
        c.connect_toggled(move |_| {
            a.set_sensitive(false);
            plan.borrow_mut().take();
        });
    }
    for c in [&widgets.age, &widgets.keep] {
        let a = apply.clone();
        let plan = approved.clone();
        c.connect_value_changed(move |_| {
            a.set_sensitive(false);
            plan.borrow_mut().take();
        });
    }
    let (w, v) = (widgets.clone(), view.clone());
    save.connect_clicked(move |_| {
        v.buffer().set_text(
            &w.value()
                .save()
                .map(|_| "Manual cleanup preferences saved.".into())
                .unwrap_or_else(|e| e),
        );
    });
    let (widgets_p, apply_p, view_p, plan) = (
        widgets.clone(),
        apply.clone(),
        view.clone(),
        approved.clone(),
    );
    preview.connect_clicked(move |b| {
        let config = widgets_p.value();
        let encoded = serde_json::to_string(&config).unwrap();
        b.set_sensitive(false);
        apply_p.set_sensitive(false);
        widgets_p.sensitive(false);
        let (b, a, v, w, plan) = (
            b.clone(),
            apply_p.clone(),
            view_p.clone(),
            widgets_p.clone(),
            plan.clone(),
        );
        v.buffer().set_text("Checking selected categories…");
        job(
            move || w::cleanup_preview(&config),
            move |r| {
                b.set_sensitive(true);
                w.sensitive(true);
                match r {
                    Ok(s) => {
                        v.buffer().set_text(&s);
                        *plan.borrow_mut() = Some(encoded);
                        a.set_sensitive(true);
                    }
                    Err(e) => v.buffer().set_text(&e),
                }
            },
        );
    });
    let parent = win.clone();
    apply.connect_clicked(move|b|{let Some(json)=approved.borrow().clone()else{return};let (b,v,preview,widgets,approved)=(b.clone(),view.clone(),preview.clone(),widgets.clone(),approved.clone());confirm(&parent,"Delete the selected cache and Trash contents?","This permanently removes the categories shown in your preview. Docker eligibility is rechecked at execution. No updates or restart will run.","Clean selected",true,move||{b.set_sensitive(false);preview.set_sensitive(false);widgets.sensitive(false);approved.borrow_mut().take();v.buffer().set_text("Cleaning selected categories… Ubuntu may ask for administrator authentication.");let (b,v,preview,widgets)=(b.clone(),v.clone(),preview.clone(),widgets.clone());let json=json.clone();job(move||privileged("cleanup",Some(json)),move|r|{b.set_sensitive(false);preview.set_sensitive(true);widgets.sensitive(true);let text=r.unwrap_or_else(|e|format!("Cleanup did not finish:\n{e}"));let _=w::record("cleanup-result",&text);v.buffer().set_text(&text);});});});
    stack.add_titled(&scroll(&p), Some("cleanup"), "Disk cleanup");
}
fn monitor(stack: &gtk::Stack, win: &adw::ApplicationWindow) {
    let p=page("System monitor","Live counters refresh every three seconds while this page is open. History is kept in memory for three minutes.");
    let view = text_panel(&p);
    view.buffer()
        .set_text("Open this page to begin collecting samples.");
    let chart = gtk::DrawingArea::builder()
        .content_height(100)
        .hexpand(true)
        .build();
    p.prepend(&chart);
    let history = Rc::new(RefCell::new(VecDeque::<(f64, f64)>::new()));
    let values = history.clone();
    chart.set_draw_func(move |_, cr, width, height| {
        let values = values.borrow();
        for (index, color) in [(0, (0.2, 0.55, 0.95)), (1, (0.15, 0.7, 0.45))] {
            cr.set_source_rgb(color.0, color.1, color.2);
            cr.set_line_width(2.);
            for (i, (cpu, ram)) in values.iter().enumerate() {
                let y = height as f64 * (1. - if index == 0 { *cpu } else { *ram });
                let x = i as f64 / 59. * width as f64;
                if i == 0 {
                    cr.move_to(x, y);
                } else {
                    cr.line_to(x, y);
                }
            }
            let _ = cr.stroke();
        }
    });
    p.append(&label("Chart: blue CPU • green memory • 0–100%", "caption"));
    let bar = buttons(&p);
    let snap = button(&bar, "Save performance snapshot");
    let v = view.clone();
    snap.connect_clicked(move |_| {
        let b = v.buffer();
        let text = b.text(&b.start_iter(), &b.end_iter(), false);
        let _ = w::record("performance", &text);
    });
    exports(&p, &view);
    let prev = Rc::new(RefCell::new(None::<w::Sample>));
    let source = Rc::new(RefCell::new(None::<glib::SourceId>));
    let source_close = source.clone();
    win.connect_close_request(move |_| {
        if let Some(s) = source_close.borrow_mut().take() {
            s.remove();
        }
        glib::Propagation::Proceed
    });
    let weak = view.downgrade();
    stack.connect_visible_child_name_notify(move |s| {
        if let Some(id) = source.borrow_mut().take() {
            id.remove();
        }
        if s.visible_child_name().as_deref() != Some("monitor") {
            prev.borrow_mut().take();
            return;
        }
        let (weak, prev, history, chart) = (
            weak.clone(),
            prev.clone(),
            history.clone(),
            chart.downgrade(),
        );
        *source.borrow_mut() = Some(glib::timeout_add_local(Duration::from_secs(3), move || {
            let (Some(view), Some(chart)) = (weak.upgrade(), chart.upgrade()) else {
                return glib::ControlFlow::Break;
            };
            let sample = w::sample();
            let text = w::sample_text(&sample, prev.borrow().as_ref());
            let cpu = prev
                .borrow()
                .as_ref()
                .filter(|p| sample.total > p.total)
                .map(|p| {
                    1. - sample.idle.saturating_sub(p.idle) as f64 / (sample.total - p.total) as f64
                })
                .unwrap_or(0.);
            let mem = 1. - sample.memory_available as f64 / sample.memory_total.max(1) as f64;
            {
                let mut h = history.borrow_mut();
                h.push_back((cpu.clamp(0., 1.), mem.clamp(0., 1.)));
                if h.len() > 60 {
                    h.pop_front();
                }
            }
            chart.queue_draw();
            view.buffer().set_text(&text);
            *prev.borrow_mut() = Some(sample);
            glib::ControlFlow::Continue
        }));
    });
    stack.add_titled(&scroll(&p), Some("monitor"), "System monitor");
}
fn processes(stack: &gtk::Stack, win: &adw::ApplicationWindow) {
    let p=page("Apps & processes","Your user processes only. CPU is measured between refreshes; 100% means one fully occupied CPU core. RSS includes shared memory.");
    let bar = buttons(&p);
    let refresh = button(&bar, "Refresh processes");
    let sort = gtk::DropDown::from_strings(&["Memory usage", "CPU usage"]);
    bar.append(&sort);
    let quit = button(&bar, "Request quit…");
    let kill = button(&bar, "Force quit…");
    kill.add_css_class("destructive-action");
    let live = gtk::CheckButton::with_label("Refresh every 3 seconds while this page is open");
    p.append(&live);
    let process_history = label(
        "Select a process to track CPU and memory across refreshes.",
        "caption",
    );
    p.append(&process_history);
    let samples = Rc::new(RefCell::new(VecDeque::<String>::new()));
    let tracked = Rc::new(Cell::new(None::<u32>));
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::Single);
    p.append(&list);
    let status = label(
        "Refresh to load processes. Refresh again for interval CPU usage.",
        "caption",
    );
    p.append(&status);
    let values = Rc::new(RefCell::new(Vec::<w::Process>::new()));
    let last = Rc::new(Cell::new(Instant::now()));
    let (data, ls, st) = (values.clone(), list.clone(), status.clone());
    refresh.connect_clicked(move |b| {
        b.set_sensitive(false);
        let elapsed = last.get().elapsed().as_secs_f64();
        last.set(Instant::now());
        let prior = data.borrow().clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(w::processes(&prior, elapsed));
        });
        let (b, data, ls, st, sort) = (
            b.clone(),
            data.clone(),
            ls.clone(),
            st.clone(),
            sort.clone(),
        );
        let (samples, tracked, process_history) =
            (samples.clone(), tracked.clone(), process_history.clone());
        glib::timeout_add_local(Duration::from_millis(100), move || match rx.try_recv() {
            Ok(mut entries) => {
                b.set_sensitive(true);
                if sort.selected() == 0 {
                    entries.sort_by_key(|p| std::cmp::Reverse(p.rss));
                } else {
                    entries.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
                }
                let selected = ls
                    .selected_row()
                    .and_then(|row| data.borrow().get(row.index() as usize).map(|p| p.pid));
                if tracked.get() != selected {
                    samples.borrow_mut().clear();
                    tracked.set(selected);
                }
                if let Some(process) = entries.iter().find(|p| Some(p.pid) == selected) {
                    let mut history = samples.borrow_mut();
                    history.push_back(format!("{:.1}% / {}", process.cpu, w::bytes(process.rss)));
                    if history.len() > 8 {
                        history.pop_front();
                    }
                    process_history.set_text(&format!(
                        "{} · CPU / RSS, oldest → newest\n{}",
                        process.name,
                        history.iter().cloned().collect::<Vec<_>>().join("   ·   ")
                    ));
                }
                while let Some(child) = ls.first_child() {
                    ls.remove(&child);
                }
                for e in entries.iter().take(100) {
                    let r = adw::ActionRow::builder()
                        .use_markup(false)
                        .title(&e.name)
                        .subtitle(format!(
                            "PID {} · {} RSS · {:.1}% CPU",
                            e.pid,
                            w::bytes(e.rss),
                            e.cpu
                        ))
                        .build();
                    ls.append(&r);
                }
                st.set_text(&format!(
                    "{} processes; showing the first 100. Select a row before requesting quit.",
                    entries.len()
                ));
                if let Some(index) = entries
                    .iter()
                    .take(100)
                    .position(|p| Some(p.pid) == selected)
                {
                    ls.select_row(ls.row_at_index(index as i32).as_ref());
                }
                *data.borrow_mut() = entries;
                glib::ControlFlow::Break
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(_) => {
                b.set_sensitive(true);
                st.set_text("Process collection stopped.");
                glib::ControlFlow::Break
            }
        });
    });
    for (button, force) in [(quit, false), (kill, true)] {
        let (values, list, win, status) =
            (values.clone(), list.clone(), win.clone(), status.clone());
        button.connect_clicked(move |_|{let Some(index)=list.selected_row().map(|r|r.index() as usize)else{status.set_text("Select a process first.");return};let Some(process)=values.borrow().get(index).cloned()else{return};let message=format!("{} (PID {}). {}",process.name,process.pid,if force{"Force quit can lose unsaved work and interrupt writes."}else{"Sends a termination request. The application may exit without saving; save your work first."});let status=status.clone();confirm(&win,if force{"Force quit this process?"}else{"Request this process to quit?"},&message,if force{"Force quit"}else{"Request quit"},true,move||{status.set_text(&w::signal_process(&process,force).unwrap_or_else(|e|e));});});
    }
    let source = Rc::new(RefCell::new(None::<glib::SourceId>));
    let close_source = source.clone();
    win.connect_close_request(move |_| {
        if let Some(id) = close_source.borrow_mut().take() {
            id.remove();
        }
        glib::Propagation::Proceed
    });
    let update: Rc<dyn Fn()> = Rc::new({
        let stack = stack.downgrade();
        let live = live.downgrade();
        let refresh = refresh.downgrade();
        move || {
            if let Some(id) = source.borrow_mut().take() {
                id.remove();
            }
            let (Some(stack), Some(live), Some(refresh)) =
                (stack.upgrade(), live.upgrade(), refresh.upgrade())
            else {
                return;
            };
            if stack.visible_child_name().as_deref() != Some("processes") || !live.is_active() {
                return;
            }
            *source.borrow_mut() =
                Some(glib::timeout_add_local(Duration::from_secs(3), move || {
                    if refresh.is_sensitive() {
                        refresh.emit_clicked();
                    }
                    glib::ControlFlow::Continue
                }));
        }
    });
    let update_page = update.clone();
    stack.connect_visible_child_name_notify(move |_| update_page());
    live.connect_toggled(move |_| update());
    stack.add_titled(&scroll(&p), Some("processes"), "Apps & processes");
}
fn startup(stack: &gtk::Stack, win: &adw::ApplicationWindow) {
    let p=page("Startup & services","Manage login applications and optional services installed in your user account. Changes affect future activation; they do not close running apps.");
    let bar = buttons(&p);
    let load = button(&bar, "Load startup entries");
    let boot = button(&bar, "Startup & restore report");
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    list.add_css_class("boxed-list");
    p.append(&list);
    let view = text_panel(&p);
    exports(&p, &view);
    let v = view.clone();
    boot.connect_clicked(move |b| {
        report_job(b, &v, || {
            let mut s = String::new();
            for args in [vec!["time"], vec!["blame", "--no-pager"]] {
                s.push_str(&w::command("/usr/bin/systemd-analyze", &args));
                s.push_str("\n\n");
            }
            let path = w::home().join("Documents/maintenance-session/session.json");
            s.push_str("Last session restoration:\n");
            s.push_str(
                &std::fs::read_to_string(path)
                    .unwrap_or_else(|_| "No saved session report.".into()),
            );
            Ok(s)
        })
    });
    let parent = win.clone();
    load.connect_clicked(move|b|{
        b.set_sensitive(false);let (b,list,view,parent)=(b.clone(),list.clone(),view.clone(),parent.clone());job(||Ok(serde_json::json!({"apps":pref::startups(),"units":pref::user_units()}).to_string()),move|result|{
            b.set_sensitive(true);while let Some(c)=list.first_child(){list.remove(&c);}match result.and_then(|s|serde_json::from_str::<serde_json::Value>(&s).map_err(|e|e.to_string())){Ok(v)=>{
                for app in v["apps"].as_array().into_iter().flatten(){let Ok(app)=serde_json::from_value::<pref::Startup>(app.clone())else{continue};let r=adw::ActionRow::builder().use_markup(false).title(&app.title).subtitle(format!("Login app · {}",app.id)).build();let toggle=gtk::Button::with_label(if app.enabled{"Disable"}else{"Enable"});toggle.set_valign(gtk::Align::Center);let enabled=Rc::new(Cell::new(app.enabled));let (parent,view)=(parent.clone(),view.clone());toggle.connect_clicked(move|b|{let desired=!enabled.get();let (id,b,enabled,view)=(app.id.clone(),b.clone(),enabled.clone(),view.clone());confirm(&parent,"Change this login application?",&format!("{} will be {} at your next login. Recovery can restore the previous entry.",app.title,if desired{"enabled"}else{"disabled"}),"Save",false,move||{b.set_sensitive(false);let (id,b,enabled,view)=(id.clone(),b.clone(),enabled.clone(),view.clone());job(move||pref::set_startup(&id,desired),move|r|{b.set_sensitive(true);if r.is_ok(){enabled.set(desired);b.set_label(if desired{"Disable"}else{"Enable"});}view.buffer().set_text(&r.unwrap_or_else(|e|e));});});});r.add_suffix(&toggle);list.append(&r);}
                for unit in v["units"].as_array().into_iter().flatten(){let Some(id)=unit[0].as_str().map(str::to_owned)else{continue};let enabled=Rc::new(Cell::new(unit[1].as_bool().unwrap_or(false)));let r=adw::ActionRow::builder().use_markup(false).title(&id).subtitle("Optional user service · future activation only").build();let toggle=gtk::Button::with_label(if enabled.get(){"Disable"}else{"Enable"});toggle.set_valign(gtk::Align::Center);let (parent,view)=(parent.clone(),view.clone());toggle.connect_clicked(move|b|{let desired=!enabled.get();let (id,b,enabled,view)=(id.clone(),b.clone(),enabled.clone(),view.clone());confirm(&parent,"Change future service activation?","This updates the user service enablement. No service will be started or stopped.","Save",false,move||{b.set_sensitive(false);let (id,b,enabled,view)=(id.clone(),b.clone(),enabled.clone(),view.clone());job(move||pref::set_unit(&id,desired),move|r|{b.set_sensitive(true);if r.is_ok(){enabled.set(desired);b.set_label(if desired{"Disable"}else{"Enable"});}view.buffer().set_text(&r.unwrap_or_else(|e|e));});});});r.add_suffix(&toggle);list.append(&r);}
                view.buffer().set_text("Entries loaded. The maintenance restoration login entry can be enabled or disabled here. Reboot policy remains in Schedule.");
            },Err(e)=>view.buffer().set_text(&e)}
        });
    });
    stack.add_titled(&scroll(&p), Some("startup"), "Startup & services");
}
fn tuning(stack: &gtk::Stack, win: &adw::ApplicationWindow) {
    let p=page("Performance settings","Change one setting at a time and compare normal work using saved performance snapshots. Every tuning action offers recovery.");
    let desktop = adw::PreferencesGroup::builder()
        .title("Desktop responsiveness")
        .build();
    let animations = gtk::DropDown::from_strings(&["Animations enabled", "Animations disabled"]);
    animations.set_selected(
        if tend::output(
            "/usr/bin/gsettings",
            &["get", "org.gnome.desktop.interface", "enable-animations"],
        )
        .as_deref()
            == Ok("false")
        {
            1
        } else {
            0
        },
    );
    let apply_anim = gtk::Button::with_label("Apply");
    let ab = gtk::Box::new(Orientation::Horizontal, 6);
    ab.append(&animations);
    ab.append(&apply_anim);
    row(&desktop,"Window animations","Disabling animations removes transition delays; it does not increase application throughput",&ab);
    let display = gtk::Button::with_label("Open Displays");
    row(
        &desktop,
        "Resolution, scaling & refresh rate",
        "Use Ubuntu’s display controls and their automatic revert confirmation",
        &display,
    );
    display.connect_clicked(|_| {
        std::thread::spawn(|| {
            let _ = std::process::Command::new("/usr/bin/gnome-control-center")
                .arg("display")
                .spawn();
        });
    });
    let app = gtk::DropDown::from_strings(&["ChatGPT", "Visual Studio Code"]);
    let wayland = gtk::Button::with_label("Use Wayland…");
    let wb = gtk::Box::new(Orientation::Horizontal, 6);
    wb.append(&app);
    wb.append(&wayland);
    row(&desktop,"Native Wayland launcher","Experimental compatibility option; keeps GPU acceleration enabled and applies after the next full app launch",&wb);
    p.append(&desktop);
    let memory = adw::PreferencesGroup::builder()
        .title("Memory and compressed swap")
        .build();
    let swappiness = gtk::SpinButton::with_range(0., 100., 1.);
    swappiness.set_value(
        std::fs::read_to_string("/proc/sys/vm/swappiness")
            .ok()
            .and_then(|s| s.trim().parse::<f64>().ok())
            .unwrap_or(60.),
    );
    row(&memory,"Swappiness","0–100 · Ubuntu baseline 60; higher values favor moving cold pages to swap. Benefit depends on workload.",&swappiness);
    let cap = gtk::SpinButton::with_range(512., 8192., 256.);
    cap.set_value(4096.);
    row(&memory,"zram cap (MiB)","Logical capacity: the smaller of half RAM and this cap. Applied at the next ordinary boot.",&cap);
    let compression = gtk::DropDown::from_strings(&["lz4", "zstd"]);
    row(
        &memory,
        "Compression algorithm",
        "LZ4 favors speed; Zstandard may reduce memory use with additional CPU cost",
        &compression,
    );
    p.append(&memory);
    let saved = std::fs::read_to_string("/etc/systemd/zram-generator.conf.d/99-tend.conf")
        .unwrap_or_default();
    if let Some(line) = saved.lines().find(|s| s.starts_with("zram-size")) {
        if let Some(v) = line
            .rsplit(',')
            .next()
            .and_then(|s| s.trim().trim_end_matches(')').parse::<f64>().ok())
        {
            cap.set_value(v);
        }
    }
    compression.set_selected(if saved.contains("compression-algorithm = zstd") {
        1
    } else {
        0
    });
    let bar = buttons(&p);
    let read = button(&bar, "Read memory status");
    let prepare = button(&bar, "Prepare zram support…");
    let save = button(&bar, "Apply memory settings…");
    let view = text_panel(&p);
    view.buffer().set_text("No changes applied. Live swap remains active. zram capacity changes never trigger a restart. Existing configuration files remain in place; Tend adds its own override.");
    exports(&p, &view);
    let v = view.clone();
    read.connect_clicked(move |b| {
        report_job(b, &v, || {
            let mut s = w::system_report()?;
            s.push_str("\nSaved Tend zram configuration:\n");
            s.push_str(
                &std::fs::read_to_string("/etc/systemd/zram-generator.conf.d/99-tend.conf")
                    .unwrap_or_else(|_| "No Tend override".into()),
            );
            s.push_str(&format!(
                "\nzswap enabled: {}",
                std::fs::read_to_string("/sys/module/zswap/parameters/enabled")
                    .unwrap_or_else(|_| "unavailable".into())
                    .trim()
            ));
            Ok(s)
        })
    });
    let v = view.clone();
    apply_anim.connect_clicked(move |_| {
        v.buffer()
            .set_text(&pref::animations(animations.selected() == 0).unwrap_or_else(|e| e));
    });
    let (parent, v) = (win.clone(), view.clone());
    wayland.connect_clicked(move |_|{let id=if app.selected()==0{"chatgpt.desktop"}else{"code.desktop"};let v=v.clone();confirm(&parent,"Use native Wayland on the next app launch?","Check typing, clipboard, shortcuts, dialogs, touch and suspend/resume during normal work. Restore the launcher in Recovery if compatibility worsens. Tend does not close the app.","Save launcher",false,move||v.buffer().set_text(&pref::wayland(id).unwrap_or_else(|e|e)));});
    let (parent, v) = (win.clone(), view.clone());
    prepare.connect_clicked(move|b|{let (v,b)=(v.clone(),b.clone());confirm(&parent,"Install compressed-swap support?","Install Ubuntu’s systemd-zram-generator package from the configured repositories. Tend will not restart the computer.","Install",false,move||report_job(&b,&v,||privileged("install-zram",None)));});
    let parent = win.clone();
    save.connect_clicked(move|b|{let settings=pref::MemorySettings{swappiness:swappiness.value_as_int() as u32,zram_mb:cap.value_as_int() as u32,compression:if compression.selected()==0{"lz4"}else{"zstd"}.into()};let json=serde_json::to_string(&settings).unwrap();let body=format!("Swappiness {} → {} now.\nzram → min(half RAM, {} MiB), {}, priority 100 at your next ordinary boot.\nDisk swap remains available. The prior Tend configuration will be saved for Recovery. No restart will be scheduled.",std::fs::read_to_string("/proc/sys/vm/swappiness").unwrap_or_default().trim(),settings.swappiness,settings.zram_mb,settings.compression);let (b,v)=(b.clone(),view.clone());confirm(&parent,"Apply these memory settings?",&body,"Apply",false,move||{let json=json.clone();report_job(&b,&v,move||{let r=privileged("memory",Some(json))?;w::record("memory",&r)?;Ok(r)});});});
    stack.add_titled(&scroll(&p), Some("tuning"), "Performance settings");
}
fn development(stack: &gtk::Stack, win: &adw::ApplicationWindow) {
    let p=page("Development tools","Inspect container usage, logs and package caches. Project dependencies and virtual environments are preserved.");
    let bar = buttons(&p);
    let inspect = button(&bar, "Inspect Docker & caches");
    let npm = button(&bar, "Verify npm cache");
    let logs = button(&bar, "Docker log sizes…");
    let view = text_panel(&p);
    view.buffer().set_text("Docker statistics are collected once per request. Folder scans on Storage also report node_modules, target, build, dist and virtual-environment sizes.");
    exports(&p, &view);
    let v = view.clone();
    inspect.connect_clicked(move |b| report_job(b, &v, w::development_report));
    let v = view.clone();
    npm.connect_clicked(move |b| report_job(b, &v, || w::verify_npm()));
    let (parent, v) = (win.clone(), view.clone());
    logs.connect_clicked(move|b|{let (b,v)=(b.clone(),v.clone());confirm(&parent,"Read local Docker log sizes?","Ubuntu may request administrator authentication to read log metadata. Log contents are not read or changed.","Read sizes",false,move||report_job(&b,&v,||privileged("docker-logs",None)));});
    stack.add_titled(&scroll(&p), Some("development"), "Development tools");
}
fn updates(stack: &gtk::Stack, win: &adw::ApplicationWindow) {
    let p=page("Updates & reports","Install configured stable updates separately from cleanup. This action disables unattended-upgrades’ automatic reboot for this run.");
    let bar = buttons(&p);
    let pending = button(&bar, "Show pending updates");
    let apply = button(&bar, "Install stable updates…");
    let history = button(&bar, "Load history");
    let health = button(&bar, "System report");
    let view = text_panel(&p);
    view.buffer().set_text("No updates requested. Pending updates are based on the last refreshed APT package lists. Reports can contain local paths and application names; review them before sharing.");
    exports(&p, &view);
    let v = view.clone();
    pending.connect_clicked(move|b|report_job(b,&v,||{let packages=w::command("/usr/bin/apt",&["list","--upgradable"]);Ok(format!("Packages available according to cached repository metadata:\n{packages}\n\nConfigured unattended-upgrades origins and holds decide which packages can be installed.\nReboot required: {}",std::path::Path::new("/var/run/reboot-required").exists()))}));
    let (parent, v) = (win.clone(), view.clone());
    apply.connect_clicked(move|b|{let (b,v)=(b.clone(),v.clone());confirm(&parent,"Install configured stable updates?","Refresh APT metadata and run the configured unattended-upgrades engine. Package holds and repository policies remain in effect. Connect AC power. Cleanup and the maintenance reboot hook will not run.","Install updates",false,move||report_job(&b,&v,||{let r=privileged("updates",None);let text=r.clone().unwrap_or_else(|e|format!("Update failure: {e}"));w::record("updates",&text)?;r}));});
    let v = view.clone();
    history.connect_clicked(move |b| report_job(b, &v, w::history));
    health.connect_clicked(move |b| report_job(b, &view, w::system_report));
    stack.add_titled(&scroll(&p), Some("reports"), "Updates & reports");
}
fn recovery(stack: &gtk::Stack, win: &adw::ApplicationWindow) {
    let p=page("Recovery","Restore individual tuning changes. Cache deletion cannot be undone. Restore repeated changes to the same setting in reverse order.");
    let bar = buttons(&p);
    let load = button(&bar, "Load saved changes");
    let memory = button(&bar, "Restore previous memory settings…");
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    list.add_css_class("boxed-list");
    p.append(&list);
    let status = label("No recovery action requested.", "body");
    p.append(&status);
    let (parent, st) = (win.clone(), status.clone());
    memory.connect_clicked(move|b|{let (b,st)=(b.clone(),st.clone());confirm(&parent,"Restore previous memory settings?","Restore the previous swappiness now and Tend’s previous zram configuration for the next ordinary boot. Live swap stays active and no restart is requested.","Restore",false,move||{b.set_sensitive(false);let (b,st)=(b.clone(),st.clone());job(||privileged("memory-restore",None),move|r|{b.set_sensitive(true);let text=r.unwrap_or_else(|e|e);let _=w::record("memory-restore",&text);st.set_text(&text);});});});
    let parent = win.clone();
    load.connect_clicked(move |_| {
        while let Some(c) = list.first_child() {
            list.remove(&c);
        }
        let entries = pref::undos();
        status.set_text(&format!(
            "{} saved changes. Reload after each restoration.",
            entries.len()
        ));
        for (index, u) in entries.iter().enumerate().rev() {
            let r = adw::ActionRow::builder()
                .use_markup(false)
                .title(&u.label)
                .subtitle("Restore the value saved immediately before this change")
                .build();
            let b = gtk::Button::with_label("Restore…");
            b.set_valign(gtk::Align::Center);
            let (parent, status, recovery_list) = (parent.clone(), status.clone(), list.clone());
            let expected = serde_json::to_string(&u).unwrap();
            b.connect_clicked(move |_| {
                let (status, list, expected) =
                    (status.clone(), recovery_list.clone(), expected.clone());
                confirm(
                    &parent,
                    "Restore this setting?",
                    "The saved value replaces the current setting. No restart is scheduled.",
                    "Restore",
                    false,
                    move || {
                        if pref::undos()
                            .get(index)
                            .and_then(|u| serde_json::to_string(u).ok())
                            .as_deref()
                            != Some(&expected)
                        {
                            status.set_text("History changed. Reload saved changes first.");
                            return;
                        }
                        status.set_text(&pref::restore(index).unwrap_or_else(|e| e));
                        while let Some(c) = list.first_child() {
                            list.remove(&c);
                        }
                    },
                );
            });
            r.add_suffix(&b);
            list.append(&r);
        }
    });
    stack.add_titled(&scroll(&p), Some("recovery"), "Recovery");
}
