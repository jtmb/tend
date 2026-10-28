//! StatusNotifierItem + DBusMenu, using the existing GIO dependency. No polling.
use adw::prelude::*;
use gtk::{gio, glib};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};
thread_local! {static CONNECTION:RefCell<Option<gio::DBusConnection>>=const{RefCell::new(None)}; static REVISION:Cell<u32>=const{Cell::new(1)};}
pub fn refresh() {
    CONNECTION.with(|cell| {
        if let Some(connection) = cell.borrow().as_ref() {
            let revision = REVISION.with(|r| {
                let n = r.get().wrapping_add(1).max(1);
                r.set(n);
                n
            });
            let _ = connection.emit_signal(
                None,
                "/StatusNotifierItem",
                "org.kde.StatusNotifierItem",
                "NewToolTip",
                None,
            );
            let _ = connection.emit_signal(
                None,
                "/Menu",
                "com.canonical.dbusmenu",
                "LayoutUpdated",
                Some(&(revision, 0i32).to_variant()),
            );
        }
    });
}

const ITEM: &str = r#"<node><interface name="org.kde.StatusNotifierItem">
<method name="Activate"><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
<method name="SecondaryActivate"><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
<method name="ContextMenu"><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
<method name="Scroll"><arg type="i" direction="in"/><arg type="s" direction="in"/></method>
<property name="Category" type="s" access="read"/><property name="Id" type="s" access="read"/>
<property name="Title" type="s" access="read"/><property name="Status" type="s" access="read"/>
<property name="WindowId" type="u" access="read"/><property name="IconName" type="s" access="read"/>
<property name="IconThemePath" type="s" access="read"/><property name="IconPixmap" type="a(iiay)" access="read"/>
<property name="OverlayIconName" type="s" access="read"/><property name="AttentionIconName" type="s" access="read"/>
<property name="AttentionMovieName" type="s" access="read"/><property name="ItemIsMenu" type="b" access="read"/>
<property name="Menu" type="o" access="read"/><property name="ToolTip" type="(sa(iiay)ss)" access="read"/>
<signal name="NewToolTip"/><signal name="NewIcon"/><signal name="NewStatus"><arg type="s"/></signal>
</interface></node>"#;
const MENU: &str = r#"<node><interface name="com.canonical.dbusmenu">
<property name="Version" type="u" access="read"/><property name="TextDirection" type="s" access="read"/>
<property name="Status" type="s" access="read"/><property name="IconThemePath" type="as" access="read"/>
<method name="GetLayout"><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="as" direction="in"/><arg type="u" direction="out"/><arg type="(ia{sv}av)" direction="out"/></method>
<method name="GetGroupProperties"><arg type="ai" direction="in"/><arg type="as" direction="in"/><arg type="a(ia{sv})" direction="out"/></method>
<method name="GetProperty"><arg type="i" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method>
<method name="Event"><arg type="i" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="in"/><arg type="u" direction="in"/></method>
<method name="EventGroup"><arg type="a(isvu)" direction="in"/><arg type="ai" direction="out"/></method>
<method name="AboutToShow"><arg type="i" direction="in"/><arg type="b" direction="out"/></method>
<method name="AboutToShowGroup"><arg type="ai" direction="in"/><arg type="ai" direction="out"/><arg type="ai" direction="out"/></method>
<signal name="LayoutUpdated"><arg type="u"/><arg type="i"/></signal>
<signal name="ItemsPropertiesUpdated"><arg type="a(ia{sv})"/><arg type="a(ias)"/></signal>
</interface></node>"#;
const CHILDREN: [i32; 10] = [1, 4, 5, 6, 9, 10, 7, 8, 2, 3];
fn properties(id: i32) -> HashMap<String, glib::Variant> {
    let mut p = HashMap::new();
    if id == 0 {
        p.insert("children-display".into(), "submenu".to_variant());
    } else if id == 2 {
        p.insert("type".into(), "separator".to_variant());
    } else if let Some(label) = match id {
        1 => Some("Open Tend".into()),
        3 => Some("Quit Tend".into()),
        4 => Some(crate::health_ui::panel_status()),
        5 => Some("Record diagnostics".into()),
        6 => Some("System logs".into()),
        9 => Some("Activity and reports".into()),
        10 => Some("Storage and cleanup".into()),
        7 => Some("Snooze alerts for one hour".into()),
        8 => Some("Resume alerts".into()),
        _ => None,
    } {
        p.insert("label".into(), label.to_variant());
        let enabled = match id {
            4 => false,
            7 => crate::health_ui::enabled() && !crate::health_ui::snoozed(),
            8 => crate::health_ui::enabled() && crate::health_ui::snoozed(),
            _ => true,
        };
        p.insert("enabled".into(), enabled.to_variant());
        p.insert("visible".into(), true.to_variant());
    }
    p
}
fn layout(id: i32, depth: i32) -> glib::Variant {
    let children: Vec<glib::Variant> = if id == 0 && depth != 0 {
        CHILDREN.into_iter().map(|i| layout(i, 0)).collect()
    } else {
        vec![]
    };
    (id, properties(id), children).to_variant()
}
fn activate(app: &adw::Application, id: i32, event: &str) {
    if event != "clicked" {
        return;
    }
    match id {
        1 => app.activate_action("show", None),
        3 => app.activate_action("quit", None),
        5 => app.activate_action("open-diagnostics", None),
        6 => app.activate_action("open-system-logs", None),
        9 => app.activate_action("open-activity", None),
        10 => app.activate_action("open-storage", None),
        7 => crate::health_ui::snooze(),
        8 => crate::health_ui::resume(),
        _ => {}
    }
}
pub fn install(app: &adw::Application, ready: Rc<Cell<bool>>) {
    // Preview instances must never register an icon on the real desktop.
    let protocol_test = cfg!(feature = "docs-demo")
        && (std::env::var_os("TEND_PANEL_QA").is_some()
            || std::env::var_os("TEND_HEALTH_QA").is_some() || std::env::var_os("TEND_APP_BUDGET_QA").is_some());
    if std::env::var_os("TEND_SCREENSHOT").is_some() && !protocol_test {
        return;
    }
    #[cfg(feature = "docs-demo")]
    if std::env::var_os("TEND_DEMO_DIR").is_some() && !protocol_test {
        return;
    }
    let weak = app.downgrade();
    gio::bus_get(
        gio::BusType::Session,
        None::<&gio::Cancellable>,
        move |result| {
            let Ok(connection) = result else {
                return;
            };
            CONNECTION.with(|cell| *cell.borrow_mut() = Some(connection.clone()));
            let item = gio::DBusNodeInfo::for_xml(ITEM).unwrap();
            let menu = gio::DBusNodeInfo::for_xml(MENU).unwrap();
            let action_app = weak.clone();
            let item_id = connection
                .register_object("/StatusNotifierItem", &item.interfaces()[0])
                .method_call(move |_, _, _, _, method, _, invocation| {
                    if matches!(method, "Activate" | "SecondaryActivate" | "ContextMenu") {
                        if let Some(app) = action_app.upgrade() {
                            app.activate_action("show", None);
                        }
                    }
                    invocation.return_value(Some(&().to_variant()));
                })
                .property(|_, _, _, _, name| match name {
                    "Category" => "SystemServices".to_variant(),
                    "Id" => "tend".to_variant(),
                    "Title" => "Tend".to_variant(),
                    "Status" => "Active".to_variant(),
                    "WindowId" => 0u32.to_variant(),
                    "IconName" => "io.github.jtmb.Tend-symbolic".to_variant(),
                    "IconThemePath" => glib::user_data_dir()
                        .join("icons/hicolor/scalable/apps")
                        .to_string_lossy()
                        .to_variant(),
                    "IconPixmap" => Vec::<(i32, i32, Vec<u8>)>::new().to_variant(),
                    "ItemIsMenu" => false.to_variant(),
                    "Menu" => glib::variant::ObjectPath::try_from("/Menu")
                        .unwrap()
                        .to_variant(),
                    "ToolTip" => (
                        "io.github.jtmb.Tend-symbolic",
                        Vec::<(i32, i32, Vec<u8>)>::new(),
                        "Tend",
                        crate::health_ui::tooltip(),
                    )
                        .to_variant(),
                    _ => "".to_variant(),
                })
                .build();
            if item_id.is_err() {
                return;
            }
            let action_app = weak.clone();
            let menu_id = connection
                .register_object("/Menu", &menu.interfaces()[0])
                .property(|_, _, _, _, name| match name {
                    "Version" => 3u32.to_variant(),
                    "TextDirection" => "ltr".to_variant(),
                    "IconThemePath" => Vec::<String>::new().to_variant(),
                    _ => "normal".to_variant(),
                })
                .method_call(move |_, _, _, _, method, params, invocation| {
                    let response = match method {
                        "GetLayout" => {
                            let (id, depth, _) = params.get::<(i32, i32, Vec<String>)>().unwrap();
                            glib::Variant::tuple_from_iter([
                                REVISION.with(|r| r.get()).to_variant(),
                                layout(id, depth),
                            ])
                        }
                        "GetGroupProperties" => {
                            let (ids, _) = params.get::<(Vec<i32>, Vec<String>)>().unwrap();
                            (ids.into_iter()
                                .map(|id| (id, properties(id)))
                                .collect::<Vec<_>>(),)
                                .to_variant()
                        }
                        "GetProperty" => {
                            let (id, name) = params.get::<(i32, String)>().unwrap();
                            if let Some(v) = properties(id).remove(&name) {
                                (v,).to_variant()
                            } else {
                                invocation.return_dbus_error(
                                    "com.canonical.dbusmenu.Error.UnknownProperty",
                                    "Unknown menu property",
                                );
                                return;
                            }
                        }
                        "Event" => {
                            let (id, event, _, _) =
                                params.get::<(i32, String, glib::Variant, u32)>().unwrap();
                            if let Some(app) = action_app.upgrade() {
                                activate(&app, id, &event);
                            }
                            ().to_variant()
                        }
                        "EventGroup" => {
                            let (events,) = params
                                .get::<(Vec<(i32, String, glib::Variant, u32)>,)>()
                                .unwrap();
                            if let Some(app) = action_app.upgrade() {
                                for (id, event, _, _) in events {
                                    activate(&app, id, &event);
                                }
                            }
                            (Vec::<i32>::new(),).to_variant()
                        }
                        "AboutToShow" => (false,).to_variant(),
                        "AboutToShowGroup" => (Vec::<i32>::new(), Vec::<i32>::new()).to_variant(),
                        _ => {
                            invocation.return_dbus_error(
                                "org.freedesktop.DBus.Error.UnknownMethod",
                                "Unknown menu method",
                            );
                            return;
                        }
                    };
                    invocation.return_value(Some(&response));
                })
                .build();
            if menu_id.is_err() {
                let _ = connection.unregister_object(item_id.unwrap());
                return;
            }
            let status = ready.clone();
            let gone = ready.clone();
            let restore = weak.clone();
            gio::bus_watch_name_on_connection(
                &connection,
                "org.kde.StatusNotifierWatcher",
                gio::BusNameWatcherFlags::NONE,
                move |conn, _, _| {
                    let status = status.clone();
                    conn.call(
                        Some("org.kde.StatusNotifierWatcher"),
                        "/StatusNotifierWatcher",
                        "org.kde.StatusNotifierWatcher",
                        "RegisterStatusNotifierItem",
                        Some(&("/StatusNotifierItem",).to_variant()),
                        None,
                        gio::DBusCallFlags::NONE,
                        3000,
                        None::<&gio::Cancellable>,
                        move |r| {
                            status.set(r.is_ok());
                        },
                    );
                },
                move |_, _| {
                    let was_ready = gone.replace(false);
                    if was_ready {
                        if let Some(app) = restore.upgrade() {
                            if app.windows().first().is_some_and(|w| !w.is_visible()) {
                                app.activate_action("show", None);
                            }
                        }
                    }
                },
            );
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn panel_protocol_and_menu_types() {
        assert!(gio::DBusNodeInfo::for_xml(ITEM).is_ok());
        assert!(gio::DBusNodeInfo::for_xml(MENU).is_ok());
        let root = layout(0, -1);
        assert_eq!(root.type_().as_str(), "(ia{sv}av)");
        let (id, _, children) = root
            .get::<(i32, HashMap<String, glib::Variant>, Vec<glib::Variant>)>()
            .unwrap();
        assert_eq!(id, 0);
        assert_eq!(children.len(), 10);
        assert!(!properties(4)["enabled"].get::<bool>().unwrap());
        assert!(!properties(7)["enabled"].get::<bool>().unwrap());
        assert_eq!(properties(1)["label"].get::<String>().unwrap(), "Open Tend");
        assert_eq!(properties(3)["label"].get::<String>().unwrap(), "Quit Tend");
    }
}
