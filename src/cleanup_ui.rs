use crate::{column, job, label, page, row, scroll};
use adw::prelude::*;
use std::{cell::RefCell, path::PathBuf, rc::Rc};
use tend::{cleanup_rules as rules, workstation as w};
pub fn section(win: &adw::ApplicationWindow, changed: Rc<dyn Fn()>) -> gtk::Box {
    let p = column(10);
    let group=adw::PreferencesGroup::builder().title("Additional cleanup rules").description("Saved rules are included in cleanup previews, manual cleanup and user cron jobs. Files are permanently removed only after their minimum age.").build();
    let policy = Rc::new(RefCell::new(rules::Policy::load().unwrap_or_default()));
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::Single);
    let status=label("Add a cache folder or a specific user-owned folder. Repositories, credentials and dependencies are protected.","caption");
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let add = gtk::Button::with_label("Add rule…");
    let edit = gtk::Button::with_label("Edit selected…");
    let remove = gtk::Button::with_label("Remove selected");
    bar.append(&add);
    bar.append(&edit);
    bar.append(&remove);
    p.append(&group);
    p.append(&list);
    p.append(&bar);
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
                match next.save() {
                    Ok(()) => {
                        *policy.borrow_mut() = next;
                        rebuild();
                        changed();
                        status.set_text("Rule saved. Preview cleanup to inspect eligible files.");
                    }
                    Err(e) => status.set_text(&format!("Rule not saved: {e}")),
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
    let p=page("Cleanup scheduling","User cron jobs run saved Docker/cache selections and additional cleanup rules. APT stays with the existing system maintenance job. These jobs never install updates or request restarts.");
    let loaded = rules::Schedule::load().unwrap_or_default();
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
    let mode = gtk::DropDown::from_strings(&["Daily", "Weekly", "Custom cron"]);
    mode.set_selected(2);
    row(
        &g,
        "Frequency",
        "Select daily, weekly or a five-field cron expression",
        &mode,
    );
    let hour = gtk::SpinButton::with_range(0., 23., 1.);
    hour.set_value(10.);
    let minute = gtk::SpinButton::with_range(0., 59., 1.);
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
    row(&g, "Weekday", "Used for weekly schedules", &day);
    let expression = gtk::Entry::builder()
        .text(&loaded.expression)
        .hexpand(true)
        .build();
    row(
        &g,
        "Cron expression",
        "minute hour day month weekday",
        &expression,
    );
    let catchup = gtk::Switch::builder().active(loaded.catch_up).build();
    row(
        &g,
        "Catch up missed runs",
        "Check every 15 minutes; allow 20 minutes after boot, then run only the latest missed occurrence",
        &catchup,
    );
    p.append(&g);
    let note=label("Schedules are disabled until you enable and save them. Other cron jobs are preserved. Review saved cleanup selections, including Trash, before enabling automatic deletion.","caption");
    p.append(&note);
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let preview = gtk::Button::with_label("Preview schedule");
    let save = gtk::Button::with_label("Save schedule");
    save.add_css_class("suggested-action");
    let result = gtk::Button::with_label("Last scheduled result");
    bar.append(&preview);
    bar.append(&save);
    bar.append(&result);
    p.append(&bar);
    let status = label("No schedule changes applied.", "body");
    status.set_selectable(true);
    p.append(&status);
    let update: Rc<dyn Fn()> = Rc::new({
        let mode = mode.clone();
        let hour = hour.clone();
        let minute = minute.clone();
        let day = day.clone();
        let expression = expression.clone();
        move || {
            let selected = mode.selected();
            hour.set_sensitive(selected != 2);
            minute.set_sensitive(selected != 2);
            day.set_sensitive(selected == 1);
            expression.set_sensitive(selected == 2);
        }
    });
    update();
    mode.connect_selected_notify(move |_| update());
    let controls: Rc<dyn Fn() -> rules::Schedule> = Rc::new(move || rules::Schedule {
        enabled: enabled.is_active(),
        expression: match mode.selected() {
            0 => format!("{} {} * * *", minute.value_as_int(), hour.value_as_int()),
            1 => format!(
                "{} {} * * {}",
                minute.value_as_int(),
                hour.value_as_int(),
                day.selected()
            ),
            _ => expression.text().to_string(),
        },
        catch_up: catchup.is_active(),
    });
    let get = controls.clone();
    let out = status.clone();
    preview.connect_clicked(move |b| {
        let s = get();
        let out = out.clone();
        b.set_sensitive(false);
        let b = b.clone();
        job(
            move || {
                rules::validate_cron(&s.expression)?;
                Ok(format!(
                    "{}\n{}\n{}",
                    s.expression,
                    if s.enabled {
                        "Will enable user cleanup jobs"
                    } else {
                        "Will disable user cleanup jobs"
                    },
                    format!(
                        "{}\n{}",
                        w::cleanup_preview(&w::Cleanup::load())?,
                        rules::preview(&rules::Policy::load()?)?
                    )
                ))
            },
            move |r| {
                b.set_sensitive(true);
                out.set_text(&r.unwrap_or_else(|e| e));
            },
        );
    });
    let out = status.clone();
    save.connect_clicked(move |b| {
        b.set_sensitive(false);
        let s = controls();
        let (b, out) = (b.clone(), out.clone());
        job(
            move || s.save(),
            move |r| {
                b.set_sensitive(true);
                out.set_text(&r.unwrap_or_else(|e| e));
            },
        );
    });
    result.connect_clicked(move |_| {
        let result = std::fs::read_to_string(w::state_dir().join("scheduled-cleanup.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
        status.set_text(
            &result
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
        &scroll(&p),
        Some("cleanup-scheduling"),
        "Cleanup scheduling",
    );
}
