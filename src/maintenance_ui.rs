use crate::{editor, job, label, page, row, toggle};
use adw::prelude::*;
use std::{cell::RefCell, rc::Rc};
use tend::Settings;

pub fn add(stack: &gtk::Stack, _win: &adw::ApplicationWindow) {
    let p = page(
        "Maintenance",
        "Daily maintenance and advance reminders. Changes take effect when saved.",
    );
    let loaded = Settings::read();
    let baseline = Rc::new(RefCell::new(loaded.as_ref().ok().cloned()));
    let initial = loaded.as_ref().cloned().unwrap_or_default();
    let unavailable = if cfg!(feature = "docs-demo") && std::env::var_os("TEND_DEMO_DIR").is_some()
    {
        Some("System saves are disabled in this isolated preview.")
    } else if crate::administrator_available().is_err() {
        Some("Administrator helper setup is required to save. No password prompt will open.")
    } else {
        None
    };
    let footer = editor::Footer::new(
        !std::path::Path::new(tend::CONFIG).exists(),
        baseline.borrow().is_some(),
        unavailable,
    );
    if let Err(e) = &loaded {
        editor::error(&p, "Cannot load maintenance settings. Review the defaults before replacing the saved file.", e);
    }
    let sg = adw::PreferencesGroup::builder()
        .title("Daily schedule")
        .build();
    let enabled = toggle(
        &sg,
        "Automatic maintenance",
        "Updates and conservative cleanup on a daily schedule",
        initial.enabled,
    );
    let time = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let hour = gtk::SpinButton::with_range(0., 23., 1.);
    let minute = gtk::SpinButton::with_range(0., 59., 1.);
    hour.set_tooltip_text(Some("Hour, 0–23"));
    minute.set_tooltip_text(Some("Minute, 0–59"));
    hour.set_value(initial.hour as f64);
    minute.set_value(initial.minute as f64);
    for s in [&hour, &minute] {
        s.connect_output(|s| {
            s.set_text(&format!("{:02}", s.value_as_int()));
            gtk::glib::Propagation::Stop
        });
        s.set_text(&format!("{:02}", s.value_as_int()));
    }
    time.append(&hour);
    time.append(&label(":", "body"));
    time.append(&minute);
    row(
        &sg,
        "Start time",
        "24-hour clock · America/Toronto, including daylight saving",
        &time,
    );
    p.append(&sg);
    let wg = adw::PreferencesGroup::builder()
        .title("Notifications")
        .description("Desktop reminders before maintenance. Missed reminders are skipped.")
        .build();
    let w60 = toggle(
        &wg,
        "One hour before",
        "Notify 60 minutes before maintenance",
        initial.warnings[0],
    );
    let w30 = toggle(
        &wg,
        "Thirty minutes before",
        "Notify 30 minutes before maintenance",
        initial.warnings[1],
    );
    let w15 = toggle(
        &wg,
        "Fifteen minutes before",
        "Notify 15 minutes before maintenance",
        initial.warnings[2],
    );
    p.append(&wg);
    let rg = adw::PreferencesGroup::builder()
        .title("After maintenance")
        .build();
    let reboot = toggle(
        &rg,
        "Restart after successful maintenance",
        "Includes a five-minute warning with a cancel option",
        initial.reboot,
    );
    p.append(&rg);
    p.append(&label("Supported applications and containers reopen after maintenance restarts. Save your work; terminal jobs and unsaved changes may not be restored.", "caption"));
    let get: Rc<dyn Fn() -> Settings> = Rc::new({
        let (enabled, hour, minute, reboot, w60, w30, w15) = (
            enabled.clone(),
            hour.clone(),
            minute.clone(),
            reboot.clone(),
            w60.clone(),
            w30.clone(),
            w15.clone(),
        );
        move || Settings {
            enabled: enabled.is_active(),
            hour: hour.value_as_int() as u8,
            minute: minute.value_as_int() as u8,
            reboot: reboot.is_active(),
            warnings: [w60.is_active(), w30.is_active(), w15.is_active()],
        }
    });
    let changed: Rc<dyn Fn()> = Rc::new({
        let (get, baseline, footer) = (get.clone(), baseline.clone(), footer.clone());
        move || {
            let s = get();
            footer.sync(baseline.borrow().as_ref() != Some(&s), s.validate().is_ok());
        }
    });
    for s in [&enabled, &reboot, &w60, &w30, &w15] {
        let f = changed.clone();
        s.connect_active_notify(move |_| f());
    }
    for s in [&hour, &minute] {
        let f = changed.clone();
        s.connect_value_changed(move |_| f());
    }
    let saved = baseline.clone();
    footer.discard.connect_clicked(move |_| {
        if let Some(s) = saved.borrow().as_ref() {
            enabled.set_active(s.enabled);
            hour.set_value(s.hour as f64);
            minute.set_value(s.minute as f64);
            reboot.set_active(s.reboot);
            w60.set_active(s.warnings[0]);
            w30.set_active(s.warnings[1]);
            w15.set_active(s.warnings[2]);
        }
    });
    let f = footer.clone();
    let body = p.clone();
    footer.save.connect_clicked(move |_| {
        let settings = get();
        if let Err(e) = settings.validate() {
            f.error(&e);
            return;
        }
        f.saving();
        body.set_sensitive(false);
        let (f, baseline, body) = (f.clone(), baseline.clone(), body.clone());
        job(
            move || {
                crate::privileged("apply", Some(serde_json::to_string(&settings).unwrap()))?;
                Ok(settings)
            },
            move |r| {
                body.set_sensitive(true);
                match r {
                    Ok(s) => {
                        *baseline.borrow_mut() = Some(s);
                        f.success();
                    }
                    Err(e) => f.error(&e),
                }
            },
        );
    });
    stack.add_titled(&editor::shell(&p, &footer), Some("schedule"), "Maintenance");
}
