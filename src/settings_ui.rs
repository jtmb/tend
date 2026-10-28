//! Native, section-by-section settings transfer. Loading a draft performs no
//! system operation. Each review uses typed controls and its own explicit Save.
use crate::{editor, job, label, row};
use adw::prelude::*;
use gtk::{gio, glib};
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};
use tend::{
    settings_bundle::{Appearance, Bundle, Exclusions, Maintenance, Section},
};
type Refresh = Rc<dyn Fn(&Value)>;
struct View {
    pending: Rc<dyn Fn() -> bool>,
    refresh: Refresh,
}
thread_local! {static VIEWS:RefCell<BTreeMap<Section,View>>=const{RefCell::new(BTreeMap::new())}; static INVALIDATORS:RefCell<BTreeMap<Section,Rc<dyn Fn()>>>=const{RefCell::new(BTreeMap::new())};}
pub fn register(
    section: Section,
    pending: impl Fn() -> bool + 'static,
    refresh: impl Fn(&Value) + 'static,
) {
    VIEWS.with(|v| {
        v.borrow_mut().insert(
            section,
            View {
                pending: Rc::new(pending),
                refresh: Rc::new(refresh),
            },
        );
    });
}
pub fn shutdown() {
    VIEWS.with(|v| v.borrow_mut().clear());
    INVALIDATORS.with(|v| v.borrow_mut().clear());
}
pub fn invalidator(section: Section, callback: Rc<dyn Fn()>) {
    INVALIDATORS.with(|v| {
        v.borrow_mut().insert(section, callback);
    });
}
fn pending(section: Section) -> bool {
    VIEWS.with(|v| v.borrow().get(&section).is_some_and(|v| (v.pending)()))
}
fn refresh(section: Section, value: &Value) {
    let f = VIEWS.with(|v| v.borrow().get(&section).map(|v| v.refresh.clone()));
    if let Some(f) = f {
        f(value);
    }
    if section == Section::Exclusions {
        for dependent in [Section::ManualCleanup, Section::DailyCleanup] {
            let invalidate = INVALIDATORS.with(|v| v.borrow().get(&dependent).cloned());
            if let Some(invalidate) = invalidate {
                invalidate();
            }
            let f = VIEWS.with(|v| v.borrow().get(&dependent).map(|v| v.refresh.clone()));
            if let (Some(f), Ok(current)) = (f, dependent.current()) {
                if !pending(dependent) {
                    f(&current);
                }
            }
        }
    }
}
fn named(path: &[Key]) -> String {
    let words = path
        .iter()
        .filter_map(|k| match k {
            Key::Name(s) => Some(s.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let key = words.last().copied().unwrap_or("Setting");
    let name = match key {
        "enabled" => "Enabled",
        "threshold" => match words.first().copied().unwrap_or("") {
            "memory" => "Alert below (% available)",
            "disk" => "Alert below (GiB free)",
            "cpu" => "Alert at or above (% CPU)",
            _ => "Alert at or above (% time stalled)",
        },
        "cpu" => "CPU use",
        "memory" => "Available memory",
        "disk" => "Root free space",
        "memory_pressure" => "Memory stalls",
        "io_pressure" => "I/O stalls",
        "hold_seconds" => "Sustained duration (seconds)",
        "cooldown_seconds" => "Repeat cooldown (seconds)",
        "recovery" => "Notify on recovery",
        "crashes" => "Check new crash reports",
        "max_mib" => "History budget (MiB)",
        "retention_days" => "Retention (days)",
        "theme" => "Color scheme",
        "close_to_panel" => "Close to top panel",
        "projects" => "Project folders",
        "name" => "Name",
        "root" => "Project folder",
        "folder" => "Cleanup folder",
        "pattern" => "Filename patterns",
        "days" => "Minimum age (days)",
        "recursive" => "Include subfolders",
        "expression" => "Cron expression",
        "catch_up" => "Catch up missed runs",
        "warnings" => "Advance reminders",
        "hour" => "Hour (0–23)",
        "minute" => "Minute (0–59)",
        "paths" => "Protected paths",
        "path" => "Protected path",
        "note" => "Note",
        "rules" => "Cleanup rules",
        "apt" => "Obsolete APT downloads",
        "docker_build" => "Unused Docker build cache",
        "docker_images" => "Dangling Docker images",
        "npm_logs" => "Old npm diagnostic logs",
        "npm_verify" => "Verify npm cache",
        "pip" => "Pip download cache",
        "thumbnails" => "Old thumbnails",
        "trash" => "Empty Trash",
        "age_days" | "build_days" => "Build cache age (days)",
        "image_days" => "Image age (days)",
        "thumbnail_days" => "Thumbnail age (days)",
        "keep_gb" => "Retain build cache (GB)",
        "npm_log_days" => "npm log age (days)",
        "npm_verify_days" => "npm verification interval (days)",
        "id" => "Project identifier",
        _ => key,
    };
    name.into()
}
#[derive(Clone)]
enum Key {
    Name(String),
    Index(usize),
}
fn get<'a>(value: &'a Value, path: &[Key]) -> &'a Value {
    let mut v = value;
    for k in path {
        v = match k {
            Key::Name(s) => &v[s],
            Key::Index(i) => &v[*i],
        };
    }
    v
}
fn put(value: &mut Value, path: &[Key], next: Value) {
    let mut v = value;
    for k in path {
        let child = match k {
            Key::Name(s) => v.get_mut(s),
            Key::Index(i) => v.get_mut(*i),
        };
        let Some(child) = child else {
            return;
        };
        v = child;
    }
    *v = next;
}
type Change = Rc<dyn Fn()>;
type Handlers = Rc<RefCell<Vec<(glib::WeakRef<glib::Object>, glib::SignalHandlerId)>>>;
fn track<T: IsA<glib::Object>>(handlers: &Handlers, object: &T, id: glib::SignalHandlerId) {
    let object: &glib::Object = object.as_ref();
    let mut h = handlers.borrow_mut();
    h.retain(|(w, _)| w.upgrade().is_some());
    h.push((object.downgrade(), id));
}
fn release(dialog: &adw::Dialog, handlers: Handlers) {
    dialog.connect_closed(move |_| {
        for (weak, id) in handlers.borrow_mut().drain(..) {
            if let Some(object) = weak.upgrade() {
                object.disconnect(id);
            }
        }
    });
}
fn form(
    parent: &gtk::Box,
    draft: &Rc<RefCell<Value>>,
    path: Vec<Key>,
    changed: &Change,
    handlers: &Handlers,
    epoch: &Rc<Cell<u64>>,
) {
    let value = get(&draft.borrow(), &path).clone();
    match value {
        Value::Object(map) => {
            let group = adw::PreferencesGroup::new();
            let mut children = vec![];
            let mut fields = 0;
            let mut fields_order = map.into_iter().collect::<Vec<_>>();
            fields_order.sort_by_key(|(key, _)| match key.as_str() {
                "enabled" | "theme" | "max_mib" => 0,
                "name" | "close_to_panel" | "retention_days" => 1,
                "root" | "folder" | "path" | "hour" => 2,
                "pattern" | "minute" => 3,
                "days" => 4,
                "recursive" => 5,
                _ => 10,
            });
            for (k, v) in fields_order {
                if k == "version" || k == "id" {
                    continue;
                }
                let mut p = path.clone();
                p.push(Key::Name(k.clone()));
                if !matches!(&v, Value::Array(_) | Value::Object(_) | Value::Null) {
                    fields += 1;
                }
                match v {
                    Value::Bool(b) => {
                        let control = gtk::Switch::builder().active(b).build();
                        control.set_widget_name(&format!("bundle-{}", path_key(&p)));
                        row(&group, &named(&p), "", &control);
                        let (d, p, f) = (draft.clone(), p.clone(), changed.clone());
                        let generation = epoch.clone();
                        let token = epoch.get();
                        let signal = control.connect_active_notify(move |c| {
                            if generation.get() != token {
                                return;
                            }
                            put(&mut d.borrow_mut(), &p, c.is_active().into());
                            f();
                        });
                        track(&handlers, &control, signal);
                    }
                    Value::Number(n) => {
                        if k == "theme" {
                            let c =
                                gtk::DropDown::from_strings(&["Follow system", "Light", "Dark"]);
                            c.set_selected(n.as_u64().unwrap_or(0) as u32);
                            row(&group, &named(&p), "", &c);
                            let (d, p, f) = (draft.clone(), p.clone(), changed.clone());
                            let generation = epoch.clone();
                            let token = epoch.get();
                            let signal = c.connect_selected_notify(move |c| {
                                if generation.get() != token {
                                    return;
                                }
                                put(&mut d.borrow_mut(), &p, c.selected().into());
                                f();
                            });
                            track(&handlers, &c, signal);
                            continue;
                        }
                        let (min, max, step) = range(&p);
                        let c = gtk::SpinButton::with_range(min, max, step);
                        c.set_digits(if step < 1. { 1 } else { 0 });
                        c.set_value(n.as_f64().unwrap_or(0.));
                        c.set_widget_name(&format!("bundle-{}", path_key(&p)));
                        c.update_property(&[gtk::accessible::Property::Label(&named(&p))]);
                        row(&group, &named(&p), "", &c);
                        let (d, p, f) = (draft.clone(), p.clone(), changed.clone());
                        let generation = epoch.clone();
                        let token = epoch.get();
                        let signal = c.connect_value_changed(move |c| {
                            if generation.get() != token {
                                return;
                            }
                            put(
                                &mut d.borrow_mut(),
                                &p,
                                if step < 1. {
                                    json!(c.value())
                                } else {
                                    json!(c.value_as_int())
                                },
                            );
                            f();
                        });
                        track(&handlers, &c, signal);
                    }
                    Value::String(text) => {
                        let c = adw::EntryRow::builder().title(named(&p)).text(text).build();
                        c.set_widget_name(&format!("bundle-{}", path_key(&p)));
                        group.add(&c);
                        let (d, p, f) = (draft.clone(), p.clone(), changed.clone());
                        let generation = epoch.clone();
                        let token = epoch.get();
                        let signal = c.connect_changed(move |c| {
                            if generation.get() != token {
                                return;
                            }
                            put(&mut d.borrow_mut(), &p, c.text().to_string().into());
                            f();
                        });
                        track(&handlers, &c, signal);
                    }
                    _ => children.push(p),
                }
            }
            if fields > 0 {
                parent.append(&group);
            }
            for p in children {
                let box_ = crate::column(12);
                box_.append(&label(&named(&p), "title-4"));
                form(&box_, draft, p, changed, handlers, epoch);
                parent.append(&box_);
            }
        }
        Value::Array(values) => {
            if path_key(&path) == "warnings" {
                let group = adw::PreferencesGroup::new();
                for (i, v) in values.into_iter().enumerate() {
                    let mut p = path.clone();
                    p.push(Key::Index(i));
                    let c = gtk::Switch::builder()
                        .active(v.as_bool().unwrap_or(false))
                        .build();
                    row(
                        &group,
                        [
                            "One hour before",
                            "Thirty minutes before",
                            "Fifteen minutes before",
                        ][i],
                        "",
                        &c,
                    );
                    let (d, f) = (draft.clone(), changed.clone());
                    let generation = epoch.clone();
                    let token = epoch.get();
                    let signal = c.connect_active_notify(move |c| {
                        if generation.get() != token {
                            return;
                        }
                        put(&mut d.borrow_mut(), &p, c.is_active().into());
                        f();
                    });
                    track(&handlers, &c, signal);
                }
                parent.append(&group);
                return;
            }
            let bar = crate::workbench::buttons(parent);
            let choose = gtk::DropDown::new(None::<gio::ListModel>, None::<gtk::Expression>);
            choose.set_hexpand(true);
            bar.insert(&choose, -1);
            let add = gtk::Button::with_label("Add item");
            let remove = gtk::Button::with_label("Remove from draft");
            bar.insert(&add, -1);
            bar.insert(&remove, -1);
            let item = crate::column(12);
            parent.append(&item);
            let render: Rc<dyn Fn()> = Rc::new({
                let (draft, path, item, choose, remove, changed, handlers, epoch) = (
                    draft.clone(),
                    path.clone(),
                    item.clone(),
                    choose.clone(),
                    remove.clone(),
                    changed.clone(),
                    handlers.clone(),
                    epoch.clone(),
                );
                move || {
                    epoch.set(epoch.get().wrapping_add(1));
                    while let Some(c) = item.first_child() {
                        item.remove(&c);
                    }
                    let values = get(&draft.borrow(), &path)
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    let i = choose.selected() as usize;
                    remove.set_sensitive(i < values.len());
                    if i < values.len() {
                        let mut p = path.clone();
                        p.push(Key::Index(i));
                        form(&item, &draft, p, &changed, &handlers, &epoch);
                    } else {
                        item.append(&label("No items in this draft.", "caption"));
                    }
                }
            });
            let rebuild: Rc<dyn Fn()> = Rc::new({
                let (draft, path, choose, render, add) = (
                    draft.clone(),
                    path.clone(),
                    choose.clone(),
                    render.clone(),
                    add.clone(),
                );
                move || {
                    let values = get(&draft.borrow(), &path)
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    let labels = values
                        .iter()
                        .enumerate()
                        .map(|(i, v)| {
                            v.get("name")
                                .or_else(|| v.get("path"))
                                .and_then(Value::as_str)
                                .filter(|s| !s.is_empty())
                                .map(|text| {
                                    format!(
                                        "{}. {}{}",
                                        i + 1,
                                        text.chars().take(40).collect::<String>(),
                                        if text.chars().count() > 40 { "…" } else { "" }
                                    )
                                })
                                .unwrap_or_else(|| format!("Item {}", i + 1))
                        })
                        .collect::<Vec<_>>();
                    let refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
                    choose.set_model(Some(&gtk::StringList::new(&refs)));
                    add.set_sensitive(
                        values.len()
                            < if path_key(&path) == "projects" {
                                50
                            } else {
                                64
                            },
                    );
                    render();
                }
            });
            let r = render.clone();
            let signal = choose.connect_selected_notify(move |_| r());
            track(&handlers, &choose, signal);
            rebuild();
            let (d, p, f, r) = (
                draft.clone(),
                path.clone(),
                changed.clone(),
                rebuild.clone(),
            );
            let selected = choose.clone();
            let signal=add.connect_clicked(move |_|{let mut v=d.borrow_mut();let list=get(&v,&p).as_array().cloned().unwrap_or_default();let mut list=list;if list.len()>=64{return;}let next=match path_key(&p).as_str(){"projects"=>json!({"id":format!("workspace-{}-{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos(),std::process::id()),"name":"","root":""}),"paths"=>json!({"path":"","enabled":true,"note":""}),_=>json!({"name":"","folder":"","pattern":"*.cache","days":30,"recursive":false,"enabled":false})};list.push(next);let selected_index=list.len().saturating_sub(1) as u32;put(&mut v,&p,list.into());drop(v);r();selected.set_selected(selected_index);f();});
            track(&handlers, &add, signal);
            let (d, p, f, r, c) = (
                draft.clone(),
                path.clone(),
                changed.clone(),
                rebuild,
                choose.clone(),
            );
            let signal = remove.connect_clicked(move |_| {
                let mut v = d.borrow_mut();
                let mut list = get(&v, &p).as_array().cloned().unwrap_or_default();
                let i = c.selected() as usize;
                if i < list.len() {
                    list.remove(i);
                }
                put(&mut v, &p, list.into());
                drop(v);
                r();
                f();
            });
            track(&handlers, &remove, signal);
        }
        _ => {}
    }
}
fn field_title(field: &str) -> String {
    if let Some(index) = field.strip_prefix("warnings.") {
        return match index {
            "1" => "Reminder · one hour before",
            "2" => "Reminder · thirty minutes before",
            "3" => "Reminder · fifteen minutes before",
            _ => "Reminder",
        }
        .into();
    }
    let mut path = vec![];
    let mut labels = vec![];
    for part in field.split('.') {
        if let Ok(index) = part.parse::<usize>() {
            path.push(Key::Index(index.saturating_sub(1)));
            labels.push(format!("Item {index}"));
        } else {
            path.push(Key::Name(part.into()));
            labels.push(named(&path));
        }
    }
    labels.join(" › ")
}
fn path_key(path: &[Key]) -> String {
    path.iter()
        .map(|k| match k {
            Key::Name(s) => s.clone(),
            Key::Index(i) => i.to_string(),
        })
        .collect::<Vec<_>>()
        .join("-")
}
fn range(path: &[Key]) -> (f64, f64, f64) {
    let k = path_key(path);
    let field = k.rsplit('-').next().unwrap_or("");
    match field {
        "threshold" => {
            if k.starts_with("disk") {
                (1., 1024., 0.1)
            } else if k.starts_with("memory_pressure") || k.starts_with("io_pressure") {
                (0.1, 100., 0.1)
            } else if k.starts_with("memory") {
                (1., 90., 0.1)
            } else {
                (1., 100., 0.1)
            }
        }
        "max_mib" => (128., 2048., 64.),
        "retention_days" => (1., 365., 1.),
        "hold_seconds" => (30., 3600., 15.),
        "cooldown_seconds" => (60., 86400., 60.),
        "hour" => (0., 23., 1.),
        "minute" => (0., 59., 1.),
        "keep_gb" => (1., 100., 1.),
        "npm_verify_days" => (1., 30., 1.),
        "days" | "npm_log_days" | "thumbnail_days" => (1., 3650., 1.),
        _ => (7., 3650., 1.),
    }
}
fn apply(section: Section, value: &Value, expected: &Value) -> Result<(), String> {
    let value = section.canonical(value)?;
    if section.current()? != *expected {
        return Err("Saved settings changed while this review was open. Close it and review again; nothing was saved.".into());
    }
    match section {
        Section::Appearance => serde_json::from_value::<Appearance>(value)
            .map_err(|e| e.to_string())?
            .save_checked(
                &serde_json::from_value::<Appearance>(expected.clone())
                    .map_err(|e| e.to_string())?,
            ),
        Section::Monitoring
        | Section::History
        | Section::Workspaces
        | Section::ManualCleanup
        | Section::FolderRules => section.save_user(&value, expected),
        Section::UserSchedule => {
            #[cfg(feature = "docs-demo")]
            if std::env::var_os("TEND_DEMO_DIR").is_some() {
                return Err("Cron writes are disabled in this isolated preview.".into());
            }
            section.save_user(&value, expected)
        }
        Section::DailyCleanup => {
            crate::privileged("daily-policy", Some(value.to_string())).map(|_| ())
        }
        Section::Maintenance => {
            let draft: Maintenance = serde_json::from_value(value).map_err(|e| e.to_string())?;
            let current = tend::Settings::read()?;
            let s = tend::Settings {
                enabled: draft.enabled,
                hour: draft.hour,
                minute: draft.minute,
                warnings: draft.warnings,
                reboot: current.reboot,
            };
            crate::privileged(
                "apply",
                Some(serde_json::to_string(&s).map_err(|e| e.to_string())?),
            )
            .map(|_| ())
        }
        Section::Exclusions => {
            let d: Exclusions = serde_json::from_value(value).map_err(|e| e.to_string())?;
            let mut current = tend::exclusions::Policy::read()?;
            current.paths = d.paths;
            current.compile()?;
            crate::privileged(
                "exclusions-save",
                Some(serde_json::to_string(&current).map_err(|e| e.to_string())?),
            )
            .map(|_| ())
        }
    }
}
fn availability(section: Section) -> Option<String> {
    if !matches!(
        section,
        Section::DailyCleanup | Section::Maintenance | Section::Exclusions
    ) {
        return None;
    }
    #[cfg(feature = "docs-demo")]
    if std::env::var_os("TEND_DEMO_DIR").is_some() {
        return Some("System saves are disabled in this isolated preview.".into());
    }
    if section == Section::DailyCleanup && !tend::daily_policy::Policy::ready() {
        return Some("Matching system integration is required before saving this policy.".into());
    }
    if section == Section::Exclusions && !tend::exclusions::integration_supported() {
        return Some("Matching administrator, workers and daily adapter are required before saving shared exclusions.".into());
    }
    crate::administrator_available().err()
}
fn review(
    parent: &adw::ApplicationWindow,
    app: &adw::Application,
    section: Section,
    current: Value,
    imported: Value,
    unavailable: Option<String>,
) {
    let body=crate::page(section.title(),"Imported draft · changes take effect only after Save. Scheduled jobs and monitoring start disabled in imported drafts.");
    if let Some(title) = body.first_child() {
        body.remove(&title);
    }
    let warning=match section{
        Section::ManualCleanup|Section::FolderRules=>"Saving changes the policy used by any existing user cleanup jobs. No cleanup runs from this editor.",
        Section::DailyCleanup=>"Saving changes the next existing maintenance run. It does not start maintenance.",
        Section::Maintenance=>"Existing after-maintenance behavior is preserved. Restart, login and authentication policies are never imported.",
        Section::Exclusions=>"Saving replaces shared protection. Review every path that will lose protection; an in-progress cleanup must finish first.",
        _=>"",};
    if !warning.is_empty() {
        body.append(&label(warning, "caption"));
    }
    let handlers: Handlers = Rc::new(RefCell::new(vec![]));

    let draft = Rc::new(RefCell::new(imported));
    let baseline = Rc::new(RefCell::new(current));
    let footer = editor::Footer::new(false, true, unavailable.as_deref());
    let changed: Change = Rc::new({
        let (d, b, f) = (draft.clone(), baseline.clone(), footer.clone());
        move || {
            let value = d.borrow().clone();
            f.sync(value != *b.borrow(), section.canonical(&value).is_ok());
        }
    });
    let epoch = Rc::new(Cell::new(0u64));
    let form_box = crate::column(16);
    form(&form_box, &draft, vec![], &changed, &handlers, &epoch);
    body.append(&form_box);
    changed();
    let (f, d, b, app) = (footer.clone(), draft.clone(), baseline.clone(), app.clone());
    let busybody = body.clone();
    let signal = footer.save.connect_clicked(move |_| {
        let value = d.borrow().clone();
        let expected = b.borrow().clone();
        let (f, b, d, body, app) = (
            f.clone(),
            b.clone(),
            d.clone(),
            busybody.clone(),
            app.clone(),
        );
        f.saving();
        body.set_sensitive(false);
        let saved = value.clone();
        job(
            move || apply(section, &value, &expected),
            move |result| {
                body.set_sensitive(true);
                match result {
                    Ok(()) => {
                        *b.borrow_mut() = saved.clone();
                        let pending = d.borrow().clone();
                        f.success();
                        f.sync(pending != saved, section.canonical(&pending).is_ok());
                        refresh(section, &saved);
                        if section == Section::Monitoring {
                            crate::health_ui::reload(&app);
                        }
                        if section == Section::Appearance {
                            if let Ok(a) = serde_json::from_value::<Appearance>(saved) {
                                adw::StyleManager::default().set_color_scheme(match a.theme {
                                    1 => adw::ColorScheme::ForceLight,
                                    2 => adw::ColorScheme::ForceDark,
                                    _ => adw::ColorScheme::Default,
                                });
                            }
                        }
                    }
                    Err(e) => {f.error(&e);if section==Section::Appearance&&e.starts_with("Some appearance preferences changed"){f.notice.show("Appearance was partly changed. Review the saved values before retrying.",Some(&e),true);}},
                }
            },
        );
    });
    track(&handlers, &footer.save, signal);
    let (f, d, b, form_box, changed) = (
        footer.clone(),
        draft.clone(),
        baseline.clone(),
        form_box.clone(),
        changed.clone(),
    );
    let h = handlers.clone();
    let reset_epoch = epoch.clone();
    let signal = footer.discard.connect_clicked(move |_| {
        reset_epoch.set(reset_epoch.get().wrapping_add(1));
        *d.borrow_mut() = b.borrow().clone();

        while let Some(w) = form_box.first_child() {
            form_box.remove(&w);
        }
        form(&form_box, &d, vec![], &changed, &h, &reset_epoch);
        f.sync(false, true);
    });
    track(&handlers, &footer.discard, signal);
    let dialog = dialog(
        parent,
        section.title(),
        &editor::shell(&body, &footer),
        720,
        700,
    );
    release(&dialog, handlers);
    dialog.present(Some(parent));
}
fn dialog(
    _parent: &adw::ApplicationWindow,
    title: &str,
    content: &impl IsA<gtk::Widget>,
    width: i32,
    height: i32,
) -> adw::Dialog {
    let toolbar = adw::ToolbarView::new();
    toolbar.set_content(Some(content));
    let header = adw::HeaderBar::new();
    header.set_show_start_title_buttons(false);
    toolbar.add_top_bar(&header);
    adw::Dialog::builder()
        .title(title)
        .content_width(width)
        .content_height(height)
        .child(&toolbar)
        .build()
}
#[cfg(feature = "docs-demo")]
thread_local! {static PICKER:RefCell<std::collections::VecDeque<Option<std::path::PathBuf>>>=const{RefCell::new(std::collections::VecDeque::new())};}
#[cfg(feature = "docs-demo")]
pub fn qa_pick(path: std::path::PathBuf) {
    assert!(
        std::env::var_os("TEND_BUNDLE_QA").is_some() && std::env::var_os("TEND_DEMO_DIR").is_some()
    );
    let home = tend::workstation::home();
    assert!(
        path.starts_with(home.parent().unwrap())
            && !path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
    );
    PICKER.with(|q| q.borrow_mut().push_back(Some(path)));
}
#[cfg(feature = "docs-demo")]
pub fn qa_cancel() {
    assert!(
        std::env::var_os("TEND_BUNDLE_QA").is_some() && std::env::var_os("TEND_DEMO_DIR").is_some()
    );
    PICKER.with(|q| q.borrow_mut().push_back(None));
}
fn pick(
    parent: &adw::ApplicationWindow,
    save: bool,
    done: impl FnOnce(Result<Option<std::path::PathBuf>, String>) + 'static,
) {
    #[cfg(feature = "docs-demo")]
    if let Some(path) = PICKER.with(|q| q.borrow_mut().pop_front()) {
        glib::idle_add_local_once(move || done(Ok(path)));
        return;
    }
    let dialog = gtk::FileDialog::builder()
        .title(if save {
            "Export Tend settings"
        } else {
            "Import Tend settings"
        })
        .initial_name("tend-settings.json")
        .build();
    let callback = move |result: Result<gio::File, glib::Error>| {
        done(match result {
            Ok(file) => file
                .path()
                .map(|path| Some(path))
                .ok_or_else(|| "Choose a local settings file.".into()),
            Err(e)
                if e.matches(gtk::DialogError::Dismissed)
                    || e.matches(gio::IOErrorEnum::Cancelled) =>
            {
                Ok(None)
            }
            Err(e) => Err(format!("File chooser unavailable: {e}")),
        });
    };
    if save {
        dialog.save(Some(parent), None::<&gio::Cancellable>, callback);
    } else {
        dialog.open(Some(parent), None::<&gio::Cancellable>, callback);
    }
}
struct Operation {
    busy: Cell<bool>,
    buttons: Vec<glib::WeakRef<gtk::Button>>,
}
impl Operation {
    fn start(&self) -> bool {
        if self.busy.replace(true) {
            return false;
        }
        for weak in &self.buttons {
            if let Some(b) = weak.upgrade() {
                b.set_sensitive(false);
            }
        }
        true
    }
    fn finish(&self) {
        self.busy.set(false);
        for weak in &self.buttons {
            if let Some(b) = weak.upgrade() {
                b.set_sensitive(true);
            }
        }
    }
}
pub fn add(
    parent: &gtk::Box,
    win: &adw::ApplicationWindow,
    app: &adw::Application,
    stack: &gtk::Stack,
) {
    let group=adw::PreferencesGroup::builder().title("Settings transfer").description("Export Tend preferences and policies, including project paths. Credentials, logs, restart/login settings and machine-level tuning are excluded.").build();
    let notice = editor::Notice::new();
    let actions = crate::column(8);
    let bar = crate::workbench::buttons(&actions);
    let export = gtk::Button::with_label("Export settings");
    let import = gtk::Button::with_label("Import settings");
    let drafts = gtk::Button::with_label("Review imported drafts");
    for b in [&export, &import, &drafts] {
        bar.insert(b, -1);
    }
    actions.append(&notice.root);
    group.add(&actions);
    parent.append(&group);
    let request = Rc::new(Cell::new(0u64));
    let changed = request.clone();
    stack.connect_visible_child_name_notify(move |stack| {
        if stack.visible_child_name().as_deref() != Some("care") {
            changed.set(changed.get().wrapping_add(1));
        }
    });
    let cancelled = request.clone();
    group.connect_unmap(move |_| cancelled.set(cancelled.get().wrapping_add(1)));
    let operation = Rc::new(Operation {
        busy: Cell::new(false),
        buttons: [&export, &import, &drafts]
            .into_iter()
            .map(|b| b.downgrade())
            .collect(),
    });
    let (w, n, op) = (win.clone(), notice.clone(), operation.clone());
    export.connect_clicked(move |_| {
        if !op.start() {
            return;
        }
        let (n, op) = (n.clone(), op.clone());
        pick(&w, true, move |result| {
            let path = match result {
                Ok(Some(p)) => p,
                Ok(None) => {
                    op.finish();
                    return;
                }
                Err(e) => {
                    op.finish();
                    n.show("Settings were not exported.", Some(&e), true);
                    return;
                }
            };
            job(
                move || {
                    let (bundle, warnings) = Bundle::capture();
                    bundle.save_export(&path)?;
                    Ok((bundle.sections.len(), warnings))
                },
                move |result| {
                    op.finish();
                    match result {
                        Ok((count, warnings)) => n.show(
                            &format!("Exported {count} sections."),
                            (!warnings.is_empty())
                                .then(|| warnings.join("\n"))
                                .as_deref(),
                            false,
                        ),
                        Err(e) => n.show("Settings were not exported.", Some(&e), true),
                    }
                },
            );
        });
    });
    let (w, a, n, r, op) = (
        win.clone(),
        app.clone(),
        notice.clone(),
        request.clone(),
        operation.clone(),
    );
    import.connect_clicked(move |_| {
        if !op.start() {
            return;
        }
        r.set(r.get().wrapping_add(1));
        let token = r.get();
        let (w, a, n, r, op) = (w.clone(), a.clone(), n.clone(), r.clone(), op.clone());
        pick(&w.clone(), false, move |result| {
            let path = match result {
                Ok(Some(p)) => p,
                Ok(None) => {
                    op.finish();
                    return;
                }
                Err(e) => {
                    op.finish();
                    n.show("Settings were not imported.", Some(&e), true);
                    return;
                }
            };
            job(
                move || Bundle::read(&path)?.imported(),
                move |result| {
                    if r.get() != token || !w.is_mapped() {
                        op.finish();
                        return;
                    }
                    match result {
                        Ok(bundle) => {
                            let done: Rc<dyn Fn()> = Rc::new(move || op.finish());
                            open(&w, &a, bundle, false, r, token, done);
                        }
                        Err(e) => {
                            op.finish();
                            n.show("Settings were not imported.", Some(&e), true);
                        }
                    }
                },
            );
        });
    });
    let (w, a, n, r, op) = (win.clone(), app.clone(), notice, request, operation);
    drafts.connect_clicked(move |_| {
        if !op.start() {
            return;
        }
        r.set(r.get().wrapping_add(1));
        let token = r.get();
        let (w, a, n, r, op) = (w.clone(), a.clone(), n.clone(), r.clone(), op.clone());
        job(Bundle::read_drafts, move |result| {
            if r.get() != token || !w.is_mapped() {
                op.finish();
                return;
            }
            match result {
                Ok(Some(b)) => {
                    let done: Rc<dyn Fn()> = Rc::new(move || op.finish());
                    open(&w, &a, b, true, r, token, done);
                }
                Ok(None) => {
                    op.finish();
                    n.show(
                        "No imported drafts. Choose Import settings first.",
                        None,
                        false,
                    );
                }
                Err(e) => {
                    op.finish();
                    n.show("Imported drafts could not be read.", Some(&e), true);
                }
            }
        });
    });
}
pub fn open(
    parent: &adw::ApplicationWindow,
    app: &adw::Application,
    bundle: Bundle,
    already_loaded: bool,
    request: Rc<Cell<u64>>,
    token: u64,
    done: Rc<dyn Fn()>,
) {
    let (parent, app) = (parent.clone(), app.clone());
    job(
        move || {
            let current = bundle
                .sections
                .keys()
                .map(|s| (*s, s.current()))
                .collect::<BTreeMap<_, _>>();
            Ok((bundle, current))
        },
        move |result| {
            done();
            if request.get() == token && parent.is_mapped() {
                if let Ok((bundle, current)) = result {
                    comparison(&parent, &app, bundle, current, already_loaded);
                }
            }
        },
    );
}
fn comparison(
    parent: &adw::ApplicationWindow,
    app: &adw::Application,
    bundle: Bundle,
    current: BTreeMap<Section, Result<Value, String>>,
    already_loaded: bool,
) {
    let body=crate::page("Review imported settings","Loading drafts does not change active settings. Review one section, then Save in its editor. Monitoring, jobs and additional cleanup rules are disabled in imported drafts.");
    let handlers: Handlers = Rc::new(RefCell::new(vec![]));
    if let Some(title) = body.first_child() {
        body.remove(&title);
    }
    let keys = bundle.sections.keys().copied().collect::<Vec<_>>();
    if keys.is_empty() {
        body.append(&label(
            "This bundle contains no settings sections.",
            "caption",
        ));
    }
    let titles = keys.iter().map(|s| s.title()).collect::<Vec<_>>();
    let chooser = gtk::DropDown::from_strings(&titles);
    chooser.set_widget_name("bundle-section");
    chooser.set_tooltip_text(Some("Settings section"));
    body.append(&chooser);
    let details = crate::column(6);
    let offset = Rc::new(Cell::new(0usize));
    let pager = crate::workbench::buttons(&body);
    let previous = gtk::Button::with_label("Previous changes");
    let next = gtk::Button::with_label("Next changes");
    pager.insert(&previous, -1);
    pager.insert(&next, -1);
    let changed_count = label("", "caption");
    body.append(&changed_count);
    body.append(&crate::scroll(&details));
    let notice = editor::Notice::new();
    body.append(&notice.root);
    let bar = crate::workbench::buttons(&body);
    let load = gtk::Button::with_label("Load as inactive drafts");
    let edit = gtk::Button::with_label("Review selected section");
    load.add_css_class("suggested-action");
    bar.insert(&load, -1);
    bar.insert(&edit, -1);
    edit.set_sensitive(already_loaded && !keys.is_empty());
    load.set_sensitive(!already_loaded && !keys.is_empty());
    let loaded = Rc::new(Cell::new(already_loaded));
    let bundle = Rc::new(bundle);
    let current = Rc::new(current);
    let dialog = dialog(parent, "Import settings", &crate::scroll(&body), 760, 700);
    let alive = Rc::new(Cell::new(true));
    dialog.set_can_close(false);
    let cancel = alive.clone();
    dialog.connect_close_attempt(move |d| {
        cancel.set(false);
        d.force_close();
    });
    let closed = alive.clone();
    dialog.connect_closed(move |_| closed.set(false));
    let render: Rc<dyn Fn()> = Rc::new({
        let (
            details,
            keys,
            bundle,
            current,
            chooser,
            count,
            edit,
            loaded,
            notice,
            offset,
            previous,
            next,
        ) = (
            details.clone(),
            keys.clone(),
            bundle.clone(),
            current.clone(),
            chooser.clone(),
            changed_count,
            edit.clone(),
            loaded.clone(),
            notice.clone(),
            offset.clone(),
            previous.clone(),
            next.clone(),
        );
        move || {
            while let Some(c) = details.first_child() {
                details.remove(&c);
            }
            let Some(key) = keys.get(chooser.selected() as usize) else {
                return;
            };
            match &current[key] {
                Ok(active) => {
                    let changes =
                        tend::settings_bundle::comparison(*key, active, &bundle.sections[key]);
                    let start = offset.get().min(changes.len().saturating_sub(1) / 50 * 50);
                    offset.set(start);
                    count.set_text(&format!(
                        "{} changed fields · showing {}–{} · values are not active",
                        changes.len(),
                        if changes.is_empty() { 0 } else { start + 1 },
                        (start + 50).min(changes.len())
                    ));
                    previous.set_sensitive(start > 0);
                    next.set_sensitive(start + 50 < changes.len());
                    for (field, before, after) in changes.into_iter().skip(start).take(50) {
                        let expander = gtk::Expander::builder().label(field_title(&field)).build();
                        let value = label(&format!("Current: {before}\nDraft: {after}"), "caption");
                        value.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                        value.set_selectable(true);
                        expander.set_child(Some(&value));
                        details.append(&expander);
                    }
                    if details.first_child().is_none() {
                        details.append(&label(
                            "This section already matches the current settings.",
                            "caption",
                        ));
                    }
                    edit.set_sensitive(loaded.get());
                    notice.root.set_visible(false);
                }
                Err(e) => {
                    count.set_text("Current settings unavailable");
                    notice.show(
                        "Repair the saved settings before reviewing this section.",
                        Some(e),
                        true,
                    );
                    edit.set_sensitive(false);
                }
            }
        }
    });
    let r = render.clone();
    let page = offset.clone();
    let signal = chooser.connect_selected_notify(move |_| {
        page.set(0);
        r();
    });
    track(&handlers, &chooser, signal);
    for (button, forward) in [(&previous, false), (&next, true)] {
        let (page, r) = (offset.clone(), render.clone());
        let signal = button.connect_clicked(move |_| {
            page.set(if forward {
                page.get() + 50
            } else {
                page.get().saturating_sub(50)
            });
            r();
        });
        track(&handlers, button, signal);
    }
    render();
    let (b, n, l, e, flag, r) = (
        bundle.clone(),
        notice.clone(),
        load.clone(),
        edit.clone(),
        loaded,
        render,
    );
    let signal = load.connect_clicked(move |_| {
        l.set_sensitive(false);
        let (b, n, l, e, flag, r) = (
            b.clone(),
            n.clone(),
            l.clone(),
            e.clone(),
            flag.clone(),
            r.clone(),
        );
        let b = (*b).clone();
        job(
            move || b.save_drafts(),
            move |result| match result {
                Ok(()) => {
                    flag.set(true);
                    e.set_sensitive(true);
                    r();
                    n.show(
                        "Inactive drafts saved. Review a section to continue.",
                        None,
                        false,
                    );
                }
                Err(err) => {
                    l.set_sensitive(true);
                    n.show("Drafts were not saved.", Some(&err), true)
                }
            },
        );
    });
    track(&handlers, &load, signal);
    let (w, a, c, b, n, d, keys) = (
        parent.clone(),
        app.clone(),
        current,
        bundle,
        notice,
        dialog.downgrade(),
        keys,
    );
    let signal = edit.connect_clicked(move |button| {
        if !button.is_sensitive() {
            return;
        }
        let Some(key) = keys.get(chooser.selected() as usize).copied() else {
            return;
        };
        if pending(key) {
            n.show(
                "Save or discard this section's existing unsaved changes first.",
                None,
                true,
            );
            return;
        }
        let Ok(expected) = &c[&key] else {
            return;
        };
        button.set_sensitive(false);
        let edit_button = button.downgrade();
        let expected = expected.clone();
        let imported = b.sections[&key].clone();
        let (w, a, n, d, alive) = (w.clone(), a.clone(), n.clone(), d.clone(), alive.clone());
        job(
            move || {
                if key.current()? != expected {
                    return Err(
                        "Saved settings changed. Reopen this comparison before continuing.".into(),
                    );
                }
                Ok((expected, availability(key)))
            },
            move |r| {
                if !alive.get() {
                    return;
                }
                let Some(dialog) = d.upgrade() else {
                    return;
                };
                match r {
                    Ok((active, unavailable)) => {
                        alive.set(false);
                        dialog.force_close();
                        review(&w, &a, key, active, imported, unavailable);
                    }
                    Err(e) => {
                        if let Some(button) = edit_button.upgrade() {
                            button.set_sensitive(true);
                        }
                        n.show("Review could not be opened.", Some(&e), true);
                    }
                }
            },
        );
    });
    track(&handlers, &edit, signal);
    release(&dialog, handlers);
    dialog.present(Some(parent));
}
