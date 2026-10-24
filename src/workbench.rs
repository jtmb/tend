use crate::{column, job, label, page, privileged, row, scroll};
use adw::prelude::*;
use gtk::{glib, Orientation};
use std::{
    cell::Cell,
    path::PathBuf,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tend::{preferences as pref, workstation as w};

pub(crate) fn text_panel(parent: &gtk::Box) -> gtk::TextView {
    let view = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(16)
        .bottom_margin(16)
        .left_margin(16)
        .right_margin(16)
        .build();
    let sc = gtk::ScrolledWindow::builder()
        .child(&view)
        .min_content_height(240)
        .vexpand(true)
        .build();
    sc.add_css_class("card");
    let empty = column(8);
    empty.set_margin_top(16);
    empty.set_margin_bottom(16);
    let icon = gtk::Image::from_icon_name("document-open-recent-symbolic");
    icon.set_pixel_size(28);
    icon.add_css_class("dim-label");
    let title = label("No report yet", "heading");
    title.set_halign(gtk::Align::Center);
    let hint = label(
        "Choose an action above to collect a report. Nothing runs automatically.",
        "dim-label",
    );
    hint.set_justify(gtk::Justification::Center);
    hint.set_halign(gtk::Align::Center);
    hint.set_max_width_chars(60);
    hint.set_widget_name("report-hint");
    empty.append(&icon);
    empty.append(&title);
    empty.append(&hint);
    let loading = column(10);
    loading.set_margin_top(24);
    loading.set_margin_bottom(24);
    let spinner = gtk::Spinner::builder()
        .spinning(true)
        .halign(gtk::Align::Center)
        .build();
    let working = label("Collecting report…", "body");
    working.set_halign(gtk::Align::Center);
    loading.append(&spinner);
    loading.append(&working);
    let error = crate::editor::Notice::new();
    let stack = gtk::Stack::new();
    stack.set_widget_name("report-state");
    stack.add_named(&empty, Some("empty"));
    stack.add_named(&loading, Some("loading"));
    stack.add_named(&error.root, Some("error"));
    stack.add_named(&sc, Some("report"));
    stack.set_hhomogeneous(false);
    stack.set_vhomogeneous(false);
    stack.set_visible_child_name("empty");
    parent.append(&stack);
    let target = stack.downgrade();
    view.buffer().connect_changed(move |b| {
        let Some(stack) = target.upgrade() else {
            return;
        };
        let text = b.text(&b.start_iter(), &b.end_iter(), false);
        if text.is_empty() {
            stack.set_visible_child_name("empty");
        } else if text == "Working…" || text.starts_with("Collecting a read-only preview") {
            stack.set_visible_child_name("loading");
        } else if [
            "Could not complete operation:",
            "Preview failed:",
            "Could not read journal:",
            "Cleanup did not finish:",
        ]
        .iter()
        .any(|s| text.starts_with(s))
        {
            error.show(
                "The operation could not finish. Review the details and try again.",
                Some(&text),
                true,
            );
            stack.set_visible_child_name("error");
        } else {
            stack.set_visible_child_name("report");
        }
    });
    view
}
pub(crate) fn report_hint(view: &gtk::TextView, message: &str) {
    view.buffer().set_text(message);
    if let Some(stack) = view
        .ancestor(gtk::Stack::static_type())
        .and_downcast::<gtk::Stack>()
    {
        if let Some(empty) = stack.child_by_name("empty") {
            let mut child = empty.first_child();
            while let Some(w) = child {
                child = w.next_sibling();
                if w.widget_name() == "report-hint" {
                    if let Ok(l) = w.downcast::<gtk::Label>() {
                        l.set_text(message);
                    }
                }
            }
        }
        stack.set_visible_child_name("empty");
    }
}
pub(crate) fn buttons(parent: &gtk::Box) -> gtk::FlowBox {
    let b = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .row_spacing(8)
        .column_spacing(8)
        .max_children_per_line(6)
        .min_children_per_line(1)
        .homogeneous(false)
        .halign(gtk::Align::Start)
        .build();
    b.add_css_class("action-flow");
    parent.append(&b);
    b
}
pub(crate) fn button(parent: &gtk::FlowBox, title: &str) -> gtk::Button {
    let b = gtk::Button::with_label(title);
    parent.insert(&b, -1);
    b
}
pub(crate) fn confirm<F: Fn() + 'static>(
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
pub(crate) fn exports(parent: &gtk::Box, view: &gtk::TextView) {
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
    if let Some(stack) = view
        .ancestor(gtk::Stack::static_type())
        .and_downcast::<gtk::Stack>()
    {
        let (bar, status) = (bar.clone(), status.clone());
        let update = move |stack: &gtk::Stack| {
            let has_report = stack.visible_child_name().as_deref() == Some("report");
            bar.set_visible(has_report);
            status.set_visible(has_report);
        };
        update(&stack);
        stack.connect_visible_child_name_notify(update);
    }
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
    crate::cleanup_policy_ui::manual(stack, win);
    crate::cleanup_policy_ui::daily(stack, win);
    crate::cleanup_ui::schedule_page(stack);
    crate::log_reader::add(stack, win);
    crate::task_manager::add(stack, win);
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
    path.set_width_chars(35);
    controls.insert(&path, -1);
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
    report_hint(
        &view,
        "Choose a folder, then Scan folder. Large entries will have an Open button.",
    );
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
                view.buffer().set_text("Entries loaded. The maintenance restoration login entry can be enabled or disabled here. Reboot policy remains in Maintenance.");
            },Err(e)=>view.buffer().set_text(&e)}
        });
    });
    stack.add_titled(&scroll(&p), Some("startup"), "Startup & services");
}
fn tuning(stack: &gtk::Stack, win: &adw::ApplicationWindow) {
    let p=page("Performance","Change one setting at a time and compare normal work using saved performance snapshots. Every tuning action offers recovery.");
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
    report_hint(&view, "No changes applied. Live swap remains active. zram capacity changes never trigger a restart. Existing configuration files remain in place; Tend adds its own override.");
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
    report_hint(&view, "Docker statistics are collected once per request. Folder scans on Storage also report node_modules, target, build, dist and virtual-environment sizes.");
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
    report_hint(&view, "No updates requested. Pending updates are based on the last refreshed APT package lists. Reports can contain local paths and application names; review them before sharing.");
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
