use crate::{job, label, page, scroll};
use adw::prelude::*;
use gtk::{gio, glib, Orientation};
use std::{cell::{Cell, RefCell}, rc::Rc, sync::Arc};
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
    let entries=Rc::new(RefCell::new(Vec::<Arc<logs::Entry>>::new()));
    let shown=Rc::new(RefCell::new(Vec::<usize>::new()));
    let store=gio::ListStore::new::<glib::BoxedAnyObject>();
    let selection=gtk::SingleSelection::new(Some(store.clone()));selection.set_autoselect(false);selection.set_can_unselect(true);
    let factory=gtk::SignalListItemFactory::new();
    factory.connect_setup(|_,item|{
        let item=item.downcast_ref::<gtk::ListItem>().unwrap();let row=gtk::Box::new(Orientation::Horizontal,12);row.set_widget_name("log-event-row");
        row.set_margin_start(12);row.set_margin_end(12);row.set_margin_top(10);row.set_margin_bottom(10);
        let icon=gtk::Image::new();row.append(&icon);let words=gtk::Box::new(Orientation::Vertical,4);words.set_hexpand(true);
        let title=gtk::Label::new(None);title.set_xalign(0.);title.set_ellipsize(gtk::pango::EllipsizeMode::End);words.append(&title);
        let message=gtk::Label::new(None);message.set_xalign(0.);message.add_css_class("dim-label");message.set_ellipsize(gtk::pango::EllipsizeMode::End);words.append(&message);row.append(&words);item.set_child(Some(&row));
    });
    let data=entries.clone();factory.connect_bind(move|_,item|{
        let item=item.downcast_ref::<gtk::ListItem>().unwrap();let object=item.item().unwrap().downcast::<glib::BoxedAnyObject>().unwrap();let index=*object.borrow::<usize>();let entries=data.borrow();let Some(event)=entries.get(index)else{return;};
        let row=item.child().unwrap().downcast::<gtk::Box>().unwrap();let icon=row.first_child().unwrap().downcast::<gtk::Image>().unwrap();let words=row.last_child().unwrap().downcast::<gtk::Box>().unwrap();
        let title=words.first_child().unwrap().downcast::<gtk::Label>().unwrap();let message=words.last_child().unwrap().downcast::<gtk::Label>().unwrap();
        title.set_text(&format!("{}{} · {}",if event.source.starts_with("Crash report ·"){"Updated "}else{""},logs::time(event.time),event.source));
        message.set_text(&event.message.chars().take(220).collect::<String>().replace('\n'," "));
        row.set_tooltip_text(Some(&tend::presentation::exact(event.time)));
        icon.set_icon_name(Some(if event.crash||event.priority<=3{"dialog-error-symbolic"}else if event.priority==4{"dialog-warning-symbolic"}else{"dialog-information-symbolic"}));
    });
    let list=gtk::ListView::new(Some(selection.clone()),Some(factory));list.set_widget_name("log-events-view");list.add_css_class("boxed-list");list.set_show_separators(true);
    list.update_property(&[gtk::accessible::Property::Label("Matching log events")]);
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
    pane.add_css_class("card");
    p.append(&pane);
    let paging=crate::workbench::buttons(&p);let previous=gtk::Button::with_label("Previous events");let next=gtk::Button::with_label("Next events");let range=label("No events loaded","caption");
    previous.set_sensitive(false);next.set_sensitive(false);paging.insert(&previous,-1);paging.insert(&next,-1);paging.insert(&range,-1);
    export.set_tooltip_text(Some("Export all matching loaded events, including other pages"));
    let page_index=Rc::new(Cell::new(0usize));let export_busy=Rc::new(Cell::new(false));let epoch=Rc::new(Cell::new(0u64));
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
    let render: Rc<dyn Fn()> = Rc::new({
        let(page_index,previous,next,range,busy)=(page_index.clone(),previous.downgrade(),next.downgrade(),range.clone(),export_busy.clone());
        let store=store.clone();let selection=selection.clone();
        let results = results.clone();
        let empty_title = empty_title.clone();
        let empty_description = empty_description.clone();
        let entries = entries.clone();
        let shown = shown.clone();
        let search = search.downgrade();
        let priority = priority.downgrade();
        let status = status.clone();
        let export = export.downgrade();
        let detail = detail.buffer();
        move || {
            let(Some(search),Some(priority),Some(export),Some(previous),Some(next))=(search.upgrade(),priority.upgrade(),export.upgrade(),previous.upgrade(),next.upgrade())else{return;};
            let query = search.text().to_lowercase();
            let max = match priority.selected() {
                1 => 3,
                2 => 4,
                3 => 6,
                _ => 7,
            };
            let filtered: Vec<_> = entries
                .borrow()
                .iter().enumerate()
                .filter(|(_,e)| {
                    e.priority <= max
                        && (query.is_empty()
                            || format!("{} {} {}", e.source, e.message, e.details)
                                .to_lowercase()
                                .contains(&query))
                })
                .map(|(index,_)|index)
                .collect();
            detail.set_text("Select an event to see its details.");
            selection.set_selected(gtk::INVALID_LIST_POSITION);
            let page=page_index.get().min(filtered.len().saturating_sub(1)/25);page_index.set(page);let start=page*25;let end=(start+25).min(filtered.len());
            let visible=filtered[start..end].iter().map(|index|glib::BoxedAnyObject::new(*index)).collect::<Vec<_>>();
            store.splice(0,store.n_items(),&visible);
            previous.set_sensitive(page>0);next.set_sensitive(end<filtered.len());
            range.set_text(&if filtered.is_empty(){"No matching events".into()}else{format!("{}–{} of {}",start+1,end,filtered.len())});
            status.set_text(&format!("{} matching events. Pages show 25 at a time; export includes all matches. Searches apply to the most recent 1,000 loaded records. Missing entries may be rotated or unreadable.",filtered.len()));
            if filtered.is_empty() {
                empty_title.set_text("No matching events");
                empty_description.set_text("Try a wider time range or a different filter. Protected and rotated logs may be unavailable.");
                results.set_visible_child_name("empty");
            } else {
                results.set_visible_child_name("events");
            }
            export.set_sensitive(!filtered.is_empty()&&!busy.get());
            *shown.borrow_mut() = filtered;
        }
    });
    let (render_previous,index)=(render.clone(),page_index.clone());previous.connect_clicked(move |_|{index.set(index.get().saturating_sub(1));render_previous();});
    let (render_next,index)=(render.clone(),page_index.clone());next.connect_clicked(move |_|{index.set(index.get().saturating_add(1));render_next();});
    let filter_epoch=epoch.clone();let filter:Rc<dyn Fn()>=Rc::new(move||{filter_epoch.set(filter_epoch.get().wrapping_add(1));page_index.set(0);render();});
    let f = filter.clone();
    search.connect_search_changed(move |_| f());
    let f = filter.clone();
    priority.connect_selected_notify(move |_| f());
    // Source/time controls describe the next query. Do not leave a report from
    // the previous query under those controls or allow it to be exported.
    let invalidate: Rc<dyn Fn()> = Rc::new({
        let (entries, filter, title, description, status, notice) = (
            entries.clone(), filter.clone(), empty_title.clone(),
            empty_description.clone(), status.clone(), notice.clone(),
        );
        move || {
            entries.borrow_mut().clear();
            filter();
            notice.root.set_visible(false);
            title.set_text("Choose logs to inspect");
            description.set_text("Select Load logs to read this source and time range.");
            status.set_text("Source or time range changed. Select Load logs to refresh.");
        }
    });
    let changed = invalidate.clone();
    source.connect_selected_notify(move |_| changed());
    period.connect_selected_notify(move |_| invalidate());
    let data = entries.clone();
    let f = filter;
    let src = source.clone();
    let per = period.clone();
    let st = status.clone();
    let loading_results = results.clone();
    let loading_title = empty_title.clone();
    let loading_description = empty_description.clone();
    let query_controls: Vec<gtk::Widget> = vec![
        source.clone().upcast(), period.clone().upcast(),
        priority.clone().upcast(), search.clone().upcast(), crashes.clone().upcast(),
    ];
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
        data.borrow_mut().clear();
        f();
        for control in &query_controls {
            control.set_sensitive(false);
        }
        let query_controls = query_controls.clone();
        notice.root.set_visible(false);
        loading_title.set_text("Loading logs…");
        loading_description
            .set_text("Reading up to 1,000 recent records. Existing logs are never changed.");
        loading_results.set_visible_child_name("empty");
        b.set_sensitive(false);
        st.set_text("Loading logs…");
        let (source, period) = (src.selected(), per.selected());
        job(
            move || logs::entries_cancellable(source,period,1000,&std::sync::atomic::AtomicBool::new(false)).map(|rows|rows.into_iter().map(Arc::new).collect::<Vec<_>>()),
            move |result| {
                b.set_sensitive(true);
                for control in &query_controls {
                    control.set_sensitive(true);
                }
                match result {
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
    let button = load.downgrade();let(source,period,priority)=(source.downgrade(),period.downgrade(),priority.downgrade());
    crashes.connect_clicked(move |_| {
        let(Some(button),Some(source),Some(period),Some(priority))=(button.upgrade(),source.upgrade(),period.upgrade(),priority.upgrade())else{return;};
        source.set_selected(2);
        period.set_selected(3);
        priority.set_selected(0);
        button.emit_clicked();
    });
    let selected_entries=entries.clone();let buffer=detail.buffer();
    selection.connect_selected_item_notify(move|selection|{
        let object=selection.selected_item().and_then(|o|o.downcast::<glib::BoxedAnyObject>().ok());
        let index=object.as_ref().map(|o|*o.borrow::<usize>());let data=selected_entries.borrow();
        if let Some(event)=index.and_then(|i|data.get(i)){buffer.set_text(&format!("{}\n{}",tend::presentation::exact(event.time),event.details));}
        else{buffer.set_text("Select an event to see its details.");}
    });
    export.connect_clicked(move |button| {
        if export_busy.replace(true){return;}
        let data={let entries=entries.borrow();shown.borrow().iter().filter_map(|i|entries.get(*i).cloned()).collect::<Vec<_>>()};
        let token=epoch.get();button.set_sensitive(false);status.set_text("Exporting matching events…");
        let(button,status,shown,busy,epoch)=(button.downgrade(),status.downgrade(),shown.clone(),export_busy.clone(),epoch.clone());
        job(move||{let mut text=String::new();for event in data{text.push_str(&format!("{}\n{}\n\n",tend::presentation::exact(event.time),event.details));}w::export_report(&text,false)},move|result|{
            busy.set(false);if let Some(button)=button.upgrade(){button.set_sensitive(!shown.borrow().is_empty());}
            if epoch.get()==token{if let Some(status)=status.upgrade(){status.set_text(&result.unwrap_or_else(|e|e));}}
        });
    });
    let _ = win;
    stack.add_titled(&scroll(&p), Some("system-logs"), "System logs");
}
