use adw::prelude::*;
use gtk::{gio, glib};
use std::{cell::RefCell, path::PathBuf, rc::Rc};
use tend::{
    history::snapshots::{self, Change, ChangeCursor},
    workstation as w,
};
pub fn attach(parent: &gtk::Box, path: &gtk::Entry,win:&adw::ApplicationWindow) {
    parent.append(&crate::label("Storage changes", "title-2"));
    parent.append(&crate::label("Retained metadata changes from complete scans. A touched timestamp can count as a change; file contents are never compared. Scans describe an interval, not an atomic filesystem snapshot.","caption"));
    let actions = crate::workbench::buttons(parent);
    let load = gtk::Button::with_label("Load storage changes");
    let next = gtk::Button::with_label("Next 200 changes");
    next.set_sensitive(false);
    actions.insert(&load, -1);
    actions.insert(&next, -1);
    actions.insert(&crate::history_ui::button(win),-1);
    let status=crate::label("History loads only when requested. Changes are ordered by recorded date; clock adjustments can change their position. Unusual filename bytes are preserved.","caption");
    parent.append(&status);
    let store = gio::ListStore::new::<glib::BoxedAnyObject>();
    let selection = gtk::SingleSelection::new(Some(store.clone()));
    selection.set_autoselect(false);
    selection.set_can_unselect(true);
    let table = gtk::ColumnView::new(Some(selection.clone()));
    table.set_show_row_separators(true);
    table.add_css_class("data-table");
    for (index, title) in ["Path", "Change", "Before", "After", "Observed"]
        .iter()
        .enumerate()
    {
        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(move |_, o| {
            let item = o.downcast_ref::<gtk::ListItem>().unwrap();
            let text = gtk::Label::builder()
                .xalign(if index == 2 || index == 3 { 1. } else { 0. })
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .margin_start(6)
                .margin_end(6)
                .margin_top(9)
                .margin_bottom(9)
                .build();
            item.set_child(Some(&text));
        });
        factory.connect_bind(move |_, o| {
            let item = o.downcast_ref::<gtk::ListItem>().unwrap();
            let object = item
                .item()
                .unwrap()
                .downcast::<glib::BoxedAnyObject>()
                .unwrap();
            let change = object.borrow::<Change>();
            let label = item.child().unwrap().downcast::<gtk::Label>().unwrap();
            label.set_text(&match index {
                0 => change.path.to_string_lossy().into_owned(),
                1 => match change.kind.as_str() {
                    "added" => "Added",
                    "removed" => "Removed",
                    _ => "Metadata changed",
                }
                .into(),
                2 => change
                    .before_size
                    .map(w::bytes)
                    .unwrap_or_else(|| "—".into()),
                3 => change
                    .after_size
                    .map(w::bytes)
                    .unwrap_or_else(|| "—".into()),
                _ => tend::presentation::when(change.time_ms / 1000),
            });
            label.set_tooltip_text(Some(&change.path.to_string_lossy()));
        });
        let col = gtk::ColumnViewColumn::new(Some(title), Some(factory));
        col.set_expand(index == 0);
        col.set_resizable(true);
        col.set_fixed_width([280, 160, 100, 100, 180][index]);
        table.append_column(&col);
    }
    let scroller = gtk::ScrolledWindow::builder()
        .child(&table)
        .min_content_height(260)
        .vexpand(true)
        .hexpand(true)
        .build();
    scroller.add_css_class("card");
    parent.append(&scroller);
    let detail = crate::label(
        "Select an entry to inspect its logical and allocated sizes.",
        "caption",
    );
    detail.set_selectable(true);
    detail.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    parent.append(&detail);
    selection.connect_selected_item_notify(move |s| {
        if let Some(object) = s
            .selected_item()
            .and_then(|o| o.downcast::<glib::BoxedAnyObject>().ok())
        {
            let c = object.borrow::<Change>();
            use std::os::unix::ffi::OsStrExt;
            let extra = if c.path.to_str().is_none() {
                format!(
                    "\nFilename bytes (hex): {}",
                    c.path
                        .as_os_str()
                        .as_bytes()
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>()
                )
            } else {
                String::new()
            };
            detail.set_text(&format!(
                "{}\nLogical: {} → {} · Allocated: {} → {}\n{}{}",
                c.path.display(),
                c.before_size
                    .map(w::bytes)
                    .unwrap_or_else(|| "Absent".into()),
                c.after_size
                    .map(w::bytes)
                    .unwrap_or_else(|| "Absent".into()),
                c.before_allocated
                    .map(w::bytes)
                    .unwrap_or_else(|| "Absent".into()),
                c.after_allocated
                    .map(w::bytes)
                    .unwrap_or_else(|| "Absent".into()),
                tend::presentation::exact(c.time_ms / 1000),
                extra
            ));
        } else {
            detail.set_text("Select an entry to inspect its logical and allocated sizes.");
        }
    });
    let cursor: Rc<RefCell<Option<ChangeCursor>>> = Rc::new(RefCell::new(None));
    let loaded: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
    let request: Rc<dyn Fn(bool)> = Rc::new({
        let (path, load, next, status, store) = (
            path.clone(),
            load.clone(),
            next.clone(),
            status.clone(),
            store.clone(),
        );
        let (cursor, loaded) = (cursor.clone(), loaded.clone());
        move |more| {
            if !load.is_sensitive() {
                return;
            }
            let root = PathBuf::from(path.text().as_str());
            let before = if more && loaded.borrow().as_ref() == Some(&root) {
                cursor.borrow().clone()
            } else {
                None
            };
            load.set_sensitive(false);
            next.set_sensitive(false);
            status.set_text("Loading storage changes…");
            let (path, load, next, status, store, cursor, loaded) = (
                path.clone(),
                load.clone(),
                next.clone(),
                status.clone(),
                store.clone(),
                cursor.clone(),
                loaded.clone(),
            );
            let expected = root.clone();
            crate::job(
                move || snapshots::changes(&root, before.as_ref(), 200),
                move |result| {
                    load.set_sensitive(true);
                    if PathBuf::from(path.text().as_str()) != expected {
                        status.set_text("Folder changed; load its history to continue.");
                        return;
                    }
                    match result {
                        Ok(page) => {
                            let count = page.changes.len();
                            let values: Vec<_> = page
                                .changes
                                .into_iter()
                                .map(glib::BoxedAnyObject::new)
                                .collect();
                            store.splice(0, store.n_items(), &values);
                            next.set_sensitive(page.next.is_some());
                            *cursor.borrow_mut() = page.next;
                            *loaded.borrow_mut() = Some(expected);
                            status.set_text(&if count==0 {"No recorded changes. Complete an initial scan, then scan the same folder again to compare.".into()}else{format!("Showing {count} changes · newest scan first · logical sizes shown; select a row for allocated sizes")});
                        }
                        Err(e) => {
                            status.set_text(&e);
                            store.remove_all();
                            *cursor.borrow_mut() = None;
                        }
                    }
                },
            );
        }
    });
    let first = request.clone();
    load.connect_clicked(move |_| first(false));
    next.connect_clicked(move |_| request(true));
    let (table, loaded, cursor, status, next) = (
        store.clone(),
        loaded.clone(),
        cursor.clone(),
        status.clone(),
        next.clone(),
    );
    path.connect_changed(move |_| {
        next.set_sensitive(false);
        table.remove_all();
        *loaded.borrow_mut() = None;
        *cursor.borrow_mut() = None;
        status.set_text("Folder changed. Load storage changes when ready.");
    });
}
