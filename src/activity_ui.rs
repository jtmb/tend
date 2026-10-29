//! On-demand history and incident timeline in the existing Activity page.
use crate::{job, label, page, scroll};
use adw::prelude::*;
use gtk::{gio, glib};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};
use tend::{
    activity::{self, Cursor, Event, Query},
    workstation as w,
};

pub fn add(stack: &gtk::Stack, win: &adw::ApplicationWindow) {
    let body=page("Activity","A timeline of maintenance, saved Tend actions, diagnostic sessions and reported crashes. Loaded only on request; nearby events do not establish a cause.");
    let filters = crate::workbench::buttons(&body);
    let source = gtk::DropDown::from_strings(&[
        "All activity",
        "Daily maintenance",
        "Tend actions",
        "Crashes & kernel OOM kills",
    ]);
    source.set_tooltip_text(Some("Activity source"));
    let kind = gtk::DropDown::from_strings(&[
        "All categories",
        "Updates",
        "Cleanup",
        "Pressure & diagnostics",
        "Settings",
        "Crashes",
    ]);
    kind.set_tooltip_text(Some("Activity category"));
    let outcome = gtk::DropDown::from_strings(&[
        "All results",
        "Succeeded",
        "Failed",
        "Deferred",
        "Awaiting result",
    ]);
    outcome.set_tooltip_text(Some("Activity result"));
    let period = gtk::DropDown::from_strings(&[
        "Last hour",
        "Last 24 hours",
        "Last 7 days",
        "Last 30 days",
        "All retained history",
    ]);
    period.set_selected(2);
    period.set_tooltip_text(Some("Activity time range"));
    for control in [&source, &kind, &outcome, &period] {
        filters.insert(control, -1);
    }
    let search = gtk::SearchEntry::builder()
        .placeholder_text("Search summaries and saved reports")
        .hexpand(true)
        .build();
    body.append(&search);
    let crashes = gtk::CheckButton::with_label("Include readable crashes and kernel OOM kills when loading");
    crashes.set_active(true);
    body.append(&crashes);
    let actions = crate::workbench::buttons(&body);
    let load = gtk::Button::with_label("Load recent activity");
    load.add_css_class("suggested-action");
    let next = gtk::Button::with_label("Older activity");
    next.set_sensitive(false);
    let journal = gtk::Button::with_label("Maintenance journal");
    let export = gtk::Button::with_label("Export this page");
    export.set_sensitive(false);
    for button in [&load, &next, &journal, &export] {
        actions.insert(button, -1);
    }
    actions.insert(&crate::history_ui::button(win),-1);
    let status = label(
        "Choose filters and load activity. No background history query is running.",
        "caption",
    );
    body.append(&status);
    let notice = crate::editor::Notice::new();
    body.append(&notice.root);
    let store = gio::ListStore::new::<glib::BoxedAnyObject>();
    let selection = gtk::SingleSelection::new(Some(store.clone()));
    selection.set_autoselect(false);
    selection.set_can_unselect(true);
    let table = gtk::ColumnView::new(Some(selection.clone()));
    table.add_css_class("data-table");
    table.set_show_row_separators(true);
    let mut source_column = None;
    for (index, name) in ["When", "Event", "Result", "Source"].iter().enumerate() {
        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(move |_, object| {
            let item = object.downcast_ref::<gtk::ListItem>().unwrap();
            let label = gtk::Label::builder()
                .xalign(0.)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .margin_start(8)
                .margin_end(8)
                .margin_top(12)
                .margin_bottom(12)
                .build();
            if index == 0 {
                label.set_wrap(true);
                label.set_lines(2);
            }
            item.set_child(Some(&label));
        });
        factory.connect_bind(move |_, object| {
            let item = object.downcast_ref::<gtk::ListItem>().unwrap();
            let object = item
                .item()
                .unwrap()
                .downcast::<glib::BoxedAnyObject>()
                .unwrap();
            let event = object.borrow::<Event>();
            let label = item.child().unwrap().downcast::<gtk::Label>().unwrap();
            let value = match index {
                0 => tend::presentation::when(event.cursor.time_ms / 1000),
                1 => event.title.clone(),
                2 => event.outcome.clone(),
                _ => event.source.clone(),
            };
            label.set_text(&value);
            for class in ["success", "error", "warning"] {
                label.remove_css_class(class);
            }
            if index == 2 {
                match event.outcome.as_str() {
                    "Success" => label.add_css_class("success"),
                    "Failed" | "Crash report" | "OOM kill" => label.add_css_class("error"),
                    "Deferred" | "Awaiting result" => label.add_css_class("warning"),
                    _ => {}
                }
            }
            label.set_tooltip_text(Some(&if index == 0 {
                tend::presentation::exact(event.cursor.time_ms / 1000)
            } else if index == 1 {
                format!("{}\n{}", event.category, event.summary)
            } else {
                value
            }));
        });
        let col = gtk::ColumnViewColumn::new(Some(name), Some(factory));
        col.set_resizable(true);
        col.set_expand(index == 1);
        col.set_fixed_width([170, 210, 140, 160][index]);
        table.append_column(&col);
        if index == 3 {
            source_column = Some(col);
        }
    }
    let viewport = gtk::ScrolledWindow::builder()
        .child(&table)
        .min_content_height(220)
        .max_content_height(380)
        .propagate_natural_height(true)
        .hexpand(true)
        .build();
    viewport.add_css_class("card");
    let adaptive = adw::BreakpointBin::builder()
        .child(&viewport)
        .height_request(240)
        .width_request(360)
        .hexpand(true)
        .build();
    let narrow = adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 740px").unwrap());
    narrow.add_setter(&source_column.unwrap(), "visible", Some(&false.to_value()));
    adaptive.add_breakpoint(narrow);
    body.append(&adaptive);
    let details = crate::workbench::text_panel(&body);
    crate::workbench::report_hint(
        &details,
        "Choose Load recent activity, then select a record for its evidence and details.",
    );
    body.append(&label("Daily phase records include attempts, elapsed time, schedule delay and observed net space change. No old execution history is fabricated. Records may expire or rotate while browsing.","caption"));
    let generation = Rc::new(Cell::new(0u64));
    let cursor = Rc::new(Cell::new(None::<Cursor>));
    let rows = Rc::new(RefCell::new(Vec::<Event>::new()));
    let get: Rc<dyn Fn() -> Query> = Rc::new({
        let (source, kind, outcome, period, search, crashes) = (
            source.clone(),
            kind.clone(),
            outcome.clone(),
            period.clone(),
            search.clone(),
            crashes.clone(),
        );
        move || Query {
            source: source.selected() as u8,
            category: kind.selected() as u8,
            outcome: outcome.selected() as u8,
            seconds: [3600, 86400, 7 * 86400, 30 * 86400, 0][period.selected().min(4) as usize],
            search: search.text().into(),
            include_crashes: crashes.is_active(),
        }
    });
    let invalidate: Rc<dyn Fn()> = Rc::new({
        let (generation, cursor, store, rows, next, export, details, status, notice) = (
            generation.clone(),
            cursor.clone(),
            store.clone(),
            rows.clone(),
            next.clone(),
            export.clone(),
            details.clone(),
            status.clone(),
            notice.clone(),
        );
        move || {
            generation.set(generation.get().wrapping_add(1));
            cursor.set(None);
            store.remove_all();
            rows.borrow_mut().clear();
            next.set_sensitive(false);
            export.set_sensitive(false);
            notice.root.set_visible(false);
            details.buffer().set_text(
                "Activity filters changed or collection paused. Load recent activity to refresh.",
            );
            status.set_text("Load activity to read the selected sources and filters.");
        }
    });
    for control in [&source, &kind, &outcome, &period] {
        let f = invalidate.clone();
        control.connect_selected_notify(move |_| f());
    }
    let f = invalidate.clone();
    search.connect_search_changed(move |_| f());
    let f = invalidate.clone();
    crashes.connect_toggled(move |_| f());
    let detail = details.clone();
    selection.connect_selected_item_notify(move |s| {
        if let Some(object) = s
            .selected_item()
            .and_then(|o| o.downcast::<glib::BoxedAnyObject>().ok())
        {
            detail.buffer().set_text(&object.borrow::<Event>().details);
        } else {
            detail
                .buffer()
                .set_text("Select a record to see its evidence and details.");
        }
    });
    let read: Rc<dyn Fn(Option<Cursor>)> = Rc::new({
        let (
            get,
            generation,
            cursor,
            store,
            rows,
            next,
            export,
            load,
            journal,
            details,
            status,
            notice,
        ) = (
            get,
            generation.clone(),
            cursor.clone(),
            store.clone(),
            rows.clone(),
            next.clone(),
            export.clone(),
            load.clone(),
            journal.clone(),
            details.clone(),
            status.clone(),
            notice.clone(),
        );
        move |before| {
            let query = get();
            if let Err(error) = query.validate() {
                notice.show(&error, None, true);
                return;
            }
            let token = generation.get();
            load.set_sensitive(false);
            journal.set_sensitive(false);
            next.set_sensitive(false);
            export.set_sensitive(false);
            notice.root.set_visible(false);
            rows.borrow_mut().clear();
            store.remove_all();
            details.buffer().set_text("Working…");
            status.set_text("Reading bounded history and selected crash sources…");
            let (
                generation,
                cursor,
                store,
                rows,
                next,
                export,
                load,
                journal,
                details,
                status,
                notice,
            ) = (
                generation.clone(),
                cursor.clone(),
                store.clone(),
                rows.clone(),
                next.clone(),
                export.clone(),
                load.clone(),
                journal.clone(),
                details.clone(),
                status.clone(),
                notice.clone(),
            );
            job(
                move || activity::load(&query, before.as_ref()),
                move |result| {
                    load.set_sensitive(true);
                    journal.set_sensitive(true);
                    if token != generation.get() {
                        return;
                    }
                    match result {
                        Ok(page) => {
                            let objects: Vec<_> = page
                                .events
                                .iter()
                                .cloned()
                                .map(glib::BoxedAnyObject::new)
                                .collect();
                            store.splice(0, store.n_items(), &objects);
                            cursor.set(page.next);
                            next.set_sensitive(page.next.is_some());
                            export.set_sensitive(!page.events.is_empty());
                            status.set_text(&format!(
                                "{} record{} on this page{} · queries are paused until requested",
                                page.events.len(),
                                if page.events.len()==1{""}else{"s"},
                                if page.next.is_some() {
                                    " · older records available"
                                } else {
                                    ""
                                }
                            ));
                            details.buffer().set_text(if page.events.is_empty() {"No matching recorded activity in this range. Change filters or read the maintenance journal. Missing or unreadable sources are listed above."}else {"Select a record for its evidence, timing and result."});
                            if !page.warnings.is_empty() {
                                notice.show(
                                    "Some history has a limited scope.",
                                    Some(&page.warnings.join("\n\n")),
                                    false,
                                );
                            }
                            *rows.borrow_mut() = page.events;
                        }
                        Err(error) => {
                            cursor.set(None);
                            details
                                .buffer()
                                .set_text(&format!("Could not load activity: {error}"));
                            notice.show("Activity could not be loaded.", Some(&error), true);
                            status.set_text("No previous page is displayed under these filters.");
                        }
                    }
                },
            );
        }
    });
    let read_first = read.clone();
    load.connect_clicked(move |_| read_first(None));
    next.connect_clicked(move |_| {
        if let Some(before) = cursor.get() {
            read(Some(before));
        }
    });
    let (jload, jnext, jexport, gen, jdetails, jstatus, jnotice) = (
        load.clone(),
        next.clone(),
        export.clone(),
        generation.clone(),
        details.clone(),
        status.clone(),
        notice.clone(),
    );
    let clear = invalidate.clone();
    journal.connect_clicked(move|button| {
        clear();let token=gen.get();button.set_sensitive(false);jload.set_sensitive(false);jnext.set_sensitive(false);jexport.set_sensitive(false);jdetails.buffer().set_text("Working…");
        let (button,load,gen,details,status,notice)=(button.clone(),jload.clone(),gen.clone(),jdetails.clone(),jstatus.clone(),jnotice.clone());
        job(|| {
            #[cfg(feature="docs-demo")]
            if std::env::var_os("TEND_DEMO_DIR").is_some() {return Ok("Isolated maintenance journal preview. No installed service was read or run.".into());}
            tend::queries::run(std::process::Command::new("/usr/bin/journalctl").args(["-u","dev-maintenance.service","-n","200","--no-pager","--output=short-iso"]),Duration::from_secs(15),1024*1024)
        },move|result| {button.set_sensitive(true);load.set_sensitive(true);if gen.get()!=token {return;}match result {Ok(text)=>{details.buffer().set_text(&text);status.set_text("Maintenance journal · up to 200 recent lines; structured history remains separate.");},Err(error)=>{details.buffer().set_text(&format!("Could not read maintenance journal: {error}"));notice.show("Maintenance journal unavailable.",Some(&error),true);}}});
    });
    let export_status = status.clone();
    export.connect_clicked(move |button| {
        let token = generation.get();
        let (generation, rows) = (generation.clone(), rows.clone());
        let report = rows
            .borrow()
            .iter()
            .map(|e| e.details.as_str())
            .collect::<Vec<_>>()
            .join("\n\n────────────────\n\n");
        let (button, status) = (button.clone(), export_status.clone());
        button.set_sensitive(false);
        job(
            move || w::export_report(&report, false),
            move |result| {
                if generation.get() == token {
                    button.set_sensitive(!rows.borrow().is_empty());
                    status.set_text(&result.unwrap_or_else(|e| format!("Export failed: {e}")));
                }
            },
        );
    });
    let pause = invalidate.clone();
    stack.connect_visible_child_name_notify(move |s| {
        if s.visible_child_name().as_deref() != Some("activity") {
            pause();
        }
    });
    win.connect_unmap(move |_| invalidate());
    stack.add_titled(&scroll(&body), Some("activity"), "Activity");
}
