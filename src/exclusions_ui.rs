//! One shared policy editor, opened from cleanup pages. Activation uses the
//! existing narrow helper; drafts and validation never change system settings.
use crate::{column, editor, job, label, row};
use adw::prelude::*;
use gtk::glib;
use std::{cell::RefCell, path::PathBuf, rc::Rc};
use tend::exclusions::{Entry, Policy};
type Handlers = Rc<RefCell<Vec<(glib::WeakRef<glib::Object>, glib::SignalHandlerId)>>>;
fn track<T: IsA<glib::Object>>(handlers: &Handlers, object: &T, id: glib::SignalHandlerId) {
    let object: &glib::Object = object.as_ref();
    handlers.borrow_mut().push((object.downgrade(), id));
}
pub fn button(parent: &adw::ApplicationWindow, changed: Rc<dyn Fn()>) -> gtk::Button {
    let button = gtk::Button::with_label("Manage exclusions…");
    let parent = parent.downgrade();
    button.connect_clicked(move |button| {
        let (parent, changed, button) = (parent.clone(), changed.clone(), button.clone());
        button.set_sensitive(false);
        button.set_label("Checking exclusions…");
        job(
            move || Ok((Policy::read(), tend::exclusions::integration_supported())),
            move |result| {
                button.set_sensitive(true);
                button.set_label("Manage exclusions…");
                if let Some(parent) = parent.upgrade() {
                    let (loaded, supported) = result.unwrap_or_else(|e| (Err(e), false));
                    open(&parent, changed, loaded, supported);
                }
            },
        );
    });
    button
}
fn open(
    parent: &adw::ApplicationWindow,
    changed: Rc<dyn Fn()>,
    loaded: Result<Policy, String>,
    supported: bool,
) {
    let window = adw::Window::builder()
        .title("Cleanup exclusions")
        .transient_for(parent)
        .modal(true)
        .default_width(680)
        .default_height(640)
        .build();
    let handlers: Handlers = Rc::new(RefCell::new(vec![]));
    let on_close = handlers.clone();
    window.connect_close_request(move |_| {
        for (object, id) in on_close.borrow_mut().drain(..) {
            if let Some(object) = object.upgrade() {
                object.disconnect(id);
            }
        }
        glib::Propagation::Proceed
    });
    let layout = column(0);
    let header = adw::HeaderBar::new();
    layout.append(&header);
    let body=crate::page("Cleanup exclusions","Protected paths apply to Tend’s manual cleanup, user cron cleanup and daily cleanup. Folders include their contents; symlinks are resolved when protection is checked.");
    body.append(&label("Engine-managed operations such as Docker pruning, Empty Trash and npm verification are skipped while exclusions are active. APT and pip cleanup are skipped when their cache overlaps a protected path. Existing schedules stay as configured.","caption"));
    let initial = loaded.as_ref().ok().cloned().unwrap_or_default();
    let draft = Rc::new(RefCell::new(initial.clone()));
    let baseline = Rc::new(RefCell::new(initial));
    let unavailable = if loaded.is_err() {
        Some("Repair the preserved policy file before saving changes.")
    } else if !supported {
        Some("A matching helper, both workers and the daily adapter must be installed before saving exclusions.")
    } else {
        None
    };
    let footer = editor::Footer::new(false, loaded.is_ok(), unavailable);
    footer.save.set_label("Save exclusions");
    if let Err(error) = &loaded {
        editor::error(&body, "Could not load shared exclusions", error);
    }
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.set_placeholder(Some(&label("No protected paths configured", "caption")));
    body.append(&list);
    let controls = crate::workbench::buttons(&body);
    let add = gtk::Button::with_label("Add protected path…");
    let edit = gtk::Button::with_label("Edit protected path…");
    let remove = gtk::Button::with_label("Remove protected path");
    for b in [&add, &edit, &remove] {
        controls.insert(b, -1);
    }
    edit.set_sensitive(false);
    remove.set_sensitive(false);
    let status = label(
        "Review a draft, then save it to activate protection.",
        "caption",
    );
    status.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    body.append(&status);
    let render: Rc<dyn Fn()> = Rc::new({
        let (draft, baseline, list, footer) = (
            draft.clone(),
            baseline.clone(),
            list.clone(),
            footer.clone(),
        );
        move || {
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            for entry in &draft.borrow().paths {
                let row = adw::ActionRow::builder()
                    .use_markup(false)
                    .title(entry.path.to_string_lossy())
                    .subtitle(format!(
                        "{}{}",
                        if entry.enabled {
                            "Protected"
                        } else {
                            "Disabled"
                        },
                        if entry.note.is_empty() {
                            String::new()
                        } else {
                            format!(" · {}", entry.note)
                        }
                    ))
                    .build();
                row.set_title_lines(2);
                row.set_subtitle_lines(2);
                list.append(&row);
            }
            footer.sync(
                *draft.borrow() != *baseline.borrow(),
                draft.borrow().validate().is_ok(),
            );
        }
    });
    render();
    let (e, r) = (edit.clone(), remove.clone());
    let handler = list.connect_row_selected(move |_, row| {
        e.set_sensitive(row.is_some());
        r.set_sensitive(row.is_some());
    });
    track(&handlers, &list, handler);
    let edit_entry: Rc<dyn Fn(Option<usize>)> = Rc::new({
        let (window, draft, render) = (window.clone(), draft.clone(), render.clone());
        move |index| {
            let old = index.and_then(|i| draft.borrow().paths.get(i).cloned());
            let dialog=adw::MessageDialog::new(Some(&window),Some(if old.is_some() {"Edit protected path"}else{"Add protected path"}),Some("Protect an absolute file or folder path. This edits the draft until you save exclusions."));
            dialog.add_responses(&[("cancel", "Cancel"), ("save", "Keep in draft")]);
            dialog.set_default_response(Some("cancel"));
            dialog.set_close_response("cancel");
            let fields = column(12);
            let path = gtk::Entry::builder()
                .placeholder_text("Absolute protected path")
                .text(
                    old.as_ref()
                        .map(|e| e.path.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                )
                .build();
            fields.append(&path);
            let note = gtk::Entry::builder()
                .placeholder_text("Optional label")
                .max_length(120)
                .text(old.as_ref().map(|e| e.note.as_str()).unwrap_or(""))
                .build();
            fields.append(&note);
            let group = adw::PreferencesGroup::new();
            let enabled = gtk::Switch::builder()
                .active(old.as_ref().is_none_or(|e| e.enabled))
                .build();
            row(
                &group,
                "Enable protection",
                "Applied after saving the shared policy",
                &enabled,
            );
            fields.append(&group);
            let notice = editor::Notice::new();
            fields.append(&notice.root);
            dialog.set_extra_child(Some(&fields));
            let check: Rc<dyn Fn()> = Rc::new({
                let (dialog, path, note, enabled, draft) = (
                    dialog.downgrade(),
                    path.downgrade(),
                    note.downgrade(),
                    enabled.downgrade(),
                    draft.clone(),
                );
                move || {
                    let (Some(dialog), Some(path), Some(note), Some(enabled)) = (
                        dialog.upgrade(),
                        path.upgrade(),
                        note.upgrade(),
                        enabled.upgrade(),
                    ) else {
                        return;
                    };
                    let mut next = draft.borrow().clone();
                    let entry = Entry {
                        path: PathBuf::from(path.text().as_str()),
                        enabled: enabled.is_active(),
                        note: note.text().into(),
                    };
                    if let Some(i) = index {
                        next.paths[i] = entry;
                    } else {
                        next.paths.push(entry);
                    }
                    match next.validate() {
                        Ok(()) => {
                            dialog.set_response_enabled("save", true);
                            notice.root.set_visible(false);
                        }
                        Err(error) => {
                            dialog.set_response_enabled("save", false);
                            notice.show(&error, None, true);
                        }
                    }
                }
            });
            let c = check.clone();
            path.connect_changed(move |_| c());
            let c = check.clone();
            note.connect_changed(move |_| c());
            check();
            let (draft, render) = (draft.clone(), render.clone());
            dialog.connect_response(None, move |_, response| {
                if response == "save" {
                    let mut next = draft.borrow().clone();
                    let entry = Entry {
                        path: PathBuf::from(path.text().as_str()),
                        enabled: enabled.is_active(),
                        note: note.text().into(),
                    };
                    if let Some(i) = index {
                        next.paths[i] = entry;
                    } else {
                        next.paths.push(entry);
                    }
                    if next.validate().is_ok() {
                        *draft.borrow_mut() = next;
                        render();
                    }
                }
            });
            dialog.present();
        }
    });
    let f = edit_entry.clone();
    let handler = add.connect_clicked(move |_| f(None));
    track(&handlers, &add, handler);
    let l = list.clone();
    let handler = edit.connect_clicked(move |_| {
        if let Some(row) = l.selected_row() {
            edit_entry(Some(row.index() as usize));
        }
    });
    track(&handlers, &edit, handler);
    let (l, d, r) = (list.clone(), draft.clone(), render.clone());
    let handler = remove.connect_clicked(move |_| {
        if let Some(row) = l.selected_row() {
            d.borrow_mut().paths.remove(row.index() as usize);
            r();
        }
    });
    track(&handlers, &remove, handler);
    let (d, b, r) = (draft.clone(), baseline.clone(), render.clone());
    let handler = footer.discard.connect_clicked(move |_| {
        *d.borrow_mut() = b.borrow().clone();
        r();
    });
    track(&handlers, &footer.discard, handler);
    let (d, b, r, f, form, dialog_parent) = (
        draft,
        baseline,
        render,
        footer.clone(),
        body.clone(),
        window.clone(),
    );
    let handler=footer.save.connect_clicked(move |_| {
        let policy=d.borrow().clone();let count=b.borrow().paths.iter().filter(|old|old.enabled&&!policy.paths.iter().any(|p|p.enabled&&p.path==old.path)).count();
        let confirm=adw::MessageDialog::new(Some(&dialog_parent),Some("Save shared cleanup exclusions?"),Some(&format!("Tend’s existing cleanup workflows will use this policy. {count} currently protected paths will lose protection. A running cleanup must finish before the policy changes.")));confirm.add_responses(&[("cancel","Cancel"),("save","Save exclusions")]);confirm.set_default_response(Some("cancel"));confirm.set_close_response("cancel");
        let (d,b,r,f,form,changed,status)=(d.clone(),b.clone(),r.clone(),f.clone(),form.clone(),changed.clone(),status.clone());confirm.connect_response(None,move |_,response| {if response!="save" {return}f.saving();form.set_sensitive(false);let policy=policy.clone();let (d,b,r,f,form,changed,status)=(d.clone(),b.clone(),r.clone(),f.clone(),form.clone(),changed.clone(),status.clone());
            job(move || {policy.compile()?;crate::privileged("exclusions-save",Some(serde_json::to_string(&policy).map_err(|e|e.to_string())?))?;Policy::read()},move |result| {form.set_sensitive(true);match result {Ok(saved)=>{*d.borrow_mut()=saved.clone();*b.borrow_mut()=saved;f.success();r();changed();status.set_text("Shared exclusions saved. Cleanup previews must be refreshed.");},Err(error)=>f.error(&error)}});
        });confirm.present();
    });
    track(&handlers, &footer.save, handler);
    layout.append(&editor::shell(&body, &footer));
    window.set_content(Some(&layout));
    window.present();
}
