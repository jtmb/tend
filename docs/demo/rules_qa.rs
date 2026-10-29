//! Real rule-editor race checks in a retained, private app home. No cleanup.
use adw::prelude::*;
use gtk::{glib, Widget};
use serde_json::json;
use std::{fs, path::Path, time::Duration};
use tend::{cleanup_rules as rules, workstation as w};

fn descendants(root: &impl IsA<Widget>) -> Vec<Widget> {
    let mut result = vec![root.as_ref().clone()];
    let mut index = 0;
    while index < result.len() {
        let mut child = result[index].first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            result.push(widget);
        }
        index += 1;
    }
    result
}
fn button(root: &impl IsA<Widget>, title: &str) -> gtk::Button {
    descendants(root)
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .find(|b| b.label().as_deref() == Some(title))
        .unwrap_or_else(|| panic!("Missing rule action {title}"))
}
fn modal(title: &str) -> adw::MessageDialog {
    gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|w| w.downcast::<adw::MessageDialog>().ok())
        .find(|d| d.is_visible() && d.title().as_deref() == Some(title))
        .unwrap()
}
fn entry(dialog: &adw::MessageDialog, hint: &str) -> gtk::Entry {
    descendants(dialog)
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Entry>().ok())
        .find(|e| e.placeholder_text().as_deref() == Some(hint))
        .unwrap()
}
fn external_rule(name: &str, folder: &Path) -> rules::Rule {
    rules::Rule {
        name: name.into(),
        folder: folder.into(),
        pattern: "*.tmp".into(),
        days: 30,
        recursive: false,
        enabled: true,
    }
}
fn settle() {
    let main = glib::MainLoop::new(None, false);
    let exit = main.clone();
    glib::timeout_add_local_once(Duration::from_millis(120), move || exit.quit());
    main.run();
}
fn close(dialog: &adw::MessageDialog) {
    if dialog.is_visible() {
        button(dialog, "Cancel").emit_clicked();
        settle();
    }
}

pub fn run(app: &adw::Application, stack: &gtk::Stack, directory: &str) {
    assert!(std::env::var_os("TEND_DEMO_DIR").is_some());
    let directory = Path::new(directory).to_owned();
    fs::create_dir_all(&directory).unwrap();
    let home = w::home();
    assert_eq!(directory.parent(), home.parent());
    let (app, stack) = (app.clone(), stack.clone());
    glib::timeout_add_local_once(Duration::from_millis(600), move || {
        stack.set_visible_child_name("cleanup");
        let root = stack.child_by_name("cleanup").unwrap();
        for e in descendants(&root)
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Expander>().ok())
        {
            e.set_expanded(true);
        }
        let path = home.join(".config/tend/rules.json");
        let original = directory.join("original-rules.json");
        fs::rename(&path, &original).unwrap();
        fs::write(&path, fs::read(&original).unwrap()).unwrap();
        let cache = home.join("race-fixture-cache");
        fs::create_dir_all(&cache).unwrap();
        button(&root, "Add rule…").emit_clicked();
        let add = modal("Add cleanup rule");
        entry(&add, "Rule name").set_text("Unsaved editor rule");
        entry(&add, "Absolute folder path").set_text(&cache.to_string_lossy());
        entry(&add, "Filename patterns, e.g. *.log,*.tmp").set_text("*.tmp");
        assert!(add.is_response_enabled("save"));
        let mut concurrent = rules::Policy::load().unwrap();
        concurrent
            .rules
            .push(external_rule("Concurrent added rule", &cache));
        concurrent.save().unwrap();
        let before = fs::read(&path).unwrap();
        button(&add, "Save rule").emit_clicked();
        settle();
        let add_preserved = fs::read(&path).unwrap() == before;
        let draft_visible = add.is_visible();
        close(&add);
        let reload = descendants(&root)
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Button>().ok())
            .find(|b| b.label().as_deref() == Some("Reload rules"));
        let (mut edit_preserved, mut remove_preserved, mut confirmation_preserved, mut fresh_save) =
            (false, false, false, false);
        if let Some(reload) = &reload {
            reload.emit_clicked();
            settle();
            let list = descendants(&root)
                .into_iter()
                .filter_map(|w| w.downcast::<gtk::ListBox>().ok())
                .find(|l| l.selection_mode() == gtk::SelectionMode::Single)
                .unwrap();
            list.select_row(list.row_at_index(0).as_ref());
            button(&root, "Edit selected…").emit_clicked();
            let edit = modal("Edit cleanup rule");
            entry(&edit, "Rule name").set_text("Unsaved renamed rule");
            let mut current = rules::Policy::load().unwrap();
            current
                .rules
                .insert(0, external_rule("Concurrent front rule", &cache));
            current.save().unwrap();
            let before = fs::read(&path).unwrap();
            button(&edit, "Save rule").emit_clicked();
            settle();
            edit_preserved = fs::read(&path).unwrap() == before;
            close(&edit);
            reload.emit_clicked();
            settle();
            list.select_row(list.row_at_index(0).as_ref());
            let mut current = rules::Policy::load().unwrap();
            current
                .rules
                .push(external_rule("Concurrent before remove", &cache));
            current.save().unwrap();
            let before = fs::read(&path).unwrap();
            button(&root, "Remove selected").emit_clicked();
            settle();
            remove_preserved = fs::read(&path).unwrap() == before;
            reload.emit_clicked();
            settle();
            // Fixture settings only: no crontab is saved or command executed.
            let schedule = home.join(".config/tend/cleanup-schedule.json");
            fs::write(
                &schedule,
                r#"{"enabled":true,"expression":"0 8 * * *","catch_up":true}"#,
            )
            .unwrap();
            button(&root, "Add rule…").emit_clicked();
            let add = modal("Add cleanup rule");
            entry(&add, "Rule name").set_text("Unsaved confirmation rule");
            entry(&add, "Absolute folder path").set_text(&cache.to_string_lossy());
            button(&add, "Save rule").emit_clicked();
            settle();
            let confirm = modal("Change an automatic cleanup rule?");
            let mut current = rules::Policy::load().unwrap();
            current
                .rules
                .push(external_rule("Concurrent during confirmation", &cache));
            current.save().unwrap();
            let before = fs::read(&path).unwrap();
            button(&confirm, "Save rule").emit_clicked();
            settle();
            confirmation_preserved = fs::read(&path).unwrap() == before;
            close(&add);
            fs::rename(&schedule, directory.join("fixture-automatic-schedule.json")).unwrap();
            reload.emit_clicked();
            settle();
            button(&root, "Add rule…").emit_clicked();
            let add = modal("Add cleanup rule");
            entry(&add, "Rule name").set_text("Fresh rule after reload");
            entry(&add, "Absolute folder path").set_text(&cache.to_string_lossy());
            let expected = rules::Policy::load().unwrap().rules.len() + 1;
            button(&add, "Save rule").emit_clicked();
            settle();
            close(&add);
            let current = rules::Policy::load().unwrap();
            fresh_save = current.rules.len() == expected
                && current
                    .rules
                    .iter()
                    .any(|r| r.name == "Fresh rule after reload");
        }
        let report = json!({"stale_add_preserves_concurrent_file":add_preserved,"stale_edit_preserves_concurrent_file":edit_preserved,"stale_remove_preserves_concurrent_file":remove_preserved,"confirmation_race_preserves_concurrent_file":confirmation_preserved,"reload_available":reload.is_some(),"fresh_save_after_reload":fresh_save,"stale_add_draft_visible":draft_visible,"passed":add_preserved&&edit_preserved&&remove_preserved&&confirmation_preserved&&fresh_save&&draft_visible,"fixture_only":true,"cleanup_executed":false});
        fs::rename(&path, directory.join("rules-after-qa.json")).unwrap();
        fs::rename(&original, &path).unwrap();
        fs::write(
            directory.join("rule-editor-evidence.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        app.quit();
    });
}
