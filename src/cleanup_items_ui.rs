use adw::prelude::*;
use gtk::{gio, glib};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};
use tend::{
    cleanup_plan::{Candidate, Plan},
    workstation as w,
};
struct Item {
    candidate: Candidate,
    title: String,
    why: String,
    effect: String,
}
pub struct Items {
    pub root: gtk::Box,
    store: gio::ListStore,
    plan: RefCell<Option<Arc<Plan>>>,
    page: Cell<usize>,
    status: gtk::Label,
    next: gtk::Button,
    previous: gtk::Button,
}
impl Items {
    pub fn new() -> Rc<Self> {
        let root = crate::column(10);
        root.append(&crate::label("Previewed files", "title-3"));
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        let selection = gtk::SingleSelection::new(Some(store.clone()));
        selection.set_autoselect(false);
        selection.set_can_unselect(true);
        let table = gtk::ColumnView::new(Some(selection.clone()));
        table.add_css_class("data-table");
        table.set_show_row_separators(true);
        for (index, title) in ["File", "Category", "Allocated", "Modified"]
            .iter()
            .enumerate()
        {
            let factory = gtk::SignalListItemFactory::new();
            factory.connect_setup(move |_, o| {
                let item = o.downcast_ref::<gtk::ListItem>().unwrap();
                item.set_child(Some(
                    &gtk::Label::builder()
                        .xalign(if index == 2 { 1. } else { 0. })
                        .ellipsize(gtk::pango::EllipsizeMode::End)
                        .margin_start(6)
                        .margin_end(6)
                        .margin_top(9)
                        .margin_bottom(9)
                        .build(),
                ));
            });
            factory.connect_bind(move |_, o| {
                let item = o.downcast_ref::<gtk::ListItem>().unwrap();
                let object = item
                    .item()
                    .unwrap()
                    .downcast::<glib::BoxedAnyObject>()
                    .unwrap();
                let row = object.borrow::<Item>();
                let label = item.child().unwrap().downcast::<gtk::Label>().unwrap();
                label.set_text(&match index {
                    0 => row.candidate.path.to_string_lossy().into_owned(),
                    1 => row.title.clone(),
                    2 => w::bytes(row.candidate.stamp.allocated),
                    _ => tend::presentation::when(row.candidate.stamp.mtime),
                });
                label.set_tooltip_text(Some(&row.candidate.path.to_string_lossy()));
            });
            let col = gtk::ColumnViewColumn::new(Some(title), Some(factory));
            col.set_resizable(true);
            col.set_expand(index == 0);
            col.set_fixed_width([300, 180, 100, 160][index]);
            table.append_column(&col);
        }
        let scroll = gtk::ScrolledWindow::builder()
            .child(&table)
            .min_content_height(160)
            .max_content_height(320)
            .propagate_natural_height(true)
            .hexpand(true)
            .build();
        scroll.add_css_class("card");
        root.append(&scroll);
        let detail = crate::label(
            "Select a file to review why it is included and what removing it means.",
            "caption",
        );
        detail.set_selectable(true);
        detail.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        root.append(&detail);
        selection.connect_selected_item_notify(move |selection| {if let Some(object)=selection.selected_item().and_then(|o|o.downcast::<glib::BoxedAnyObject>().ok()) {let row=object.borrow::<Item>();detail.set_text(&format!("{}\nWhy: {}\nEffect: {}{}",row.candidate.path.display(),row.why,row.effect,if row.candidate.stamp.links>1 {"\nHard-linked file: removing this path may leave its blocks allocated elsewhere."}else{""}));}else{detail.set_text("Select a file to review why it is included and what removing it means.");}});
        let actions = crate::workbench::buttons(&root);
        let previous = gtk::Button::with_label("Previous files");
        let next = gtk::Button::with_label("Next files");
        let status = crate::label("Preview cleanup to list eligible files.", "caption");
        actions.insert(&previous, -1);
        actions.insert(&next, -1);
        root.append(&status);
        let value = Rc::new(Self {
            root,
            store,
            plan: RefCell::new(None),
            page: Cell::new(0),
            status,
            next: next.clone(),
            previous: previous.clone(),
        });
        let weak = Rc::downgrade(&value);
        previous.connect_clicked(move |_| {
            if let Some(v) = weak.upgrade() {
                v.page.set(v.page.get().saturating_sub(1));
                v.paint();
            }
        });
        let weak = Rc::downgrade(&value);
        next.connect_clicked(move |_| {
            if let Some(v) = weak.upgrade() {
                v.page.set(v.page.get() + 1);
                v.paint();
            }
        });
        value.clear();
        value
    }
    pub fn set(&self, plan: Arc<Plan>) {
        *self.plan.borrow_mut() = Some(plan);
        self.page.set(0);
        self.paint();
    }
    pub fn clear(&self) {
        self.plan.borrow_mut().take();
        self.store.remove_all();
        self.root.set_visible(false);
        self.previous.set_sensitive(false);
        self.next.set_sensitive(false);
    }
    fn paint(&self) {
        let plan = self.plan.borrow();
        let Some(plan) = plan.as_ref() else { return };
        let start = self.page.get() * 200;
        let end = (start + 200).min(plan.candidates.len());
        let rows: Vec<_> = plan
            .candidates
            .iter()
            .skip(start)
            .take(200)
            .map(|c| {
                let category = &plan.categories[c.category];
                glib::BoxedAnyObject::new(Item {
                    candidate: c.clone(),
                    title: category.title.clone(),
                    why: category.reason.clone(),
                    effect: category.consequence.clone(),
                })
            })
            .collect();
        self.store.splice(0, self.store.n_items(), &rows);
        self.previous.set_sensitive(start > 0);
        self.next.set_sensitive(end < plan.candidates.len());
        self.root.set_visible(!rows.is_empty());
        self.status.set_text(&format!(
            "Files {}–{} of {} · previewed identity is rechecked before removal",
            start + 1,
            end,
            plan.candidates.len()
        ));
    }
}
