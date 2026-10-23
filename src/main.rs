use adw::prelude::*;
use gtk::{glib, Orientation};
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
    sync::mpsc,
    time::Duration,
};
use tend::{output, Settings};
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
    b.set_margin_top(28);
    b.set_margin_bottom(28);
    b.set_margin_start(30);
    b.set_margin_end(30);
    b.append(&label(title, "page-title"));
    b.append(&label(description, "dim-label"));
    b
}
fn scroll(b: &gtk::Box) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder()
        .child(b)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .hexpand(true)
        .vexpand(true)
        .build()
}
fn row(group: &adw::PreferencesGroup, title: &str, subtitle: &str, widget: &impl IsA<gtk::Widget>) {
    let r = adw::ActionRow::builder()
        .use_markup(false)
        .title(title)
        .subtitle(subtitle)
        .build();
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
    let next = output(
        "/usr/bin/systemctl",
        &[
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
        .map(|n| format!("{:.1} GB", n / 1e9))
        .unwrap_or_else(|| "Unavailable".into());
    let report = fs::read_to_string("/var/lib/dev-maintenance/latest-report.json")
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
    let mut phases = String::new();
    let mut last = "No report yet".to_string();
    if let Some(r) = report {
        last = r["window"].as_str().unwrap_or("Unknown date").into();
        if let Some(p) = r["phases"].as_object() {
            for (k, v) in p {
                phases.push_str(&format!(
                    "{}  ·  {}\n",
                    if v["success"].as_bool() == Some(true) {
                        "✓"
                    } else {
                        "•"
                    },
                    k.replace('-', " ")
                ));
            }
        }
    }
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
                next.set_text(
                    v["next"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .unwrap_or("No run scheduled"),
                );
                free.set_text(v["free"].as_str().unwrap_or("—"));
                last.set_text(v["last"].as_str().unwrap_or("—"));
                phase.set_text(
                    v["phases"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .unwrap_or("Your next maintenance report will appear here."),
                );
                status.set_text(if v["active"] == "active" {
                    "AUTOMATIC CARE IS ON"
                } else {
                    "AUTOMATIC CARE IS OFF"
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
    header.set_title_widget(Some(&adw::WindowTitle::new(
        "Tend",
        "A little care for your computer",
    )));
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
    let sidebar = gtk::StackSidebar::new();
    sidebar.set_stack(&stack);
    sidebar.set_size_request(170, -1);
    sidebar.add_css_class("sidebar");
    layout.append(&sidebar);
    layout.append(&gtk::Separator::new(Orientation::Vertical));
    layout.append(&stack);
    let home = page(
        "Your workstation, cared for.",
        "A calm place to keep Ubuntu ready for your next project.",
    );
    let hero = column(12);
    hero.add_css_class("hero");
    let badge = label("AUTOMATIC CARE", "eyebrow");
    hero.append(&badge);
    hero.append(&label(
        "Small routines.\nMore room to create.",
        "hero-title",
    ));
    hero.append(&label(
        "Updates, thoughtful cleanup, and a familiar desktop when you return.",
        "hero-subtitle",
    ));
    home.append(&hero);
    let metrics = gtk::Box::new(Orientation::Horizontal, 14);
    let free = label("—", "metric");
    let last = label("—", "metric");
    for (heading, value) in [("SPACE AVAILABLE", &free), ("LAST MAINTENANCE", &last)] {
        let b = column(10);
        b.add_css_class("metric-card");
        b.set_hexpand(true);
        b.append(&label(heading, "eyebrow"));
        b.append(value);
        metrics.append(&b);
    }
    home.append(&metrics);
    let group = adw::PreferencesGroup::builder().title("Next up").build();
    let next = label("Reading your schedule…", "body");
    next.set_max_width_chars(40);
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
    phasebox.append(&label("THE LAST RUN", "eyebrow"));
    phasebox.append(&phase);
    home.append(&phasebox);
    let actions = gtk::Box::new(Orientation::Horizontal, 10);
    let refresh_button = gtk::Button::with_label("Refresh status");
    let run = gtk::Button::with_label("Run maintenance…");
    run.add_css_class("suggested-action");
    actions.append(&run);
    actions.append(&refresh_button);
    home.append(&actions);
    home.reorder_child_after(&actions, Some(&group));
    home.append(&label("Tend uses the existing system timers. Closing this window keeps maintenance scheduled and frees the app’s memory.","caption"));
    stack.add_titled(&scroll(&home), Some("overview"), "Overview");
    let settings = Settings::read();
    let schedule = page(
        "Make it fit your day.",
        "Changes take effect when you save. Ubuntu asks for administrator authentication.",
    );
    let loaded = settings.clone().unwrap_or_default();
    if let Err(ref e) = settings {
        schedule.append(&label(&format!("Could not read settings: {e}"), "error"));
    }
    let sg = adw::PreferencesGroup::builder()
        .title("Daily rhythm")
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
        .title("A heads-up")
        .description(
            "Desktop notifications before scheduled maintenance. Missed warnings are skipped.",
        )
        .build();
    let w60 = toggle(
        &wg,
        "One hour before",
        "Time to plan around a maintenance run",
        loaded.warnings[0],
    );
    let w30 = toggle(
        &wg,
        "Thirty minutes before",
        "A reminder to wrap up",
        loaded.warnings[1],
    );
    let w15 = toggle(
        &wg,
        "Fifteen minutes before",
        "A final scheduled reminder",
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
    schedule.append(&label("The existing one-use login and TPM keyring policy handles maintenance restarts. Supported apps and running containers reopen; unsaved work and running terminal jobs cannot be guaranteed.","caption"));
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
        saved.set_text("Waiting for Ubuntu authentication…");
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
        "Care with boundaries.",
        "The maintenance engine keeps useful development data in place.",
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
    care.append(&appearance);
    care.append(&label(
        "Tend 0.1 · Rust + GTK 4 + libadwaita\nNo webview. No background agent. No telemetry.",
        "caption",
    ));
    stack.add_titled(&scroll(&care), Some("care"), "Care & appearance");
    let logs = page(
        "See what happened.",
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
    let parent = win.clone();
    let toast_run = toast.clone();
    run.connect_clicked(move|button|{let dialog=adw::MessageDialog::new(Some(&parent),Some("Run maintenance now?"),Some("This starts the existing maintenance engine. If automatic restart is enabled, a successful run may restart your computer after its warning. Save your work first."));dialog.add_responses(&[("cancel","Cancel"),("run","Run maintenance")]);dialog.set_response_appearance("run",adw::ResponseAppearance::Suggested);dialog.set_default_response(Some("cancel"));dialog.set_close_response("cancel");let (button,toast)=(button.clone(),toast_run.clone());dialog.connect_response(None,move|_,response|{if response=="run"{button.set_sensitive(false);let (button,toast)=(button.clone(),toast.clone());job(||privileged("run",None),move|r|{button.set_sensitive(true);toast.add_toast(adw::Toast::new(&match r{Ok(_)=>"Maintenance requested. Check Activity for progress.".into(),Err(e)=>e}));});}});dialog.present();});
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
            if attempts < 6 || stack.is_transition_running() {
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
    app.connect_activate(|app| {
        if let Some(window) = app.active_window() {
            window.present();
        } else {
            build(app);
        }
    });
    app.run()
}
