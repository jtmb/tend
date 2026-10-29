use crate::{column, job, label, page, scroll, workbench};
use adw::prelude::*;
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};
use tend::{
    diagnostics::{self, Session},
    sampler::Detail,
};

struct Recording {
    session: Session,
    started: Instant,
    boot_started: Option<u64>,
    subscription: Option<crate::monitor::Handle>,
}
struct Controller {
    recording: RefCell<Option<Recording>>,
    finished: RefCell<Option<Session>>,
    start: gtk::Button,
    stop: gtk::Button,
    cancel: gtk::Button,
    save: gtk::Button,
    mark: gtk::Button,
    status: gtk::Label,
    progress: gtk::ProgressBar,
    output: gtk::TextView,
}
impl Recording {
    fn elapsed_ms(&self) -> u64 {
        match (self.boot_started, tend::observations::boot_millis()) {
            (Some(before), Some(after)) => after.saturating_sub(before),
            _ => self.started.elapsed().as_millis() as u64,
        }
    }
}
impl Controller {
    fn finish(&self, discard: bool) {
        if let Some(mut r) = self.recording.borrow_mut().take() {
            r.subscription.take();
            r.session.finish(r.elapsed_ms());
            if discard {
                *self.finished.borrow_mut() = None;
                self.output
                    .buffer()
                    .set_text("Recording discarded. No history was saved.");
            } else {
                self.output.buffer().set_text(&r.session.summary());
                *self.finished.borrow_mut() = Some(r.session);
            }
        }
        self.start.set_sensitive(true);
        self.stop.set_sensitive(false);
        self.cancel.set_sensitive(false);
        self.mark.set_sensitive(false);
        self.save.set_sensitive(
            self.finished
                .borrow()
                .as_ref()
                .is_some_and(|s| !s.points.is_empty()),
        );
        self.status.set_text(if discard {
            "Recording cancelled"
        } else {
            "Recording finished · review the evidence before saving"
        });
    }
    fn begin(self: &Rc<Self>, workload: &str, seconds: u64) {
        if self.recording.borrow().is_some() {
            return;
        }
        let session = match Session::new(workload, seconds) {
            Ok(s) => s,
            Err(e) => {
                self.status.set_text(&e);
                return;
            }
        };
        *self.finished.borrow_mut() = None;
        self.start.set_sensitive(false);
        self.stop.set_sensitive(true);
        self.cancel.set_sensitive(true);
        self.mark.set_sensitive(true);
        self.save.set_sensitive(false);
        self.output.buffer().set_text("Recording system counters and app CPU. Continue your normal workload; add a marker when you notice a stall.");
        self.progress.set_fraction(0.);
        *self.recording.borrow_mut() = Some(Recording {
            session,
            started: Instant::now(),
            boot_started: tend::observations::boot_millis(),
            subscription: None,
        });
        let weak = Rc::downgrade(self);
        match crate::monitor::subscribe(Detail::Full, Duration::from_secs(2), move |result| {
            let Some(c) = weak.upgrade() else { return };
            let due = {
                let mut data = c.recording.borrow_mut();
                let Some(r) = data.as_mut() else { return };
                let elapsed = r.elapsed_ms();
                match result {
                    Ok(capture) => r.session.push(elapsed, &capture),
                    Err(e) => {
                        r.session.errors += 1;
                        c.status.set_text(&format!("Sample unavailable: {e}"));
                    }
                }
                c.progress
                    .set_fraction((elapsed as f64 / r.session.requested_ms as f64).clamp(0., 1.));
                c.status.set_text(&format!(
                    "Recording · {}:{:02} / {}:{:02} · {} samples",
                    elapsed / 60000,
                    (elapsed / 1000) % 60,
                    r.session.requested_ms / 60000,
                    (r.session.requested_ms / 1000) % 60,
                    r.session.points.len()
                ));
                elapsed >= r.session.requested_ms
            };
            if due {
                c.finish(false);
            }
        }) {
            Ok(handle) => {
                if let Some(r) = self.recording.borrow_mut().as_mut() {
                    r.subscription = Some(handle);
                }
            }
            Err(e) => {
                self.finish(true);
                self.status.set_text(&e);
            }
        }
    }
}

pub fn add(stack: &gtk::Stack) {
    let root = page(
        "Diagnostics",
        "Record a workload, mark a slowdown, and compare measured activity.",
    );
    let intro = column(8);
    intro.add_css_class("metric-card");
    intro.append(&label("Understand a slowdown", "title-2"));
    intro.append(&label("Use the same projects and interactions for each recording. The default is three minutes, sampled every two seconds. A recording continues if Tend is hidden and stops when you quit.","body"));
    root.append(&intro);
    let incident_body=column(8);
    incident_body.append(&label("Inspect recorded kernel memory kills, memory use at the event and the affected scope. Current free memory may recover after a kill.","caption"));
    let incident_actions=workbench::buttons(&incident_body);
    let explain=gtk::Button::with_label("Explain recent memory failures");
    incident_actions.insert(&explain,-1);
    let memory_report=workbench::text_panel(&incident_body);
    workbench::report_hint(&memory_report,"Read the last 24 hours of available kernel OOM evidence. No stress test is needed.");
    workbench::exports(&incident_body,&memory_report);
    explain.connect_clicked(move|button|workbench::report_job(button,&memory_report,||tend::memory_incidents::report(2)));
    let incident_section=gtk::Expander::builder().label("Memory failures and OOM kills").child(&incident_body).build();
    incident_section.set_widget_name("memory-incidents");crate::editor::guard_expanders(&incident_section);
    root.append(&incident_section);
    let group = adw::PreferencesGroup::builder()
        .title("New recording")
        .build();
    let workload = adw::EntryRow::builder().title("Workload label").build();
    workload.set_text("Development multitasking");
    group.add(&workload);
    let duration =
        gtk::DropDown::from_strings(&["1 minute", "3 minutes", "5 minutes", "15 minutes"]);
    duration.set_selected(1);
    crate::row(
        &group,
        "Duration",
        "Bounded recording; no continuous tracing",
        &duration,
    );
    root.append(&group);
    let controls = workbench::buttons(&root);
    let start = gtk::Button::with_label("Start recording");
    start.add_css_class("suggested-action");
    let stop = gtk::Button::with_label("Finish recording");
    let cancel = gtk::Button::with_label("Discard recording");
    let save = gtk::Button::with_label("Save recording");
    for b in [&start, &stop, &cancel, &save] {
        controls.insert(b, -1);
    }
    for b in [&stop, &cancel, &save] {
        b.set_sensitive(false);
    }
    let status = label("Ready · no diagnostic collection running", "caption");
    root.append(&status);
    let progress = gtk::ProgressBar::new();
    root.append(&progress);
    let markers = column(8);
    let marker = gtk::Entry::builder()
        .placeholder_text("e.g. Scrolling stalled")
        .max_length(80)
        .hexpand(true)
        .build();
    markers.append(&marker);
    let mark = gtk::Button::with_label("Add workload marker");
    mark.set_halign(gtk::Align::Start);
    mark.set_sensitive(false);
    markers.append(&mark);
    root.append(&markers);
    let output = workbench::text_panel(&root);
    workbench::report_hint(&output, "Start a recording to see measured evidence here.");
    workbench::exports(&root, &output);
    let controller = Rc::new(Controller {
        recording: RefCell::new(None),
        finished: RefCell::new(None),
        start: start.clone(),
        stop: stop.clone(),
        cancel: cancel.clone(),
        save: save.clone(),
        mark: mark.clone(),
        status: status.clone(),
        progress,
        output: output.clone(),
    });
    let c = controller.clone();
    start.connect_clicked(move |_| {
        c.begin(
            &workload.text(),
            [60, 180, 300, 900][duration.selected().min(3) as usize],
        )
    });
    let c = controller.clone();
    stop.connect_clicked(move |_| c.finish(false));
    let c = controller.clone();
    cancel.connect_clicked(move |_| c.finish(true));
    let c = controller.clone();
    mark.connect_clicked(move |_| {
        if let Some(r) = c.recording.borrow_mut().as_mut() {
            match r.session.mark(r.elapsed_ms(), &marker.text()) {
                Ok(()) => {
                    marker.set_text("");
                    c.status.set_text("Workload marker added");
                }
                Err(e) => c.status.set_text(&e),
            }
        }
    });
    let c = controller.clone();
    save.connect_clicked(move |b| {
        let Some(session) = c.finished.borrow().clone() else {
            return;
        };
        b.set_sensitive(false);
        let (b, status) = (b.clone(), c.status.clone());
        job(
            move || session.save(),
            move |result| match result {
                Ok(message) => status.set_text(&message),
                Err(e) => {
                    status.set_text(&e);
                    b.set_sensitive(true);
                }
            },
        );
    });
    let saved=adw::PreferencesGroup::builder().title("Saved workload comparisons").description("Load up to 20 recent local recordings. Compare the same workload and repeat three times.").build();
    let left = gtk::DropDown::from_strings(&[]);
    let right = gtk::DropDown::from_strings(&[]);
    crate::row(&saved, "Baseline", "Earlier recording", &left);
    crate::row(&saved, "Comparison", "Later recording", &right);
    root.append(&saved);
    let actions = workbench::buttons(&root);
    let load = gtk::Button::with_label("Load saved recordings");
    let view = gtk::Button::with_label("View baseline");
    let compare = gtk::Button::with_label("Compare recordings");
    view.set_sensitive(false);
    compare.set_sensitive(false);
    for b in [&load, &view, &compare] {
        actions.insert(b, -1);
    }
    let sessions: Rc<RefCell<Vec<Session>>> = Rc::new(RefCell::new(vec![]));
    let (s, l, r, o, v, cmp) = (
        sessions.clone(),
        left.clone(),
        right.clone(),
        output.clone(),
        view.clone(),
        compare.clone(),
    );
    load.connect_clicked(move |b| {
        b.set_sensitive(false);
        let (b, s, l, r, o, v, cmp) = (
            b.clone(),
            s.clone(),
            l.clone(),
            r.clone(),
            o.clone(),
            v.clone(),
            cmp.clone(),
        );
        job(diagnostics::load, move |result| {
            b.set_sensitive(true);
            match result {
                Ok(values) => {
                    let names: Vec<_> = values
                        .iter()
                        .map(|s| {
                            format!(
                                "{} · {}",
                                tend::presentation::when((s.started_ms / 1000) as i64),
                                s.workload
                            )
                        })
                        .collect();
                    let strings: Vec<_> = names.iter().map(String::as_str).collect();
                    l.set_model(Some(&gtk::StringList::new(&strings)));
                    r.set_model(Some(&gtk::StringList::new(&strings)));
                    l.set_selected(if values.len() > 1 {
                        1
                    } else if values.is_empty() {
                        gtk::INVALID_LIST_POSITION
                    } else {
                        0
                    });
                    r.set_selected(if values.is_empty() {
                        gtk::INVALID_LIST_POSITION
                    } else {
                        0
                    });
                    v.set_sensitive(!values.is_empty());
                    cmp.set_sensitive(values.len() > 1);
                    *s.borrow_mut() = values;
                    o.buffer().set_text(if s.borrow().is_empty() {
                        "No saved recordings. Finish and save a recording first."
                    } else {
                        "Choose a baseline and comparison to inspect the measured differences."
                    });
                }
                Err(e) => o.buffer().set_text(&e),
            }
        });
    });
    let (s, l, o) = (sessions.clone(), left.clone(), output.clone());
    view.connect_clicked(move |_| {
        if let Some(session) = s.borrow().get(l.selected() as usize) {
            o.buffer().set_text(&session.summary());
        }
    });
    compare.connect_clicked(move |_| {
        let sessions = sessions.borrow();
        match (
            sessions.get(left.selected() as usize),
            sessions.get(right.selected() as usize),
        ) {
            (Some(a), Some(b)) if a.id != b.id => {
                output.buffer().set_text(&diagnostics::compare(a, b))
            }
            _ => output
                .buffer()
                .set_text("Choose two different recordings to compare."),
        }
    });
    stack.add_titled(&scroll(&root), Some("diagnostics"), "Diagnostics");
}
