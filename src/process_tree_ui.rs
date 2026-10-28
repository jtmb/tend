//! Virtualized app groups and ancestry. Group totals never offer a kill action.
use adw::prelude::*;
use gtk::{gio, glib};
use std::{cell::RefCell, collections::HashSet, rc::Rc, sync::Arc};
use tend::{
    process_groups,
    task_stats::{Snapshot, Task},
    workstation as w,
};

#[derive(Clone)]
struct Node {
    key: String,
    name: String,
    cpu: Option<f64>,
    memory: u64,
    io: Option<f64>,
    detail: String,
    pid: Option<(u32, u64)>,
    children: Arc<Vec<Node>>,
}
fn process(task: &Task, children: Vec<Node>, boot: &str) -> Node {
    Node {
        key: format!("{boot}:{}:{}", task.pid, task.start),
        name: task.name.clone(),
        cpu: task.cpu,
        memory: task.memory,
        io: task.read_rate.zip(task.write_rate).map(|(r, w)| r + w),
        detail: format!("PID {} · {} · {}", task.pid, task.user, task.status),
        pid: Some((task.pid, task.start)),
        children: Arc::new(children),
    }
}
fn store(nodes: Vec<Node>) -> gio::ListStore {
    let store = gio::ListStore::new::<glib::BoxedAnyObject>();
    for node in nodes {
        store.append(&glib::BoxedAnyObject::new(node));
    }
    store
}
fn value(row: &gtk::TreeListRow) -> glib::BoxedAnyObject {
    row.item().unwrap().downcast().unwrap()
}
fn retained(node: Node, query: &str) -> Option<Node> {
    if query.is_empty()
        || format!("{} {}", node.name, node.detail)
            .to_lowercase()
            .contains(query)
    {
        return Some(node);
    }
    let children = node
        .children
        .iter()
        .cloned()
        .filter_map(|n| retained(n, query))
        .collect::<Vec<_>>();
    (!children.is_empty()).then_some(Node {
        children: Arc::new(children),
        ..node
    })
}
struct View {
    roots: gio::ListStore,
    model: gtk::TreeListModel,
    selection: gtk::SingleSelection,
    query: RefCell<String>,
    other: gtk::CheckButton,
    search: gtk::SearchEntry,
}
impl View {
    fn new() -> (Rc<Self>, gtk::Box) {
        let root = crate::column(10);
        let search = gtk::SearchEntry::builder()
            .placeholder_text("Search apps, processes or PID")
            .hexpand(true)
            .build();
        root.append(&search);
        let roots = store(vec![]);
        let model = gtk::TreeListModel::new(roots.clone(), false, false, |o| {
            let object = o.downcast_ref::<glib::BoxedAnyObject>().unwrap();
            let node = object.borrow::<Node>();
            (!node.children.is_empty()).then(|| store(node.children.as_ref().clone()).upcast())
        });
        let selection = gtk::SingleSelection::new(Some(model.clone()));
        selection.set_autoselect(false);
        selection.set_can_unselect(true);
        let table = gtk::ColumnView::new(Some(selection.clone()));
        table.set_show_row_separators(true);
        table.add_css_class("data-table");
        for (index, title) in ["Name", "CPU", "Memory", "Disk I/O", "Details"]
            .iter()
            .enumerate()
        {
            let factory = gtk::SignalListItemFactory::new();
            factory.connect_setup(move |_, o| {
                let item = o.downcast_ref::<gtk::ListItem>().unwrap();
                let label = gtk::Label::builder()
                    .xalign(if (1..=3).contains(&index) { 1. } else { 0. })
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .margin_start(6)
                    .margin_end(6)
                    .margin_top(9)
                    .margin_bottom(9)
                    .build();
                if index == 0 {
                    let expander = gtk::TreeExpander::new();
                    expander.set_child(Some(&label));
                    item.set_child(Some(&expander));
                } else {
                    item.set_child(Some(&label));
                }
            });
            factory.connect_bind(move |_, o| {
                let item = o.downcast_ref::<gtk::ListItem>().unwrap();
                let row = item.item().unwrap().downcast::<gtk::TreeListRow>().unwrap();
                let object = value(&row);
                let node = object.borrow::<Node>();
                let label = if index == 0 {
                    let e = item
                        .child()
                        .unwrap()
                        .downcast::<gtk::TreeExpander>()
                        .unwrap();
                    e.set_list_row(Some(&row));
                    e.child().unwrap().downcast::<gtk::Label>().unwrap()
                } else {
                    item.child().unwrap().downcast::<gtk::Label>().unwrap()
                };
                label.set_text(&match index {
                    0 => node.name.clone(),
                    1 => node
                        .cpu
                        .map(|v| format!("{v:.1}%"))
                        .unwrap_or_else(|| "—".into()),
                    2 => w::bytes(node.memory),
                    3 => node
                        .io
                        .map(|v| format!("{}/s", w::bytes(v.max(0.) as u64)))
                        .unwrap_or_else(|| "—".into()),
                    _ => node.detail.clone(),
                });
                label.set_tooltip_text(Some(&format!("{}\n{}", node.name, node.detail)));
            });
            let col = gtk::ColumnViewColumn::new(Some(title), Some(factory));
            col.set_resizable(true);
            col.set_expand(index == 0);
            col.set_fixed_width([230, 80, 110, 110, 210][index]);
            table.append_column(&col);
        }
        let scrolled = gtk::ScrolledWindow::builder()
            .child(&table)
            .vexpand(true)
            .hexpand(true)
            .min_content_height(220)
            .build();
        scrolled.add_css_class("card");
        root.append(&scrolled);
        let other = gtk::CheckButton::with_label("Include other users’ processes");
        root.append(&other);
        let view = Rc::new(Self {
            roots,
            model,
            selection,
            query: RefCell::new(String::new()),
            other,
            search: search.clone(),
        });
        let v = view.clone();
        search.connect_search_changed(move |e| *v.query.borrow_mut() = e.text().to_lowercase());
        (view, root)
    }
    fn replace(&self, nodes: Vec<Node>) {
        let mut expanded = HashSet::new();
        let mut selected = None;
        for i in 0..self.model.n_items() {
            let row = self.model.row(i).unwrap();
            let object = value(&row);
            let node = object.borrow::<Node>();
            if row.is_expanded() {
                expanded.insert(node.key.clone());
            }
            if self.selection.selected() == i {
                selected = Some(node.key.clone());
            }
        }
        let nodes: Vec<_> = nodes
            .into_iter()
            .filter_map(|n| retained(n, &self.query.borrow()))
            .map(glib::BoxedAnyObject::new)
            .collect();
        self.roots.splice(0, self.roots.n_items(), &nodes);
        let mut i = 0;
        while i < self.model.n_items() {
            let row = self.model.row(i).unwrap();
            let object = value(&row);
            let node = object.borrow::<Node>();
            if expanded.contains(&node.key) {
                row.set_expanded(true);
            }
            if selected.as_ref() == Some(&node.key) {
                self.selection.set_selected(i);
            }
            i += 1;
        }
    }
}
pub struct Views {
    tabs: glib::WeakRef<gtk::Stack>,
    apps: Rc<View>,
    tree: Rc<View>,
    snapshot: RefCell<Option<Arc<Snapshot>>>,
}
impl Views {
    pub fn clear(&self) {
        self.snapshot.borrow_mut().take();
        self.apps.roots.remove_all();
        self.tree.roots.remove_all();
    }
    pub fn new(tabs: &gtk::Stack, open: impl Fn(u32, u64) + 'static) -> Rc<Self> {
        let open: Rc<dyn Fn(u32, u64)> = Rc::new(open);
        let (apps, a) = View::new();
        let (tree, t) = View::new();
        a.prepend(&crate::label("Apps include related helper processes. Memory sums RSS and can count shared pages more than once.","caption"));
        t.prepend(&crate::label("Expand a parent to see its children. Process identity includes boot, PID and start time.","caption"));
        for (view, page) in [(&apps, &a), (&tree, &t)] {
            let button = gtk::Button::with_label("Show selected process");
            button.set_halign(gtk::Align::Start);
            button.set_sensitive(false);
            page.append(&button);
            let b = button.clone();
            view.selection.connect_selected_item_notify(move |s| {
                let has = s
                    .selected_item()
                    .and_then(|o| o.downcast::<gtk::TreeListRow>().ok())
                    .is_some_and(|r| value(&r).borrow::<Node>().pid.is_some());
                b.set_sensitive(has);
            });
            let sel = view.selection.clone();
            let open = open.clone();
            button.connect_clicked(move |_| {
                if let Some(row) = sel
                    .selected_item()
                    .and_then(|o| o.downcast::<gtk::TreeListRow>().ok())
                {
                    if let Some((pid, start)) = value(&row).borrow::<Node>().pid {
                        open(pid, start);
                    }
                }
            });
        }
        tabs.add_titled(&a, Some("apps"), "Apps");
        tabs.add_titled(&t, Some("process-tree"), "Process tree");
        let views = Rc::new(Self {
            tabs: tabs.downgrade(),
            apps,
            tree,
            snapshot: RefCell::new(None),
        });
        for view in [&views.apps, &views.tree] {
            let weak = Rc::downgrade(&views);
            view.other.connect_toggled(move |_| {
                if let Some(v) = weak.upgrade() {
                    v.repaint();
                }
            });
            // Search updates are debounced by GTK; use the same saved snapshot.
            let weak = Rc::downgrade(&views);
            view.search.connect_search_changed(move |_| {
                if let Some(v) = weak.upgrade() {
                    v.repaint();
                }
            });
        }
        views
    }
    pub fn update(&self, snapshot: Arc<Snapshot>) {
        *self.snapshot.borrow_mut() = Some(snapshot);
        self.repaint();
    }
    fn repaint(&self) {
        let data = self.snapshot.borrow();
        let Some(s) = data.as_ref() else { return };
        let active = self.tabs.upgrade().and_then(|t| t.visible_child_name());
        let uid = unsafe { libc::geteuid() };
        let tasks: std::collections::HashMap<_, _> =
            s.tasks.iter().map(|t| ((t.pid, t.start), t)).collect();
        if active.as_deref() == Some("apps") {
            self.apps.replace(
                s.applications
                    .iter()
                    .filter(|g| self.apps.other.is_active() || g.uid == uid)
                    .map(|g| {
                        let members = g
                            .members
                            .iter()
                            .filter_map(|id| tasks.get(&(id.pid, id.start_ticks)).copied())
                            .map(|t| process(t, vec![], &s.boot_id))
                            .collect();
                        Node {
                            key: g.id.clone(),
                            name: g.name.clone(),
                            cpu: g.cpu_percent,
                            memory: g.memory_bytes,
                            io: g
                                .read_bytes_per_second
                                .zip(g.write_bytes_per_second)
                                .map(|(r, w)| r + w),
                            detail: format!(
                                "{} processes · {}",
                                g.members.len(),
                                if g.desktop_id.is_some() {
                                    "desktop app"
                                } else {
                                    "process family"
                                }
                            ),
                            pid: None,
                            children: Arc::new(members),
                        }
                    })
                    .collect(),
            );
        } else if self.apps.roots.n_items() > 0 {
            self.apps.roots.remove_all();
        }
        fn branch(
            n: process_groups::ProcessNode,
            s: &Snapshot,
            all: bool,
            uid: u32,
        ) -> Option<Node> {
            let task = &s.tasks[n.index];
            if !all && task.uid != uid {
                return None;
            }
            Some(process(
                task,
                n.children
                    .into_iter()
                    .filter_map(|n| branch(n, s, all, uid))
                    .collect(),
                &s.boot_id,
            ))
        }
        if active.as_deref() == Some("process-tree") {
            self.tree.replace(
                process_groups::forest(&s.tasks)
                    .into_iter()
                    .filter_map(|n| branch(n, s, self.tree.other.is_active(), uid))
                    .collect(),
            );
        } else if self.tree.roots.n_items() > 0 {
            self.tree.roots.remove_all();
        }
    }
}
