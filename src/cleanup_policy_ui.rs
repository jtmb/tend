//! Native editors for independent manual/cron and daily maintenance policies.
use crate::workbench::{button, buttons, confirm, exports, text_panel};
use crate::{job, label, page, row, scroll};
use adw::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
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
            .title("Retention and limits")
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
        parent.append(&limits);
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
            "Daily maintenance settings"
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
    let status = label(
        &loaded
            .map(|_| "Saved · manual & cron policy".into())
            .unwrap_or_else(|e| {
                format!("Cannot load policy: {e}. Review defaults before saving a replacement.")
            }),
        "caption",
    );
    p.append(&status);
    let description = p.first_child().unwrap().next_sibling().unwrap();
    p.reorder_child_after(&status, Some(&description));
    if saved.borrow().is_none() {
        status.add_css_class("error");
    }
    let saves = buttons(&p);
    let save = button(&saves, "Save policy");
    save.add_css_class("suggested-action");
    let discard = button(&saves, "Discard changes");
    let defaults = button(&saves, "Restore defaults");
    let revision = Rc::new(Cell::new(0u64));
    let approved = Rc::new(RefCell::new(None::<(u64, w::Cleanup, rules::Policy)>));
    let bar = buttons(&p);
    let preview = button(&bar, "Preview cleanup");
    let apply = button(&bar, "Clean selected…");
    apply.add_css_class("destructive-action");
    apply.set_sensitive(false);
    let view = text_panel(&p);
    view.buffer()
        .set_text("Preview is required. No cleanup runs when you edit or save a policy.");
    exports(&p, &view);
    let invalidate: Rc<dyn Fn()> = Rc::new({
        let (revision, approved, apply, view) = (
            revision.clone(),
            approved.clone(),
            apply.clone(),
            view.clone(),
        );
        move || {
            revision.set(revision.get().wrapping_add(1));
            approved.borrow_mut().take();
            apply.set_sensitive(false);
            view.buffer()
                .set_text("Policy changed. Preview again to review eligible items.");
        }
    });
    let changed: Rc<dyn Fn()> = Rc::new({
        let (invalidate, status, saved, controls) = (
            invalidate.clone(),
            status.clone(),
            saved.clone(),
            controls.clone(),
        );
        move || {
            invalidate();
            let current = controls.manual();
            status.set_text(
                if saved
                    .borrow()
                    .as_ref()
                    .map(|p| normalized(p) == normalized(&current))
                    .unwrap_or(false)
                {
                    "Saved · manual & cron policy"
                } else {
                    "Unsaved changes · scheduled jobs still use the saved policy"
                },
            );
        }
    });
    controls.changed(changed);
    if let Err(error) = rules::Policy::load() {
        let notice=label(&format!("Folder rules need attention: {error}. Preview and cleanup are unavailable until the saved rules are repaired."),"error");
        p.append(&notice);
        p.reorder_child_after(&notice, Some(&status));
    }
    let extra = crate::cleanup_ui::section(win, invalidate.clone());
    p.append(&extra);
    p.reorder_child_after(&extra, Some(&saves));
    let (c, s, out) = (controls.clone(), saved.clone(), status.clone());
    discard.connect_clicked(move |_| {
        if let Some(p) = s.borrow().as_ref() {
            c.set_manual(p);
            out.set_text("Saved · changes discarded");
        } else {
            out.set_text(
                "No valid saved policy. Repair the file or explicitly save reviewed defaults.",
            );
        }
    });
    let c = controls.clone();
    defaults.connect_clicked(move |_| c.set_manual(&w::Cleanup::default()));
    let (c, s, out, parent) = (controls.clone(), saved.clone(), status.clone(), win.clone());
    save.connect_clicked(move |_|{
        let policy=c.manual();let (s,out)=(s.clone(),out.clone());
        let error_out=out.clone();
        let commit=move||match policy.save(){Ok(())=>{*s.borrow_mut()=Some(policy.clone());out.remove_css_class("error");out.set_text("Saved · manual & cron policy");},Err(e)=>out.set_text(&format!("Policy not saved: {e}"))};
        match rules::Schedule::load(){Ok(schedule)if schedule.enabled=>confirm(&parent,"Change automatic cleanup policy?","Your enabled cron jobs will use these selections at their next run. Review ages, custom folder rules and whole-cache selections.","Save policy",false,commit),Ok(_)=>commit(),Err(e)=>error_out.set_text(&format!("Cannot confirm schedule state: {e}"))}
    });
    let (c, rev, plan, b, v) = (
        controls.clone(),
        revision.clone(),
        approved.clone(),
        apply.clone(),
        view.clone(),
    );
    preview.connect_clicked(move|button|{
        let policy=c.manual();let generation=rev.get();let (rev,plan,b,v,button)=(rev.clone(),plan.clone(),b.clone(),v.clone(),button.clone());
        plan.borrow_mut().take();b.set_sensitive(false);button.set_sensitive(false);v.buffer().set_text("Collecting a read-only preview…");
        job(move||{let custom=rules::Policy::load()?;let report=format!("{}\n\n{}",w::cleanup_preview(&policy)?,rules::preview(&custom)?);Ok((report,policy,custom))},move|result|{
            button.set_sensitive(true);
            if generation!=rev.get(){v.buffer().set_text("Settings changed while the preview was running. Preview again to review the current policy.");return;}
            match result{Ok((report,policy,custom))=>{v.buffer().set_text(&report);let compatible=!policy.apt||daily::Policy::ready();if !compatible{let mut end=v.buffer().end_iter();v.buffer().insert(&mut end,"\n\nSystem helper update required for APT cleanup. Preview remains available.");}b.set_sensitive(compatible);*plan.borrow_mut()=Some((generation,policy,custom));},Err(e)=>v.buffer().set_text(&format!("Preview failed: {e}"))}
        });
    });
    let (parent, rev, plan, v, editor) = (win.clone(), revision, approved, view, p.clone());
    let apply_done = apply.clone();
    apply.connect_clicked(move |_|{
        let Some((generation,policy,custom))=plan.borrow().clone()else{return};
        if generation!=rev.get(){return;}
        let (rev,plan,v,editor,apply_done)=(rev.clone(),plan.clone(),v.clone(),editor.clone(),apply_done.clone());
        confirm(&parent,"Clean the previewed items?","This permanently removes selected cache files and Trash contents. Docker checks eligibility at execution.","Clean selected",true,move||{
            if generation!=rev.get(){v.buffer().set_text("Policy changed. Preview again before cleaning.");return;}
            plan.borrow_mut().take();editor.set_sensitive(false);let (v,editor,apply_done)=(v.clone(),editor.clone(),apply_done.clone());let (policy,custom)=(policy.clone(),custom.clone());
            job(move||{
                let _lock=rules::cleanup_lock()?;
                // Folder rules saved by another instance must also invalidate this preview.
                if serde_json::to_value(rules::Policy::load()?).map_err(|e|e.to_string())?!=serde_json::to_value(&custom).map_err(|e|e.to_string())?{return Err("Folder rules changed. Preview again.".into());}
                let base=if policy.apt {if !daily::Policy::ready(){return Err("System helper update required.".into());}crate::privileged("cleanup",Some(serde_json::to_string(&policy).unwrap()))?}else{w::cleanup_user(&policy)?};
                match rules::apply(&custom){Ok(extra)=>Ok(format!("{base}\n{extra}")),Err(e)=>Err(format!("{base}\nFolder-rule cleanup failed:\n{e}"))}
            },move|result|{editor.set_sensitive(true);apply_done.set_sensitive(false);v.buffer().set_text(&result.unwrap_or_else(|e|format!("Cleanup did not finish:\n{e}")));});
        });
    });
    stack.add_titled(&scroll(&p), Some("cleanup"), "Disk cleanup");
}
fn normalized(p: &w::Cleanup) -> w::Cleanup {
    let mut p = p.clone();
    p.image_days = Some(p.image_days());
    p.thumbnail_days = Some(p.thumbnail_days());
    p
}

pub fn daily(stack: &gtk::Stack, win: &adw::ApplicationWindow) {
    let p=page("Daily maintenance","System cleanup policy · Independent of manual and user cron cleanup. Changes apply at the next existing maintenance run.");
    navigation(&p, stack, true);
    if !daily::Policy::ready() {
        p.append(&label("System integration update required · Review and preview settings here. Saving becomes available after the updated administrator helper is installed.","warning"));
    }
    let controls = Controls::build(&p, true);
    let loaded = daily::Policy::read();
    let saved = Rc::new(RefCell::new(loaded.as_ref().ok().cloned()));
    controls.set_daily(loaded.as_ref().unwrap_or(&daily::Policy::default()));
    let status = label(
        &loaded
            .map(|_| "Saved · daily policy".into())
            .unwrap_or_else(|e| format!("Cannot load policy: {e}")),
        "caption",
    );
    p.append(&status);
    let description = p.first_child().unwrap().next_sibling().unwrap();
    p.reorder_child_after(&status, Some(&description));
    if saved.borrow().is_none() {
        status.add_css_class("error");
    }
    let bar = buttons(&p);
    let save = button(&bar, "Save daily policy");
    save.add_css_class("suggested-action");
    save.set_sensitive(daily::Policy::ready());
    let discard = button(&bar, "Discard changes");
    let defaults = button(&bar, "Restore defaults");
    let preview = button(&bar, "Preview daily policy");
    let managed = adw::PreferencesGroup::builder()
        .title("Managed by Ubuntu")
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
    p.append(&managed);
    let view = text_panel(&p);
    view.buffer().set_text(
        "Preview lists the exact policy actions. It never runs pruning, updates or maintenance.",
    );
    let (c, s, out) = (controls.clone(), saved.clone(), status.clone());
    controls.changed(Rc::new(move || {
        out.set_text(if s.borrow().as_ref() == Some(&c.daily()) {
            "Saved · daily policy"
        } else {
            "Unsaved changes · daily maintenance still uses saved settings"
        })
    }));
    let (c, s, out) = (controls.clone(), saved.clone(), status.clone());
    discard.connect_clicked(move |_| {
        if let Some(p) = s.borrow().as_ref() {
            c.set_daily(p);
            out.set_text("Saved · changes discarded");
        }
    });
    let c = controls.clone();
    defaults.connect_clicked(move |_| c.set_daily(&daily::Policy::default()));
    let (c, v) = (controls.clone(), view.clone());
    preview.connect_clicked(move |_| v.buffer().set_text(&c.daily().plan().unwrap_or_else(|e| e)));
    let (parent, out, editor) = (win.clone(), status, p.clone());
    save.connect_clicked(move |b| {
        let policy = controls.daily();
        let (b, out, saved, editor) = (b.clone(), out.clone(), saved.clone(), editor.clone());
        let error_out = out.clone();
        let commit = move || {
            if !daily::Policy::ready() {
                out.set_text("System integration update required. Nothing saved.");
                return;
            }
            if let Err(e) = policy.validate() {
                out.set_text(&e);
                return;
            }
            editor.set_sensitive(false);
            let (b, out, saved, policy, editor) = (
                b.clone(),
                out.clone(),
                saved.clone(),
                policy.clone(),
                editor.clone(),
            );
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
                    b.set_sensitive(true);
                    match r {
                        Ok(policy) => {
                            *saved.borrow_mut() = Some(policy);
                            out.remove_css_class("error");
                            out.set_text("Saved · daily policy");
                        }
                        Err(e) => out.set_text(&format!("Not saved: {e}")),
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
            Err(e) => error_out.set_text(&format!("Cannot confirm maintenance state: {e}")),
        }
    });
    stack.add_titled(&scroll(&p), Some("daily-cleanup"), "Daily maintenance");
}
