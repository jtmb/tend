use crate::{editor, job, label, page, workbench};
use adw::prelude::*;
use gtk::{gio, glib};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    time::Duration,
};
use tend::{
    workspaces::{self, Association, Container, Settings},
    workstation as w,
};
struct State {
    draft: RefCell<Settings>,
    baseline: RefCell<Settings>,
    loaded: Cell<bool>,
    docker_busy: Cell<bool>,
    cached_busy: Cell<bool>,
    sampled: Cell<bool>,
    table_scroll: gtk::ScrolledWindow,
    editing: RefCell<Option<String>>,
    list: gtk::ListBox,
    store: gio::ListStore,
    footer: editor::Footer,
    status: gtk::Label,
    summary: gtk::Label,
    containers: gtk::Label,
    associations: RefCell<Vec<Association>>,
    docker: RefCell<Option<(u64, Vec<Container>)>>,
    subscription: RefCell<Option<crate::monitor::Handle>>,
    refresh: gtk::Button,
    edit: gtk::Button,
    remove: gtk::Button,
    storage: gtk::Button,
    cached: gtk::Button,
    inspect: gtk::Button,
}
impl State {
    fn selected(&self) -> Option<workspaces::Workspace> {
        let id = self.list.selected_row()?.widget_name();
        self.draft
            .borrow()
            .projects
            .iter()
            .find(|p| p.id == id)
            .cloned()
    }
    fn dirty(&self) -> bool {
        *self.draft.borrow() != *self.baseline.borrow()
    }
    fn render(&self) {
        let selected = self.selected().map(|p| p.id);
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        for p in &self.draft.borrow().projects {
            let row = adw::ActionRow::builder()
                .use_markup(false)
                .title(&p.name)
                .subtitle(p.root.to_string_lossy())
                .build();
            row.set_widget_name(&p.id);
            self.list.append(&row);
            if selected.as_ref() == Some(&p.id) {
                self.list.select_row(Some(&row));
            }
        }
        if self.list.selected_row().is_none() {
            self.list.select_row(self.list.row_at_index(0).as_ref());
        }
        self.footer.sync(self.dirty(), self.loaded.get());
        self.paint();
    }
    fn paint(&self) {
        let selected = self.selected();
        let enabled = selected.is_some() && self.loaded.get();
        for b in [
            &self.edit,
            &self.remove,
            &self.storage,
            &self.cached,
            &self.inspect,
        ] {
            b.set_sensitive(enabled);
        }
        self.inspect
            .set_sensitive(enabled && !self.docker_busy.get());
        self.cached
            .set_sensitive(enabled && !self.cached_busy.get());
        self.refresh
            .set_sensitive(enabled && self.subscription.borrow().is_none());
        let Some(p) = selected else {
            self.summary
                .set_text("Load or add a project, then select it to inspect recorded activity.");
            self.containers
                .set_text("Container metadata loads only when requested.");
            self.store.remove_all();
            self.table_scroll.set_visible(false);
            return;
        };
        let data = self.associations.borrow();
        let selected: Vec<_> = data
            .iter()
            .filter(|a| a.workspace_id == p.id)
            .cloned()
            .collect();
        let totals = workspaces::totals(&selected);
        let t = totals.get(&p.id);
        self.summary.set_text(&if !self.sampled.get() {
            "Activity not loaded. Refresh to check this project.".into()
        } else if let Some(t) = t {
            format!(
                "{} matching {} · RSS {} · CPU {}",
                t.processes,
                if t.processes == 1 {
                    "process"
                } else {
                    "processes"
                },
                w::bytes(t.memory),
                t.cpu
                    .map(|v| format!("{v:.1}%"))
                    .unwrap_or_else(|| "Unavailable".into())
            )
        } else {
            "No matching processes in the last requested sample. Refresh activity to check again."
                .into()
        });
        let values: Vec<_> = selected
            .into_iter()
            .map(glib::BoxedAnyObject::new)
            .collect();
        self.store.splice(0, self.store.n_items(), &values);
        self.table_scroll.set_visible(!values.is_empty());
        if let Some((time, containers)) = self.docker.borrow().as_ref() {
            let mut lines = vec![format!(
                "Container labels read {} · cached until you inspect again",
                tend::presentation::when(*time as i64)
            )];
            for c in containers
                .iter()
                .filter(|c| c.workspace_id.as_ref() == Some(&p.id))
            {
                lines.push(format!(
                    "{} · {} · {}",
                    c.name.trim_start_matches('/'),
                    c.state,
                    c.compose_service
                        .as_deref()
                        .unwrap_or("No Compose service label")
                ));
            }
            if lines.len() == 1 {
                lines.push("No matching Compose working-directory labels.".into());
            }
            lines.push("Matches use local Compose directory labels; they do not verify a container’s files or attribute its resource usage.".into());
            self.containers.set_text(&lines.join("\n"));
        } else {
            self.containers
                .set_text("Container metadata loads only when requested.");
        }
    }
}
pub fn add(stack: &gtk::Stack, win: &adw::ApplicationWindow, location: workbench::StorageLocation) {
    let body=page("Workspaces","Group development activity around project folders. Storage and Docker checks run only when requested.");
    let load = gtk::Button::with_label("Load workspaces");
    load.set_halign(gtk::Align::Start);
    body.append(&load);
    let form = crate::column(10);
    let expander = gtk::Expander::builder()
        .label("Add or edit a project")
        .child(&form)
        .build();
    body.append(&expander);
    let group = adw::PreferencesGroup::builder()
        .title("Project folders")
        .description("Add or edit projects below, then save the draft at the bottom of this page.")
        .build();
    let name = adw::EntryRow::builder().title("Project name").build();
    let path = adw::EntryRow::builder()
        .title("Absolute project folder")
        .build();
    group.add(&name);
    group.add(&path);
    form.append(&group);
    let edit_controls = workbench::buttons(&form);
    let add = gtk::Button::with_label("Add project to draft");
    let clear = gtk::Button::with_label("Clear form");
    let choose = gtk::Button::with_label("Choose folder…");
    for b in [&add, &clear, &choose] {
        edit_controls.insert(b, -1);
    }
    add.set_sensitive(false);
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.set_placeholder(Some(&label("No registered projects yet", "caption")));
    let project_scroll = gtk::ScrolledWindow::builder()
        .child(&list)
        .min_content_height(50)
        .max_content_height(240)
        .propagate_natural_height(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    body.append(&project_scroll);
    let actions = workbench::buttons(&body);
    let edit = gtk::Button::with_label("Edit selected");
    let remove = gtk::Button::with_label("Remove from draft");
    let storage = gtk::Button::with_label("Inspect project storage");
    let cached = gtk::Button::with_label("Load last scan");
    for b in [&edit, &remove, &storage, &cached] {
        b.set_sensitive(false);
        actions.insert(b, -1);
    }
    let summary = label(
        "Load or add a project, then select it to inspect activity.",
        "title-4",
    );
    body.append(&summary);
    body.append(&label("Associations use working directories or guarded ancestry. RSS includes shared pages. CPU is a share of total capacity over the requested two-second interval.","caption"));
    let inspect_controls = workbench::buttons(&body);
    let refresh = gtk::Button::with_label("Refresh project activity");
    let inspect = gtk::Button::with_label("Inspect Compose labels");
    refresh.set_sensitive(false);
    inspect.set_sensitive(false);
    inspect_controls.insert(&refresh, -1);
    inspect_controls.insert(&inspect, -1);
    let store = gio::ListStore::new::<glib::BoxedAnyObject>();
    let selection = gtk::SingleSelection::new(Some(store.clone()));
    selection.set_autoselect(false);
    selection.set_can_unselect(true);
    let table = gtk::ColumnView::new(Some(selection.clone()));
    table.set_show_row_separators(true);
    table.add_css_class("data-table");
    for (index, title) in ["Process", "PID", "CPU", "Memory", "Association"]
        .iter()
        .enumerate()
    {
        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(move |_, o| {
            let item = o.downcast_ref::<gtk::ListItem>().unwrap();
            item.set_child(Some(
                &gtk::Label::builder()
                    .xalign(if (1..=3).contains(&index) { 1. } else { 0. })
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
            let a = object.borrow::<Association>();
            let text = item.child().unwrap().downcast::<gtk::Label>().unwrap();
            text.set_text(&match index {
                0 => a.name.clone(),
                1 => a.process.pid.to_string(),
                2 => a
                    .cpu
                    .map(|v| format!("{v:.1}%"))
                    .unwrap_or_else(|| "—".into()),
                3 => w::bytes(a.memory),
                _ => a.evidence.clone(),
            });
            text.set_tooltip_text(Some(&a.evidence));
        });
        let col = gtk::ColumnViewColumn::new(Some(title), Some(factory));
        col.set_expand(index == 0 || index == 4);
        col.set_resizable(true);
        col.set_fixed_width([160, 90, 85, 110, 270][index]);
        table.append_column(&col);
    }
    let scroller = gtk::ScrolledWindow::builder()
        .child(&table)
        .min_content_height(90)
        .max_content_height(300)
        .propagate_natural_height(true)
        .hexpand(true)
        .vexpand(false)
        .build();
    scroller.add_css_class("card");
    scroller.set_visible(false);
    body.append(&scroller);
    let containers = label("Container metadata loads only when requested.", "caption");
    containers.set_selectable(true);
    body.append(&containers);
    let status = label("Ready · no project monitoring running", "caption");
    body.append(&status);
    status.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    status.set_lines(3);
    status.set_ellipsize(gtk::pango::EllipsizeMode::End);
    status.set_selectable(true);
    body.reorder_child_after(&status, Some(&actions));
    let footer = editor::Footer::new(false, true, None);
    footer.sync(false, false);
    footer
        .notice
        .show("Load workspaces to edit settings.", None, false);
    let state = Rc::new(State {
        draft: RefCell::new(Settings::default()),
        baseline: RefCell::new(Settings::default()),
        loaded: Cell::new(false),
        docker_busy: Cell::new(false),
        cached_busy: Cell::new(false),
        sampled: Cell::new(false),
        table_scroll: scroller.clone(),
        editing: RefCell::new(None),
        list: list.clone(),
        store,
        footer: footer.clone(),
        status: status.clone(),
        summary,
        containers,
        associations: RefCell::new(vec![]),
        docker: RefCell::new(None),
        subscription: RefCell::new(None),
        refresh: refresh.clone(),
        edit: edit.clone(),
        remove: remove.clone(),
        storage: storage.clone(),
        cached: cached.clone(),
        inspect: inspect.clone(),
    });
    let weak = Rc::downgrade(&state);
    list.connect_row_selected(move |_, _| {
        if let Some(s) = weak.upgrade() {
            s.paint();
        }
    });
    let (s, b, a, form_expander) = (state.clone(), body.clone(), add.clone(), expander.clone());
    load.connect_clicked(move |_| {
        if s.dirty() {
            s.status
                .set_text("Save or discard the current draft before loading settings.");
            return;
        }
        b.set_sensitive(false);
        let (s, b, a, form_expander) = (s.clone(), b.clone(), a.clone(), form_expander.clone());
        job(Settings::read, move |result| {
            b.set_sensitive(true);
            match result {
                Ok(value) => {
                    s.subscription.borrow_mut().take();
                    s.associations.borrow_mut().clear();
                    s.sampled.set(false);
                    s.docker.borrow_mut().take();
                    *s.draft.borrow_mut() = value.clone();
                    *s.baseline.borrow_mut() = value;
                    s.loaded.set(true);
                    a.set_sensitive(true);
                    s.status
                        .set_text("Workspaces loaded · checks run on request");
                    s.render();
                    form_expander.set_expanded(s.draft.borrow().projects.is_empty());
                }
                Err(e) => {
                    s.loaded.set(false);
                    a.set_sensitive(false);
                    s.status.set_text(&e);
                    s.footer.sync(false, false);
                    s.footer.error(&e);
                    s.paint();
                }
            }
        });
    });
    let (parent, entry) = (win.clone(), path.clone());
    choose.connect_clicked(move |_| {
        let dialog = gtk::FileDialog::builder()
            .title("Choose project folder")
            .build();
        let entry = entry.clone();
        dialog.select_folder(Some(&parent), None::<&gio::Cancellable>, move |result| {
            if let Ok(file) = result {
                if let Some(path) = file.path().and_then(|p| p.to_str().map(str::to_owned)) {
                    entry.set_text(&path);
                }
            }
        });
    });
    let (s, n, p, b, a, form_expander) = (
        state.clone(),
        name.clone(),
        path.clone(),
        body.clone(),
        add.clone(),
        expander.clone(),
    );
    add.connect_clicked(move |_| {
        if !s.loaded.get() {
            return;
        }
        let draft = s.draft.borrow().clone();
        let editing = s.editing.borrow().clone();
        let name = n.text().to_string();
        let path = PathBuf::from(p.text().as_str());
        s.status.set_text("Checking project folder…");
        b.set_sensitive(false);
        let (s, n, p, b, a, form_expander) = (
            s.clone(),
            n.clone(),
            p.clone(),
            b.clone(),
            a.clone(),
            form_expander.clone(),
        );
        job(
            move || {
                let mut draft = draft;
                let position = editing
                    .as_ref()
                    .and_then(|id| draft.projects.iter().position(|p| &p.id == id));
                if let Some(index) = position {
                    draft.projects.remove(index);
                }
                draft.add(&name, &path)?;
                if let Some(index) = position {
                    let mut project = draft.projects.pop().unwrap();
                    project.id = editing.unwrap();
                    draft.projects.insert(index, project);
                }
                draft.validate()?;
                Ok(draft)
            },
            move |result| {
                b.set_sensitive(true);
                match result {
                    Ok(draft) => {
                        *s.draft.borrow_mut() = draft;
                        s.editing.borrow_mut().take();
                        n.set_text("");
                        p.set_text("");
                        a.set_label("Add project to draft");
                        s.associations.borrow_mut().clear();
                        s.sampled.set(false);
                        s.docker.borrow_mut().take();
                        s.status
                            .set_text("Project draft updated. Save changes to keep it.");
                        s.render();
                        form_expander.set_expanded(false);
                    }
                    Err(e) => s.status.set_text(&e),
                }
            },
        );
    });
    let (s, n, p, a, form_expander) = (
        state.clone(),
        name.clone(),
        path.clone(),
        add.clone(),
        expander.clone(),
    );
    edit.connect_clicked(move |_| {
        if let Some(project) = s.selected() {
            *s.editing.borrow_mut() = Some(project.id);
            n.set_text(&project.name);
            p.set_text(&project.root.to_string_lossy());
            a.set_label("Update project draft");
            form_expander.set_expanded(true);
            n.grab_focus();
        }
    });
    let (s, n, p, a) = (state.clone(), name.clone(), path.clone(), add.clone());
    clear.connect_clicked(move |_| {
        s.editing.borrow_mut().take();
        n.set_text("");
        p.set_text("");
        a.set_label("Add project to draft");
    });
    let (s, n, p, a) = (state.clone(), name.clone(), path.clone(), add.clone());
    remove.connect_clicked(move |_| {
        if let Some(project) = s.selected() {
            if s.editing.borrow().as_ref() == Some(&project.id) {
                s.editing.borrow_mut().take();
                n.set_text("");
                p.set_text("");
                a.set_label("Add project to draft");
            }
            s.draft.borrow_mut().projects.retain(|p| p.id != project.id);
            s.status
                .set_text("Project removed from the draft. Save changes to keep it.");
            s.render();
        }
    });
    let (s, n, p, a) = (state.clone(), name.clone(), path.clone(), add.clone());
    footer.discard.connect_clicked(move |_| {
        *s.draft.borrow_mut() = s.baseline.borrow().clone();
        s.editing.borrow_mut().take();
        n.set_text("");
        p.set_text("");
        a.set_label("Add project to draft");
        s.status.set_text("Draft discarded");
        s.render();
    });
    let (s, b) = (state.clone(), body.clone());
    footer.save.connect_clicked(move |_| {
        let draft = s.draft.borrow().clone();
        s.footer.saving();
        b.set_sensitive(false);
        let (s, b, saved) = (s.clone(), b.clone(), draft.clone());
        job(
            move || draft.save(),
            move |result| {
                b.set_sensitive(true);
                match result {
                    Ok(()) => {
                        *s.baseline.borrow_mut() = saved;
                        s.footer.success();
                        s.status.set_text("Workspace settings saved");
                    }
                    Err(e) => s.footer.error(&e),
                }
            },
        );
    });
    let (s, target) = (state.clone(), stack.clone());
    storage.connect_clicked(move |_| {
        if let Some(project) = s.selected() {
            location.path.set_text(&project.root.to_string_lossy());
            target.set_visible_child_name("storage");
        }
    });
    let s = state.clone();
    cached.connect_clicked(move |_| {
        let Some(project)=s.selected() else {return};if s.cached_busy.replace(true) {return}s.paint();
        let (s,expected)=(s.clone(),project.clone());job(move ||tend::history::snapshots::latest(&project.root),move |result| {
            s.cached_busy.set(false);s.paint();if s.selected().as_ref()!=Some(&expected) {return}
            s.status.set_text(&match result {Ok(Some(report))=>format!("Recorded scan for {} · {} · {} {} · {} allocated · inspect storage for a new scan",expected.name,tend::presentation::when(report.time_ms/1000),report.files,if report.files==1 {"file"}else{"files"},w::bytes(report.allocated)),Ok(None)=>"No complete scan recorded for this project yet.".into(),Err(e)=>e});
        });
    });
    let s = state.clone();
    inspect.connect_clicked(move |_| {
        if s.docker_busy.replace(true) {return}s.paint();let settings=s.draft.borrow().clone();let expected=settings.clone();let s=s.clone();
        job(move ||workspaces::containers(&settings),move |result| {
            s.docker_busy.set(false);s.paint();if *s.draft.borrow()!=expected {s.status.set_text("Projects changed; inspect Compose labels again.");return}
            match result {Ok(containers)=>{*s.docker.borrow_mut()=Some((w::now(),containers));s.paint();s.status.set_text("Compose labels loaded; matching is based on declared working directories.");},Err(e)=>s.status.set_text(&e)}
        });
    });
    let s = state.clone();
    refresh.connect_clicked(move |_| {
        if s.subscription.borrow().is_some() {
            return;
        }
        s.refresh.set_sensitive(false);
        s.status
            .set_text("Sampling project activity for two seconds…");
        let count = Rc::new(Cell::new(0));
        let weak = Rc::downgrade(&s);
        match crate::monitor::subscribe(
            tend::sampler::Detail::Full,
            Duration::from_secs(2),
            move |result| {
                let Some(s) = weak.upgrade() else { return };
                match result {
                    Ok(capture) => {
                        count.set(count.get() + 1);
                        if let Some(full) = &capture.full {
                            s.sampled.set(true);
                            *s.associations.borrow_mut() = workspaces::processes(
                                &full.tasks,
                                &full.boot_id,
                                &s.draft.borrow(),
                            );
                        }
                        if count.get() >= 2 {
                            s.subscription.borrow_mut().take();
                            s.status.set_text(&format!(
                                "Activity sampled {} · refresh to measure again",
                                tend::presentation::when(w::now() as i64)
                            ));
                        }
                        s.paint();
                    }
                    Err(e) => {
                        s.subscription.borrow_mut().take();
                        s.status.set_text(&e);
                        s.paint();
                    }
                }
            },
        ) {
            Ok(handle) => *s.subscription.borrow_mut() = Some(handle),
            Err(e) => {
                s.status.set_text(&e);
                s.refresh.set_sensitive(true);
            }
        }
    });
    let weak = Rc::downgrade(&state);
    win.connect_close_request(move |_| {
        if let Some(s) = weak.upgrade() {
            s.subscription.borrow_mut().take();
            s.status
                .set_text("Activity sampling stopped. Refresh to measure again.");
            s.refresh.set_sensitive(s.selected().is_some());
        }
        glib::Propagation::Proceed
    });
    {
        let pending=Rc::downgrade(&state);let(n,p)=(name.downgrade(),path.downgrade());let update=Rc::downgrade(&state);
        crate::settings_ui::register(tend::settings_bundle::Section::Workspaces,move||pending.upgrade().is_some_and(|s|s.dirty()||n.upgrade().is_some_and(|e|!e.text().is_empty())||p.upgrade().is_some_and(|e|!e.text().is_empty())),move|value|{
            if let(Some(s),Ok(v))=(update.upgrade(),serde_json::from_value::<Settings>(value.clone())){s.subscription.borrow_mut().take();s.associations.borrow_mut().clear();s.docker.borrow_mut().take();s.loaded.set(true);*s.draft.borrow_mut()=v.clone();*s.baseline.borrow_mut()=v;s.footer.success();s.render();}
        });
    }
    stack.add_titled(
        &editor::shell(&body, &footer),
        Some("workspaces"),
        "Workspaces",
    );
}
