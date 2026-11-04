use crate::{column, label};
use adw::prelude::*;
use gtk::{gio, glib, Orientation};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashSet, VecDeque},
    rc::Rc,
    sync::Arc,
    time::Duration,
};
use tend::{
    task_stats::{Snapshot, Task},
    workstation as w,
};
const PROCESS_HINT: &str = "Select a process for details. Other users’ processes are read-only.";
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
    previous: Option<Arc<Snapshot>>,
    sequence: u64,
    generation: u64,
    traces: BTreeMap<String, Trace>,
    diagnostics: String,
}
// Cancelled when the page loses focus, hides, changes speed or is left before
// its first frame. Full /proc collection must not compete with initial paint.
struct PaintRequest {
    clock: glib::WeakRef<gtk::gdk::FrameClock>,
    handler: Option<glib::SignalHandlerId>,
}
impl Drop for PaintRequest {
    fn drop(&mut self) {
        if let (Some(clock), Some(handler)) = (self.clock.upgrade(), self.handler.take()) {
            clock.disconnect(handler);
        }
    }
}
fn after_paint(
    window: &adw::ApplicationWindow,
    pending: &Rc<RefCell<Option<PaintRequest>>>,
    action: impl FnOnce() + 'static,
) {
    if let Some(clock) = window.frame_clock() {
        let weak = Rc::downgrade(pending);
        let action = RefCell::new(Some(action));
        let handler = clock.connect_after_paint(move |_| {
            let Some(pending) = weak.upgrade() else {
                return;
            };
            pending.borrow_mut().take();
            let action = action.borrow_mut().take();
            if let Some(action) = action {
                action();
            }
        });
        *pending.borrow_mut() = Some(PaintRequest {
            clock: clock.downgrade(),
            handler: Some(handler),
        });
        window.queue_draw();
    } else {
        action();
    }
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
const COUNTER_NOTE: &str = "CPU: share of one core, like top (a multi-core process can exceed 100%). Memory: RSS, including shared pages. Disk I/O: physical bytes. — means unavailable or collecting.";
fn row_id(object: &glib::BoxedAnyObject) -> (u32, u64) {
    let value = object.borrow::<Task>();
    (value.pid, value.start)
}
fn selected_id(selection: &gtk::SingleSelection) -> Option<(u32, u64)> {
    selection.selected_item().map(|object| {
        let t = task(&object);
        (t.pid, t.start)
    })
}
fn restore_selection(
    selection: &gtk::SingleSelection,
    sorted: &gtk::SortListModel,
    id: Option<(u32, u64)>,
) {
    if selected_id(selection) == id {
        return;
    }
    if let Some(id) = id {
        for i in 0..sorted.n_items() {
            let object = sorted.item(i).unwrap();
            let value = task(&object);
            if (value.pid, value.start) == id {
                selection.set_selected(i);
                return;
            }
        }
    }
    selection.set_selected(gtk::INVALID_LIST_POSITION);
}
// Keep the same GObjects for surviving identities; a reused PID is a new row.
// New rows are returned for bounded, idle-time insertion into the actual model.
fn reconcile_tasks(store: &gio::ListStore, tasks: &[Task]) -> Vec<glib::BoxedAnyObject> {
    let wanted = tasks
        .iter()
        .map(|t| ((t.pid, t.start), t))
        .collect::<BTreeMap<_, _>>();
    let mut existing = HashSet::new();
    for i in (0..store.n_items()).rev() {
        let object = store
            .item(i)
            .unwrap()
            .downcast::<glib::BoxedAnyObject>()
            .unwrap();
        let id = row_id(&object);
        if let Some(value) = wanted.get(&id) {
            *object.borrow_mut::<Task>() = (*value).clone();
            existing.insert(id);
        } else {
            store.remove(i);
        }
    }
    tasks
        .iter()
        .filter(|t| !existing.contains(&(t.pid, t.start)))
        .cloned()
        .map(glib::BoxedAnyObject::new)
        .collect()
}
fn render_cell(label: &gtk::Label, column: usize, value: &Task) {
    let text = cell(column, value);
    if label.text().as_str() != text {
        label.set_text(&text);
    }
    let tip = format!(
        "{} · PID {} · {}\n{}",
        value.name, value.pid, value.user, value.executable
    );
    if label.tooltip_text().as_deref() != Some(tip.as_str()) {
        label.set_tooltip_text(Some(&tip));
    }
}
struct BoundCell {
    label: glib::WeakRef<gtk::Label>,
    object: glib::WeakRef<glib::BoxedAnyObject>,
    column: usize,
}
struct ProcessRows {
    store: gio::ListStore,
    cells: Rc<RefCell<BTreeMap<usize, BoundCell>>>,
    pending: Rc<RefCell<Option<glib::SourceId>>>,
    serial: Rc<Cell<u64>>,
    present: Rc<RefCell<HashSet<(u32, u64)>>>,
}
impl ProcessRows {
    fn new(store: &gio::ListStore) -> Rc<Self> {
        Rc::new(Self {
            store: store.clone(),
            cells: Default::default(),
            pending: Default::default(),
            serial: Default::default(),
            present: Default::default(),
        })
    }
    fn cancel(&self) {
        self.serial.set(self.serial.get().wrapping_add(1));
        if let Some(source) = self.pending.borrow_mut().take() {
            source.remove();
        }
    }
    fn clear(&self) {
        self.cancel();
        self.store.remove_all();
        self.present.borrow_mut().clear();
        self.cells.borrow_mut().clear();
    }
    fn ensure(&self, value: &Task) {
        let inserted = self.present.borrow_mut().insert((value.pid, value.start));
        if inserted {
            self.store.append(&glib::BoxedAnyObject::new(value.clone()));
        }
    }
    fn refresh_cells(&self) {
        // No strong widget/item references survive a refresh. A recycled GTK
        // cell replaces its registry entry, so scrolling cannot grow this map.
        let mut current = vec![];
        self.cells.borrow_mut().retain(|_, cell| {
            if let (Some(label), Some(object)) = (cell.label.upgrade(), cell.object.upgrade()) {
                current.push((label, object, cell.column));
                true
            } else {
                false
            }
        });
        for (label, object, column) in current {
            render_cell(&label, column, &object.borrow::<Task>());
        }
    }
    fn update(
        &self,
        tasks: &[Task],
        filter: &gtk::CustomFilter,
        sorted: &gtk::SortListModel,
        selection: &gtk::SingleSelection,
        status: &gtk::Label,
    ) {
        self.cancel();
        let serial = self.serial.get();
        let selected = selected_id(selection);
        let born = reconcile_tasks(&self.store, tasks);
        *self.present.borrow_mut() = (0..self.store.n_items())
            .map(|i| row_id(&self.store.item(i).unwrap().downcast().unwrap()))
            .collect();
        filter.changed(gtk::FilterChange::Different);
        if let Some(sorter) = sorted.sorter() {
            sorter.changed(gtk::SorterChange::Different);
        }
        self.refresh_cells();
        restore_selection(selection, sorted, selected);
        if born.is_empty() {
            status.set_text(COUNTER_NOTE);
            return;
        }
        let (store, present, pending, epoch) = (
            self.store.clone(),
            self.present.clone(),
            self.pending.clone(),
            self.serial.clone(),
        );
        let (sorted, selection, status) = (sorted.clone(), selection.clone(), status.clone());
        let total = tasks.len();
        let mut born = VecDeque::from(born);
        status.set_text(&format!(
            "Loading process rows… {} of {total}. Filters apply to rows received so far.",
            store.n_items()
        ));
        // An always-ready idle can fill the whole table before the next paint.
        // Pace batches across frames; this timer exists only during population.
        let source = glib::timeout_add_local_full(
            Duration::from_millis(32),
            glib::Priority::LOW,
            move || {
                if epoch.get() != serial {
                    return glib::ControlFlow::Break;
                }
                let selected = selected_id(&selection);
                let mut batch = vec![];
                for _ in 0..25 {
                    let Some(object) = born.pop_front() else {
                        break;
                    };
                    if present.borrow_mut().insert(row_id(&object)) {
                        batch.push(object);
                    }
                }
                if !batch.is_empty() {
                    store.splice(store.n_items(), 0, &batch);
                }
                restore_selection(&selection, &sorted, selected);
                if epoch.get() != serial {
                    return glib::ControlFlow::Break;
                }
                if born.is_empty() {
                    pending.borrow_mut().take();
                    status.set_text(COUNTER_NOTE);
                    glib::ControlFlow::Break
                } else {
                    status.set_text(&format!(
                    "Loading process rows… {} of {total}. Filters apply to rows received so far.",
                    store.n_items()
                ));
                    glib::ControlFlow::Continue
                }
            },
        );
        *self.pending.borrow_mut() = Some(source);
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
fn ingest(state: &mut State, s: Arc<Snapshot>) {
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
    tabs: glib::WeakRef<gtk::Stack>,
    groups: Rc<crate::process_tree_ui::Views>,
    rows: Rc<ProcessRows>,
    filter: gtk::CustomFilter,
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
    export: gtk::Button,
}
impl Ui {
    fn pause(&self) {
        self.groups.clear();
        self.rows.clear();
        self.export.set_sensitive(false);
        self.quit.set_sensitive(false);
        self.end.set_sensitive(false);
        self.summary.set_text("Monitoring paused");
        self.info
            .set_text("Open Task Manager or refresh to collect a new snapshot.");
    }
    fn update(&self, state: &State) {
        self.export.set_sensitive(state.previous.is_some());
        let Some(s) = &state.previous else { return };
        let active = self
            .tabs
            .upgrade()
            .and_then(|tabs| tabs.visible_child_name());
        if matches!(active.as_deref(), Some("apps" | "process-tree")) {
            self.groups.update(s.clone());
        }
        // A hidden ColumnView still binds rows after a model splice. Avoid
        // rebuilding process widgets while only the live charts are visible.
        if active.as_deref() == Some("processes") {
            self.rows.update(
                &s.tasks,
                &self.filter,
                &self.sorted,
                &self.selection,
                &self.status,
            );
            self.selection_details(&self.selection);
        } else {
            self.rows.cancel();
            self.status.set_text(COUNTER_NOTE);
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
        // Histories are collected for every resource, but the native resource
        // cards and chart belong to the Performance tab. Do not build or
        // measure those hidden widgets during process-list navigation.
        if active.as_deref() != Some("performance") {
            return;
        }
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
    fn selection_details(&self, selection: &gtk::SingleSelection) {
        if let Some(object) = selection.selected_item() {
            let t = task(&object);
            let own = t.uid == unsafe { libc::geteuid() } && t.pid != std::process::id();
            self.quit.set_sensitive(own);
            self.end.set_sensitive(own);
            self.info.set_text(&format!(
                "{} · PID {} · {} · {}\n{}",
                t.name, t.pid, t.user, t.status, t.executable
            ));
        } else {
            self.quit.set_sensitive(false);
            self.end.set_sensitive(false);
            self.info.set_text(PROCESS_HINT);
        }
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
    let tabs = gtk::Stack::builder()
        .vexpand(true)
        .hexpand(true)
        .hhomogeneous(false)
        .vhomogeneous(false)
        .build();
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
    let rows = ProcessRows::new(&store);
    let state = Rc::new(RefCell::new(State::default()));
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
    let mut cpu_column = None;
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
        let cells = rows.cells.clone();
        factory.connect_bind(move |_, o| {
            let item = o.downcast_ref::<gtk::ListItem>().unwrap();
            let object = item
                .item()
                .unwrap()
                .downcast::<glib::BoxedAnyObject>()
                .unwrap();
            let text = item.child().unwrap().downcast::<gtk::Label>().unwrap();
            render_cell(&text, i, &object.borrow::<Task>());
            cells.borrow_mut().insert(
                text.as_ptr() as usize,
                BoundCell {
                    label: text.downgrade(),
                    object: object.downgrade(),
                    column: i,
                },
            );
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
        if i == 1 {
            cpu_column = Some(col);
        }
    }
    sorted.set_sorter(table.sorter().as_ref());
    // Open on the busiest processes instead of an idle, alphabetically irrelevant
    // page of zero-percent rows. Users can still sort by any other column.
    table.sort_by_column(cpu_column.as_ref(), gtk::SortType::Descending);
    let table_scroll = gtk::ScrolledWindow::builder()
        .child(&table)
        .hscrollbar_policy(gtk::PolicyType::Always)
        .overlay_scrolling(false)
        .name("process-table-scroll")
        .vexpand(true)
        .hexpand(true)
        .min_content_height(240)
        .build();
    table_scroll.add_css_class("card");
    // An empty ColumnView's first layout is expensive and conveys no data.
    // Show an honest waiting state until real rows arrive; collection still
    // begins after paint and the staged population retains every process.
    let waiting=column(8);
    waiting.set_valign(gtk::Align::Center);
    let waiting_title=label("Waiting for process data","heading");
    waiting_title.set_halign(gtk::Align::Center);
    waiting.append(&waiting_title);
    let hint=label("Live rows appear after a snapshot. Use Refresh if monitoring is paused.","caption");
    hint.set_halign(gtk::Align::Center);hint.set_justify(gtk::Justification::Center);
    hint.set_max_width_chars(48);waiting.append(&hint);
    let process_results=gtk::Stack::builder().vexpand(true).hexpand(true).hhomogeneous(false).vhomogeneous(false).height_request(240).build();
    process_results.set_widget_name("process-results-state");
    process_results.add_named(&waiting,Some("waiting"));
    process_results.add_named(&table_scroll,Some("rows"));
    process_results.set_visible_child_name("waiting");
    let target=process_results.downgrade();
    store.connect_items_changed(move|model,_,_,_|{
        if let Some(target)=target.upgrade(){target.set_visible_child_name(if model.n_items()==0{"waiting"}else{"rows"});}
    });
    process.append(&process_results);
    let info = label(PROCESS_HINT,"caption");
    info.set_widget_name("process-selection-details");
    process.append(&info);
    let only = gtk::CheckButton::with_label("Only my processes");
    process.append(&only);
    let f = filter.clone();
    let search_query = query.clone();
    search.connect_search_changed(move |s| {
        *search_query.borrow_mut() = s.text().to_lowercase();
        f.changed(gtk::FilterChange::Different);
    });
    let own_filter = filter.clone();
    only.connect_toggled(move |b| {
        mine.set(b.is_active());
        own_filter.changed(gtk::FilterChange::Different);
    });
    tabs.add_titled(&process, Some("processes"), "Processes");
    let (target, process_selection, process_sorted) =
        (tabs.clone(), selection.clone(), sorted.clone());
    let (link_rows, link_state, link_search, link_only, link_filter, link_query) = (
        rows.clone(),
        state.clone(),
        search.clone(),
        only.clone(),
        filter.clone(),
        query.clone(),
    );
    let groups = crate::process_tree_ui::Views::new(&tabs, move |pid, start| {
        link_query.borrow_mut().clear();
        link_search.set_text("");
        link_only.set_active(false);
        link_filter.changed(gtk::FilterChange::Different);
        target.set_visible_child_name("processes");
        if let Some(snapshot) = &link_state.borrow().previous {
            if let Some(value) = snapshot
                .tasks
                .iter()
                .find(|t| t.pid == pid && t.start == start)
            {
                link_rows.ensure(value);
            }
        }
        for index in 0..process_sorted.n_items() {
            let object = process_sorted.item(index).unwrap();
            let t = task(&object);
            if t.pid == pid && t.start == start {
                process_selection.set_selected(index);
                break;
            }
        }
    });
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
    let names = gtk::Button::with_label("Refresh app names");
    controls.insert(&names, -1);
    let export = gtk::Button::with_label("Export snapshot");
    export.set_sensitive(false);
    controls.insert(&export, -1);

    let status = label(
        "Monitoring stops when you leave this page or close Tend.",
        "caption",
    );
    root.append(&status);
    let selected = Rc::new(RefCell::new("cpu".into()));
    let ui = Rc::new(Ui {
        tabs: tabs.downgrade(),
        groups,
        rows,
        filter,
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
        export: export.clone(),
    });
    let weak_ui = Rc::downgrade(&ui);
    let tab_state = state.clone();
    tabs.connect_visible_child_name_notify(move |tabs| {
        if let Some(ui) = weak_ui.upgrade() {
            if tabs.visible_child_name().as_deref() != Some("processes") {
                ui.rows.clear();
            }
            if !matches!(
                tabs.visible_child_name().as_deref(),
                Some("apps" | "process-tree")
            ) {
                ui.groups.clear();
            }
            ui.update(&tab_state.borrow());
        }
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
    selection.connect_selected_item_notify(move |s| u.selection_details(s));
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
            let (data, ui, busy) = (data.clone(), ui.clone(), busy.clone());
            let generation = data.borrow().generation;
            let failed = (ui.clone(), busy.clone());
            if let Err(error) =
                crate::monitor::request(tend::sampler::Detail::Full, move |result| {
                    busy.set(false);
                    if data.borrow().generation != generation {
                        return;
                    }
                    match result {
                        Ok(capture) => {
                            if let Some(snapshot) = &capture.full {
                                let changed = {
                                    let mut state = data.borrow_mut();
                                    if state.sequence == capture.sequence {
                                        false
                                    } else {
                                        state.sequence = capture.sequence;
                                        ingest(&mut state, snapshot.clone());
                                        true
                                    }
                                };
                                // GTK updates can synchronously emit selection
                                // signals. Release the exclusive state borrow first.
                                if changed {
                                    ui.update(&data.borrow());
                                }
                            }
                        }
                        Err(error) => ui.status.set_text(&format!("Collection stopped: {error}")),
                    }
                })
            {
                failed.1.set(false);
                failed.0.status.set_text(&error);
            }
        }
    });
    let tick_refresh = tick.clone();
    refresh.connect_clicked(move |_| tick_refresh());
    let tick_names = tick.clone();
    names.connect_clicked(move |button| {
        button.set_sensitive(false);
        let (button, tick) = (button.clone(), tick_names.clone());
        crate::job(
            || {
                tend::process_groups::refresh_catalog();
                Ok(())
            },
            move |_| {
                button.set_sensitive(true);
                tick();
            },
        );
    });
    let source = Rc::new(RefCell::new(None::<crate::monitor::Handle>));
    let pending_paint = Rc::new(RefCell::new(None::<PaintRequest>));
    let source_close = source.clone();
    let pending_close = pending_paint.clone();
    win.connect_close_request(move |_| {
        pending_close.borrow_mut().take();
        source_close.borrow_mut().take();
        glib::Propagation::Proceed
    });
    let start: Rc<dyn Fn()> = Rc::new({
        let window = win.downgrade();
        let stack = stack.downgrade();
        let speed = speed.downgrade();
        let data = state.clone();
        let export = export.clone();
        let ui = ui.clone();
        let pending_paint = pending_paint.clone();
        move || {
            pending_paint.borrow_mut().take();
            source.borrow_mut().take();
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
                data.borrow_mut().generation += 1;
                ui.pause();
                export.set_sensitive(false);
                return;
            }
            let seconds = match speed.selected() {
                1 => 1,
                2 => 2,
                3 => 5,
                _ => {
                    tick();
                    return;
                }
            };
            if data.borrow().previous.is_none() {
                ui.summary.set_text("Collecting live counters…");
                ui.status.set_text("Collecting a fresh snapshot…");
                ui.info.set_text(PROCESS_HINT);
            }
            let (data, ui, source) = (data.clone(), ui.clone(), source.clone());
            let failed = ui.clone();
            let begin = move || match crate::monitor::subscribe(
                tend::sampler::Detail::Full,
                Duration::from_secs(seconds),
                move |result| match result {
                    Ok(capture) => {
                        if let Some(snapshot) = &capture.full {
                            let changed = {
                                let mut state = data.borrow_mut();
                                if state.sequence == capture.sequence {
                                    false
                                } else {
                                    state.sequence = capture.sequence;
                                    ingest(&mut state, snapshot.clone());
                                    true
                                }
                            };
                            if changed {
                                ui.update(&data.borrow());
                            }
                        }
                    }
                    Err(error) => ui.status.set_text(&format!("Collection stopped: {error}")),
                },
            ) {
                Ok(handle) => *source.borrow_mut() = Some(handle),
                Err(error) => failed.status.set_text(&error),
            };
            if let Some(window) = window.upgrade() {
                after_paint(&window, &pending_paint, begin);
            }
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
        text.push_str("PID\tName\tUser\tCPU (one core)\tMemory\tDisk I/O\n");
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

#[cfg(test)]
mod row_contracts {
    use super::*;
    #[test]
    fn live_rows_keep_identity_and_replace_reused_pids() {
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        let first = Task {
            pid: 41,
            start: 100,
            name: "first".into(),
            ..Default::default()
        };
        let old = Task {
            pid: 42,
            start: 200,
            name: "old process".into(),
            ..Default::default()
        };
        let born = reconcile_tasks(&store, &[first.clone(), old]);
        store.splice(0, 0, &born);
        let stable = store.item(0).unwrap();
        let retired = store.item(1).unwrap();
        let changed = Task {
            name: "after exec".into(),
            memory: 8192,
            cpu: Some(12.),
            ..first
        };
        let reused = Task {
            pid: 42,
            start: 300,
            name: "different process".into(),
            ..Default::default()
        };
        let new = Task {
            pid: 43,
            start: 400,
            ..Default::default()
        };
        let born = reconcile_tasks(&store, &[changed, reused, new]);
        assert_eq!(store.n_items(), 1);
        assert_eq!(store.item(0).unwrap(), stable);
        assert_eq!(task(&stable).name, "after exec");
        assert_eq!(task(&stable).memory, 8192);
        assert_eq!(born.len(), 2);
        assert!(born
            .iter()
            .all(|o| o.upcast_ref::<glib::Object>() != &retired));
        store.splice(store.n_items(), 0, &born);
        assert_eq!(store.n_items(), 3);
        assert_eq!(task(&retired).start, 200);
        assert_eq!(task(&store.item(1).unwrap()).start, 300);
    }
}
