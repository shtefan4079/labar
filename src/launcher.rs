use crate::utils::fix_icon_name;
use gdk::EventButton;
use gio::AppInfo;
use gtk::prelude::*;
use gtk::{
    Box, Button, FlowBox, IconSize, Image, Label, Menu, MenuItem, Orientation, ScrolledWindow,
    SearchEntry, Separator, Window, WindowType,
};
use gtk_layer_shell::LayerShell;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn toggle_pin_json(desktop_file: &str, pin: bool) {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let path_str = format!("{}/.config/labar/pinned.json", home);
    let path = Path::new(&path_str);

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let mut apps: Vec<PinnedApp> = Vec::new();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(path) {
            apps = serde_json::from_str(&content).unwrap_or_default();
        }
    }

    if pin {
        if !apps.iter().any(|a| a.desktop_file == desktop_file) {
            apps.push(PinnedApp {
                desktop_file: desktop_file.to_string(),
            });
        }
    } else {
        apps.retain(|a| a.desktop_file != desktop_file);
    }

    if let Ok(mut file) = fs::File::create(path) {
        let json = serde_json::to_string_pretty(&apps).unwrap_or_default();
        let _ = file.write_all(json.as_bytes());
    }
}

fn pin_to_taskbar(desktop_file: &str) {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let path = format!("{}/.config/taskbar_pinned.txt", home);

    let mut lines = Vec::new();
    if let Ok(content) = fs::read_to_string(&path) {
        for line in content.lines() {
            if !line.trim().is_empty() {
                lines.push(line.to_string());
            }
        }
    }

    if !lines.iter().any(|l| l == desktop_file) {
        lines.push(desktop_file.to_string());
    }

    if let Ok(mut file) = fs::File::create(&path) {
        for l in lines {
            writeln!(file, "{}", l).ok();
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct PinnedApp {
    desktop_file: String,
}

#[derive(Clone)]
struct AppData {
    name: String,
    icon: String,
    description: String,
    desktop_file: String,
    app_info: gio::AppInfo,
    pinned: bool,
}

pub struct AppLauncher {
    window: Window,
    backdrop: Window,
    main_box: Box,
    search_entry: SearchEntry,
    apps_grid: FlowBox,
    pinned_grid: FlowBox,
    pinned_separator: Separator,
    scroll: ScrolledWindow,
    pinned_label: Label,
    all_apps: Arc<Mutex<Vec<AppData>>>,
    pinned_apps: Arc<Mutex<Vec<String>>>,
    trigger_button: Arc<Mutex<Option<gtk::Widget>>>,
}

impl AppLauncher {
    pub fn new() -> Self {
        let backdrop = Window::new(WindowType::Toplevel);
        backdrop.init_layer_shell();
        <Window as LayerShell>::set_layer(&backdrop, gtk_layer_shell::Layer::Overlay);
        <Window as LayerShell>::set_anchor(&backdrop, gtk_layer_shell::Edge::Top, true);
        <Window as LayerShell>::set_anchor(&backdrop, gtk_layer_shell::Edge::Bottom, true);
        <Window as LayerShell>::set_anchor(&backdrop, gtk_layer_shell::Edge::Left, true);
        <Window as LayerShell>::set_anchor(&backdrop, gtk_layer_shell::Edge::Right, true);
        backdrop.set_decorated(false);
        backdrop.set_widget_name("backdrop-window");

        let backdrop_css = gtk::CssProvider::new();
        backdrop_css
            .load_from_data(b"#backdrop-window { background: transparent; }")
            .ok();
        gtk::StyleContext::add_provider_for_screen(
            &gdk::Screen::default().unwrap(),
            &backdrop_css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        let window = Window::new(WindowType::Toplevel);

        window.init_layer_shell();
        <Window as LayerShell>::set_layer(&window, gtk_layer_shell::Layer::Overlay);
        <Window as LayerShell>::set_keyboard_interactivity(&window, true);
        window.set_title("Launcher");
        window.set_size_request(600, 700);
        window.set_decorated(false);
        window.set_resizable(false);
        window.set_widget_name("main-window");

        let main_box = Box::new(Orientation::Vertical, 0);
        window.add(&main_box);

        let search_entry = SearchEntry::new();
        search_entry.set_placeholder_text(Some(&crate::locales::LOCALE.search_placeholder));
        search_entry.set_widget_name("search-entry");
        search_entry.set_margin_start(20);
        search_entry.set_margin_end(20);
        search_entry.set_margin_top(20);
        main_box.pack_start(&search_entry, false, false, 0);

        let pinned_label = Label::new(Some(&crate::locales::LOCALE.pinned_label));
        pinned_label.set_halign(gtk::Align::Start);
        pinned_label.set_widget_name("section-label");
        pinned_label.set_margin_start(20);
        pinned_label.set_margin_top(15);
        main_box.pack_start(&pinned_label, false, false, 0);

        let pinned_grid = FlowBox::new();
        pinned_grid.set_selection_mode(gtk::SelectionMode::None);
        pinned_grid.set_homogeneous(true);
        pinned_grid.set_valign(gtk::Align::Start);
        pinned_grid.set_max_children_per_line(6);
        pinned_grid.set_row_spacing(10);
        pinned_grid.set_column_spacing(10);
        pinned_grid.set_margin_start(20);
        pinned_grid.set_margin_end(20);
        pinned_grid.set_margin_top(10);
        main_box.pack_start(&pinned_grid, false, false, 0);

        let pinned_separator = Separator::new(Orientation::Horizontal);
        pinned_separator.set_margin_start(20);
        pinned_separator.set_margin_end(20);
        pinned_separator.set_margin_top(10);
        pinned_separator.set_margin_bottom(5);
        pinned_separator.set_no_show_all(true);
        main_box.pack_start(&pinned_separator, false, false, 0);

        let scroll = ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_margin_start(20);
        scroll.set_margin_end(20);
        scroll.set_margin_top(10);
        main_box.pack_start(&scroll, true, true, 0);

        let apps_grid = FlowBox::new();
        apps_grid.set_selection_mode(gtk::SelectionMode::None);
        apps_grid.set_homogeneous(true);
        apps_grid.set_valign(gtk::Align::Start);
        apps_grid.set_max_children_per_line(6);
        apps_grid.set_row_spacing(10);
        apps_grid.set_column_spacing(10);
        scroll.add(&apps_grid);

        let power_box = Box::new(Orientation::Horizontal, 10);
        power_box.set_halign(gtk::Align::End);
        power_box.set_margin_end(20);
        power_box.set_margin_bottom(20);
        power_box.set_margin_top(10);

        let power_actions = [
            ("system-shutdown-symbolic", "systemctl poweroff"),
            ("system-reboot-symbolic", "systemctl reboot"),
            ("system-log-out-symbolic", "labwc --exit"),
        ];

        for (icon, cmd) in power_actions.iter() {
            let btn = Button::from_icon_name(Some(icon), gtk::IconSize::Button);
            btn.set_widget_name("power-button");
            let cmd_str = cmd.to_string();
            btn.connect_clicked(move |_| {
                Command::new("sh").arg("-c").arg(&cmd_str).spawn().ok();
            });
            power_box.pack_start(&btn, false, false, 0);
        }
        main_box.pack_start(&power_box, false, false, 0);

        let provider = gtk::CssProvider::new();
        provider.load_from_data(include_bytes!("launcher.css")).ok();
        gtk::StyleContext::add_provider_for_screen(
            &gdk::Screen::default().unwrap(),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        let instance = AppLauncher {
            window,
            backdrop,
            main_box,
            search_entry,
            apps_grid,
            pinned_grid,
            pinned_separator,
            scroll,
            pinned_label,
            all_apps: Arc::new(Mutex::new(Vec::new())),
            pinned_apps: Arc::new(Mutex::new(Vec::new())),
            trigger_button: Arc::new(Mutex::new(None)),
        };

        let win_for_backdrop = instance.window.clone();
        let backdrop_for_click = instance.backdrop.clone();
        instance.backdrop.connect_button_press_event(move |_, _| {
            win_for_backdrop.hide();
            backdrop_for_click.hide();
            glib::Propagation::Stop
        });

        let apps_grid_clone = instance.apps_grid.clone();
        let pinned_grid_clone = instance.pinned_grid.clone();
        let pinned_label_clone = instance.pinned_label.clone();
        let pinned_separator_clone = instance.pinned_separator.clone();
        let all_apps_clone = instance.all_apps.clone();
        let pinned_apps_clone = instance.pinned_apps.clone();
        let win_clone = instance.window.clone();

        instance.search_entry.connect_changed(move |entry| {
            let text = entry.text().to_string().to_lowercase();
            Self::refresh_ui(
                &apps_grid_clone,
                &pinned_grid_clone,
                &pinned_label_clone,
                &pinned_separator_clone,
                &all_apps_clone,
                &pinned_apps_clone,
                &win_clone,
                Some(&text),
            );
        });

        let backdrop_for_escape = instance.backdrop.clone();
        instance.window.connect_key_press_event(move |w, e| {
            if e.keyval() == gdk::keys::constants::Escape {
                w.hide();
                backdrop_for_escape.hide();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });

        instance.load_applications();
        instance
    }

    fn load_pinned_list() -> Vec<String> {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let mut pinned_set: HashSet<String> = HashSet::new();

        let json_path = format!("{}/.config/labar/pinned.json", home);
        if let Ok(content) = fs::read_to_string(&json_path) {
            if let Ok(apps) = serde_json::from_str::<Vec<PinnedApp>>(&content) {
                for app in apps {
                    pinned_set.insert(app.desktop_file);
                }
            }
        }

        pinned_set.into_iter().collect()
    }

    fn load_applications(&self) {
        let mut apps = Vec::new();
        let pinned_list = Self::load_pinned_list();

        for app in gio::AppInfo::all() {
            if app.should_show() {
                let name = app.name().to_string();
                let exec = app.executable().to_string_lossy().to_string();
                let description = app.description().map(|s| s.to_string()).unwrap_or_default();

                let desktop_file = app.id().map(|s| s.to_string()).unwrap_or_default();

                let icon_str = if let Some(icon) = app.icon() {
                    use gtk::prelude::IconExt;
                    IconExt::to_string(&icon)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "application-x-executable".to_string())
                } else {
                    "application-x-executable".to_string()
                };

                let is_pinned = pinned_list.contains(&desktop_file);

                apps.push(AppData {
                    name,
                    icon: fix_icon_name(&icon_str),
                    description,
                    desktop_file,
                    app_info: app,
                    pinned: is_pinned,
                });
            }
        }

        apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        *self.all_apps.lock().unwrap() = apps;
        *self.pinned_apps.lock().unwrap() = pinned_list;

        Self::refresh_ui(
            &self.apps_grid,
            &self.pinned_grid,
            &self.pinned_label,
            &self.pinned_separator,
            &self.all_apps,
            &self.pinned_apps,
            &self.window,
            None,
        );
    }

    fn refresh_ui(
        apps_grid: &FlowBox,
        pinned_grid: &FlowBox,
        pinned_label: &Label,
        pinned_separator: &Separator,
        all_apps: &Arc<Mutex<Vec<AppData>>>,
        pinned_apps: &Arc<Mutex<Vec<String>>>,
        window: &Window,
        filter: Option<&str>,
    ) {
        apps_grid.foreach(|w| apps_grid.remove(w));
        pinned_grid.foreach(|w| pinned_grid.remove(w));

        let apps = all_apps.lock().unwrap();
        let pinned = pinned_apps.lock().unwrap().clone();

        let is_searching = filter.map(|f| !f.is_empty()).unwrap_or(false);
        let filter_lower = filter.unwrap_or("").to_lowercase();

        let show_pinned = !is_searching && !pinned.is_empty();

        if show_pinned {
            pinned_label.show();
            pinned_grid.show();

            pinned_separator.hide();

            for desktop_file in &pinned {
                if let Some(app) = apps.iter().find(|a| a.desktop_file == *desktop_file) {
                    let btn = Self::create_app_button(
                        app,
                        window,
                        pinned_grid,
                        all_apps,
                        pinned_apps,
                        false,
                    );
                    pinned_grid.add(&btn);
                }
            }
        } else {
            pinned_label.hide();
            pinned_grid.hide();
            pinned_separator.hide();
        }

        if is_searching {
            let mut count = 0;
            for app in apps.iter() {
                let matches = app.name.to_lowercase().contains(&filter_lower)
                    || app.description.to_lowercase().contains(&filter_lower);

                if matches {
                    let btn = Self::create_app_button(
                        app,
                        window,
                        apps_grid,
                        all_apps,
                        pinned_apps,
                        true,
                    );
                    apps_grid.add(&btn);
                    count += 1;
                }
            }

            if count == 0 {
                let label = Label::new(Some(&crate::locales::LOCALE.nothing_found));
                label.set_widget_name("no-results-label");
                label.set_margin_top(40);
                apps_grid.add(&label);
            }

            apps_grid.show_all();

            pinned_separator.show();
        } else {
            apps_grid.hide();
            pinned_separator.hide();
        }

        if show_pinned {
            pinned_grid.show_all();
        }
    }

    fn create_app_button(
        app: &AppData,
        window: &Window,
        grid: &FlowBox,
        all_apps: &Arc<Mutex<Vec<AppData>>>,
        pinned_apps: &Arc<Mutex<Vec<String>>>,
        list_mode: bool,
    ) -> gtk::EventBox {
        let event_box = gtk::EventBox::new();
        event_box.set_widget_name(if list_mode {
            "app-list-item"
        } else {
            "app-button"
        });

        let orientation = if list_mode {
            Orientation::Horizontal
        } else {
            Orientation::Vertical
        };
        let btn_box = Box::new(orientation, if list_mode { 12 } else { 0 });

        if list_mode {
            btn_box.set_size_request(-1, 64);
        } else {
            event_box.set_size_request(100, 100);
        }

        event_box.add(&btn_box);

        let img = if let Some(gicon) = app.app_info.icon() {
            Image::from_gicon(&gicon, IconSize::Dialog)
        } else {
            Image::from_icon_name(Some("application-x-executable"), IconSize::Dialog)
        };
        img.set_pixel_size(48);
        if list_mode {
            img.set_margin_start(12);
        }
        btn_box.pack_start(&img, false, false, if list_mode { 0 } else { 8 });

        if list_mode {
            let text_box = Box::new(Orientation::Vertical, 2);
            text_box.set_valign(gtk::Align::Center);

            let name_label = Label::new(Some(&app.name));
            name_label.set_halign(gtk::Align::Start);
            name_label.set_widget_name("app-list-name");
            text_box.pack_start(&name_label, false, false, 0);

            if !app.description.is_empty() {
                let desc_label = Label::new(Some(&app.description));
                desc_label.set_halign(gtk::Align::Start);
                desc_label.set_max_width_chars(60);
                desc_label.set_ellipsize(pango::EllipsizeMode::End);
                desc_label.set_widget_name("app-list-desc");
                text_box.pack_start(&desc_label, false, false, 0);
            }

            btn_box.pack_start(&text_box, true, true, 0);
        } else {
            let label = Label::new(Some(&app.name));
            label.set_max_width_chars(12);
            label.set_ellipsize(pango::EllipsizeMode::End);
            label.set_justify(gtk::Justification::Center);
            label.set_widget_name("app-label");
            btn_box.pack_start(&label, false, false, 0);
        }

        let app_info = app.app_info.clone();
        let win_weak = window.downgrade();

        event_box.connect_button_press_event({
            let desktop_file = app.desktop_file.clone();
            let is_pinned = app.pinned;
            let grid_clone = grid.clone();
            let all_apps_clone = all_apps.clone();
            let pinned_apps_clone = pinned_apps.clone();
            let event_box_clone = event_box.clone();

            move |_, event| {
                if event.button() == 1 {
                    let _ = app_info.launch(&[], None::<&gio::AppLaunchContext>);
                    if let Some(win) = win_weak.upgrade() {
                        win.hide();
                    }
                    return glib::Propagation::Stop;
                } else if event.button() == 3 {
                    let menu = Menu::new();

                    let pin_label = if is_pinned {
                        &crate::locales::LOCALE.unpin
                    } else {
                        &crate::locales::LOCALE.pin
                    };
                    let pin_item = MenuItem::with_label(pin_label);
                    let df = desktop_file.clone();
                    let pinned_apps_c = pinned_apps_clone.clone();
                    let all_apps_c = all_apps_clone.clone();

                    pin_item.connect_activate(move |_| {
                        if is_pinned {
                            toggle_pin_json(&df, false);
                            pinned_apps_c.lock().unwrap().retain(|p| p != &df);
                        } else {
                            toggle_pin_json(&df, true);
                            pinned_apps_c.lock().unwrap().push(df.clone());
                        }

                        if let Some(app) = all_apps_c
                            .lock()
                            .unwrap()
                            .iter_mut()
                            .find(|a| a.desktop_file == df)
                        {
                            app.pinned = !is_pinned;
                        }
                    });
                    menu.append(&pin_item);

                    let taskbar_item = MenuItem::with_label(&crate::locales::LOCALE.pin_to_taskbar);
                    let df2 = desktop_file.clone();
                    taskbar_item.connect_activate(move |_| {
                        pin_to_taskbar(&df2);
                    });
                    menu.append(&taskbar_item);

                    menu.show_all();
                    menu.popup_at_pointer(Some(event));
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            }
        });

        event_box
    }

    pub fn get_window(&self) -> &Window {
        &self.window
    }

    pub fn set_trigger_button<W: IsA<gtk::Widget>>(&self, button: &W) {
        let mut btn = self.trigger_button.lock().unwrap();
        *btn = Some(button.clone().upcast());
    }

    pub fn toggle(&self) {
        if self.window.is_visible() {
            self.window.hide();
            self.backdrop.hide();
        } else {
            *self.pinned_apps.lock().unwrap() = Self::load_pinned_list();

            let pinned = self.pinned_apps.lock().unwrap().clone();
            for app in self.all_apps.lock().unwrap().iter_mut() {
                app.pinned = pinned.contains(&app.desktop_file);
            }

            self.search_entry.set_text("");
            Self::refresh_ui(
                &self.apps_grid,
                &self.pinned_grid,
                &self.pinned_label,
                &self.pinned_separator,
                &self.all_apps,
                &self.pinned_apps,
                &self.window,
                None,
            );

            self.backdrop.show_all();
            self.window.show_all();
            self.window.present();

            self.position_window();
        }
    }

    fn position_window(&self) {
        <Window as LayerShell>::set_anchor(&self.window, gtk_layer_shell::Edge::Bottom, true);
        <Window as LayerShell>::set_anchor(&self.window, gtk_layer_shell::Edge::Left, false);
        <Window as LayerShell>::set_anchor(&self.window, gtk_layer_shell::Edge::Right, false);
        <Window as LayerShell>::set_anchor(&self.window, gtk_layer_shell::Edge::Top, false);
    }
}
