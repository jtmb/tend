//! One shared editor for local history limits. Saving never scans, prunes,
//! erases history or activates a job. Cleanup follows later explicit writes.
use crate::{editor, job, label, row};
use adw::prelude::*;
use gtk::glib;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use tend::history::{self, Settings};

pub fn button(win: &adw::ApplicationWindow) -> gtk::Button {
    let button = gtk::Button::with_label("History limits");
    let win = win.downgrade();
    button.connect_clicked(move |_| {
        if let Some(win) = win.upgrade() {
            open(&win, None);
        }
    });
    button
}
pub fn add(parent: &gtk::Box, win: &adw::ApplicationWindow) {
    let group=adw::PreferencesGroup::builder().title("Local history").description("Control the storage budget and retention for Tend's local reports and metadata changes.").build();
    row(
        &group,
        "History limits",
        "Choose a storage limit and how long to keep reports",
        &button(win),
    );
    parent.append(&group);
}
pub fn open(win: &adw::ApplicationWindow, draft: Option<Settings>) {
    let loaded = Settings::read();
    let baseline = Rc::new(RefCell::new(loaded.clone().ok()));
    let initial = draft
        .clone()
        .unwrap_or_else(|| loaded.clone().unwrap_or_default());
    let body = crate::column(20);
    body.set_margin_start(24);
    body.set_margin_end(24);
    body.set_margin_top(20);
    body.set_margin_bottom(24);
    body.append(&label("Choose how much space Tend may use for history. Limits apply as new history is recorded; saving does not remove existing records.","dim-label"));
    if draft.is_some() {
        body.append(&label(
            "Imported draft · review these values before saving.",
            "caption",
        ));
    }
    if let Err(e) = loaded {
        editor::error(&body,"Saved history limits could not be read. Review the defaults before replacing the file.",&e);
    }
    let group = adw::PreferencesGroup::builder()
        .title("Budget and retention")
        .build();
    let budget = gtk::SpinButton::with_range(128., 2048., 64.);
    budget.set_value(initial.max_mib as f64);
    budget.set_widget_name("history-budget");
    budget.set_tooltip_text(Some("Total history budget in MiB"));
    budget.update_property(&[gtk::accessible::Property::Label("History budget in MiB")]);
    row(
        &group,
        "Total history budget",
        "128–2048 MiB · total allowance for Tend history",
        &budget,
    );
    let retention = gtk::SpinButton::with_range(1., 365., 1.);
    retention.set_value(initial.retention_days as f64);
    retention.set_widget_name("history-retention");
    retention.set_tooltip_text(Some("Private history retention in days"));
    retention.update_property(&[gtk::accessible::Property::Label(
        "Private history retention in days",
    )]);
    row(
        &group,
        "Private history retention",
        "1–365 days · reports, diagnostics and file-change records",
        &retention,
    );
    body.append(&group);
    let notes = adw::PreferencesGroup::builder().title("Retention").build();
    notes.add(&label("Two complete snapshots per scanned folder are kept separately from change records. File contents are never stored.","caption"));
    let details = crate::column(12);
    details.append(&label(&format!("{} MiB of the total is reserved for system maintenance records and their journal, retaining up to 30 days and 1024 outcomes. Private events have a 32 MiB allowance inside the remaining budget.",history::SYSTEM_HISTORY_RESERVE_MIB),"caption"));
    details.append(&label("The total includes databases, sidecars and staging files. Existing oversized stores pause further writes instead of discarding complete snapshots. Writes also pause when less than 2 GiB is free. Legacy report files are retained separately.","caption"));
    let expander = gtk::Expander::builder()
        .label("Storage limits and maintenance records")
        .child(&details)
        .build();
    notes.add(&expander);
    body.append(&notes);
    let footer = editor::Footer::new(
        !tend::workstation::home()
            .join(".config/tend/history.json")
            .exists(),
        baseline.borrow().is_some(),
        None,
    );
    let get: Rc<dyn Fn() -> Settings> = Rc::new({
        let (budget, retention) = (budget.clone(), retention.clone());
        move || Settings {
            version: 1,
            max_mib: budget.value_as_int() as u32,
            retention_days: retention.value_as_int() as u32,
        }
    });
    footer.sync(
        baseline.borrow().as_ref() != Some(&get()),
        get().validate().is_ok(),
    );
    let changing = Rc::new(Cell::new(false));
    let handlers = Rc::new(RefCell::new(
        Vec::<(glib::Object, glib::SignalHandlerId)>::new(),
    ));
    for spin in [&budget, &retention] {
        let (get, baseline, footer, changing) = (
            get.clone(),
            baseline.clone(),
            footer.clone(),
            changing.clone(),
        );
        let id = spin.connect_value_changed(move |_| {
            if !changing.get() {
                let draft = get();
                footer.sync(
                    baseline.borrow().as_ref() != Some(&draft),
                    draft.validate().is_ok(),
                );
            }
        });
        handlers.borrow_mut().push((spin.clone().upcast(), id));
    }
    {
        let (baseline, footer, get) = (baseline.clone(), footer.clone(), get.clone());
        let button = footer.save.clone();
        let id = button.connect_clicked(move |_| {
            let saved = get();
            let write = saved.clone();
            let (baseline, footer, get) = (baseline.clone(), footer.clone(), get.clone());
            footer.saving();
            job(
                move || write.save(),
                move |result| match result {
                    Ok(()) => {
                        *baseline.borrow_mut() = Some(saved.clone());
                        footer.success();
                        let draft = get();
                        footer.sync(draft != saved, draft.validate().is_ok());
                    }
                    Err(e) => footer.error(&e),
                },
            );
        });
        handlers.borrow_mut().push((button.upcast(), id));
    }
    {
        let (baseline, footer) = (baseline.clone(), footer.clone());
        let button = footer.discard.clone();
        let id = button.connect_clicked(move |_| {
            if let Some(s) = baseline.borrow().as_ref() {
                changing.set(true);
                budget.set_value(s.max_mib as f64);
                retention.set_value(s.retention_days as f64);
                changing.set(false);
                footer.sync(false, true);
            }
        });
        handlers.borrow_mut().push((button.upcast(), id));
    }
    let toolbar = adw::ToolbarView::new();
    toolbar.set_content(Some(&editor::shell(&body, &footer)));
    let header = adw::HeaderBar::new();
    header.set_show_start_title_buttons(false);
    toolbar.add_top_bar(&header);
    let dialog = adw::Dialog::builder()
        .title("History limits")
        .content_width(640)
        .content_height(620)
        .child(&toolbar)
        .build();
    dialog.connect_closed(move |_| {
        for (object, id) in handlers.borrow_mut().drain(..) {
            object.disconnect(id);
        }
    });
    dialog.present(Some(win));
}
