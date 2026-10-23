//! Compile-only documentation renderer. Never included in default app builds.
//! Records real GTK widgets on a separate virtual display. No system actions,
//! log loads, policy saves, deletion, process signals or privileged calls.
use adw::prelude::*;
use gtk::{glib, Widget};
use std::{fs, path::Path, time::Duration};

fn descendants(root: &impl IsA<Widget>) -> Vec<Widget> {
    let root = root.as_ref();
    let mut result = vec![root.clone()];
    let mut child = root.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        result.extend(descendants(&widget));
    }
    result
}

fn performance(stack: &gtk::Stack) {
    stack.set_visible_child_name("task-manager");
    if let Some(page) = stack.child_by_name("task-manager") {
        for widget in descendants(&page) {
            if let Some(tabs) = widget.downcast_ref::<gtk::Stack>() {
                if tabs.child_by_name("performance").is_some() {
                    tabs.set_visible_child_name("performance");
                }
            }
            if let Some(speed) = widget.downcast_ref::<gtk::DropDown>() {
                // Only update the in-memory sampling control; never save settings.
                speed.set_selected(1);
            }
        }
    }
}

fn resource(stack: &gtk::Stack, name: &str) {
    if let Some(page) = stack.child_by_name("task-manager") {
        for widget in descendants(&page) {
            if let Some(row) = widget.downcast_ref::<gtk::ListBoxRow>() {
                if row.widget_name() == name {
                    if let Some(list) = row.parent().and_downcast::<gtk::ListBox>() {
                        list.select_row(Some(row));
                    }
                }
            }
        }
    }
}

pub fn record(
    app: &adw::Application,
    win: &adw::ApplicationWindow,
    outer: &gtk::Box,
    stack: &gtk::Stack,
    directory: &str,
) {
    let directory = Path::new(directory).to_path_buf();
    fs::create_dir_all(&directory).expect("Create documentation frame directory");
    eprintln!("Starting native demo renderer");
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
    performance(stack);
    let (app, win, outer, stack) = (app.clone(), win.clone(), outer.clone(), stack.clone());
    let paint = gtk::WidgetPaintable::new(Some(&outer));
    let mut tick = 0usize;
    // Warm up real counters for 24 seconds so the chart has useful history.
    let warmup = 192;
    glib::timeout_add_local(Duration::from_millis(125), move || {
        tick += 1;
        if tick % 64 == 0 {
            eprintln!(
                "Demo tick {tick}: mapped={}, size={}x{}",
                win.is_mapped(),
                outer.width(),
                outer.height()
            );
        }
        if tick < warmup {
            if tick == warmup - 8 {
                stack.set_visible_child_name("overview");
            }
            if tick % 8 == 0 {
                // Xvfb has no window manager/focus. Use the existing read-only
                // refresh callback rather than changing normal focus policy.
                if let Some(page) = stack.child_by_name("task-manager") {
                    for widget in descendants(&page) {
                        if let Some(button) = widget.downcast_ref::<gtk::Button>() {
                            if button.label().as_deref() == Some("Refresh now") {
                                button.emit_clicked();
                            }
                        }
                    }
                }
            }
            return glib::ControlFlow::Continue;
        }
        let frame = tick - warmup;
        match frame {
            0 | 232 => stack.set_visible_child_name("overview"),
            24 => performance(&stack),
            64 => resource(&stack, "memory"),
            88 => resource(&stack, "cpu"),
            104 => stack.set_visible_child_name("cleanup"),
            132 => {
                if let Some(page) = stack.child_by_name("cleanup") {
                    if let Some(scroll) = page.downcast_ref::<gtk::ScrolledWindow>() {
                        let adjustment = scroll.vadjustment();
                        adjustment.set_value(adjustment.upper() - adjustment.page_size());
                    }
                }
            }
            160 => stack.set_visible_child_name("system-logs"),
            176 => {
                if let Some(page) = stack.child_by_name("system-logs") {
                    for widget in descendants(&page) {
                        if let Some(dropdown) = widget.downcast_ref::<gtk::DropDown>() {
                            match dropdown.tooltip_text().as_deref() {
                                Some("Log source") => dropdown.set_selected(2),
                                Some("Severity filter") => dropdown.set_selected(1),
                                _ => (),
                            }
                        }
                    }
                }
            }
            200 => stack.set_visible_child_name("tuning"),
            256 => {
                app.quit();
                return glib::ControlFlow::Break;
            }
            _ => (),
        }
        if (24..104).contains(&frame) && frame % 8 == 0 {
            if let Some(page) = stack.child_by_name("task-manager") {
                for widget in descendants(&page) {
                    if let Some(button) = widget.downcast_ref::<gtk::Button>() {
                        if button.label().as_deref() == Some("Refresh now") {
                            button.emit_clicked();
                        }
                    }
                }
            }
        }
        let snapshot = gtk::Snapshot::new();
        paint.snapshot(&snapshot, outer.width() as f64, outer.height() as f64);
        if let (Some(node), Some(renderer)) = (snapshot.to_node(), win.renderer()) {
            renderer
                .render_texture(&node, None)
                .save_to_png(directory.join(format!("{frame:04}.png")))
                .expect("Save documentation frame");
        }
        glib::ControlFlow::Continue
    });
}
