use crate::{job, label, page, scroll};
use adw::prelude::*;
use gtk::Orientation;
use std::{cell::RefCell, rc::Rc};
use tend::{system_logs as logs, workstation as w};
pub fn add(stack: &gtk::Stack, win: &adw::ApplicationWindow) {
    let p=page("System logs","Read recent events and crashed applications. Filter the results, then select an event for details. Logs are loaded only on request.");
    let controls = crate::workbench::buttons(&p);
    let source =
        gtk::DropDown::from_strings(&["System journal", "My application logs", "Recent crashes"]);
    source.set_tooltip_text(Some("Log source"));
    let period =
        gtk::DropDown::from_strings(&["This boot", "Last hour", "Last 24 hours", "Last 7 days"]);
    period.set_selected(2);
    period.set_tooltip_text(Some("Time range"));
    let priority = gtk::DropDown::from_strings(&[
        "All severities",
        "Errors and critical",
        "Warnings and worse",
        "Information and worse",
    ]);
    priority.set_tooltip_text(Some("Severity filter"));
    controls.insert(&source, -1);
    controls.insert(&period, -1);
    controls.insert(&priority, -1);

    let search = gtk::SearchEntry::builder()
        .placeholder_text("Search application, service, PID or message")
        .hexpand(true)
        .build();
    p.append(&search);
    let actions = crate::workbench::buttons(&p);
    let load = gtk::Button::with_label("Load logs");
    load.add_css_class("suggested-action");
    let crashes = gtk::Button::with_label("Last crashed apps");
    let export = gtk::Button::with_label("Export filtered logs");
    export.set_sensitive(false);
    actions.insert(&load, -1);
    actions.insert(&crashes, -1);
    actions.insert(&export, -1);

    let status=label("Choose a source and select Load logs. Protected reports are skipped without an authentication prompt.","caption");
    p.append(&status);
    let notice = crate::editor::Notice::new();
    p.append(&notice.root);
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::Single);
    let empty = gtk::Box::new(Orientation::Vertical, 10);
    empty.set_valign(gtk::Align::Center);
    empty.set_halign(gtk::Align::Center);
    empty.set_margin_top(24);
    empty.set_margin_bottom(24);
    let empty_icon = gtk::Image::from_icon_name("view-list-symbolic");
    empty_icon.set_pixel_size(36);
    empty_icon.add_css_class("dim-label");
    let empty_title = label("Choose logs to inspect", "heading");
    empty_title.set_halign(gtk::Align::Center);
    let empty_description = label(
        "Load recent events or view reported application crashes.",
        "caption",
    );
    empty_description.set_max_width_chars(52);
    empty_description.set_justify(gtk::Justification::Center);
    empty.append(&empty_icon);
    empty.append(&empty_title);
    empty.append(&empty_description);
    let results = gtk::Stack::new();
    results.add_named(&empty, Some("empty"));
    results.add_named(&list, Some("events"));
    results.set_visible_child_name("empty");
    let pane = gtk::ScrolledWindow::builder()
        .child(&results)
        .min_content_height(230)
        .vexpand(true)
        .build();
    p.append(&pane);
    let detail = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .wrap_mode(gtk::WrapMode::WordChar)
        .monospace(true)
        .top_margin(10)
        .bottom_margin(10)
        .left_margin(10)
        .right_margin(10)
        .build();
    detail
        .buffer()
        .set_text("Select an event to see its details. No log data is uploaded.");
    let detail_scroll = gtk::ScrolledWindow::builder()
        .child(&detail)
        .min_content_height(160)
        .build();
    detail_scroll.add_css_class("card");
    p.append(&label("Event details", "section-label"));
    p.append(&detail_scroll);
    let entries = Rc::new(RefCell::new(Vec::<logs::Entry>::new()));
    let shown = Rc::new(RefCell::new(Vec::<logs::Entry>::new()));
    let filter: Rc<dyn Fn()> = Rc::new({
        let list = list.clone();
        let results = results.clone();
        let empty_title = empty_title.clone();
        let empty_description = empty_description.clone();
        let entries = entries.clone();
        let shown = shown.clone();
        let search = search.clone();
        let priority = priority.clone();
        let status = status.clone();
        let export = export.clone();
        let detail = detail.buffer();
        move || {
            let query = search.text().to_lowercase();
            let max = match priority.selected() {
                1 => 3,
                2 => 4,
                3 => 6,
                _ => 7,
            };
            let filtered: Vec<_> = entries
                .borrow()
                .iter()
                .filter(|e| {
                    e.priority <= max
                        && (query.is_empty()
                            || format!("{} {} {}", e.source, e.message, e.details)
                                .to_lowercase()
                                .contains(&query))
                })
                .cloned()
                .collect();
            detail.set_text("Select an event to see its details.");
            while let Some(c) = list.first_child() {
                list.remove(&c);
            }
            for e in filtered.iter().take(500) {
                let row = adw::ActionRow::new();
                row.set_use_markup(false);
                row.set_title(&format!(
                    "{}{} · {}",
                    if e.source.starts_with("Crash report ·") {
                        "Updated "
                    } else {
                        ""
                    },
                    logs::time(e.time),
                    e.source
                ));
                row.set_tooltip_text(Some(&tend::presentation::exact(e.time)));
                row.set_subtitle(
                    &e.message
                        .chars()
                        .take(220)
                        .collect::<String>()
                        .replace('\n', " "),
                );
                let icon = gtk::Image::from_icon_name(if e.crash || e.priority <= 3 {
                    "dialog-error-symbolic"
                } else if e.priority == 4 {
                    "dialog-warning-symbolic"
                } else {
                    "dialog-information-symbolic"
                });
                row.add_prefix(&icon);
                list.append(&row);
            }
            status.set_text(&format!("{} matching events; showing up to 500. Searches apply to the most recent 1,000 loaded records. Missing entries may be rotated or unreadable.",filtered.len()));
            if filtered.is_empty() {
                empty_title.set_text("No matching events");
                empty_description.set_text("Try a wider time range or a different filter. Protected and rotated logs may be unavailable.");
                results.set_visible_child_name("empty");
            } else {
                results.set_visible_child_name("events");
            }
            export.set_sensitive(!filtered.is_empty());
            *shown.borrow_mut() = filtered;
        }
    });
    let f = filter.clone();
    search.connect_search_changed(move |_| f());
    let f = filter.clone();
    priority.connect_selected_notify(move |_| f());
    let data = entries;
    let f = filter;
    let src = source.clone();
    let per = period.clone();
    let st = status.clone();
    let loading_results = results.clone();
    let loading_title = empty_title.clone();
    let loading_description = empty_description.clone();
    load.connect_clicked(move |b| {
        let (b, data, f, st, notice, error_title, error_description) = (
            b.clone(),
            data.clone(),
            f.clone(),
            st.clone(),
            notice.clone(),
            loading_title.clone(),
            loading_description.clone(),
        );
        notice.root.set_visible(false);
        loading_title.set_text("Loading logs…");
        loading_description
            .set_text("Reading up to 1,000 recent records. Existing logs are never changed.");
        loading_results.set_visible_child_name("empty");
        b.set_sensitive(false);
        st.set_text("Loading logs…");
        let (source, period) = (src.selected(), per.selected());
        job(
            move || logs::load(source, period, 1000),
            move |result| {
                b.set_sensitive(true);
                match result.and_then(|s| {
                    serde_json::from_str::<Vec<logs::Entry>>(&s).map_err(|e| e.to_string())
                }) {
                    Ok(rows) => {
                        *data.borrow_mut() = rows;
                        f();
                    }
                    Err(e) => {
                        error_title.set_text("Logs unavailable");
                        error_description.set_text("Try another source or time range. Technical details are available above.");
                        st.set_text("Could not read logs. Try a different source or time range.");
                        notice.show("Logs could not be loaded.", Some(&e), true);
                    }
                }
            },
        );
    });
    let button = load.clone();
    crashes.connect_clicked(move |_| {
        source.set_selected(2);
        period.set_selected(3);
        priority.set_selected(0);
        button.emit_clicked();
    });
    let rows = shown.clone();
    let buffer = detail.buffer();
    list.connect_row_selected(move |_, row| {
        if let Some(r) = row {
            if let Some(e) = rows.borrow().get(r.index() as usize) {
                buffer.set_text(&format!(
                    "{}\n{}",
                    tend::presentation::exact(e.time),
                    e.details
                ));
            }
        } else {
            buffer.set_text("Select an event to see its details.");
        }
    });
    export.connect_clicked(move |_| {
        let mut text = String::new();
        for e in shown.borrow().iter() {
            text.push_str(&format!(
                "{}\n{}\n\n",
                tend::presentation::exact(e.time),
                e.details
            ));
        }
        status.set_text(&w::export_report(&text, false).unwrap_or_else(|e| e));
    });
    let _ = win;
    stack.add_titled(&scroll(&p), Some("system-logs"), "System logs");
}
