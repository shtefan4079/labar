use crate::utils::exec_command;
use glib;
use gtk::prelude::*;
use gtk::{
    Box, Button, Dialog, DialogFlags, Entry, Image, Label, Orientation, ResponseType,
    ScrolledWindow, Switch, Window, WindowType,
};
use gtk_layer_shell::LayerShell;
use std::cell::RefCell;
use std::process::Command;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone)]
struct WiFiNetwork {
    ssid: String,
    bssid: String,
    signal: i32,
    secured: bool,
    connected: bool,
    known: bool,
}

pub struct WiFiPopup {
    window: Window,
    backdrop: Window,
    main_box: Box,
    networks_list: Box,
    wifi_switch: Switch,
    trigger_button: Arc<Mutex<Option<gtk::Widget>>>,
    networks: Rc<RefCell<Vec<WiFiNetwork>>>,
    wifi_enabled: Rc<RefCell<bool>>,
}

impl WiFiPopup {
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
        window.set_title("WiFi");
        window.set_default_size(500, 600);
        window.set_decorated(false);
        window.set_resizable(false);
        window.set_widget_name("main-window");

        let main_box = Box::new(Orientation::Vertical, 0);
        window.add(&main_box);

        let header_box = Box::new(Orientation::Horizontal, 12);
        header_box.set_widget_name("header-box");
        header_box.set_margin_start(20);
        header_box.set_margin_end(20);
        header_box.set_margin_top(20);
        header_box.set_margin_bottom(15);

        let title = Label::new(Some(&crate::locales::LOCALE.wifi_title));
        title.set_widget_name("header-title");
        header_box.pack_start(&title, false, false, 0);

        let wifi_switch = Switch::new();
        wifi_switch.set_active(true);
        wifi_switch.set_widget_name("wifi-switch");
        header_box.pack_start(&wifi_switch, false, false, 0);

        let spacer = Box::new(Orientation::Horizontal, 0);
        header_box.pack_start(&spacer, true, true, 0);

        let refresh_btn =
            Button::from_icon_name(Some("view-refresh-symbolic"), gtk::IconSize::Button);
        refresh_btn.set_widget_name("refresh-button");
        header_box.pack_start(&refresh_btn, false, false, 0);

        main_box.pack_start(&header_box, false, false, 0);
        main_box.pack_start(
            &gtk::Separator::new(Orientation::Horizontal),
            false,
            false,
            0,
        );

        let scroll = ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        main_box.pack_start(&scroll, true, true, 0);

        let networks_list = Box::new(Orientation::Vertical, 2);
        networks_list.set_valign(gtk::Align::Start);
        scroll.add(&networks_list);

        let instance = WiFiPopup {
            window,
            backdrop,
            main_box,
            networks_list,
            wifi_switch,
            trigger_button: Arc::new(Mutex::new(None)),
            networks: Rc::new(RefCell::new(Vec::new())),
            wifi_enabled: Rc::new(RefCell::new(true)),
        };

        let win_for_backdrop = instance.window.clone();
        let backdrop_for_click = instance.backdrop.clone();
        instance.backdrop.connect_button_press_event(move |_, _| {
            win_for_backdrop.hide();
            backdrop_for_click.hide();
            glib::Propagation::Stop
        });

        let provider = gtk::CssProvider::new();
        provider.load_from_data(include_bytes!("wifi.css")).ok();
        gtk::StyleContext::add_provider_for_screen(
            &gdk::Screen::default().unwrap(),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        let wifi_enabled_clone = instance.wifi_enabled.clone();
        let networks_list_clone = instance.networks_list.clone();
        let networks_clone = instance.networks.clone();
        instance.wifi_switch.connect_state_set(move |_, state| {
            if state {
                let _ = Command::new("nmcli").args(["radio", "wifi", "on"]).status();
            } else {
                let _ = Command::new("nmcli")
                    .args(["radio", "wifi", "off"])
                    .status();
            }
            *wifi_enabled_clone.borrow_mut() = state;

            let networks_list_c = networks_list_clone.clone();
            let networks_c = networks_clone.clone();
            let wifi_enabled_c = wifi_enabled_clone.clone();
            glib::timeout_add_local(Duration::from_millis(1500), move || {
                Self::do_refresh(&networks_list_c, &networks_c, &wifi_enabled_c);
                glib::ControlFlow::Break
            });

            glib::Propagation::Proceed
        });

        let networks_list_clone2 = instance.networks_list.clone();
        let networks_clone2 = instance.networks.clone();
        let wifi_enabled_clone2 = instance.wifi_enabled.clone();
        refresh_btn.connect_clicked(move |_| {
            Self::do_refresh(
                &networks_list_clone2,
                &networks_clone2,
                &wifi_enabled_clone2,
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

        instance
    }

    fn do_refresh(
        networks_list: &Box,
        networks: &Rc<RefCell<Vec<WiFiNetwork>>>,
        wifi_enabled: &Rc<RefCell<bool>>,
    ) {
        networks_list.foreach(|w| unsafe { w.destroy() });

        let radio_status = exec_command("nmcli radio wifi");
        let enabled = radio_status.contains("enabled");
        *wifi_enabled.borrow_mut() = enabled;

        if !enabled {
            let label = Label::new(Some(&crate::locales::LOCALE.wifi_disabled));
            label.set_widget_name("wifi-disabled-label");
            label.set_margin_top(40);
            networks_list.add(&label);
            networks_list.show_all();
            return;
        }

        let current_ssid =
            exec_command("nmcli -t -f active,ssid dev wifi | grep '^yes:' | cut -d: -f2")
                .trim()
                .to_string();

        let known_networks = exec_command(
            "nmcli -t -f TYPE,NAME connection show | grep '802-11-wireless' | cut -d: -f2",
        );

        let _ = Command::new("nmcli")
            .args(["dev", "wifi", "rescan"])
            .output();

        let output = exec_command("nmcli -t -f SSID,BSSID,SIGNAL,SECURITY dev wifi list");

        let mut nets = Vec::new();
        let mut seen_ssids = std::collections::HashSet::new();

        for line in output.lines() {
            if line.is_empty() {
                continue;
            }

            let mut parts = Vec::new();
            let mut current = String::new();
            let mut escaped = false;

            for c in line.chars() {
                if escaped {
                    current.push(c);
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == ':' {
                    parts.push(current.clone());
                    current.clear();
                } else {
                    current.push(c);
                }
            }
            parts.push(current);

            if parts.len() < 4 {
                continue;
            }

            let ssid = parts[0].clone();
            if ssid.is_empty() || ssid == "--" {
                continue;
            }
            if seen_ssids.contains(&ssid) {
                continue;
            }
            seen_ssids.insert(ssid.clone());

            let bssid = parts[1].clone();
            let signal: i32 = parts[2].parse().unwrap_or(50);
            let security = parts[3].clone();

            let is_connected = !current_ssid.is_empty() && ssid == current_ssid;
            let is_known = known_networks.contains(&ssid);
            let is_secured = !security.is_empty() && security != "--";

            nets.push(WiFiNetwork {
                ssid,
                bssid,
                signal,
                secured: is_secured,
                connected: is_connected,
                known: is_known,
            });
        }

        nets.sort_by(|a, b| {
            if a.connected != b.connected {
                return b.connected.cmp(&a.connected);
            }
            b.signal.cmp(&a.signal)
        });

        *networks.borrow_mut() = nets.clone();

        if nets.is_empty() {
            let label = Label::new(Some(&crate::locales::LOCALE.wifi_no_networks));
            label.set_widget_name("no-networks-label");
            label.set_margin_top(40);
            networks_list.add(&label);
        } else {
            for net in nets {
                let item = Self::create_network_item(&net, networks_list, networks, wifi_enabled);
                networks_list.add(&item);
            }
        }

        networks_list.show_all();
    }

    fn create_network_item(
        net: &WiFiNetwork,
        networks_list: &Box,
        networks: &Rc<RefCell<Vec<WiFiNetwork>>>,
        wifi_enabled: &Rc<RefCell<bool>>,
    ) -> gtk::EventBox {
        let event_box = gtk::EventBox::new();
        event_box.set_widget_name("network-item");

        let row = Box::new(Orientation::Horizontal, 12);
        row.set_size_request(-1, 70);
        event_box.add(&row);

        let icon_name = if net.signal >= 75 {
            "network-wireless-signal-excellent-symbolic"
        } else if net.signal >= 50 {
            "network-wireless-signal-good-symbolic"
        } else if net.signal >= 25 {
            "network-wireless-signal-ok-symbolic"
        } else {
            "network-wireless-signal-weak-symbolic"
        };

        let icon = Image::from_icon_name(Some(icon_name), gtk::IconSize::Dialog);
        icon.set_pixel_size(36);
        icon.set_margin_start(16);
        row.pack_start(&icon, false, false, 0);

        let info_box = Box::new(Orientation::Vertical, 4);
        info_box.set_valign(gtk::Align::Center);
        info_box.set_margin_start(4);

        let ssid_box = Box::new(Orientation::Horizontal, 8);

        let ssid_label = Label::new(Some(&net.ssid));
        ssid_label.set_halign(gtk::Align::Start);
        ssid_label.set_widget_name(if net.connected {
            "network-name-connected"
        } else {
            "network-name"
        });
        ssid_box.pack_start(&ssid_label, false, false, 0);

        if net.secured {
            let lock_icon =
                Image::from_icon_name(Some("dialog-password-symbolic"), gtk::IconSize::Button);
            lock_icon.set_pixel_size(16);
            ssid_box.pack_start(&lock_icon, false, false, 0);
        }

        info_box.pack_start(&ssid_box, false, false, 0);

        let status = if net.connected {
            crate::locales::LOCALE.wifi_connected.clone()
        } else if net.known {
            format!("{} • {}%", crate::locales::LOCALE.wifi_saved, net.signal)
        } else {
            format!("{}: {}%", crate::locales::LOCALE.signal, net.signal)
        };

        let status_label = Label::new(Some(&status));
        status_label.set_halign(gtk::Align::Start);
        status_label.set_widget_name("network-status");
        info_box.pack_start(&status_label, false, false, 0);

        row.pack_start(&info_box, true, true, 0);

        let buttons_box = Box::new(Orientation::Horizontal, 10);
        buttons_box.set_margin_end(16);

        if net.known && !net.connected {
            let forget_btn = Button::with_label(&crate::locales::LOCALE.forget);
            forget_btn.set_widget_name("forget-button");

            let ssid = net.ssid.clone();
            let networks_list_c = networks_list.clone();
            let networks_c = networks.clone();
            let wifi_enabled_c = wifi_enabled.clone();

            forget_btn.connect_clicked(move |_| {
                let cmd = format!("nmcli connection delete \"{}\"", ssid);
                let _ = Command::new("sh").arg("-c").arg(&cmd).status();

                glib::timeout_add_local(Duration::from_millis(500), {
                    let nl = networks_list_c.clone();
                    let n = networks_c.clone();
                    let we = wifi_enabled_c.clone();
                    move || {
                        Self::do_refresh(&nl, &n, &we);
                        glib::ControlFlow::Break
                    }
                });
            });

            buttons_box.pack_start(&forget_btn, false, false, 0);
        }

        let btn_label = if net.connected {
            &crate::locales::LOCALE.disconnect
        } else {
            &crate::locales::LOCALE.connect
        };
        let btn_name = if net.connected {
            "disconnect-button"
        } else {
            "connect-button"
        };

        let connect_btn = Button::with_label(btn_label);
        connect_btn.set_widget_name(btn_name);

        let ssid = net.ssid.clone();
        let connected = net.connected;
        let known = net.known;
        let secured = net.secured;
        let networks_list_c = networks_list.clone();
        let networks_c = networks.clone();
        let wifi_enabled_c = wifi_enabled.clone();

        connect_btn.connect_clicked(move |btn| {
            if connected {
                let cmd = format!("nmcli connection down \"{}\"", ssid);
                let _ = Command::new("sh").arg("-c").arg(&cmd).status();
            } else if known || !secured {
                let cmd = format!("nmcli dev wifi connect \"{}\"", ssid);
                let _ = Command::new("sh").arg("-c").arg(&cmd).status();
            } else {
                Self::show_password_dialog(&ssid, &networks_list_c, &networks_c, &wifi_enabled_c);
                return;
            }

            glib::timeout_add_local(Duration::from_millis(2000), {
                let nl = networks_list_c.clone();
                let n = networks_c.clone();
                let we = wifi_enabled_c.clone();
                move || {
                    Self::do_refresh(&nl, &n, &we);
                    glib::ControlFlow::Break
                }
            });
        });

        buttons_box.pack_start(&connect_btn, false, false, 0);
        row.pack_start(&buttons_box, false, false, 0);

        event_box
    }

    fn show_password_dialog(
        ssid: &str,
        networks_list: &Box,
        networks: &Rc<RefCell<Vec<WiFiNetwork>>>,
        wifi_enabled: &Rc<RefCell<bool>>,
    ) {
        let dialog = Dialog::with_buttons(
            Some(&crate::locales::LOCALE.enter_password),
            None::<&Window>,
            DialogFlags::MODAL,
            &[
                (&crate::locales::LOCALE.cancel, ResponseType::Cancel),
                (&crate::locales::LOCALE.connect, ResponseType::Ok),
            ],
        );
        dialog.set_default_size(400, -1);

        let content = dialog.content_area();
        content.set_margin_start(20);
        content.set_margin_end(20);
        content.set_margin_top(20);
        content.set_margin_bottom(20);
        content.set_spacing(10);

        let label = Label::new(Some(&format!(
            "{}",
            &crate::locales::LOCALE.connecting_to.replace("{}", ssid)
        )));
        content.pack_start(&label, false, false, 0);

        let password_entry = Entry::new();
        password_entry.set_visibility(false);
        password_entry.set_placeholder_text(Some(&crate::locales::LOCALE.password_placeholder));
        content.pack_start(&password_entry, false, false, 0);

        dialog.show_all();

        let ssid_clone = ssid.to_string();
        let networks_list_c = networks_list.clone();
        let networks_c = networks.clone();
        let wifi_enabled_c = wifi_enabled.clone();
        let entry_clone = password_entry.clone();

        dialog.connect_response(move |dlg, response| {
            if response == ResponseType::Ok {
                let password = entry_clone.text();
                let cmd = format!(
                    "nmcli dev wifi connect \"{}\" password \"{}\"",
                    ssid_clone, password
                );
                let _ = Command::new("sh").arg("-c").arg(&cmd).status();

                glib::timeout_add_local(Duration::from_millis(2000), {
                    let nl = networks_list_c.clone();
                    let n = networks_c.clone();
                    let we = wifi_enabled_c.clone();
                    move || {
                        Self::do_refresh(&nl, &n, &we);
                        glib::ControlFlow::Break
                    }
                });
            }
            dlg.close();
        });
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
            Self::do_refresh(&self.networks_list, &self.networks, &self.wifi_enabled);
            self.wifi_switch.set_active(*self.wifi_enabled.borrow());

            self.backdrop.show_all();
            self.window.show_all();
            self.window.present();

            self.position_window();
        }
    }

    fn position_window(&self) {
        <Window as LayerShell>::set_anchor(&self.window, gtk_layer_shell::Edge::Bottom, true);
        <Window as LayerShell>::set_anchor(&self.window, gtk_layer_shell::Edge::Right, true);
        <Window as LayerShell>::set_anchor(&self.window, gtk_layer_shell::Edge::Left, false);
        <Window as LayerShell>::set_anchor(&self.window, gtk_layer_shell::Edge::Top, false);
    }

    pub fn get_window(&self) -> &Window {
        &self.window
    }
}
