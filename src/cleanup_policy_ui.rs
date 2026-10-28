//! Native editors for independent manual/cron and daily maintenance policies.
use crate::workbench::{button, buttons, confirm, exports, report_hint, text_panel};
use crate::{editor, job, page, row};
use adw::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};
use tend::{cleanup_rules as rules, daily_policy as daily, workstation as w};

#[derive(Clone)]
struct Controls {
    checks: Vec<gtk::Switch>,
    numbers: Vec<gtk::SpinButton>,
}
impl Controls {
    fn build(parent: &gtk::Box, daily: bool) -> Self {
        let group = adw::PreferencesGroup::builder()
            .title("Cleanup categories")
            .build();
        let mut checks = Vec::new();
        let categories = if daily {
            vec![
                (
                    "Obsolete APT downloads",
                    "Remove package downloads no longer available from repositories",
                ),
                (
                    "Unused Docker build cache",
                    "Unused cache only; retain a useful cache allowance",
                ),
                (
                    "Dangling Docker images",
                    "Creation age; preserve tagged images, containers and volumes",
                ),
                (
                    "npm diagnostic logs",
                    "Expired logs only; preserve cached package downloads",
                ),
                (
                    "Verify npm cache",
                    "Check integrity and reclaim unneeded entries at the selected interval",
                ),
            ]
        } else {
            vec![
                (
                    "Obsolete APT downloads",
                    "Manual cleanup only; user cron leaves APT to daily maintenance",
                ),
                (
                    "Unused Docker build cache",
                    "Unused cache only; retain a useful cache allowance",
                ),
                (
                    "Dangling Docker images",
                    "Creation age; preserve tagged images, containers and volumes",
                ),
                (
                    "npm diagnostic logs",
                    "Expired logs only; preserve cached package downloads",
                ),
                (
                    "Pip download cache",
                    "Whole cache: remove all downloads and wheels; preserve installed environments",
                ),
                (
                    "Old thumbnails",
                    "Expired previews; applications regenerate these when needed",
                ),
                (
                    "Empty Trash",
                    "Whole Trash: permanently remove all items, including mounted volumes",
                ),
            ]
        };
        for (title, desc) in categories {
            let c = gtk::Switch::new();
            row(&group, title, desc, &c);
            checks.push(c);
        }
        parent.append(&group);
        let limits = adw::PreferencesGroup::builder()
            .description("Minimum ages use complete days. Disabled categories are skipped.")
            .build();
        let fields = vec![
            (
                "Build cache age",
                "Days since Docker last used the cache",
                7.,
                3650.,
                1,
            ),
            (
                "Image age",
                "Days since a dangling image was created",
                7.,
                3650.,
                2,
            ),
            (
                "Build cache allowance",
                "GB · Docker applies this allowance when pruning",
                1.,
                100.,
                1,
            ),
            (
                "npm log age",
                "Days since a diagnostic log was modified",
                1.,
                3650.,
                3,
            ),
            (
                if daily {
                    "npm verification interval"
                } else {
                    "Thumbnail age"
                },
                if daily {
                    "Days between successful cache verifications"
                } else {
                    "Days since a thumbnail was modified"
                },
                1.,
                if daily { 30. } else { 3650. },
                if daily { 4 } else { 5 },
            ),
        ];
        let mut numbers = vec![];
        for (title, desc, min, max, owner) in fields {
            let n = gtk::SpinButton::with_range(min, max, 1.);
            n.set_numeric(true);
            n.set_width_chars(5);
            row(&limits, title, desc, &n);
            let target = n.clone();
            checks[owner].connect_active_notify(move |c| target.set_sensitive(c.is_active()));
            numbers.push(n);
        }
        // Five full ActionRows need not be measured and painted every time a
        // cleanup page opens. Keep the actual editable values one disclosure
        // away, with the current draft visible in a compact summary.
        let retention = gtk::Expander::builder()
            .label("Retention and limits")
            .child(&limits)
            .build();
        retention.set_widget_name("cleanup-retention");
        editor::guard_expanders(&retention);
        parent.append(&retention);
        let summary = crate::label("", "caption");
        summary.set_widget_name("cleanup-retention-summary");
        summary.set_tooltip_text(Some("Current draft limits. Expand Retention and limits to edit. Disabled categories remain skipped."));
        parent.append(&summary);
        let update_summary: Rc<dyn Fn()> = Rc::new({
            let values = numbers.iter().map(|n|n.downgrade()).collect::<Vec<_>>();
            let summary = summary.downgrade();
            move || {
                let Some(summary) = summary.upgrade() else { return };
                let Some(values) = values.iter().map(|n|n.upgrade().map(|n|n.value_as_int())).collect::<Option<Vec<_>>>() else { return };
                summary.set_text(&format!(
                    "Build cache: {} days, keep {} GB · Images: {} days · npm logs: {} days · {}: {} days",
                    values[0], values[2], values[1], values[3],
                    if daily { "npm verification" } else { "Thumbnails" }, values[4]
                ));
            }
        });
        for number in &numbers {
            let update = update_summary.clone();
            number.connect_value_changed(move |_| update());
        }
        update_summary();
        let summary = summary.downgrade();
        retention.connect_expanded_notify(move |expander| {
            if let Some(summary) = summary.upgrade() { summary.set_visible(!expander.is_expanded()); }
        });
        Self { checks, numbers }
    }
    fn manual(&self) -> w::Cleanup {
        w::Cleanup {
            apt: self.checks[0].is_active(),
            docker_build: self.checks[1].is_active(),
            docker_images: self.checks[2].is_active(),
            npm_logs: self.checks[3].is_active(),
            pip: self.checks[4].is_active(),
            thumbnails: self.checks[5].is_active(),
            trash: self.checks[6].is_active(),
            age_days: self.numbers[0].value_as_int() as u32,
            image_days: Some(self.numbers[1].value_as_int() as u32),
            keep_gb: self.numbers[2].value_as_int() as u32,
            npm_log_days: self.numbers[3].value_as_int() as u32,
            thumbnail_days: Some(self.numbers[4].value_as_int() as u32),
            ..Default::default()
        }
    }
    fn daily(&self) -> daily::Policy {
        daily::Policy {
            apt: self.checks[0].is_active(),
            docker_build: self.checks[1].is_active(),
            docker_images: self.checks[2].is_active(),
            npm_logs: self.checks[3].is_active(),
            npm_verify: self.checks[4].is_active(),
            build_days: self.numbers[0].value_as_int() as u32,
            image_days: self.numbers[1].value_as_int() as u32,
            keep_gb: self.numbers[2].value_as_int() as u32,
            npm_log_days: self.numbers[3].value_as_int() as u32,
            npm_verify_days: self.numbers[4].value_as_int() as u32,
            ..Default::default()
        }
    }
    fn set(&self, checks: &[bool], numbers: &[u32]) {
        for (c, v) in self.checks.iter().zip(checks) {
            c.set_active(*v);
        }
        for (n, v) in self.numbers.iter().zip(numbers) {
            n.set_value(*v as f64);
        }
        for (index, owner) in [1, 2, 1, 3, if self.checks.len() == 5 { 4 } else { 5 }]
            .iter()
            .enumerate()
        {
            self.numbers[index].set_sensitive(self.checks[*owner].is_active());
        }
    }
    fn set_manual(&self, p: &w::Cleanup) {
        self.set(
            &[
                p.apt,
                p.docker_build,
                p.docker_images,
                p.npm_logs,
                p.pip,
                p.thumbnails,
                p.trash,
            ],
            &[
                p.age_days,
                p.image_days(),
                p.keep_gb,
                p.npm_log_days,
                p.thumbnail_days(),
            ],
        );
    }
    fn set_daily(&self, p: &daily::Policy) {
        self.set(
            &[
                p.apt,
                p.docker_build,
                p.docker_images,
                p.npm_logs,
                p.npm_verify,
            ],
            &[
                p.build_days,
                p.image_days,
                p.keep_gb,
                p.npm_log_days,
                p.npm_verify_days,
            ],
        );
    }
    fn changed(&self, f: Rc<dyn Fn()>) {
        for c in &self.checks {
            let f = f.clone();
            c.connect_active_notify(move |_| f());
        }
        for c in &self.numbers {
            let f = f.clone();
            c.connect_value_changed(move |_| f());
        }
    }
}
fn navigation(p: &gtk::Box, stack: &gtk::Stack, is_daily: bool) {
    let bar = buttons(p);
    let other = button(
        &bar,
        if is_daily {
            "Manual & cron settings"
        } else {
            "Daily cleanup settings"
        },
    );
    let stack = stack.clone();
    other.connect_clicked(move |_| {
        stack.set_visible_child_name(if is_daily { "cleanup" } else { "daily-cleanup" })
    });
}

pub fn manual(stack: &gtk::Stack, win: &adw::ApplicationWindow) {
    let p=page("Disk cleanup","Manual & cron · Preview eligible items before cleaning. Saving changes the policy used by your enabled user cleanup jobs.");
    navigation(&p, stack, false);
    let controls = Controls::build(&p, false);
    let loaded = w::Cleanup::load();
    let saved = Rc::new(RefCell::new(loaded.as_ref().ok().cloned()));
    controls.set_manual(loaded.as_ref().unwrap_or(&w::Cleanup::default()));
    let footer = editor::Footer::new(
        !w::home().join(".config/tend/cleanup.json").exists(),
        saved.borrow().is_some(),
        None,
    );
    if let Err(e) = &loaded {
        editor::error(
            &p,
            "Cannot load policy. Review defaults before saving a replacement.",
            e,
        );
    }
    let save = footer.save.clone();
    let discard = footer.discard.clone();
    let saves = buttons(&p);
    let defaults = button(&saves, "Restore defaults");
    let revision = Rc::new(Cell::new(0u64));
    let approved = Rc::new(RefCell::new(None::<(u64, Arc<tend::cleanup_plan::Plan>)>));
    let bar = buttons(&p);
    let preview = button(&bar, "Preview cleanup");
    let apply = button(&bar, "Clean selected…");
    apply.add_css_class("destructive-action");
    apply.set_sensitive(false);
    let view = text_panel(&p);
    report_hint(
        &view,
        "Preview is required. No cleanup runs when you edit or save a policy.",
    );
    exports(&p, &view);
    let items = crate::cleanup_items_ui::Items::new();
    p.append(&items.root);
    let invalidate: Rc<dyn Fn()> = Rc::new({
        let (revision, approved, apply, view) = (
            revision.clone(),
            approved.clone(),
            apply.clone(),
            view.clone(),
        );
        let items = items.clone();
        move || {
            items.clear();
            revision.set(revision.get().wrapping_add(1));
            approved.borrow_mut().take();
            apply.set_sensitive(false);
            report_hint(
                &view,
                "Policy changed. Preview again to review eligible items.",
            );
        }
    });
    let changed: Rc<dyn Fn()> = Rc::new({
        let (invalidate, footer, saved, controls) = (
            invalidate.clone(),
            footer.clone(),
            saved.clone(),
            controls.clone(),
        );
        move || {
            invalidate();
            let current = controls.manual();
            footer.sync(
                saved
                    .borrow()
                    .as_ref()
                    .map(|p| normalized(p) != normalized(&current))
                    .unwrap_or(true),
                current.validate().is_ok(),
            );
        }
    });
    crate::settings_ui::invalidator(tend::settings_bundle::Section::ManualCleanup,invalidate.clone());
    saves.insert(&crate::exclusions_ui::button(win, invalidate.clone()), -1);
    controls.changed(changed);
    let extra = crate::cleanup_ui::section(win, invalidate.clone());
    p.append(&extra);
    p.reorder_child_after(&extra, Some(&saves));
    let (c, s) = (controls.clone(), saved.clone());
    discard.connect_clicked(move |_| {
        if let Some(p) = s.borrow().as_ref() {
            c.set_manual(p);
        }
    });
    let c = controls.clone();
    defaults.connect_clicked(move |_| c.set_manual(&w::Cleanup::default()));
    {
        let(f,c,s)=(footer.clone(),controls.clone(),saved.clone());let pending=footer.clone();
        crate::settings_ui::register(tend::settings_bundle::Section::ManualCleanup,move||pending.pending(),move|value|{
            if let Ok(policy)=serde_json::from_value::<w::Cleanup>(value.clone()){*s.borrow_mut()=Some(policy.clone());c.set_manual(&policy);f.success();}
        });
    }
    let (c, s, out, parent) = (controls.clone(), saved.clone(), footer.clone(), win.clone());
    save.connect_clicked(move |_| {
        let policy = c.manual(); let (s, out) = (s.clone(), out.clone());
        let error_out = out.clone();
        let commit = move || {
            out.saving();
            match policy.save() {
                Ok(()) => { *s.borrow_mut() = Some(policy.clone()); out.success(); },
                Err(e) => out.error(&e)
            }
        };
        match rules::Schedule::load() {
            Ok(schedule) if schedule.enabled => confirm(&parent, "Change automatic cleanup policy?", "Your enabled cron jobs will use these selections at their next run. This saves category and retention fields; folder rules have their own Save rule action.", "Save changes", false, commit),
            Ok(_) => commit(), Err(e) => error_out.error(&format!("Cannot confirm schedule state: {e}"))
        }
    });
    let (c, rev, plan, b, v) = (
        controls.clone(),
        revision.clone(),
        approved.clone(),
        apply.clone(),
        view.clone(),
    );
    let list_items = items.clone();
    preview.connect_clicked(move|button|{
        let policy=c.manual();let generation=rev.get();let (rev,plan,b,v,button)=(rev.clone(),plan.clone(),b.clone(),v.clone(),button.clone());
        plan.borrow_mut().take();b.set_sensitive(false);button.set_sensitive(false);v.buffer().set_text("Collecting a read-only preview…");
        let list_items=list_items.clone();list_items.clear();
        job(move||{let custom=rules::Policy::load()?;let plan=tend::cleanup_plan::Plan::build(&policy,&custom,true)?;let compatible=!plan.categories.iter().any(|c|matches!(c.action,tend::cleanup_plan::Action::Apt)&&c.skip.is_none()&&c.selected)||tend::exclusions::integration_supported();Ok((plan,compatible))},move|result|{
            button.set_sensitive(true);
            if generation!=rev.get(){report_hint(&v,"Settings changed while the preview was running. Preview again to review the current policy.");return;}
            match result{Ok((preview,compatible))=>{v.buffer().set_text(&preview.render());if !compatible {let mut end=v.buffer().end_iter();v.buffer().insert(&mut end,"\n\nSystem helper update required for APT cleanup. Preview remains available.");}let preview=Arc::new(preview);list_items.set(preview.clone());b.set_sensitive(compatible);*plan.borrow_mut()=Some((generation,preview));},Err(e)=>v.buffer().set_text(&format!("Preview failed: {e}"))}
        });
    });
    let (parent, rev, plan, v, editor) = (win.clone(), revision, approved, view, p.clone());
    let apply_done = apply.clone();
    apply.connect_clicked(move |_|{
        let Some((generation,selection))=plan.borrow().clone()else{return};
        if generation!=rev.get(){return;}
        let (rev,plan,v,editor,apply_done)=(rev.clone(),plan.clone(),v.clone(),editor.clone(),apply_done.clone());
        confirm(&parent,"Clean the previewed items?","This permanently removes selected cache files and Trash contents. Docker checks eligibility at execution.","Clean selected",true,move||{
            if generation!=rev.get(){report_hint(&v,"Policy changed. Preview again before cleaning.");return;}
            plan.borrow_mut().take();editor.set_sensitive(false);let (v,editor,apply_done)=(v.clone(),editor.clone(),apply_done.clone());let selection=selection.clone();
            job(move||w::audit_result("cleanup",(||{
                let _lock=rules::cleanup_lock()?;
                // Folder rules saved by another instance must also invalidate this preview.
                if serde_json::to_value(rules::Policy::load()?).map_err(|e|e.to_string())?!=serde_json::to_value(&selection.custom).map_err(|e|e.to_string())?{return Err("Folder rules changed. Preview again.".into());}
                let guard=tend::exclusions::Guard::acquire()?;selection.valid_for(&guard.compiled.policy)?;
                let mut report=format!("Manual cleanup · exclusions revision {}\n",selection.exclusions.revision);
                if selection.cleanup.apt&&selection.categories.iter().any(|c|matches!(c.action,tend::cleanup_plan::Action::Apt)&&c.skip.is_none()) {
                    if !tend::exclusions::integration_supported() {return Err("System helper update required.".into())}
                    report.push_str(&crate::privileged("apt-cleanup",Some(serde_json::json!({"revision":selection.exclusions.revision}).to_string()))?);
                }
                match selection.execute_unrecorded() {Ok(text)=>Ok(format!("{report}\n{text}")),Err(error)=>Err(format!("{report}\n{error}"))}
            })()),move|result|{editor.set_sensitive(true);apply_done.set_sensitive(false);v.buffer().set_text(&result.unwrap_or_else(|e|format!("Cleanup did not finish:\n{e}")));});
        });
    });
    let leave = invalidate.clone();
    stack.connect_visible_child_name_notify(move |s| {
        if s.visible_child_name().as_deref() != Some("cleanup") {
            leave();
        }
    });
    stack.add_titled(&editor::shell(&p, &footer), Some("cleanup"), "Disk cleanup");
}
fn normalized(p: &w::Cleanup) -> w::Cleanup {
    let mut p = p.clone();
    p.image_days = Some(p.image_days());
    p.thumbnail_days = Some(p.thumbnail_days());
    p
}

pub fn daily(stack: &gtk::Stack, win: &adw::ApplicationWindow) {
    let p=page("Daily cleanup","System cleanup policy · Independent of manual and user cron cleanup. Changes apply at the next existing maintenance run.");
    navigation(&p, stack, true);
    let controls = Controls::build(&p, true);
    let loaded = daily::Policy::read();
    let saved = Rc::new(RefCell::new(loaded.as_ref().ok().cloned()));
    controls.set_daily(loaded.as_ref().unwrap_or(&daily::Policy::default()));
    let footer = editor::Footer::new(
        !std::path::Path::new(daily::CONFIG).exists(),
        saved.borrow().is_some(),
        if daily::Policy::ready() {
            None
        } else {
            Some("System integration update required to save. Review and preview remain available.")
        },
    );
    if let Err(e) = &loaded {
        editor::error(
            &p,
            "Cannot load daily policy. Review defaults before saving a replacement.",
            e,
        );
    }
    let save = footer.save.clone();
    let discard = footer.discard.clone();
    let bar = buttons(&p);
    let defaults = button(&bar, "Restore defaults");
    let preview = button(&bar, "Preview daily policy");
    let revision = Rc::new(Cell::new(0u64));
    let managed = adw::PreferencesGroup::builder()
        .description("Existing system services retain ownership of these policies.")
        .build();
    for (title, desc) in [
        (
            "Temporary files",
            "/tmp: 10 days · /var/tmp: 30 days · system exclusions apply",
        ),
        (
            "System journal",
            "512 MB and 30 days · existing log rotation remains active",
        ),
    ] {
        let r = adw::ActionRow::builder()
            .title(title)
            .subtitle(desc)
            .build();
        managed.add(&r);
    }
    let managed_info = gtk::Expander::builder()
        .label("Managed by Ubuntu · read only")
        .child(&managed)
        .build();
    p.append(&managed_info);
    let view = text_panel(&p);
    report_hint(
        &view,
        "Preview lists the exact policy actions. It never runs pruning, updates or maintenance.",
    );
    let invalidate: Rc<dyn Fn()> = Rc::new({
        let (revision, view) = (revision.clone(), view.clone());
        move || {
            revision.set(revision.get().wrapping_add(1));
            report_hint(&view,"Preview this draft to review its current actions. No maintenance runs from a preview.");
        }
    });
    crate::settings_ui::invalidator(tend::settings_bundle::Section::DailyCleanup,invalidate.clone());
    bar.insert(&crate::exclusions_ui::button(win, invalidate.clone()), -1);
    let (c, s, out, preview_view) = (
        controls.clone(),
        saved.clone(),
        footer.clone(),
        view.clone(),
    );
    let changed = invalidate.clone();
    controls.changed(Rc::new(move || {
        changed();
        report_hint(
            &preview_view,
            "Preview this draft to review its current actions. No maintenance runs from a preview.",
        );
        let current = c.daily();
        out.sync(
            s.borrow().as_ref() != Some(&current),
            current.validate().is_ok(),
        );
    }));
    let (c, s) = (controls.clone(), saved.clone());
    discard.connect_clicked(move |_| {
        if let Some(p) = s.borrow().as_ref() {
            c.set_daily(p);
        }
    });
    let c = controls.clone();
    defaults.connect_clicked(move |_| c.set_daily(&daily::Policy::default()));
    let (c, v, rev) = (controls.clone(), view.clone(), revision);
    preview.connect_clicked(move |button| {
        let (policy, generation) = (c.daily(), rev.get());
        let (v, rev, button) = (v.clone(), rev.clone(), button.clone());
        button.set_sensitive(false);
        v.buffer().set_text("Checking policy and protected paths…");
        job(
            move || policy.plan(),
            move |result| {
                button.set_sensitive(true);
                if rev.get() == generation {
                    v.buffer()
                        .set_text(&result.unwrap_or_else(|e| format!("Preview unavailable: {e}")));
                }
            },
        );
    });
    {
        let(f,c,s)=(footer.clone(),controls.clone(),saved.clone());let pending=footer.clone();
        crate::settings_ui::register(tend::settings_bundle::Section::DailyCleanup,move||pending.pending(),move|value|{
            if let Ok(policy)=serde_json::from_value::<daily::Policy>(value.clone()){*s.borrow_mut()=Some(policy.clone());c.set_daily(&policy);f.success();}
        });
    }
    let (parent, out, editor) = (win.clone(), footer.clone(), p.clone());
    save.connect_clicked(move |_| {
        let policy = controls.daily();
        let (out, saved, editor) = (out.clone(), saved.clone(), editor.clone());
        let error_out = out.clone();
        let commit = move || {
            if !daily::Policy::ready() {
                out.error("System integration update required. Nothing saved.");
                return;
            }
            if let Err(e) = policy.validate() {
                out.error(&e);
                return;
            }
            out.saving();
            editor.set_sensitive(false);
            let (out, saved, policy, editor) =
                (out.clone(), saved.clone(), policy.clone(), editor.clone());
            job(
                move || {
                    crate::privileged(
                        "daily-policy",
                        Some(serde_json::to_string(&policy).unwrap()),
                    )
                    .map(|_| policy)
                },
                move |r| {
                    editor.set_sensitive(true);
                    match r {
                        Ok(policy) => {
                            *saved.borrow_mut() = Some(policy);
                            out.success();
                        }
                        Err(e) => out.error(&e),
                    }
                },
            );
        };
        match tend::Settings::read() {
            Ok(s) if s.enabled => confirm(
                &parent,
                "Change daily cleanup policy?",
                "Existing daily maintenance will use these selections at its next run.",
                "Save daily policy",
                false,
                commit,
            ),
            Ok(_) => commit(),
            Err(e) => error_out.error(&format!("Cannot confirm maintenance state: {e}")),
        }
    });
    stack.add_titled(
        &editor::shell(&p, &footer),
        Some("daily-cleanup"),
        "Daily cleanup",
    );
}
