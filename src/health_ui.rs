//! Window-independent, opt-in monitoring. Uses the existing sampler; no daemon,
//! autostart, filesystem watcher, scheduler, process scan or per-sample history.
use crate::{editor, job, label, monitor, panel, row, toggle};
use adw::prelude::*;
use gtk::{gio, glib};
use std::{
    cell::RefCell,
    collections::VecDeque,
    hash::{Hash, Hasher},
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tend::{
    health::{Engine, Settings},
    observations::{self, Observation},
    sampler::{Detail, Update},
    workstation as w,
};

struct Runtime {
    settings: Settings,
    engine: Engine,
    subscription: Option<monitor::Handle>,
    generation: u64,
    last: Option<(u64, Observation)>,
    error: Option<String>,
    delivery_error: Option<String>,
    labels: Vec<glib::WeakRef<gtk::Label>>,
    crash_busy: bool,
    next_crash: u64,
    crash_since: u64,
    last_crash_notice: Option<u64>,
    crash_ids: VecDeque<u64>,
    cancel: Arc<AtomicBool>,
    audit_busy: bool,
}
thread_local! {static RUNTIME:RefCell<Option<Rc<RefCell<Runtime>>>>=const{RefCell::new(None)};}
fn runtime() -> Option<Rc<RefCell<Runtime>>> {
    RUNTIME.with(|r| r.borrow().clone())
}
fn clock() -> u64 {
    observations::boot_millis().unwrap_or(0)
}
pub fn summary() -> String {
    let Some(r) = runtime() else {
        return "Monitoring unavailable".into();
    };
    let r = r.borrow();
    if !r.settings.enabled {
        return if r.error.is_some() {
            "Monitoring off · settings unavailable".into()
        } else {
            "Monitoring off".into()
        };
    }
    let now = clock();
    let state = if r.engine.snoozed(now) {
        "Notifications snoozed"
    } else {
        "Monitoring live"
    };
    let Some((at, o)) = &r.last else {
        return format!(
            "{state} · waiting for a sample{}",
            if r.error.is_some() {
                " · collection unavailable"
            } else {
                ""
            }
        );
    };
    let age = now.saturating_sub(*at) / 1000;
    if age > 45 || r.error.is_some() {
        return format!("Monitoring stale · last sample {age}s ago");
    }
    let cpu = o
        .cpu_percent
        .map(|p| format!("CPU {p:.0}%"))
        .unwrap_or_else(|| "CPU unavailable".into());
    let memory = if o.memory_total > 0 {
        format!(
            "memory {:.0}% available",
            100. * o.memory_available as f64 / o.memory_total as f64
        )
    } else {
        "memory unavailable".into()
    };
    format!(
        "{state} · {cpu} · {memory} · {} active conditions{}",
        r.engine.active_count(),
        if r.delivery_error.is_some() {
            " · notification/history issue"
        } else {
            ""
        }
    )
}
pub fn tooltip() -> String {
    let text = summary();
    let Some(r) = runtime() else {
        return text;
    };
    let r = r.borrow();
    let detail = r.error.as_ref().or(r.delivery_error.as_ref());
    match detail {
        Some(e) => format!("{text}\n{e}"),
        None => text,
    }
}
pub fn enabled() -> bool {
    runtime().is_some_and(|r| r.borrow().settings.enabled)
}
pub fn snoozed() -> bool {
    runtime().is_some_and(|r| r.borrow().engine.snoozed(clock()))
}
fn refresh() {
    let text = summary();
    if let Some(r) = runtime() {
        let mut r = r.borrow_mut();
        r.labels.retain(|weak| {
            if let Some(label) = weak.upgrade() {
                label.set_text(&text);
                true
            } else {
                false
            }
        });
    }
    panel::refresh();
}
fn bind(label: &gtk::Label) {
    if let Some(r) = runtime() {
        let mut r = r.borrow_mut();
        r.labels.retain(|l| l.upgrade().is_some());
        r.labels.push(label.downgrade());
    }
    label.set_text(&summary());
}
pub fn snooze() {
    if let Some(r) = runtime() {
        let mut r = r.borrow_mut();
        if r.settings.enabled {
            r.engine.snooze(clock(), 3600);
        }
    }
    refresh();
}
pub fn resume() {
    if let Some(r) = runtime() {
        r.borrow_mut().engine.resume();
    }
    refresh();
}
pub fn install(app: &adw::Application) {
    if runtime().is_some() {
        return;
    }
    let loaded = Settings::read();
    let initial = loaded.clone().unwrap_or_default();
    let r = Rc::new(RefCell::new(Runtime {
        engine: Engine::new(initial.clone()).unwrap(),
        settings: initial.clone(),
        subscription: None,
        generation: 0,
        last: None,
        error: loaded.err(),
        delivery_error: None,
        labels: vec![],
        crash_busy: false,
        next_crash: 0,
        crash_since: w::now(),
        last_crash_notice: None,
        crash_ids: VecDeque::new(),
        cancel: Arc::new(AtomicBool::new(false)),
        audit_busy: false,
    }));
    RUNTIME.with(|cell| *cell.borrow_mut() = Some(r));
    if initial.enabled {
        apply(app, &initial);
    }
}
fn apply(app: &adw::Application, settings: &Settings) {
    let Some(r) = runtime() else {
        return;
    };
    {
        let mut r = r.borrow_mut();
        r.cancel.store(true, Ordering::Relaxed);
        r.subscription.take();
        r.generation += 1;
        r.settings = settings.clone();
        r.engine = Engine::new(settings.clone()).unwrap();
        r.last = None;
        r.error = None;
        r.delivery_error = None;
        r.crash_busy = false;
        r.next_crash = clock().saturating_add(300000);
        r.crash_since = w::now();
        r.last_crash_notice = None;
        r.crash_ids.clear();
        r.cancel = Arc::new(AtomicBool::new(false));
    }
    if settings.enabled {
        let app = app.downgrade();
        let handle = monitor::subscribe(Detail::Basic, Duration::from_secs(15), move |update| {
            if let Some(app) = app.upgrade() {
                receive(&app, update);
            }
        });
        match handle {
            Ok(handle) => r.borrow_mut().subscription = Some(handle),
            Err(e) => r.borrow_mut().error = Some(e),
        }
    }
    refresh();
}
fn notify(app: &adw::Application, title: String, body: String, generation: u64) {
    #[cfg(feature = "docs-demo")]
    if std::env::var_os("TEND_DEMO_DIR").is_some() && std::env::var_os("TEND_HEALTH_QA").is_none() {
        return;
    }
    let weak = app.downgrade();
    gio::bus_get(
        gio::BusType::Session,
        None::<&gio::Cancellable>,
        move |bus| {
            if !runtime()
                .is_some_and(|r| r.borrow().generation == generation && r.borrow().settings.enabled)
            {
                return;
            }
            let Ok(bus) = bus else {
                delivery_failure(generation, "Desktop notification bus unavailable.");
                return;
            };
            let hints = std::collections::HashMap::<String, glib::Variant>::from([(
                "urgency".into(),
                1u8.to_variant(),
            )]);
            let args = (
                "Tend",
                0u32,
                "io.github.jtmb.Tend",
                title,
                glib::markup_escape_text(&body).to_string(),
                Vec::<String>::new(),
                hints,
                -1i32,
            )
                .to_variant();
            bus.call(
                Some("org.freedesktop.Notifications"),
                "/org/freedesktop/Notifications",
                "org.freedesktop.Notifications",
                "Notify",
                Some(&args),
                None,
                gio::DBusCallFlags::NONE,
                3000,
                None::<&gio::Cancellable>,
                move |result| {
                    if weak.upgrade().is_none() {
                        return;
                    }
                    if let Err(e) = result {
                        delivery_failure(
                            generation,
                            &format!("Desktop notification delivery failed: {e}"),
                        );
                    }
                },
            );
        },
    );
}
fn delivery_failure(generation: u64, message: &str) {
    if let Some(r) = runtime() {
        let mut r = r.borrow_mut();
        if r.generation != generation {
            return;
        }
        r.delivery_error = Some(message.chars().take(1200).collect());
        drop(r);
        refresh();
    }
}
fn receive(app: &adw::Application, update: Update) {
    receive_at(app, update, clock());
}
fn receive_at(app: &adw::Application, update: Update, now: u64) {
    let Some(r) = runtime() else {
        return;
    };
    let (notices, generation, crash) = {
        let mut r = r.borrow_mut();
        if !r.settings.enabled {
            return;
        }
        let notices = match update {
            Ok(capture) => {
                r.error = None;
                let notices = r.engine.update(&capture.observation, now);
                r.last = Some((now, capture.observation.clone()));
                notices
            }
            Err(e) => {
                r.error = Some(e);
                r.engine.missing();
                vec![]
            }
        };
        let crash = r.settings.crashes && !r.crash_busy && now >= r.next_crash;
        if crash {
            r.crash_busy = true;
            r.next_crash = now.saturating_add(300000);
        }
        (notices, r.generation, crash)
    };
    if !notices.is_empty() {
        for notice in &notices {
            notify(app, notice.title(), notice.body(), generation);
        }
        let report = notices
            .iter()
            .map(|n| format!("{}\n{}", n.title(), n.body()))
            .collect::<Vec<_>>()
            .join("\n\n");
        // At most one bounded write in flight. Notifications still work when
        // history is locked, full or unavailable; writes never drive retries.
        if !r.borrow().audit_busy {
            r.borrow_mut().audit_busy = true;
            job(
                move || w::record("health-alert", &report),
                move |result| {
                    if let Some(r) = runtime() {
                        let mut r = r.borrow_mut();
                        r.audit_busy = false;
                        if r.generation != generation {
                            return;
                        }
                        drop(r);
                        if let Err(e) = result {
                            delivery_failure(generation, &e);
                        }
                    }
                },
            );
        } else {
            delivery_failure(
                generation,
                "An alert was not saved because another history write is pending.",
            );
        }
    }
    if crash {
        read_crashes(app, generation);
    }
    refresh();
}
fn read_crashes(app: &adw::Application, generation: u64) {
    let Some(r) = runtime() else {
        return;
    };
    let cancel = r.borrow().cancel.clone();
    let checked = w::now();
    let weak = app.downgrade();
    job(
        move || tend::system_logs::entries_cancellable(2, 1, 100, &cancel),
        move |result| {
            let Some(r) = runtime() else {
                return;
            };
            let Some(app) = weak.upgrade() else {
                return;
            };
            let mut r = r.borrow_mut();
            if r.generation != generation || !r.settings.enabled {
                return;
            }
            r.crash_busy = false;
            let entries = match result {
                Ok(e) => e,
                Err(e) => {
                    r.delivery_error = Some(format!("Optional crash read unavailable: {e}"));
                    drop(r);
                    refresh();
                    return;
                }
            };
            // Only events after opt-in/last successful check. Clock rollback resets
            // the baseline rather than replaying old crash notifications.
            if checked < r.crash_since {
                r.crash_since = checked;
                drop(r);
                refresh();
                return;
            }
            let since = r.crash_since;
            r.crash_since = checked.saturating_sub(2);
            let mut sources = vec![];
            for e in entries
                .into_iter()
                .filter(|e| e.crash && e.time > since as i64 && e.time <= checked as i64)
            {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                (e.time, &e.source, &e.details).hash(&mut hasher);
                let key = hasher.finish();
                if r.crash_ids.contains(&key) {
                    continue;
                }
                if r.crash_ids.len() == 256 {
                    r.crash_ids.pop_front();
                }
                r.crash_ids.push_back(key);
                if sources.len() < 5 {
                    sources.push(e.source.chars().take(160).collect::<String>());
                }
            }
            let now = clock();
            let snoozed = r.engine.snoozed(now)
                || r.last_crash_notice.is_some_and(|last| {
                    now.saturating_sub(last) < r.settings.cooldown_seconds as u64 * 1000
                });
            if !sources.is_empty() && !snoozed {
                r.last_crash_notice = Some(now);
            }
            drop(r);
            if !sources.is_empty() && !snoozed {
                notify(&app,"Recent application crashes".into(),format!("{}\nOpen System logs for the reports. Up to 100 recent visible reports are checked; inaccessible reports are not included.",sources.join("\n")),generation);
            }
            refresh();
        },
    );
}
pub fn shutdown() {
    RUNTIME.with(|cell| {
        if let Some(r) = cell.borrow_mut().take() {
            let mut r = r.borrow_mut();
            r.cancel.store(true, Ordering::Relaxed);
            r.subscription.take();
            r.generation += 1;
        }
    });
}

pub fn add(parent: &gtk::Box, win: &adw::ApplicationWindow, app: &adw::Application) {
    let group=adw::PreferencesGroup::builder().title("Health monitoring").description("Optional resource alerts while Tend runs, including with its window hidden. Quit stops monitoring.").build();
    let state = label("", "caption");
    bind(&state);
    group.add(&state);
    let open = gtk::Button::with_label("Configure monitoring");
    row(
        &group,
        "Alerts and thresholds",
        "Defaults off · no daemon or autostart",
        &open,
    );
    let win = win.clone();
    let app = app.clone();
    open.connect_clicked(move |_| open_editor(&win, &app));
    parent.append(&group);
}
fn open_editor(win: &adw::ApplicationWindow, app: &adw::Application) {
    let loaded = Settings::read();
    let baseline = Rc::new(RefCell::new(loaded.clone().ok()));
    let initial = loaded.clone().unwrap_or_default();
    let body=crate::page("Health monitoring","Samples every 15 seconds while enabled. Only alert transitions are saved; no continuous resource history is recorded.");
    // The dialog header supplies the title and an explicit close button.
    if let Some(title) = body.first_child() {
        body.remove(&title);
    }
    if let Err(e) = loaded {
        editor::error(&body,"Saved monitoring settings could not be loaded. Monitoring remains off until a valid policy is saved.",&e);
    }
    let group = adw::PreferencesGroup::builder()
        .title("Resource alerts")
        .build();
    let enabled = toggle(
        &group,
        "Enable monitoring",
        "Runs while Tend is open or hidden · Quit stops it",
        initial.enabled,
    );
    enabled.set_widget_name("monitor-enabled");
    let mut controls = Vec::new();
    for (name, description, rule, min, max, step) in [
        (
            "High CPU use",
            "Alert at or above this percent",
            &initial.cpu,
            1.,
            100.,
            1.,
        ),
        (
            "Low available memory",
            "Alert below this percent of total RAM",
            &initial.memory,
            1.,
            90.,
            1.,
        ),
        (
            "Low free disk space",
            "Alert below this many GiB on the root filesystem",
            &initial.disk,
            1.,
            1024.,
            1.,
        ),
        (
            "Memory stalls",
            "Memory full PSI avg10: percent time all runnable tasks were stalled",
            &initial.memory_pressure,
            0.1,
            100.,
            0.1,
        ),
        (
            "I/O stalls",
            "I/O full PSI avg10: percent time all runnable tasks were stalled",
            &initial.io_pressure,
            0.1,
            100.,
            0.1,
        ),
    ] {
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let check = gtk::CheckButton::new();
        check.set_active(rule.enabled);
        check.set_tooltip_text(Some(&format!("Enable {name}")));
        let spin = gtk::SpinButton::with_range(min, max, step);
        spin.set_digits(if step < 1. { 1 } else { 0 });
        spin.set_value(rule.threshold);
        spin.set_tooltip_text(Some(&format!("{name} threshold")));
        line.append(&check);
        line.append(&spin);
        row(&group, name, description, &line);
        check.set_widget_name(&format!("monitor-rule-{}", controls.len()));
        spin.set_widget_name(&format!("monitor-threshold-{}", controls.len()));
        controls.push((check, spin));
    }
    body.append(&group);
    let timing=adw::PreferencesGroup::builder().title("Notification policy").description("Recovery requires the same sustained duration, beyond a 10% hysteresis margin. Cooldown limits repeated alerts per condition.").build();
    let hold = gtk::SpinButton::with_range(30., 3600., 15.);
    hold.set_widget_name("monitor-hold");
    hold.set_value(initial.hold_seconds as f64);
    hold.set_tooltip_text(Some("Sustained duration in seconds"));
    row(
        &timing,
        "Sustained duration",
        "Seconds continuously beyond the threshold",
        &hold,
    );
    let cooldown = gtk::SpinButton::with_range(60., 86400., 60.);
    cooldown.set_widget_name("monitor-cooldown");
    cooldown.set_value(initial.cooldown_seconds as f64);
    cooldown.set_tooltip_text(Some("Cooldown in seconds"));
    row(
        &timing,
        "Repeat cooldown",
        "Minimum seconds between alerts for the same condition",
        &cooldown,
    );
    let recovery = toggle(
        &timing,
        "Notify on recovery",
        "Send a notice when an announced condition recovers",
        initial.recovery,
    );
    let crashes=toggle(&timing,"Notify about new crashes","Optional bounded log read every five minutes; visible journal and crash headers only. Starts with new events after enabling.",initial.crashes);
    crashes.set_widget_name("monitor-crashes");
    body.append(&timing);
    let live = adw::PreferencesGroup::builder()
        .title("Current session")
        .build();
    let status = label("", "caption");
    bind(&status);
    live.add(&status);
    let snooze_button = gtk::Button::with_label("Snooze for one hour");
    snooze_button.connect_clicked(|_| snooze());
    row(
        &live,
        "Snooze notifications",
        "Collection continues; no alerts or recoveries are sent during snooze",
        &snooze_button,
    );
    let resume_button = gtk::Button::with_label("Resume notifications");
    resume_button.connect_clicked(|_| resume());
    live.add(&resume_button);
    body.append(&live);
    let footer = editor::Footer::new(
        !tend::health::path().exists(),
        baseline.borrow().is_some(),
        None,
    );
    let get: Rc<dyn Fn() -> Settings> = Rc::new({
        let (enabled, controls, hold, cooldown, recovery, crashes) = (
            enabled.clone(),
            controls.clone(),
            hold.clone(),
            cooldown.clone(),
            recovery.clone(),
            crashes.clone(),
        );
        move || {
            let rule = |i: usize| tend::health::Rule {
                enabled: controls[i].0.is_active(),
                threshold: controls[i].1.value(),
            };
            Settings {
                version: 1,
                enabled: enabled.is_active(),
                cpu: rule(0),
                memory: rule(1),
                disk: rule(2),
                memory_pressure: rule(3),
                io_pressure: rule(4),
                hold_seconds: hold.value_as_int() as u32,
                cooldown_seconds: cooldown.value_as_int() as u32,
                recovery: recovery.is_active(),
                crashes: crashes.is_active(),
            }
        }
    });
    let changing = Rc::new(std::cell::Cell::new(false));
    let changed: Rc<dyn Fn()> = Rc::new({
        let (get, baseline, footer, changing) = (
            get.clone(),
            baseline.clone(),
            footer.clone(),
            changing.clone(),
        );
        move || {
            if changing.get() {
                return;
            }
            let draft = get();
            footer.sync(
                baseline.borrow().as_ref() != Some(&draft),
                draft.validate().is_ok(),
            );
        }
    });
    let handlers = Rc::new(RefCell::new(
        Vec::<(glib::Object, glib::SignalHandlerId)>::new(),
    ));
    for b in [&enabled, &recovery, &crashes] {
        let changed = changed.clone();
        let id = b.connect_active_notify(move |_| changed());
        handlers.borrow_mut().push((b.clone().upcast(), id));
    }
    for (check, spin) in &controls {
        let on_check = changed.clone();
        let id = check.connect_toggled(move |_| on_check());
        handlers.borrow_mut().push((check.clone().upcast(), id));
        let on_spin = changed.clone();
        let id = spin.connect_value_changed(move |_| on_spin());
        handlers.borrow_mut().push((spin.clone().upcast(), id));
    }
    for spin in [&hold, &cooldown] {
        let changed = changed.clone();
        let id = spin.connect_value_changed(move |_| changed());
        handlers.borrow_mut().push((spin.clone().upcast(), id));
    }
    {
        let (baseline, footer, get, app) =
            (baseline.clone(), footer.clone(), get.clone(), app.clone());
        let save = footer.save.clone();
        let id = save.connect_clicked(move |_| {
            let draft = get();
            let saved = draft.clone();
            let (baseline, footer, app, current) =
                (baseline.clone(), footer.clone(), app.clone(), get.clone());
            footer.saving();
            job(
                move || saved.save(),
                move |result| match result {
                    Ok(()) => {
                        apply(&app, &draft);
                        *baseline.borrow_mut() = Some(draft.clone());
                        footer.success();
                        let pending = current();
                        footer.sync(pending != draft, pending.validate().is_ok());
                    }
                    Err(e) => footer.error(&e),
                },
            );
        });
        handlers.borrow_mut().push((save.upcast(), id));
    }
    {
        let (baseline, footer) = (baseline.clone(), footer.clone());
        let discard = footer.discard.clone();
        let id = discard.connect_clicked(move |_| {
            let Some(s) = baseline.borrow().clone() else {
                return;
            };
            changing.set(true);
            enabled.set_active(s.enabled);
            recovery.set_active(s.recovery);
            crashes.set_active(s.crashes);
            hold.set_value(s.hold_seconds as f64);
            cooldown.set_value(s.cooldown_seconds as f64);
            for (i, m) in tend::health::Metric::ALL.into_iter().enumerate() {
                controls[i].0.set_active(m.rule(&s).enabled);
                controls[i].1.set_value(m.rule(&s).threshold);
            }
            changing.set(false);
            footer.sync(false, true);
        });
        handlers.borrow_mut().push((discard.upcast(), id));
    }
    let shell = editor::shell(&body, &footer);
    let toolbar = adw::ToolbarView::new();
    toolbar.set_content(Some(&shell));
    let header = adw::HeaderBar::new();
    header.set_show_start_title_buttons(false);
    toolbar.add_top_bar(&header);
    let dialog = adw::Dialog::builder()
        .title("Health monitoring")
        .content_width(680)
        .content_height(690)
        .child(&toolbar)
        .build();
    dialog.connect_closed(move |_| {
        for (object, id) in handlers.borrow_mut().drain(..) {
            object.disconnect(id);
        }
    });
    dialog.present(Some(win));
}

#[cfg(feature = "docs-demo")]
pub fn qa_observe(app: &adw::Application, cpu: f64, at: u64) {
    assert!(
        std::env::var_os("TEND_HEALTH_QA").is_some() && std::env::var_os("TEND_DEMO_DIR").is_some()
    );
    let o = Observation {
        time_unix_ms: w::now() * 1000,
        boot_id: observations::boot_id().into(),
        cpu_percent: Some(cpu),
        ..Default::default()
    };
    receive_at(
        app,
        Ok(Arc::new(tend::sampler::Capture {
            sequence: 0,
            observation: o,
            full: None,
        })),
        at,
    );
}
#[cfg(feature = "docs-demo")]
pub fn qa_crashes(app: &adw::Application) {
    assert!(
        std::env::var_os("TEND_HEALTH_QA").is_some() && std::env::var_os("TEND_DEMO_DIR").is_some()
    );
    let r = runtime().unwrap();
    let mut r = r.borrow_mut();
    r.crash_since = w::now().saturating_sub(5);
    r.crash_busy = true;
    let generation = r.generation;
    drop(r);
    read_crashes(app, generation);
}
