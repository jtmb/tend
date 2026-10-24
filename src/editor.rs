//! Shared, page-local commit controls. No persistence or privileged work here.
use crate::{column, label, scroll};
use adw::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Clone)]
pub struct Notice {
    pub root: gtk::Box,
    summary: gtk::Label,
    icon: gtk::Image,
    details: gtk::Expander,
    detail: gtk::Label,
}
impl Notice {
    pub fn new() -> Self {
        let root = column(6);
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let icon = gtk::Image::from_icon_name("dialog-information-symbolic");
        icon.set_valign(gtk::Align::Start);
        let summary = label("", "body");
        summary.set_hexpand(true);
        line.append(&icon);
        line.append(&summary);
        let detail = label("", "monospace");
        detail.set_selectable(true);
        detail.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        let detail_scroll = gtk::ScrolledWindow::builder()
            .child(&detail)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .max_content_height(160)
            .propagate_natural_height(true)
            .build();
        let details = gtk::Expander::builder()
            .label("Details")
            .child(&detail_scroll)
            .build();
        details.set_visible(false);
        root.append(&line);
        root.append(&details);
        root.set_visible(false);
        Self {
            root,
            summary,
            icon,
            details,
            detail,
        }
    }
    pub fn show(&self, summary: &str, details: Option<&str>, error: bool) {
        self.root.set_visible(true);
        self.summary.set_text(summary);
        for w in [
            &self.summary.clone().upcast::<gtk::Widget>(),
            &self.icon.clone().upcast(),
        ] {
            if error {
                w.add_css_class("error");
            } else {
                w.remove_css_class("error");
            }
        }
        self.icon.set_icon_name(Some(if error {
            "dialog-error-symbolic"
        } else {
            "dialog-information-symbolic"
        }));
        self.detail.set_text(details.unwrap_or(""));
        self.details
            .set_visible(details.is_some_and(|s| !s.is_empty()));
        if details.is_none() {
            self.details.set_expanded(false);
        }
    }
}

#[derive(Clone)]
pub struct Footer {
    pub root: gtk::Box,
    pub save: gtk::Button,
    pub discard: gtk::Button,
    pub notice: Notice,
    dirty: Rc<Cell<bool>>,
    valid: Rc<Cell<bool>>,
    busy: Rc<Cell<bool>>,
    defaults: Rc<Cell<bool>>,
    baseline: Rc<Cell<bool>>,
    unavailable: Option<String>,
    failure: Rc<RefCell<Option<String>>>,
}
impl Footer {
    pub fn new(defaults: bool, baseline: bool, unavailable: Option<&str>) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        root.add_css_class("editor-footer");
        root.set_widget_name("editor-footer");
        let notice = Notice::new();
        notice.root.set_hexpand(true);
        notice.root.set_valign(gtk::Align::Center);
        root.append(&notice.root);
        let actions = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .column_spacing(8)
            .row_spacing(8)
            .min_children_per_line(1)
            .max_children_per_line(2)
            .halign(gtk::Align::End)
            .build();
        actions.add_css_class("action-flow");
        actions.set_valign(gtk::Align::Center);
        let discard = gtk::Button::with_label("Discard changes");
        let save = gtk::Button::with_label("Save changes");
        save.add_css_class("suggested-action");
        actions.insert(&discard, -1);
        actions.insert(&save, -1);
        root.append(&actions);
        let result = Self {
            root,
            save,
            discard,
            notice,
            dirty: Rc::new(Cell::new(!baseline)),
            valid: Rc::new(Cell::new(true)),
            busy: Rc::new(Cell::new(false)),
            defaults: Rc::new(Cell::new(defaults)),
            baseline: Rc::new(Cell::new(baseline)),
            unavailable: unavailable.map(str::to_owned),
            failure: Rc::new(RefCell::new(None)),
        };
        result.refresh();
        result
    }
    pub fn sync(&self, dirty: bool, valid: bool) {
        self.dirty.set(dirty);
        self.valid.set(valid);
        self.failure.borrow_mut().take();
        self.refresh();
    }
    pub fn saving(&self) {
        self.busy.set(true);
        self.refresh();
    }
    pub fn success(&self) {
        self.busy.set(false);
        self.dirty.set(false);
        self.defaults.set(false);
        self.baseline.set(true);
        self.failure.borrow_mut().take();
        self.refresh();
    }
    pub fn error(&self, error: &str) {
        self.busy.set(false);
        *self.failure.borrow_mut() = Some(error.into());
        self.refresh();
    }
    fn refresh(&self) {
        self.save.set_sensitive(
            self.dirty.get() && self.valid.get() && !self.busy.get() && self.unavailable.is_none(),
        );
        self.discard
            .set_sensitive(self.dirty.get() && !self.busy.get() && self.baseline.get());
        if self.busy.get() {
            self.notice.show("Saving changes…", None, false);
        } else if let Some(error) = self.failure.borrow().as_ref() {
            self.notice.show(
                "Changes were not saved. Your draft is preserved.",
                Some(error),
                true,
            );
        } else if !self.valid.get() {
            self.notice.show(
                "Some settings are invalid. Review this draft before saving.",
                None,
                true,
            );
        } else {
            let status = if self.dirty.get() {
                "Unsaved changes"
            } else if self.defaults.get() {
                "Using effective defaults · no saved overrides"
            } else {
                "Saved · no unsaved changes"
            };
            self.notice.show(status, self.unavailable.as_deref(), false);
            // Capability limits remain visible without expanding technical details.
            if let Some(reason) = &self.unavailable {
                self.notice.summary.set_text(&format!("{status}. {reason}"));
                self.notice.details.set_visible(false);
            }
        }
    }
}

pub fn shell(body: &gtk::Box, footer: &Footer) -> gtk::Box {
    let root = column(0);
    root.set_widget_name("editor-page");
    root.append(&scroll(body));
    root.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    let clamp = adw::Clamp::builder()
        .maximum_size(1080)
        .tightening_threshold(860)
        .child(&footer.root)
        .build();
    root.append(&clamp);
    root
}

pub fn error(parent: &gtk::Box, summary: &str, detail: &str) {
    let n = Notice::new();
    n.show(summary, Some(detail), true);
    let after_description = parent.first_child().and_then(|w| w.next_sibling());
    parent.append(&n.root);
    if let Some(description) = after_description {
        parent.reorder_child_after(&n.root, Some(&description));
    }
}
