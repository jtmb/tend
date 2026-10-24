use crate::{column, label};
use adw::prelude::*;
use gtk::{gio, glib, Orientation};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    rc::Rc,
    sync::mpsc,
    time::Duration,
};
use tend::{
    task_stats::{self as stats, Snapshot, Task},
    workstation as w,
};
#[derive(Clone, Default)]
struct Trace {
    points: VecDeque<(f64, f64)>,
    percent: bool,
    title: String,
    detail: String,
    color: (f64, f64, f64),
}
#[derive(Default)]
struct State {
    previous: Option<Snapshot>,
    traces: BTreeMap<String, Trace>,
    diagnostics: String,
}
fn rate(v: f64) -> String {
    format!("{}/s", w::bytes(v.max(0.) as u64))
}
fn task(object: &glib::Object) -> std::cell::Ref<'_, Task> {
    object
        .downcast_ref::<glib::BoxedAnyObject>()
        .unwrap()
        .borrow::<Task>()
}
fn cell(column: usize, t: &Task) -> String {
    match column {
        0 => t.name.clone(),
        1 => t
            .cpu
            .map(|v| format!("{v:.1}%"))
            .unwrap_or_else(|| "—".into()),
        2 => w::bytes(t.memory),
        3 => t
            .read_rate
            .zip(t.write_rate)
            .map(|(r, w)| rate(r + w))
            .unwrap_or_else(|| "—".into()),
        4 => t.pid.to_string(),
        5 => t.user.clone(),
        _ => t.status.clone(),
    }
}
fn compare(column: usize, a: &Task, b: &Task) -> std::cmp::Ordering {
    match column {
        0 => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        1 => a.cpu.unwrap_or(-1.).total_cmp(&b.cpu.unwrap_or(-1.)),
        2 => a.memory.cmp(&b.memory),
        3 => (a.read_rate.unwrap_or(-1.) + a.write_rate.unwrap_or(-1.))
            .total_cmp(&(b.read_rate.unwrap_or(-1.) + b.write_rate.unwrap_or(-1.))),
        4 => a.pid.cmp(&b.pid),
        5 => a.user.cmp(&b.user),
        _ => a.status.cmp(&b.status),
    }
}
fn add_trace(
    state: &mut State,
    id: &str,
    title: String,
    detail: String,
    value: (f64, f64),
    percent: bool,
    color: (f64, f64, f64),
) {
    let t = state.traces.entry(id.into()).or_default();
    t.title = title;
    t.detail = detail;
    t.percent = percent;
    t.color = color;
    t.points.push_back(value);
    if t.points.len() > 60 {
        t.points.pop_front();
    }
}
fn ingest(state: &mut State, s: Snapshot) {
    state.diagnostics = w::sample_text(&s.system, state.previous.as_ref().map(|p| &p.system));
    let mem = &s.system;
    let used = mem.memory_total.saturating_sub(mem.memory_available);
    add_trace(
        state,
        "cpu",
        "CPU".into(),
        format!(
            "{} utilization · {} logical processors\nUptime {} · {} processes",
            s.cpu
                .map(|v| format!("{v:.1}%"))
                .unwrap_or_else(|| "Collecting…".into()),
            s.cores,
            s.uptime,
            s.tasks.len()
        ),
        (s.cpu.unwrap_or(0.), 0.),
        true,
        (0.12, 0.5, 0.85),
    );
    add_trace(
        state,
        "memory",
        "Memory".into(),
        format!(
            "{} / {} used · {} available\nSwap {} / {} used · available-memory estimate",
            w::bytes(used),
            w::bytes(mem.memory_total),
            w::bytes(mem.memory_available),
            w::bytes(mem.swap_total.saturating_sub(mem.swap_free)),
            w::bytes(mem.swap_total)
        ),
        (100. * used as f64 / mem.memory_total.max(1) as f64, 0.),
        true,
        (0.62, 0.32, 0.8),
    );
    for d in &s.disks {
        add_trace(
            state,
            &format!("disk:{}", d.name),
            format!("Disk · {}", d.name),
            format!(
                "{:.1}% active time\nRead {} · Write {}",
                d.busy,
                rate(d.read_rate),
                rate(d.write_rate)
            ),
            (d.busy, 0.),
            true,
            (0.25, 0.64, 0.3),
        );
    }
    for a in &s.adapters {
        add_trace(
            state,
            &format!("net:{}", a.name),
            format!("Network · {}", a.name),
            format!(
                "Receive {} · Send {}\nAdapter totals; loopback traffic excluded",
                rate(a.receive_rate),
                rate(a.send_rate)
            ),
            (a.receive_rate, a.send_rate),
            false,
            (0.85, 0.48, 0.12),
        );
    }
    for (name, busy) in &s.gpu {
        add_trace(
            state,
            &format!("gpu:{name}"),
            format!("GPU · {name}"),
            format!("{busy:.1}% busy · kernel-reported utilization"),
            (*busy, 0.),
            true,
            (0.7, 0.25, 0.45),
        );
    }
    // Keep histories only for resources still present.
    state.traces.retain(|id, _| {
        id == "cpu"
            || id == "memory"
            || s.disks.iter().any(|d| id == &format!("disk:{}", d.name))
            || s.adapters.iter().any(|a| id == &format!("net:{}", a.name))
            || s.gpu.iter().any(|(n, _)| id == &format!("gpu:{n}"))
    });
    state.previous = Some(s);
}
struct Ui {
    store: gio::ListStore,
    selection: gtk::SingleSelection,
    sorted: gtk::SortListModel,
    summary: gtk::Label,
    status: gtk::Label,
    resources: gtk::ListBox,
    keys: RefCell<Vec<String>>,
    title: gtk::Label,
    details: gtk::Label,
    scale: gtk::Label,
    legend: gtk::Label,
    graph: gtk::DrawingArea,
    selected: Rc<RefCell<String>>,
    info: gtk::Label,
    diagnostics: gtk::Label,
    quit: gtk::Button,
    end: gtk::Button,
}
impl Ui {
    fn update(&self, state: &State) {
        let Some(s) = &state.previous else { return };
        let selected = self.selection.selected_item().map(|v| {
            let t = task(&v);
            (t.pid, t.start)
        });
        let values: Vec<_> = s
            .tasks
            .iter()
            .cloned()
            .map(glib::BoxedAnyObject::new)
            .collect();
        self.store.splice(0, self.store.n_items(), &values);
        if let Some(id) = selected {
            if let Some(i) = (0..self.sorted.n_items()).find(|i| {
                let o = self.sorted.item(*i).unwrap();
                let t = task(&o);
                (t.pid, t.start) == id
            }) {
                self.selection.set_selected(i);
            }
        }
        self.summary.set_text(&format!(
            "CPU {}    Memory {:.0}%    {} processes",
            s.cpu
                .map(|v| format!("{v:.0}%"))
                .unwrap_or_else(|| "—".into()),
            100. * s
                .system
                .memory_total
                .saturating_sub(s.system.memory_available) as f64
                / s.system.memory_total.max(1) as f64,
            s.tasks.len()
        ));
        self.status.set_text("CPU: share of total capacity. Memory: RSS, including shared pages. Disk I/O: physical bytes. — means unavailable or collecting.");
        let mut keys: Vec<_> = state.traces.keys().cloned().collect();
        keys.sort_by_key(|s| {
            let rank = if s == "cpu" {
                0
            } else if s == "memory" {
                1
            } else if s.starts_with("disk:") {
                2
            } else if s.starts_with("net:") {
                3
            } else {
                4
            };
            let virtual_adapter = s.strip_prefix("net:").is_some_and(|name| {
                !std::path::Path::new("/sys/class/net")
                    .join(name)
                    .join("device")
                    .exists()
            });
            (rank, virtual_adapter, s.clone())
        });
        let rebuild = *self.keys.borrow() != keys;
        if rebuild {
            while let Some(c) = self.resources.first_child() {
                self.resources.remove(&c);
            }
            for key in &keys {
                let trace = &state.traces[key];
                let r = adw::ActionRow::new();
                r.set_use_markup(false);
                r.set_title(&trace.title);
                r.set_subtitle(trace.detail.lines().next().unwrap_or(""));
                r.set_widget_name(key);
                self.resources.append(&r);
            }
            *self.keys.borrow_mut() = keys.clone();
        }
        for (i, key) in keys.iter().enumerate() {
            if let Some(r) = self.resources.row_at_index(i as i32) {
                if let Ok(row) = r.clone().downcast::<adw::ActionRow>() {
                    row.set_subtitle(state.traces[key].detail.lines().next().unwrap_or(""));
                }
                let is_selected = { key == self.selected.borrow().as_str() };
                if is_selected {
                    self.resources.select_row(Some(&r));
                }
            }
        }
        self.diagnostics.set_text(&state.diagnostics);
        self.paint(state);
    }
    fn paint(&self, state: &State) {
        let key = self.selected.borrow();
        let Some(trace) = state.traces.get(key.as_str()) else {
            return;
        };
        self.title.set_text(&trace.title);
        self.legend.set_text(if key.starts_with("net:") {
            "60 samples · oldest → newest · receive: solid · send: dashed"
        } else if key.starts_with("disk:") {
            "60 samples · oldest → newest · read: solid · write: dashed"
        } else if key.as_str() == "memory" {
            "60 samples · oldest → newest · physical memory in use"
        } else if key.starts_with("gpu:") {
            "60 samples · oldest → newest · GPU utilization"
        } else {
            "60 samples · oldest → newest · total CPU utilization"
        });
        self.details.set_text(&trace.detail);
        self.scale.set_text(&if trace.percent {
            "100%".into()
        } else {
            rate(
                trace
                    .points
                    .iter()
                    .map(|v| v.0.max(v.1))
                    .fold(1024., f64::max),
            )
        });
        self.graph.queue_draw();
    }
}
pub fn add(stack: &gtk::Stack, win: &adw::ApplicationWindow) {
    let root = column(14);
    root.set_margin_top(24);
    root.set_margin_bottom(24);
    root.set_margin_start(24);
    root.set_margin_end(24);
    root.append(&label("Task Manager", "page-title"));
    let tabs = gtk::Stack::builder().vexpand(true).hexpand(true).build();
    let switcher = gtk::StackSwitcher::builder()
        .stack(&tabs)
        .halign(gtk::Align::Start)
        .build();
    root.append(&switcher);
    let summary = label("Open this page to collect live counters.", "title-4");
    root.append(&summary);
    root.append(&tabs);
    let process = column(10);
    let toolbar = column(8);
    let search = gtk::SearchEntry::builder()
        .placeholder_text("Search name, PID or user")
        .hexpand(true)
        .build();
    toolbar.append(&search);
    let quit = gtk::Button::with_label("Request quit…");
    let end = gtk::Button::with_label("End task…");
    end.add_css_class("destructive-action");
    quit.set_sensitive(false);
    end.set_sensitive(false);
    let actions = crate::workbench::buttons(&toolbar);
    actions.insert(&quit, -1);
    actions.insert(&end, -1);
    process.append(&toolbar);
    let store = gio::ListStore::new::<glib::BoxedAnyObject>();
    let query = Rc::new(RefCell::new(String::new()));
    let mine = Rc::new(Cell::new(false));
    let q = query.clone();
    let own = mine.clone();
    let filter = gtk::CustomFilter::new(move |o| {
        let t = task(o);
        let q = q.borrow();
        (!own.get() || t.uid == unsafe { libc::geteuid() })
            && (q.is_empty()
                || format!("{} {} {}", t.name, t.pid, t.user)
                    .to_lowercase()
                    .contains(q.as_str()))
    });
    let filtered = gtk::FilterListModel::new(Some(store.clone()), Some(filter.clone()));
    let sorted = gtk::SortListModel::new(Some(filtered), None::<gtk::Sorter>);
    let selection = gtk::SingleSelection::new(Some(sorted.clone()));
    selection.set_autoselect(false);
    selection.set_can_unselect(true);
    let table = gtk::ColumnView::new(Some(selection.clone()));
    table.set_show_row_separators(true);
    table.set_show_column_separators(true);
    table.add_css_class("data-table");
    let mut memory_column = None;
    for (i, title) in ["Name", "CPU", "Memory", "Disk I/O", "PID", "User", "Status"]
        .iter()
        .enumerate()
    {
        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(move |_, o| {
            let item = o.downcast_ref::<gtk::ListItem>().unwrap();
            let text = gtk::Label::builder()
                .xalign(if i == 0 || i >= 5 { 0. } else { 1. })
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .margin_start(4)
                .margin_end(4)
                .margin_top(8)
                .margin_bottom(8)
                .build();
            if (1..=3).contains(&i) {
                text.add_css_class("tm-usage");
            }
            item.set_child(Some(&text));
        });
        factory.connect_bind(move |_, o| {
            let item = o.downcast_ref::<gtk::ListItem>().unwrap();
            let value = item.item().unwrap();
            let t = task(&value);
            let text = item.child().unwrap().downcast::<gtk::Label>().unwrap();
            text.set_text(&cell(i, &t));
            text.set_tooltip_text(Some(&format!(
                "{} · PID {} · {}\n{}",
                t.name, t.pid, t.user, t.executable
            )));
        });
        let col = gtk::ColumnViewColumn::new(Some(title), Some(factory));
        col.set_resizable(true);
        col.set_expand(i == 0);
        col.set_fixed_width(match i {
            0 => 150,
            1 => 75,
            2 => 110,
            3 => 110,
            4 => 85,
            5 => 85,
            _ => 100,
        });
        col.set_sorter(Some(&gtk::CustomSorter::new(move |a, b| {
            compare(i, &task(a), &task(b)).into()
        })));
        table.append_column(&col);
        if i == 2 {
            memory_column = Some(col);
        }
    }
    sorted.set_sorter(table.sorter().as_ref());
    table.sort_by_column(memory_column.as_ref(), gtk::SortType::Descending);
    let table_scroll = gtk::ScrolledWindow::builder()
        .child(&table)
        .vexpand(true)
        .hexpand(true)
        .min_content_height(240)
        .build();
    table_scroll.add_css_class("card");
    process.append(&table_scroll);
    let info = label(
        "Select a process for details. Other users’ processes are read-only.",
        "caption",
    );
    process.append(&info);
    let only = gtk::CheckButton::with_label("Only my processes");
    process.append(&only);
    let f = filter.clone();
    search.connect_search_changed(move |s| {
        *query.borrow_mut() = s.text().to_lowercase();
        f.changed(gtk::FilterChange::Different);
    });
    only.connect_toggled(move |b| {
        mine.set(b.is_active());
        filter.changed(gtk::FilterChange::Different);
    });
    tabs.add_titled(&process, Some("processes"), "Processes");
    let performance = gtk::Box::new(Orientation::Horizontal, 16);
    let resources = gtk::ListBox::new();
    resources.add_css_class("boxed-list");
    resources.set_size_request(185, -1);
    let rs = gtk::ScrolledWindow::builder()
        .child(&resources)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .min_content_width(185)
        .build();
    performance.append(&rs);
    let panel = column(14);
    panel.set_hexpand(true);
    let title = label("CPU", "page-title");
    panel.append(&title);
    let scale = label("100%", "caption");
    scale.set_halign(gtk::Align::End);
    panel.append(&scale);
    let graph = gtk::DrawingArea::builder()
        .content_height(250)
        .hexpand(true)
        .vexpand(true)
        .build();
    graph.add_css_class("tm-chart");
    panel.append(&graph);
    let legend = label(
        "60 samples · oldest → newest · total CPU utilization",
        "caption",
    );
    panel.append(&legend);
    let details = label("Collecting…", "title-4");
    panel.append(&details);
    panel.append(&label("GPU usage appears when a readable kernel utilization counter is available. Virtual block devices are excluded to avoid counting disk traffic twice.","caption"));
    let mut child = panel.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Ok(text) = widget.downcast::<gtk::Label>() {
            text.set_max_width_chars(52);
            text.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        }
    }
    let diagnostics = label("Collecting…", "monospace");
    diagnostics.set_selectable(true);
    let extra = gtk::Expander::builder()
        .label("Paging, pressure & sensors")
        .child(&diagnostics)
        .build();
    panel.append(&extra);
    let detail_scroll = gtk::ScrolledWindow::builder()
        .child(&panel)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .hexpand(true)
        .build();
    performance.append(&detail_scroll);
    tabs.add_titled(&performance, Some("performance"), "Performance");
    let controls = crate::workbench::buttons(&root);
    let speed_label = label("Update speed", "caption");
    speed_label.set_wrap(false);
    controls.insert(&speed_label, -1);
    let speed = gtk::DropDown::from_strings(&["Paused", "1 second", "2 seconds", "5 seconds"]);
    speed.set_selected(2);
    controls.insert(&speed, -1);
    let refresh = gtk::Button::with_label("Refresh now");
    controls.insert(&refresh, -1);
    let export = gtk::Button::with_label("Export snapshot");
    controls.insert(&export, -1);

    let status = label(
        "Monitoring stops when you leave this page or close Tend.",
        "caption",
    );
    root.append(&status);
    let state = Rc::new(RefCell::new(State::default()));
    let selected = Rc::new(RefCell::new("cpu".into()));
    let ui = Rc::new(Ui {
        store,
        selection: selection.clone(),
        sorted,
        summary,
        status,
        resources: resources.clone(),
        keys: RefCell::new(Vec::new()),
        title,
        details,
        scale,
        legend,
        graph: graph.clone(),
        selected: selected.clone(),
        info,
        diagnostics,
        quit: quit.clone(),
        end: end.clone(),
    });
    let data = state.clone();
    let key = selected.clone();
    graph.set_draw_func(move |_, cr, width, height| {
        let s = data.borrow();
        let key = key.borrow();
        let Some(t) = s.traces.get(key.as_str()) else {
            return;
        };
        let width = width as f64;
        let height = height as f64;
        let max = if t.percent {
            100.
        } else {
            t.points.iter().map(|v| v.0.max(v.1)).fold(1024., f64::max)
        };
        cr.set_source_rgba(t.color.0, t.color.1, t.color.2, 0.15);
        cr.set_line_width(1.);
        for i in 0..=4 {
            let y = i as f64 * height / 4.;
            cr.move_to(0., y);
            cr.line_to(width, y);
        }
        for i in 0..=6 {
            let x = i as f64 * width / 6.;
            cr.move_to(x, 0.);
            cr.line_to(x, height);
        }
        let _ = cr.stroke();
        for line in 0..if t.percent { 1 } else { 2 } {
            cr.set_source_rgba(
                t.color.0,
                t.color.1,
                t.color.2,
                if line == 0 { 1. } else { 0.55 },
            );
            cr.set_line_width(2.);
            cr.set_dash(if line == 0 { &[] } else { &[5., 4.] }, 0.);
            for (i, p) in t.points.iter().enumerate() {
                let x = (60 - t.points.len() + i) as f64 / 59. * width;
                let y = height * (1. - (if line == 0 { p.0 } else { p.1 } / max).clamp(0., 1.));
                if i == 0 {
                    cr.move_to(x, y);
                } else {
                    cr.line_to(x, y);
                }
            }
            let _ = cr.stroke();
        }
    });
    let data = state.clone();
    let u = ui.clone();
    resources.connect_row_selected(move |_, row| {
        if let Some(r) = row {
            *u.selected.borrow_mut() = r.widget_name().to_string();
            u.paint(&data.borrow());
        }
    });
    let u = ui.clone();
    selection.connect_selected_item_notify(move |s| {
        if let Some(o) = s.selected_item() {
            let t = task(&o);
            let own = t.uid == unsafe { libc::geteuid() } && t.pid != std::process::id();
            u.quit.set_sensitive(own);
            u.end.set_sensitive(own);
            u.info.set_text(&format!(
                "{} · PID {} · {} · {}\n{}",
                t.name, t.pid, t.user, t.status, t.executable
            ));
        } else {
            u.quit.set_sensitive(false);
            u.end.set_sensitive(false);
        }
    });
    for (b, force) in [(quit, false), (end, true)] {
        let parent = win.clone();
        let sel = selection.clone();
        let u = ui.clone();
        b.connect_clicked(move |_| {
            let Some(o) = sel.selected_item() else { return };
            let t = task(&o).clone();
            let dialog = adw::MessageDialog::new(
                Some(&parent),
                Some(if force {
                    "End this task?"
                } else {
                    "Request this task to quit?"
                }),
                Some(&format!(
                    "{} (PID {}). Save your work first. {}",
                    t.name,
                    t.pid,
                    if force {
                        "Force termination can lose unsaved work and interrupt writes."
                    } else {
                        "A termination request may close the process without saving."
                    }
                )),
            );
            dialog.add_responses(&[
                ("cancel", "Cancel"),
                ("end", if force { "End task" } else { "Request quit" }),
            ]);
            dialog.set_close_response("cancel");
            dialog.set_default_response(Some("cancel"));
            dialog.set_response_appearance("end", adw::ResponseAppearance::Destructive);
            let u = u.clone();
            dialog.connect_response(None, move |_, r| {
                if r == "end" {
                    let p = w::Process {
                        pid: t.pid,
                        start: t.start,
                        ticks: t.ticks,
                        name: t.name.clone(),
                        rss: t.memory,
                        cpu: t.cpu.unwrap_or(0.),
                    };
                    u.status
                        .set_text(&w::signal_process(&p, force).unwrap_or_else(|e| e));
                }
            });
            dialog.present();
        });
    }
    let busy = Rc::new(Cell::new(false));
    let tick: Rc<dyn Fn()> = Rc::new({
        let data = state.clone();
        let ui = ui.clone();
        move || {
            if busy.replace(true) {
                return;
            }
            let previous = data.borrow().previous.clone();
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(stats::collect(previous.as_ref()));
            });
            let (data, ui, busy) = (data.clone(), ui.clone(), busy.clone());
            glib::timeout_add_local(Duration::from_millis(80), move || match rx.try_recv() {
                Ok(s) => {
                    busy.set(false);
                    ingest(&mut data.borrow_mut(), s);
                    ui.update(&data.borrow());
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(_) => {
                    busy.set(false);
                    ui.status
                        .set_text("Collection stopped. Choose Refresh now to retry.");
                    glib::ControlFlow::Break
                }
            });
        }
    });
    let tick_refresh = tick.clone();
    refresh.connect_clicked(move |_| tick_refresh());
    let source = Rc::new(RefCell::new(None::<glib::SourceId>));
    let source_close = source.clone();
    win.connect_close_request(move |_| {
        if let Some(s) = source_close.borrow_mut().take() {
            s.remove();
        }
        glib::Propagation::Proceed
    });
    let start: Rc<dyn Fn()> = Rc::new({
        let window = win.downgrade();
        let stack = stack.downgrade();
        let speed = speed.downgrade();
        let data = state.clone();
        move || {
            if let Some(s) = source.borrow_mut().take() {
                s.remove();
            }
            let (Some(stack), Some(speed)) = (stack.upgrade(), speed.upgrade()) else {
                return;
            };
            let minimized = window
                .upgrade()
                .and_then(|w| w.surface())
                .and_then(|s| s.downcast::<gtk::gdk::Toplevel>().ok())
                .map(|s| s.state().contains(gtk::gdk::ToplevelState::MINIMIZED))
                .unwrap_or(false);
            let active = window
                .upgrade()
                .map(|w| w.is_active() && w.is_visible() && w.is_mapped())
                .unwrap_or(false);
            if !active || minimized || stack.visible_child_name().as_deref() != Some("task-manager")
            {
                data.borrow_mut().previous = None;
                return;
            }
            tick();
            let seconds = match speed.selected() {
                1 => 1,
                2 => 2,
                3 => 5,
                _ => return,
            };
            let tick = tick.clone();
            *source.borrow_mut() = Some(glib::timeout_add_local(
                Duration::from_secs(seconds),
                move || {
                    tick();
                    glib::ControlFlow::Continue
                },
            ));
        }
    });
    let state_start = start.clone();
    win.connect_realize(move |w| {
        if let Some(surface) = w
            .surface()
            .and_then(|s| s.downcast::<gtk::gdk::Toplevel>().ok())
        {
            let on_state = state_start.clone();
            surface.connect_state_notify(move |_| on_state());
        }
    });
    let focused = start.clone();
    win.connect_is_active_notify(move |_| focused());
    let visible = start.clone();
    win.connect_visible_notify(move |_| visible());
    let on_page = start.clone();
    stack.connect_visible_child_name_notify(move |_| on_page());
    speed.connect_selected_notify(move |_| start());
    let data = state;
    let ui_export = ui.clone();
    export.connect_clicked(move |_| {
        let s = data.borrow();
        let Some(v) = &s.previous else { return };
        let mut text = format!(
            "Tend Task Manager\n{}\nUptime {}\n\n",
            ui_export.summary.text(),
            v.uptime
        );
        text.push_str(&s.diagnostics);
        text.push_str("\n");
        for t in s.traces.values() {
            text.push_str(&format!("{}\n{}\n\n", t.title, t.detail));
        }
        text.push_str("PID\tName\tUser\tCPU (total capacity)\tMemory\tDisk I/O\n");
        for t in &v.tasks {
            text.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\n",
                t.pid,
                t.name,
                t.user,
                cell(1, t),
                cell(2, t),
                cell(3, t)
            ));
        }
        ui_export
            .status
            .set_text(&w::export_report(&text, false).unwrap_or_else(|e| e));
    });
    if std::env::var_os("TEND_SCREENSHOT").is_some()
        && std::env::var("TEND_TASK_TAB").as_deref() == Ok("performance")
    {
        tabs.set_visible_child_name("performance");
    }
    stack.add_titled(&root, Some("task-manager"), "Task Manager");
}
