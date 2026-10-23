mod cleanup_ui;
mod log_reader;
mod task_manager;
mod workbench;
use adw::prelude::*;
use gtk::{glib, Orientation};
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
    sync::mpsc,
    time::Duration,
};
use tend::{output, presentation as dates, Settings};
const ID: &str = "io.github.jtmb.Tend";
fn label(text: &str, class: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.set_xalign(0.0);
    l.set_wrap(true);
    l.add_css_class(class);
    l
}
fn column(spacing: i32) -> gtk::Box {
    gtk::Box::new(Orientation::Vertical, spacing)
}
fn page(title: &str, description: &str) -> gtk::Box {
    let b = column(20);
    b.set_margin_top(24);
    b.set_margin_bottom(28);
    b.set_margin_start(28);
    b.set_margin_end(28);
    b.append(&label(title, "page-title"));
    b.append(&label(description, "dim-label"));
    b
}
fn scroll(b: &gtk::Box) -> gtk::ScrolledWindow {
    let clamp = adw::Clamp::builder()
        .maximum_size(1080)
        .tightening_threshold(860)
        .child(b)
        .build();
    gtk::ScrolledWindow::builder()
        .child(&clamp)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .hexpand(true)
        .vexpand(true)
        .build()
}
fn row(group: &adw::PreferencesGroup, title: &str, subtitle: &str, widget: &impl IsA<gtk::Widget>) {
    let r = adw::ActionRow::new();
    r.set_use_markup(false);
    r.set_title(title);
    r.set_subtitle(subtitle);
    widget.set_valign(gtk::Align::Center);
    r.add_suffix(widget);
    r.set_activatable_widget(Some(widget));
    group.add(&r);
}
fn toggle(group: &adw::PreferencesGroup, title: &str, subtitle: &str, value: bool) -> gtk::Switch {
    let w = gtk::Switch::builder().active(value).build();
    row(group, title, subtitle, &w);
    w
}
fn job<F, C>(work: F, done: C)
where
    F: FnOnce() -> Result<String, String> + Send + 'static,
    C: FnOnce(Result<String, String>) + 'static,
{
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    let mut done = Some(done);
    glib::timeout_add_local(Duration::from_millis(100), move || match rx.try_recv() {
        Ok(result) => {
            done.take().unwrap()(result);
            glib::ControlFlow::Break
        }
        Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
        Err(_) => {
            done.take().unwrap()(Err("The operation stopped unexpectedly.".into()));
            glib::ControlFlow::Break
        }
    });
}
fn privileged(action: &str, data: Option<String>) -> Result<String, String> {
    let mut child = Command::new("/usr/bin/pkexec")
        .args(["/usr/local/libexec/tend-admin", action])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(data) = data {
        child
            .stdin
            .take()
            .ok_or("Cannot send settings")?
            .write_all(data.as_bytes())
            .map_err(|e| e.to_string())?;
    } else {
        drop(child.stdin.take());
    }
    let result = child.wait_with_output().map_err(|e| e.to_string())?;
    if result.status.success() {
        Ok(String::from_utf8_lossy(&result.stdout).trim().into())
    } else {
        let text = String::from_utf8_lossy(&result.stderr).trim().to_string();
        Err(if text.is_empty() {
            "Authentication cancelled; nothing changed.".into()
        } else {
            text
        })
    }
}
fn snapshot() -> Result<String, String> {
    let before = tend::workstation::sample();
    let next = output(
        "/usr/bin/systemctl",
        &[
            "--timestamp=unix",
            "show",
            "dev-maintenance.timer",
            "--property=NextElapseUSecRealtime",
            "--value",
        ],
    )
    .unwrap_or_default();
    let active = output(
        "/usr/bin/systemctl",
        &["is-active", "dev-maintenance.timer"],
    )
    .unwrap_or_else(|_| "inactive".into());
    let disk = output("/usr/bin/df", &["-B1", "--output=avail", "/"]).unwrap_or_default();
    let free = disk
        .lines()
        .last()
        .and_then(|s| s.trim().parse::<f64>().ok())
        .map(|n| tend::workstation::bytes(n as u64))
        .unwrap_or_else(|| "Unavailable".into());
    let report = fs::read_to_string("/var/lib/dev-maintenance/latest-report.json")
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
    let mut completed = 0;
    let mut pending = 0;
    let mut last = "No report yet".to_string();
    if let Some(r) = report {
        last = dates::date(r["window"].as_str().unwrap_or(""));
        if let Some(p) = r["phases"].as_object() {
            for v in p.values() {
                if v["success"].as_bool() == Some(true) {
                    completed += 1;
                } else {
                    pending += 1;
                }
            }
        }
    }
    std::thread::sleep(Duration::from_millis(250));
    let after = tend::workstation::sample();
    let health = tend::workstation::sample_text(&after, Some(&before));
    let cpu = health.lines().next().unwrap_or("CPU unavailable");
    let summary = format!(
        "{cpu}\nMemory {} of {} used\nDisk swap {} of {} used",
        tend::workstation::bytes(after.memory_total.saturating_sub(after.memory_available)),
        tend::workstation::bytes(after.memory_total),
        tend::workstation::bytes(after.swap_total.saturating_sub(after.swap_free)),
        tend::workstation::bytes(after.swap_total)
    );
    let phases = format!(
        "{summary}\n\nLatest maintenance: {completed} completed · {pending} need attention"
    );
    Ok(
        serde_json::json!({"next":next,"active":active,"free":free,"last":last,"phases":phases})
            .to_string(),
    )
}
fn refresh(
    next: &gtk::Label,
    free: &gtk::Label,
    last: &gtk::Label,
    phase: &gtk::Label,
    status: &gtk::Label,
    button: &gtk::Button,
) {
    button.set_sensitive(false);
    let (next, free, last, phase, status, button) = (
        next.clone(),
        free.clone(),
        last.clone(),
        phase.clone(),
        status.clone(),
        button.clone(),
    );
    job(snapshot, move |result| {
        button.set_sensitive(true);
        match result
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).map_err(|e| e.to_string()))
        {
            Ok(v) => {
                let next_time = v["next"].as_str().and_then(dates::systemd);
                next.set_text(
                    &next_time
                        .map(dates::when)
                        .unwrap_or_else(|| "Not scheduled".into()),
                );
                next.set_tooltip_text(next_time.map(dates::exact).as_deref());
                free.set_text(v["free"].as_str().unwrap_or("—"));
                last.set_text(v["last"].as_str().unwrap_or("—"));
                phase.set_text(
                    v["phases"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .unwrap_or("Your next maintenance report will appear here."),
                );
                status.set_text(if v["active"] == "active" {
                    "Scheduled maintenance enabled"
                } else {
                    "Scheduled maintenance disabled"
                });
            }
            Err(e) => status.set_text(&e),
        }
    });
}
fn build(app: &adw::Application) {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(include_str!("../assets/style.css"));
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().unwrap(),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let win = adw::ApplicationWindow::builder()
        .application(app)
        .title("Tend")
        .default_width(960)
        .default_height(760)
        .build();
    let outer = column(0);
    outer.add_css_class("app-root");
    let header = adw::HeaderBar::new();
    header.set_show_start_title_buttons(false);
    header.set_show_end_title_buttons(false);
    let window_controls = gtk::WindowControls::builder()
        .side(gtk::PackType::End)
        .decoration_layout(":minimize,maximize")
        .build();
    let close = gtk::Button::from_icon_name("window-close-symbolic");
    close.add_css_class("titlebutton");
    close.add_css_class("circular");
    close.set_tooltip_text(Some("Close window · keep in dock when enabled"));
    let window = win.downgrade();
    close.connect_clicked(move |_| {
        if let Some(w) = window.upgrade() {
            let keep = fs::read_to_string(glib::user_config_dir().join("tend/background"))
                .ok()
                .map(|s| s.trim() != "false")
                .unwrap_or(true);
            if keep {
                w.minimize()
            } else {
                w.close()
            }
        }
    });
    header.pack_end(&close);
    header.pack_end(&window_controls);
    let menu = gtk::gio::Menu::new();
    menu.append(Some("Show Tend"), Some("app.show"));
    menu.append(Some("Quit Tend"), Some("app.quit"));
    let menu_button = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .build();
    header.pack_start(&menu_button);
    header.set_title_widget(Some(&adw::WindowTitle::new("Tend", "System tools")));
    outer.append(&header);
    let toast = adw::ToastOverlay::new();
    toast.set_vexpand(true);
    outer.append(&toast);
    let layout = gtk::Box::new(Orientation::Horizontal, 0);
    toast.set_child(Some(&layout));
    let stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .hexpand(true)
        .vexpand(true)
        .build();
    if std::env::var_os("TEND_SCREENSHOT").is_some() {
        stack.set_transition_type(gtk::StackTransitionType::None);
    }
    let sidebar = gtk::Box::new(Orientation::Vertical, 3);
    sidebar.add_css_class("sidebar");
    let navigation = gtk::ScrolledWindow::builder()
        .child(&sidebar)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .width_request(208)
        .build();
    layout.append(&navigation);
    layout.append(&gtk::Separator::new(Orientation::Vertical));
    layout.append(&stack);
    let home = page(
        "Overview",
        "Storage, system health, and maintenance status.",
    );
    let badge = label("Loading maintenance status…", "body");
    home.append(&badge);
    let metrics = gtk::Box::new(Orientation::Horizontal, 14);
    let free = label("—", "metric");
    let last = label("—", "metric-date");
    for (heading, value) in [("Free space", &free), ("Last maintenance", &last)] {
        let b = column(10);
        b.add_css_class("metric-card");
        b.set_hexpand(true);
        b.append(&label(heading, "section-label"));
        b.append(value);
        metrics.append(&b);
    }
    home.append(&metrics);
    let group = adw::PreferencesGroup::builder().title("Next run").build();
    let next = label("Reading your schedule…", "body");
    next.add_css_class("numeric");
    next.set_wrap(false);
    next.set_halign(gtk::Align::End);
    next.set_max_width_chars(28);
    row(
        &group,
        "Next maintenance",
        "Toronto time · catches up after missed runs",
        &next,
    );
    home.append(&group);
    let phase = label("Loading…", "body");
    let phasebox = column(12);
    phasebox.add_css_class("metric-card");
    phasebox.append(&label("System health and recent results", "section-label"));
    phasebox.append(&phase);
    home.append(&phasebox);
    let actions = gtk::Box::new(Orientation::Horizontal, 10);
    let refresh_button = gtk::Button::with_label("Refresh status");
    let run = gtk::Button::with_label("Open disk cleanup");
    run.add_css_class("suggested-action");
    actions.append(&run);
    actions.append(&refresh_button);
    home.append(&actions);
    home.reorder_child_after(&actions, Some(&group));
    home.append(&label(
        "Closing Tend does not stop scheduled maintenance.",
        "caption",
    ));
    stack.add_titled(&scroll(&home), Some("overview"), "Overview");
    workbench::add(&stack, &win);
    let settings = Settings::read();
    let schedule = page(
        "Maintenance schedule",
        "Daily maintenance and advance reminders. Changes take effect when saved.",
    );
    let loaded = settings.clone().unwrap_or_default();
    if let Err(ref e) = settings {
        schedule.append(&label(&format!("Could not read settings: {e}"), "error"));
    }
    let sg = adw::PreferencesGroup::builder()
        .title("Maintenance schedule")
        .build();
    let enabled = toggle(
        &sg,
        "Automatic maintenance",
        "Updates and conservative cleanup on a daily schedule",
        loaded.enabled,
    );
    let time = gtk::Box::new(Orientation::Horizontal, 8);
    let hour = gtk::SpinButton::with_range(0.0, 23.0, 1.0);
    hour.set_value(loaded.hour as f64);
    hour.set_tooltip_text(Some("Hour, 0–23"));
    let minute = gtk::SpinButton::with_range(0.0, 59.0, 1.0);
    minute.set_value(loaded.minute as f64);
    minute.set_tooltip_text(Some("Minute, 0–59"));
    for spin in [&hour, &minute] {
        spin.connect_output(|s| {
            s.set_text(&format!("{:02}", s.value_as_int()));
            gtk::glib::Propagation::Stop
        });
        spin.set_text(&format!("{:02}", spin.value_as_int()));
    }
    time.append(&hour);
    time.append(&label(":", "title-3"));
    time.append(&minute);
    row(
        &sg,
        "Start time",
        "24-hour clock · America/Toronto, including daylight saving",
        &time,
    );
    schedule.append(&sg);
    let wg = adw::PreferencesGroup::builder()
        .title("Notifications")
        .description(
            "Desktop notifications before scheduled maintenance. Missed warnings are skipped.",
        )
        .build();
    let w60 = toggle(
        &wg,
        "One hour before",
        "Notify 60 minutes before maintenance",
        loaded.warnings[0],
    );
    let w30 = toggle(
        &wg,
        "Thirty minutes before",
        "Notify 30 minutes before maintenance",
        loaded.warnings[1],
    );
    let w15 = toggle(
        &wg,
        "Fifteen minutes before",
        "Notify 15 minutes before maintenance",
        loaded.warnings[2],
    );
    schedule.append(&wg);
    let rg = adw::PreferencesGroup::builder()
        .title("After maintenance")
        .build();
    let reboot = toggle(
        &rg,
        "Restart after successful maintenance",
        "Includes a five-minute warning with a cancel option",
        loaded.reboot,
    );
    schedule.append(&rg);
    schedule.append(&label("Supported applications and running containers reopen after maintenance restarts. Save your work; terminal jobs and unsaved changes may not be restored.","caption"));
    let save = gtk::Button::with_label("Save settings");
    save.add_css_class("suggested-action");
    save.set_halign(gtk::Align::End);
    header.pack_end(&save);
    save.set_visible(false);
    let save_visibility = save.clone();
    stack.connect_visible_child_name_notify(move |s| {
        save_visibility.set_visible(s.visible_child_name().as_deref() == Some("schedule"))
    });
    let saved = label("", "caption");
    schedule.append(&saved);
    let toast_save = toast.clone();
    save.connect_clicked(move |button| {
        let s = Settings {
            enabled: enabled.is_active(),
            hour: hour.value_as_int() as u8,
            minute: minute.value_as_int() as u8,
            reboot: reboot.is_active(),
            warnings: [w60.is_active(), w30.is_active(), w15.is_active()],
        };
        let json = serde_json::to_string(&s).unwrap();
        button.set_sensitive(false);
        saved.set_text("Saving maintenance settings…");
        let (button, saved, toast) = (button.clone(), saved.clone(), toast_save.clone());
        job(
            move || privileged("apply", Some(json)),
            move |r| {
                button.set_sensitive(true);
                match r {
                    Ok(_) => {
                        saved.set_text("Saved. Your timers now use these settings.");
                        toast.add_toast(adw::Toast::new("Settings saved"));
                    }
                    Err(e) => saved.set_text(&format!("Not saved: {e}")),
                }
            },
        );
    });
    stack.add_titled(&scroll(&schedule), Some("schedule"), "Schedule");
    let care = page(
        "Appearance & daily policy",
        "Display preferences and the existing daily maintenance policy.",
    );
    let cg = adw::PreferencesGroup::builder()
        .title("Cleanup policy")
        .build();
    for (t,s) in [("Docker","Unused build cache older than 14 days; a 5 GB cache allowance. Only dangling images are pruned."),("Projects and volumes","Containers, tagged images, volumes, project dependencies, and virtual environments are preserved."),("npm and Python","npm cache verification runs weekly. Python caches over 2 GB are reported for review."),("Ubuntu","Stable repository updates, obsolete APT downloads, existing temporary-file cleanup and log rotation."),("Missed runs","Persistent systemd timers catch up after startup, with a 20-minute boot grace period.")]{let r=adw::ActionRow::builder().use_markup(false).title(t).subtitle(s).build();cg.add(&r);}
    care.append(&cg);
    let appearance = adw::PreferencesGroup::builder().title("Appearance").build();
    let theme = gtk::DropDown::from_strings(&["Follow system", "Light", "Dark"]);
    let pref = glib::user_config_dir().join("tend/theme");
    let theme_idx = fs::read_to_string(&pref)
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0)
        .min(2);
    theme.set_selected(theme_idx);
    let set_theme = |n| {
        adw::StyleManager::default().set_color_scheme(match n {
            1 => adw::ColorScheme::ForceLight,
            2 => adw::ColorScheme::ForceDark,
            _ => adw::ColorScheme::Default,
        })
    };
    set_theme(theme_idx);
    theme.connect_selected_notify(move |w| {
        set_theme(w.selected());
        if let Some(parent) = pref.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&pref, w.selected().to_string());
    });
    row(&appearance, "Color scheme", "Saved for this app", &theme);
    let background_path = glib::user_config_dir().join("tend/background");
    let background = toggle(
        &appearance,
        "Close to dock",
        "The title-bar close button minimizes Tend. Dock Quit, Alt+F4 and Quit Tend always exit.",
        fs::read_to_string(&background_path).ok().as_deref() != Some("false"),
    );
    background.connect_active_notify(move |b| {
        let _ = tend::workstation::private_write(
            &background_path,
            if b.is_active() { "true" } else { "false" },
        );
    });
    care.append(&appearance);
    care.append(&label("Tend 0.2", "caption"));
    stack.add_titled(&scroll(&care), Some("care"), "Appearance");
    let logs = page(
        "Activity",
        "The latest maintenance journal, loaded only when requested.",
    );
    let load = gtk::Button::with_label("Load recent activity");
    load.set_halign(gtk::Align::Start);
    logs.append(&load);
    let view = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .build();
    view.buffer()
        .set_text("Choose “Load recent activity” to read the system journal.");
    let vs = gtk::ScrolledWindow::builder()
        .child(&view)
        .min_content_height(440)
        .vexpand(true)
        .build();
    logs.append(&vs);
    load.connect_clicked(move |b| {
        b.set_sensitive(false);
        let (b, buffer) = (b.clone(), view.buffer());
        job(
            || {
                output(
                    "/usr/bin/journalctl",
                    &[
                        "-u",
                        "dev-maintenance.service",
                        "-n",
                        "100",
                        "--no-pager",
                        "--output=short-iso",
                    ],
                )
            },
            move |r| {
                b.set_sensitive(true);
                buffer.set_text(&r.unwrap_or_else(|e| format!("Could not read journal: {e}")));
            },
        );
    });
    stack.add_titled(&scroll(&logs), Some("activity"), "Activity");
    let (n, f, l, p, b) = (
        next.clone(),
        free.clone(),
        last.clone(),
        phase.clone(),
        badge.clone(),
    );
    refresh_button.connect_clicked(move |button| refresh(&n, &f, &l, &p, &b, button));
    let cleanup_stack = stack.clone();
    run.connect_clicked(move |_| cleanup_stack.set_visible_child_name("cleanup"));
    for (heading, entries) in [
        (
            "Monitor",
            vec![
                ("overview", "Overview", "view-grid-symbolic"),
                (
                    "task-manager",
                    "Task Manager",
                    "utilities-system-monitor-symbolic",
                ),
                ("system-logs", "System logs", "view-list-symbolic"),
            ],
        ),
        (
            "Manage",
            vec![
                ("storage", "Storage", "drive-harddisk-symbolic"),
                ("cleanup", "Disk cleanup", "edit-clear-all-symbolic"),
                ("startup", "Startup & services", "system-run-symbolic"),
                ("tuning", "Performance", "preferences-system-symbolic"),
                (
                    "development",
                    "Development tools",
                    "applications-engineering-symbolic",
                ),
                (
                    "reports",
                    "Updates & reports",
                    "software-update-available-symbolic",
                ),
                ("recovery", "Recovery", "edit-undo-symbolic"),
            ],
        ),
        (
            "Preferences",
            vec![
                ("schedule", "Maintenance", "alarm-symbolic"),
                (
                    "cleanup-scheduling",
                    "Cleanup schedule",
                    "x-office-calendar-symbolic",
                ),
                ("care", "Appearance", "applications-graphics-symbolic"),
                ("activity", "Activity", "document-open-recent-symbolic"),
            ],
        ),
    ] {
        let section = label(heading, "nav-heading");
        sidebar.append(&section);
        for (id, title, icon) in entries {
            let button = gtk::ToggleButton::new();
            button.add_css_class("nav-item");
            let content = gtk::Box::new(Orientation::Horizontal, 10);
            content.append(&gtk::Image::from_icon_name(icon));
            content.append(&label(title, "body"));
            button.set_child(Some(&content));
            button.set_active(stack.visible_child_name().as_deref() == Some(id));
            let pages = stack.clone();
            button.connect_clicked(move |b| {
                pages.set_visible_child_name(id);
                b.set_active(true);
            });
            let target = button.downgrade();
            stack.connect_visible_child_name_notify(move |s| {
                if let Some(b) = target.upgrade() {
                    b.set_active(s.visible_child_name().as_deref() == Some(id));
                }
            });
            sidebar.append(&button);
        }
    }
    win.set_content(Some(&outer));
    win.present();
    refresh(&next, &free, &last, &phase, &badge, &refresh_button);
    if let Ok(path) = std::env::var("TEND_SCREENSHOT") {
        if let Ok(page) = std::env::var("TEND_SCREENSHOT_PAGE") {
            stack.set_visible_child_name(&page);
        }
        let paint = gtk::WidgetPaintable::new(Some(&outer));
        let win = win.clone();
        let app = app.clone();
        let mut attempts = 0;
        glib::timeout_add_local(Duration::from_millis(500), move || {
            attempts += 1;
            if attempts
                < (if stack.visible_child_name().as_deref() == Some("task-manager") {
                    14
                } else {
                    6
                })
                || (stack.is_transition_running() && attempts < 40)
            {
                return glib::ControlFlow::Continue;
            }
            let shot = gtk::Snapshot::new();
            paint.snapshot(&shot, outer.width() as f64, outer.height() as f64);
            if let (Some(node), Some(renderer)) = (shot.to_node(), win.renderer()) {
                let texture = renderer.render_texture(&node, None);
                if let Err(e) = texture.save_to_png(&path) {
                    eprintln!("Screenshot failed: {e}");
                }
                app.quit();
                return glib::ControlFlow::Break;
            }
            if attempts >= 40 {
                eprintln!(
                    "Screenshot failed: mapped={}, size={}x{}, renderer={}",
                    win.is_mapped(),
                    outer.width(),
                    outer.height(),
                    win.renderer().is_some()
                );
                app.quit();
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
    }
}
fn main() -> glib::ExitCode {
    let app = adw::Application::builder()
        .application_id(ID)
        .flags(if std::env::var_os("TEND_SCREENSHOT").is_some() {
            gtk::gio::ApplicationFlags::NON_UNIQUE
        } else {
            gtk::gio::ApplicationFlags::FLAGS_NONE
        })
        .build();
    let quit = gtk::gio::SimpleAction::new("quit", None);
    let weak = app.downgrade();
    quit.connect_activate(move |_, _| {
        if let Some(a) = weak.upgrade() {
            a.quit();
        }
    });
    app.add_action(&quit);
    for action in ["show", "hide"] {
        let a = gtk::gio::SimpleAction::new(action, None);
        let weak = app.downgrade();
        a.connect_activate(move |_, _| {
            if let Some(app) = weak.upgrade() {
                if let Some(w) = app.windows().first() {
                    if action == "hide" {
                        w.minimize();
                    } else {
                        w.unminimize();
                        w.present();
                    }
                }
            }
        });
        app.add_action(&a);
    }
    app.connect_activate(|app| {
        if let Some(window) = app.windows().first() {
            window.unminimize();
            window.present();
        } else {
            build(app);
        }
    });
    app.run()
}
