use crate::{job, label, page, scroll};
use adw::prelude::*;
use gtk::Orientation;
use std::{cell::RefCell, rc::Rc};
use tend::{system_logs as logs, workstation as w};
pub fn add(stack: &gtk::Stack, win: &adw::ApplicationWindow) {
    let p=page("System logs","Read recent events and crashed applications. Filter the results, then select an event for details. Logs are loaded only on request.");
    let controls = gtk::Box::new(Orientation::Horizontal, 8);
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
    controls.append(&source);
    controls.append(&period);
    controls.append(&priority);
    p.append(&controls);
    let search = gtk::SearchEntry::builder()
        .placeholder_text("Search application, service, PID or message")
        .hexpand(true)
        .build();
    p.append(&search);
    let actions = gtk::Box::new(Orientation::Horizontal, 8);
    let load = gtk::Button::with_label("Load logs");
    load.add_css_class("suggested-action");
    let crashes = gtk::Button::with_label("Last crashed apps");
    let export = gtk::Button::with_label("Export filtered logs");
    actions.append(&load);
    actions.append(&crashes);
    actions.append(&export);
    p.append(&actions);
    let status=label("Choose a source and select Load logs. Protected reports are skipped without an authentication prompt.","caption");
    p.append(&status);
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::Single);
    let pane = gtk::ScrolledWindow::builder()
        .child(&list)
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
    p.append(&detail_scroll);
    let entries = Rc::new(RefCell::new(Vec::<logs::Entry>::new()));
    let shown = Rc::new(RefCell::new(Vec::<logs::Entry>::new()));
    let filter: Rc<dyn Fn()> = Rc::new({
        let list = list.clone();
        let entries = entries.clone();
        let shown = shown.clone();
        let search = search.clone();
        let priority = priority.clone();
        let status = status.clone();
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
                row.set_title(&format!("{} · {}", logs::time(e.time), e.source));
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
    load.connect_clicked(move |b| {
        let (b, data, f, st) = (b.clone(), data.clone(), f.clone(), st.clone());
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
                    Err(e) => st.set_text(&format!("Could not read logs: {e}")),
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
                buffer.set_text(&format!("{}\n{}", logs::time(e.time), e.details));
            }
        } else {
            buffer.set_text("Select an event to see its details.");
        }
    });
    export.connect_clicked(move |_| {
        let mut text = String::new();
        for e in shown.borrow().iter() {
            text.push_str(&format!("{}\n{}\n\n", logs::time(e.time), e.details));
        }
        status.set_text(&w::export_report(&text, false).unwrap_or_else(|e| e));
    });
    let _ = win;
    stack.add_titled(&scroll(&p), Some("system-logs"), "System logs");
}
