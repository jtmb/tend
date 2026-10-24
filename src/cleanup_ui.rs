use crate::workbench::{button, buttons, exports, report_hint, text_panel};
use crate::{column, editor, job, label, page, row, scroll};
use adw::prelude::*;
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
};
use tend::{cleanup_rules as rules, workstation as w};
pub fn section(win: &adw::ApplicationWindow, changed: Rc<dyn Fn()>) -> gtk::Box {
    let p = column(10);
    let group=adw::PreferencesGroup::builder().title("Additional cleanup rules").description("Rules save separately with Save rule. The page footer saves categories and retention. Saved rules join previews, manual cleanup and user cron jobs; files must meet their minimum age.").build();
    let loaded = match rules::Policy::load() {
        Ok(p) => p,
        Err(e) => {
            editor::error(
                &p,
                "Cannot load folder rules. Repair the saved file before editing or cleaning.",
                &e,
            );
            return p;
        }
    };
    let policy = Rc::new(RefCell::new(loaded));
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::Single);
    let status=label("Add a cache folder or a specific user-owned folder. Repositories, credentials and dependencies are protected.","caption");
    let bar = buttons(&p);
    let add = gtk::Button::with_label("Add rule…");
    let edit = gtk::Button::with_label("Edit selected…");
    let remove = gtk::Button::with_label("Remove selected");
    bar.insert(&add, -1);
    bar.insert(&edit, -1);
    bar.insert(&remove, -1);
    p.append(&group);
    p.append(&list);
    p.reorder_child_after(&bar, Some(&list));
    p.append(&status);
    let rebuild: Rc<dyn Fn()> = Rc::new({
        let list = list.clone();
        let policy = policy.clone();
        move || {
            while let Some(c) = list.first_child() {
                list.remove(&c);
            }
            for rule in &policy.borrow().rules {
                let r = adw::ActionRow::new();
                r.set_use_markup(false);
                r.set_title(&format!(
                    "{}{}",
                    rule.name,
                    if rule.enabled { "" } else { " · disabled" }
                ));
                r.set_subtitle(&format!(
                    "{} · {} · {} days{}",
                    rule.folder.display(),
                    rule.pattern,
                    rule.days,
                    if rule.recursive {
                        " · includes subfolders"
                    } else {
                        ""
                    }
                ));
                list.append(&r);
            }
        }
    });
    rebuild();
    let editor: Rc<dyn Fn(Option<usize>)> = Rc::new({
        let win = win.clone();
        let policy = policy.clone();
        let changed = changed.clone();
        let rebuild = rebuild.clone();
        let status = status.clone();
        move |index| {
            let old = index.and_then(|i| policy.borrow().rules.get(i).cloned());
            let d=adw::MessageDialog::new(Some(&win),Some(if old.is_some(){"Edit cleanup rule"}else{"Add cleanup rule"}),Some("Select a specific folder and filename patterns. Preview cleanup before deleting anything."));
            d.add_responses(&[("cancel", "Cancel"), ("save", "Save rule")]);
            d.set_default_response(Some("cancel"));
            d.set_close_response("cancel");
            d.set_response_appearance("save", adw::ResponseAppearance::Suggested);
            let body = column(10);
            let name = gtk::Entry::builder()
                .placeholder_text("Rule name")
                .text(
                    old.as_ref()
                        .map(|r| r.name.as_str())
                        .unwrap_or("Old cache files"),
                )
                .build();
            body.append(&name);
            let preset = gtk::DropDown::from_strings(&[
                "Custom folder",
                "Firefox cache",
                "Chrome cache",
                "GPU shader cache",
                "uv cache",
                "npm cache downloads",
            ]);
            body.append(&preset);
            let path = gtk::Entry::builder()
                .placeholder_text("Absolute folder path")
                .text(
                    old.as_ref()
                        .map(|r| r.folder.to_string_lossy().into_owned())
                        .unwrap_or_else(|| w::home().join(".cache").to_string_lossy().into_owned()),
                )
                .hexpand(true)
                .build();
            let pathbar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let choose = gtk::Button::with_label("Choose…");
            pathbar.append(&path);
            pathbar.append(&choose);
            body.append(&pathbar);
            let parent = win.clone();
            let target = path.clone();
            choose.connect_clicked(move |_| {
                let dialog = gtk::FileDialog::builder().title("Cleanup folder").build();
                let t = target.clone();
                dialog.select_folder(Some(&parent), None::<&gtk::gio::Cancellable>, move |r| {
                    if let Ok(f) = r {
                        if let Some(p) = f.path() {
                            t.set_text(&p.to_string_lossy());
                        }
                    }
                });
            });
            let pattern = gtk::Entry::builder()
                .placeholder_text("Filename patterns, e.g. *.log,*.tmp")
                .text(
                    old.as_ref()
                        .map(|r| r.pattern.as_str())
                        .unwrap_or("*.log,*.tmp"),
                )
                .build();
            body.append(&pattern);
            let settings = adw::PreferencesGroup::new();
            let days = gtk::SpinButton::with_range(1., 3650., 1.);
            days.set_value(old.as_ref().map(|r| r.days).unwrap_or(30) as f64);
            row(
                &settings,
                "Minimum age (days)",
                "Uses file modification time",
                &days,
            );
            let recurse = gtk::Switch::builder()
                .active(old.as_ref().map(|r| r.recursive).unwrap_or(false))
                .build();
            row(
                &settings,
                "Include subfolders",
                "Symlinks and other filesystems are excluded",
                &recurse,
            );
            let enabled = gtk::Switch::builder()
                .active(old.as_ref().map(|r| r.enabled).unwrap_or(true))
                .build();
            row(
                &settings,
                "Enable rule",
                "Include this rule in manual and scheduled cleanup",
                &enabled,
            );
            let selected_pattern = pattern.clone();
            let selected_recursive = recurse.clone();
            let target = path.clone();
            preset.connect_selected_notify(move |p| {
                let suffix = match p.selected() {
                    1 => ".cache/mozilla",
                    2 => ".cache/google-chrome",
                    3 => ".cache/mesa_shader_cache",
                    4 => ".cache/uv",
                    5 => ".npm/_cacache",
                    _ => return,
                };
                selected_pattern.set_text("*");
                selected_recursive.set_active(true);
                target.set_text(&w::home().join(suffix).to_string_lossy());
            });
            body.append(&settings);
            d.set_extra_child(Some(&body));
            let (policy, changed, rebuild, status) = (
                policy.clone(),
                changed.clone(),
                rebuild.clone(),
                status.clone(),
            );
            let parent = win.clone();
            d.connect_response(None, move |_, r| {
                if r != "save" {
                    return;
                }
                let rule = rules::Rule {
                    name: name.text().into(),
                    folder: PathBuf::from(path.text().as_str()),
                    pattern: pattern.text().into(),
                    days: days.value_as_int() as u32,
                    recursive: recurse.is_active(),
                    enabled: enabled.is_active(),
                };
                let mut next = policy.borrow().clone();
                if let Some(i) = index {
                    if let Some(slot) = next.rules.get_mut(i) {
                        *slot = rule;
                    }
                } else {
                    next.rules.push(rule);
                }
                let (policy,rebuild,changed,status)=(policy.clone(),rebuild.clone(),changed.clone(),status.clone());
                let error_status=status.clone();
                let save=move||match next.save() {
                    Ok(()) => { *policy.borrow_mut()=next.clone();rebuild();changed();status.set_text("Rule saved. Preview cleanup to inspect eligible files."); },
                    Err(e)=>status.set_text(&format!("Rule not saved: {e}")),
                };
                match rules::Schedule::load() {
                    Ok(s)if s.enabled=>crate::workbench::confirm(&parent,"Change an automatic cleanup rule?","Enabled user cron jobs will use this rule at their next run. Review the folder, patterns and minimum age.","Save rule",false,save),
                    Ok(_)=>save(),Err(e)=>error_status.set_text(&format!("Cannot confirm schedule state: {e}")),
                }
            });
            d.present();
        }
    });
    let e = editor.clone();
    add.connect_clicked(move |_| e(None));
    let e = editor;
    let ls = list.clone();
    edit.connect_clicked(move |_| {
        if let Some(r) = ls.selected_row() {
            e(Some(r.index() as usize));
        }
    });
    let ls = list;
    remove.connect_clicked(move |_| {
        let Some(r) = ls.selected_row() else { return };
        let mut next = policy.borrow().clone();
        let i = r.index() as usize;
        if i >= next.rules.len() {
            return;
        }
        next.rules.remove(i);
        match next.save() {
            Ok(()) => {
                *policy.borrow_mut() = next;
                rebuild();
                changed();
                status.set_text("Rule removed. Existing files were not changed.");
            }
            Err(e) => status.set_text(&e),
        }
    });
    p
}
pub fn schedule_page(stack: &gtk::Stack) {
    let p=page("Cleanup schedule","User cron jobs run the saved manual cleanup policy and folder rules. APT stays with daily maintenance. These jobs never update packages or restart the machine.");
    let loaded = match rules::Schedule::load().and_then(|s| {
        rules::validate_cron(&s.expression)?;
        Ok(s)
    }) {
        Ok(s) => s,
        Err(e) => {
            editor::error(
                &p,
                "Cannot load the saved schedule. Repair it before changing cron jobs.",
                &e,
            );
            stack.add_titled(&scroll(&p), Some("cleanup-scheduling"), "Cleanup schedule");
            return;
        }
    };
    let footer = editor::Footer::new(
        !w::home()
            .join(".config/tend/cleanup-schedule.json")
            .exists(),
        true,
        None,
    );
    let g = adw::PreferencesGroup::builder()
        .title("Cron schedule")
        .build();
    let enabled = gtk::Switch::builder().active(loaded.enabled).build();
    row(
        &g,
        "Enable scheduled cleanup",
        "Uses the saved policy on Disk cleanup",
        &enabled,
    );
    let initial = loaded.simple_time().unwrap_or((2, 10, 0, 0));
    let mode = gtk::DropDown::from_strings(&["Daily", "Weekly", "Custom cron"]);
    mode.set_selected(initial.0);
    row(
        &g,
        "Frequency",
        "Select daily, weekly or a five-field cron expression",
        &mode,
    );
    let hour = gtk::SpinButton::with_range(0., 23., 1.);
    hour.set_value(initial.1 as f64);
    let minute = gtk::SpinButton::with_range(0., 59., 1.);
    minute.set_value(initial.2 as f64);
    hour.set_tooltip_text(Some("Hour, 0–23"));
    minute.set_tooltip_text(Some("Minute, 0–59"));
    let clock = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    for spin in [&hour, &minute] {
        spin.connect_output(|s| {
            s.set_text(&format!("{:02}", s.value_as_int()));
            gtk::glib::Propagation::Stop
        });
        spin.set_text(&format!("{:02}", spin.value_as_int()));
    }
    clock.append(&hour);
    clock.append(&label(":", "body"));
    clock.append(&minute);
    row(&g, "Time", "24-hour clock · system timezone", &clock);
    let day = gtk::DropDown::from_strings(&[
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ]);
    day.set_selected(initial.3);
    row(&g, "Weekday", "Used for weekly schedules", &day);
    let expression = gtk::Entry::builder()
        .text(&loaded.expression)
        .width_chars(14)
        .build();
    row(
        &g,
        "Cron expression",
        "minute hour day month weekday",
        &expression,
    );
    let catchup = gtk::Switch::builder().active(loaded.catch_up).build();
    row(&g, "Catch up missed runs", "Check every 15 minutes; wait 20 minutes after boot, then run only the latest missed occurrence", &catchup);
    p.append(&g);
    p.append(&label("Schedules stay disabled until enabled and saved. Other cron jobs are preserved. Review the saved policy, including any whole-cache or Trash selections, before enabling.", "caption"));
    let bar = buttons(&p);
    let preview = button(&bar, "Preview schedule");
    let result = button(&bar, "Last scheduled result");
    let report = text_panel(&p);
    report_hint(
        &report,
        "Preview the schedule to review the saved cleanup policy. No jobs run from a preview.",
    );
    exports(&p, &report);
    let baseline = Rc::new(RefCell::new(loaded.clone()));
    let controls: Rc<dyn Fn() -> rules::Schedule> = Rc::new({
        let saved_expression = baseline.clone();
        let (enabled, mode, hour, minute, day, expression, catchup) = (
            enabled.clone(),
            mode.clone(),
            hour.clone(),
            minute.clone(),
            day.clone(),
            expression.clone(),
            catchup.clone(),
        );
        move || {
            let current = (
                mode.selected(),
                hour.value_as_int() as u32,
                minute.value_as_int() as u32,
                day.selected(),
            );
            rules::Schedule {
                enabled: enabled.is_active(),
                expression: if mode.selected() == 2 {
                    expression.text().to_string()
                } else if saved_expression.borrow().simple_time() == Some(current) {
                    saved_expression.borrow().expression.clone()
                } else if mode.selected() == 0 {
                    format!("{} {} * * *", minute.value_as_int(), hour.value_as_int())
                } else {
                    format!(
                        "{} {} * * {}",
                        minute.value_as_int(),
                        hour.value_as_int(),
                        day.selected()
                    )
                },
                catch_up: catchup.is_active(),
            }
        }
    });
    let revision = Rc::new(Cell::new(0u64));
    let update: Rc<dyn Fn()> = Rc::new({
        let (revision, report) = (revision.clone(), report.clone());
        let (mode, hour, minute, day, expression, get, baseline, footer) = (
            mode.clone(),
            hour.clone(),
            minute.clone(),
            day.clone(),
            expression.clone(),
            controls.clone(),
            baseline.clone(),
            footer.clone(),
        );
        move || {
            revision.set(revision.get().wrapping_add(1));
            report_hint(&report, "Preview this draft to review the saved cleanup policy. No jobs run from a preview.");
            let selected = mode.selected();
            hour.set_sensitive(selected != 2);
            minute.set_sensitive(selected != 2);
            day.set_sensitive(selected == 1);
            expression.set_sensitive(selected == 2);
            let current = get();
            let validation = rules::validate_cron(&current.expression);
            if let Err(error) = &validation {
                expression.add_css_class("error");
                expression.set_tooltip_text(Some(error));
            } else {
                expression.remove_css_class("error");
                expression.set_tooltip_text(Some("Five-field cron expression"));
            }
            footer.sync(*baseline.borrow() != current, validation.is_ok());
        }
    });
    update();
    for d in [&mode, &day] {
        let f = update.clone();
        d.connect_selected_notify(move |_| f());
    }
    for n in [&hour, &minute] {
        let f = update.clone();
        n.connect_value_changed(move |_| f());
    }
    for c in [&enabled, &catchup] {
        let f = update.clone();
        c.connect_active_notify(move |_| f());
    }
    let f = update.clone();
    expression.connect_changed(move |_| f());
    let saved = baseline.clone();
    footer.discard.connect_clicked(move |_| {
        let s = saved.borrow();
        let simple = s.simple_time().unwrap_or((2, 10, 0, 0));
        enabled.set_active(s.enabled);
        hour.set_value(simple.1 as f64);
        minute.set_value(simple.2 as f64);
        day.set_selected(simple.3);
        expression.set_text(&s.expression);
        catchup.set_active(s.catch_up);
        mode.set_selected(simple.0);
        update();
    });
    let (get, buffer) = (controls.clone(), report.buffer());
    preview.connect_clicked(move |b| {
        let generation = revision.get();
        let revision = revision.clone();
        let s = get();
        b.set_sensitive(false);
        buffer.set_text("Working…");
        let (b, buffer) = (b.clone(), buffer.clone());
        job(
            move || {
                rules::validate_cron(&s.expression)?;
                Ok(format!(
                    "Cron: {}\nEnabled: {} · catch up: {}\n\n{}\n\n{}",
                    s.expression,
                    s.enabled,
                    s.catch_up,
                    rules::scheduled_preview()?,
                    rules::preview(&rules::Policy::load()?)?
                ))
            },
            move |r| {
                b.set_sensitive(true);
                if generation != revision.get() { buffer.set_text("Schedule changed while the preview was running. Preview again to review this draft."); return; }
                buffer.set_text(&r.unwrap_or_else(|e| format!("Preview failed: {e}")));
            },
        );
    });
    let (f, body) = (footer.clone(), p.clone());
    footer.save.connect_clicked(move |_| {
        let s = controls();
        f.saving();
        body.set_sensitive(false);
        let (f, saved, body) = (f.clone(), baseline.clone(), body.clone());
        job(
            move || {
                #[cfg(feature = "docs-demo")]
                if std::env::var_os("TEND_DEMO_DIR").is_some() {
                    return Err("Schedule writes are disabled in isolated previews.".into());
                }
                s.save()?;
                Ok(s)
            },
            move |r| {
                body.set_sensitive(true);
                match r {
                    Ok(s) => {
                        *saved.borrow_mut() = s;
                        f.success();
                    }
                    Err(e) => f.error(&e),
                }
            },
        );
    });
    result.connect_clicked(move |_| {
        let saved = std::fs::read_to_string(w::state_dir().join("scheduled-cleanup.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
        report.buffer().set_text(
            &saved
                .map(|v| {
                    format!(
                        "Last run: {}\n\n{}",
                        v["finished_unix"]
                            .as_i64()
                            .map(tend::presentation::when)
                            .unwrap_or_else(|| "Unavailable".into()),
                        v["report"].as_str().unwrap_or("No report available")
                    )
                })
                .unwrap_or_else(|| "No scheduled cleanup has run yet.".into()),
        );
    });
    stack.add_titled(
        &editor::shell(&p, &footer),
        Some("cleanup-scheduling"),
        "Cleanup schedule",
    );
}
