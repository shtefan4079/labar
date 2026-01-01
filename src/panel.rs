use chrono::Local;
use gtk::prelude::*;
use gtk::{
    Box, Button, Image, Label, Menu, MenuItem, Orientation, SeparatorMenuItem, Window, WindowType,
};
use gtk_layer_shell::LayerShell;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use crate::audio::AudioMixerPopup;
use crate::launcher::AppLauncher;
use crate::utils::fix_icon_name;
use crate::wayland::WindowHandle;
use crate::wifi::WiFiPopup;

use gdk::EventButton;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::process::Command;
use std::thread;

use crate::wayland::wlr_foreign_toplevel::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1;

fn toggle_pin_app(desktop_file: &str, pin: bool) {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let path_str = format!("{}/.config/taskbar_pinned.txt", home);
    let path = std::path::Path::new(&path_str);

    let mut lines = Vec::new();
    if let Ok(file) = File::open(path) {
        let reader = BufReader::new(file);
        for line in reader.lines() {
            if let Ok(l) = line {
                if !l.trim().is_empty() {
                    lines.push(l);
                }
            }
        }
    }

    if pin {
        if !lines.iter().any(|l| l == desktop_file) {
            lines.push(desktop_file.to_string());
        }
    } else {
        lines.retain(|l| l != desktop_file);
    }

    if let Ok(mut file) = File::create(path) {
        for l in lines {
            writeln!(file, "{}", l).ok();
        }
    }
}

fn load_pinned_apps_list() -> Vec<String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let path = format!("{}/.config/taskbar_pinned.txt", home);

    let mut apps = Vec::new();
    if let Ok(file) = File::open(&path) {
        let reader = BufReader::new(file);
        for line in reader.lines() {
            if let Ok(l) = line {
                if !l.trim().is_empty() {
                    apps.push(l);
                }
            }
        }
    }
    apps
}

pub struct Labar {
    window: Window,
    task_box: Box,
    pinned_box: Box,
    clock_label: Label,
    keyboard_label: Label,
    launcher: Rc<AppLauncher>,
    wifi: Rc<WiFiPopup>,
    audio: Rc<AudioMixerPopup>,
    wayland_windows: Arc<Mutex<Option<Arc<Mutex<Vec<WindowHandle>>>>>>,
    wayland_seat: Arc<Mutex<Option<Arc<Mutex<Option<wayland_client::protocol::wl_seat::WlSeat>>>>>>,
    wayland_conn: Arc<Mutex<Option<wayland_client::Connection>>>,
    keyboard_layout: Arc<Mutex<Option<Arc<Mutex<String>>>>>,
    desktop_shown: Rc<RefCell<bool>>,
    minimized_stack: Rc<RefCell<Vec<(String, ZwlrForeignToplevelHandleV1, bool)>>>,
}

impl Labar {
    pub fn new(launcher: Rc<AppLauncher>, wifi: Rc<WiFiPopup>, audio: Rc<AudioMixerPopup>) -> Self {
        let window = Window::new(WindowType::Toplevel);

        window.init_layer_shell();
        window.set_layer(gtk_layer_shell::Layer::Top);
        window.set_anchor(gtk_layer_shell::Edge::Bottom, true);
        window.set_anchor(gtk_layer_shell::Edge::Left, true);
        window.set_anchor(gtk_layer_shell::Edge::Right, true);
        window.auto_exclusive_zone_enable();

        window.set_size_request(-1, 60);
        window.set_widget_name("panel-window");

        let css = r#"
            #panel-window { background: rgba(15, 15, 15, 0.98); border-top: 1px solid rgba(255,255,255,0.1); }
            button { background: transparent; border: none; margin: 2px; padding: 5px; border-radius: 7px; border-bottom: 4px solid transparent; transition: all 200ms ease; }
            button:hover { background: rgba(255, 255, 255, 0.1); }
            .active-window { border-bottom: 4px solid #00aaff; background: rgba(255,255,255,0.05); }
            .pinned-running { border-bottom: 4px solid #00ff00; }
            label { color: white; font-weight: bold; }
            #keyboard-layout { color: white; font-size: 13px; font-weight: bold; padding: 8px 12px; background: rgba(255, 255, 255, 0.05); border-radius: 6px; margin: 0 8px; }
            #show-desktop { border-radius: 0; border-left: 1px solid rgba(255,255,255,0.1); min-width: 7px; margin: 0; padding: 0; }
            #show-desktop:hover { background: rgba(255, 255, 255, 0.2); }
            menu { background: rgba(32, 34, 37, 0.98); border: 1px solid rgba(255, 255, 255, 0.15); border-radius: 8px; padding: 4px; }
            menuitem { color: white; padding: 8px 12px; border-radius: 4px; }
            menuitem:hover { background: rgba(255, 255, 255, 0.12); }
        "#;
        let provider = gtk::CssProvider::new();
        provider.load_from_data(css.as_bytes()).ok();
        gtk::StyleContext::add_provider_for_screen(
            &gdk::Screen::default().unwrap(),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        let outer_box = Box::new(Orientation::Horizontal, 0);
        window.add(&outer_box);

        let wayland_windows = Arc::new(Mutex::new(None::<Arc<Mutex<Vec<WindowHandle>>>>));
        let wayland_seat = Arc::new(Mutex::new(
            None::<Arc<Mutex<Option<wayland_client::protocol::wl_seat::WlSeat>>>>,
        ));
        let wayland_conn = Arc::new(Mutex::new(None::<wayland_client::Connection>));

        let left_box = Box::new(Orientation::Horizontal, 0);
        outer_box.pack_start(&left_box, false, false, 0);

        let center_box = Box::new(Orientation::Horizontal, 0);

        let start_button = Button::new();
        let icon = Image::from_icon_name(Some("view-grid-symbolic"), gtk::IconSize::Dnd);
        start_button.set_image(Some(&icon));
        let l = launcher.clone();
        let s_btn_clone = start_button.clone();
        start_button.connect_clicked(move |_| {
            l.set_trigger_button(&s_btn_clone);
            l.toggle();
        });
        center_box.pack_start(&start_button, false, false, 0);

        let pinned_box = Box::new(Orientation::Horizontal, 0);
        center_box.pack_start(&pinned_box, false, false, 0);

        let task_box = Box::new(Orientation::Horizontal, 0);
        center_box.pack_start(&task_box, true, true, 0);

        outer_box.set_center_widget(Some(&center_box));

        let right_box = Box::new(Orientation::Horizontal, 0);
        let keyboard_label = Label::new(Some(".."));
        keyboard_label.set_widget_name("keyboard-layout");
        right_box.pack_start(&keyboard_label, false, false, 0);

        let wifi_btn = Button::new();
        let w_icon = Image::from_icon_name(Some("network-wireless-symbolic"), gtk::IconSize::Menu);
        wifi_btn.set_image(Some(&w_icon));
        let w = wifi.clone();
        let w_btn_clone = wifi_btn.clone();
        wifi_btn.connect_clicked(move |_| {
            w.set_trigger_button(&w_btn_clone);
            w.toggle();
        });
        right_box.pack_start(&wifi_btn, false, false, 0);

        let audio_btn = Button::new();
        let a_icon = Image::from_icon_name(Some("audio-volume-high-symbolic"), gtk::IconSize::Menu);
        audio_btn.set_image(Some(&a_icon));
        let a = audio.clone();
        let a_btn_clone = audio_btn.clone();
        audio_btn.connect_clicked(move |_| {
            a.set_trigger_button(&a_btn_clone);
            a.toggle();
        });
        right_box.pack_start(&audio_btn, false, false, 0);

        let clock_label = Label::new(Some("00:00"));
        right_box.pack_start(&clock_label, false, false, 0);

        let show_desktop_btn = Button::new();
        show_desktop_btn.set_widget_name("show-desktop");
        show_desktop_btn.set_label(" | ");
        show_desktop_btn.set_tooltip_text(Some(&crate::locales::LOCALE.show_desktop_tooltip));

        let desktop_shown = Rc::new(RefCell::new(false));
        let minimized_stack: Rc<RefCell<Vec<(String, ZwlrForeignToplevelHandleV1, bool)>>> =
            Rc::new(RefCell::new(Vec::new()));

        let windows_container_clone = wayland_windows.clone();
        let desktop_shown_clone = desktop_shown.clone();
        let minimized_stack_clone = minimized_stack.clone();
        let conn_clone = wayland_conn.clone();

        show_desktop_btn.connect_clicked(move |_| {
            let guard = windows_container_clone.lock().unwrap();
            if let Some(windows_arc) = guard.as_ref() {
                if let Ok(windows) = windows_arc.lock() {
                    let mut shown = desktop_shown_clone.borrow_mut();
                    let mut stack = minimized_stack_clone.borrow_mut();

                    eprintln!(
                        "[Panel] Show Desktop Clicked. Current State: Shown={}, Windows Count={}",
                        *shown,
                        windows.len()
                    );

                    if !*shown {
                        stack.clear();

                        let mut windows_to_hide = Vec::new();

                        for win in windows.iter() {
                            if !win.minimized {
                                eprintln!(
                                    "[Panel] Minimizing: {} (Active: {})",
                                    win.app_id, win.activated
                                );
                                windows_to_hide.push((
                                    win.id.clone(),
                                    win.handle.clone(),
                                    win.activated,
                                ));
                                win.handle.set_minimized();
                            }
                        }

                        *stack = windows_to_hide;

                        *shown = true;
                    } else {
                        let mut restored_count = 0;
                        let mut active_window_handle: Option<ZwlrForeignToplevelHandleV1> = None;

                        for (id, handle, was_active) in stack.iter() {
                            if windows.iter().any(|w| w.id == *id) {
                                eprintln!("[Panel] Restoring: {} (Was Active: {})", id, was_active);
                                handle.unset_minimized();
                                restored_count += 1;

                                if *was_active {
                                    active_window_handle = Some(handle.clone());
                                }
                            } else {
                                eprintln!(
                                    "[Panel] Window {} no longer exists, skipping restore",
                                    id
                                );
                            }
                        }

                        if let Some(handle) = active_window_handle {}

                        stack.clear();
                        *shown = false;
                    }

                    if let Some(conn) = conn_clone.lock().unwrap().as_ref() {
                        let _ = conn.flush();
                    }
                }
            }
        });
        right_box.pack_start(&show_desktop_btn, false, false, 0);

        outer_box.pack_end(&right_box, false, false, 0);

        let instance = Labar {
            window,
            task_box,
            pinned_box,
            clock_label,
            keyboard_label,
            launcher,
            wifi,
            audio,
            wayland_windows: wayland_windows.clone(),
            wayland_seat: wayland_seat.clone(),
            wayland_conn: wayland_conn.clone(),
            keyboard_layout: Arc::new(Mutex::new(None::<Arc<Mutex<String>>>)),
            desktop_shown,
            minimized_stack,
        };

        let label_clone = instance.clock_label.clone();
        glib::timeout_add_seconds_local(1, move || {
            let now = Local::now();
            label_clone.set_text(&now.format("%H:%M\n%d.%m.%Y").to_string());
            glib::ControlFlow::Continue
        });

        instance
    }

    pub fn set_ui_receiver(&self, receiver: glib::Receiver<crate::wayland::UiEvent>) {
        let task_box = self.task_box.clone();
        let pinned_box = self.pinned_box.clone();
        let wayland_windows = self.wayland_windows.clone();
        let wayland_seat = self.wayland_seat.clone();
        let wayland_conn = self.wayland_conn.clone();
        let keyboard_label = self.keyboard_label.clone();

        receiver.attach(None, move |event| {
            match event {
                crate::wayland::UiEvent::KeyboardLayout(layout) => {
                    keyboard_label.set_text(&layout);
                }
                crate::wayland::UiEvent::Refresh => {
                    if let Some(windows_arc) = wayland_windows.lock().unwrap().as_ref() {
                        if let Ok(windows) = windows_arc.lock() {
                            let pinned_list = load_pinned_apps_list();

                            task_box.foreach(|w| task_box.remove(w));

                            pinned_box.foreach(|w| pinned_box.remove(w));

                            let mut running_pinned: std::collections::HashSet<String> =
                                std::collections::HashSet::new();
                            for win in windows.iter() {
                                running_pinned.insert(win.app_id.clone());
                            }

                            for app_id in &pinned_list {
                                let btn = Button::new();
                                let icon_name = fix_icon_name(app_id);
                                let img = Image::from_icon_name(
                                    Some(&icon_name),
                                    gtk::IconSize::LargeToolbar,
                                );
                                img.set_pixel_size(29);
                                btn.set_image(Some(&img));
                                btn.set_tooltip_text(Some(app_id));

                                if running_pinned.contains(app_id) {
                                    btn.style_context().add_class("pinned-running");
                                }

                                let app_id_click = app_id.clone();
                                let windows_for_click = windows_arc.clone();
                                let seat_for_click = wayland_seat.clone();
                                let conn_for_click = wayland_conn.clone();

                                btn.connect_clicked(move |_| {
                                    if let Ok(wins) = windows_for_click.lock() {
                                        if let Some(win) =
                                            wins.iter().find(|w| w.app_id == app_id_click)
                                        {
                                            if win.activated {
                                                win.handle.set_minimized();
                                            } else {
                                                win.handle.unset_minimized();
                                                if let Some(seat_arc) =
                                                    seat_for_click.lock().unwrap().as_ref()
                                                {
                                                    if let Some(seat) =
                                                        seat_arc.lock().unwrap().as_ref()
                                                    {
                                                        win.handle.activate(seat);
                                                    }
                                                }
                                            }
                                            if let Some(conn) =
                                                conn_for_click.lock().unwrap().as_ref()
                                            {
                                                let _ = conn.flush();
                                            }
                                            return;
                                        }
                                    }

                                    let _ = Command::new("gtk-launch").arg(&app_id_click).spawn();
                                });

                                let app_id_menu = app_id.clone();
                                let pinned_box_for_menu = pinned_box.clone();
                                let btn_for_menu = btn.clone();
                                let windows_for_menu = windows_arc.clone();

                                btn.connect_button_press_event(move |_, event: &EventButton| {
                                    if event.button() == 3 {
                                        let menu = Menu::new();

                                        let mut window_handles: Vec<ZwlrForeignToplevelHandleV1> =
                                            Vec::new();
                                        if let Ok(wins) = windows_for_menu.lock() {
                                            for w in wins.iter() {
                                                if w.app_id == app_id_menu {
                                                    window_handles.push(w.handle.clone());
                                                }
                                            }
                                        }

                                        if !window_handles.is_empty() {
                                            let label = if window_handles.len() > 1 {
                                                crate::locales::LOCALE.close_all_windows.replace(
                                                    "{}",
                                                    &window_handles.len().to_string(),
                                                )
                                            } else {
                                                crate::locales::LOCALE.close_window.clone()
                                            };
                                            let close_item = MenuItem::with_label(&label);
                                            let handles = window_handles.clone();
                                            close_item.connect_activate(move |_| {
                                                for h in &handles {
                                                    h.close();
                                                }
                                            });
                                            menu.append(&close_item);
                                            menu.append(&SeparatorMenuItem::new());
                                        }

                                        let unpin_item =
                                            MenuItem::with_label(&crate::locales::LOCALE.unpin);
                                        let app_id_unpin = app_id_menu.clone();
                                        let pb = pinned_box_for_menu.clone();
                                        let b = btn_for_menu.clone();
                                        unpin_item.connect_activate(move |_| {
                                            toggle_pin_app(&app_id_unpin, false);
                                            pb.remove(&b);
                                        });
                                        menu.append(&unpin_item);

                                        menu.show_all();
                                        menu.popup_at_pointer(Some(event));
                                        return glib::Propagation::Stop;
                                    }
                                    glib::Propagation::Proceed
                                });

                                pinned_box.pack_start(&btn, false, false, 0);
                            }

                            for win in windows.iter() {
                                if pinned_list.iter().any(|p| p == &win.app_id) {
                                    continue;
                                }

                                let btn = Button::new();
                                let icon_name = fix_icon_name(&win.app_id);
                                let img = Image::from_icon_name(
                                    Some(&icon_name),
                                    gtk::IconSize::LargeToolbar,
                                );
                                img.set_pixel_size(29);
                                btn.set_image(Some(&img));
                                btn.set_tooltip_text(Some(&win.title));

                                if win.activated {
                                    btn.style_context().add_class("active-window");
                                }

                                let handle_clone = win.handle.clone();
                                let activated = win.activated;
                                let seat_container_inner = wayland_seat.clone();
                                let conn_for_click = wayland_conn.clone();

                                btn.connect_clicked(move |_| {
                                    if activated {
                                        handle_clone.set_minimized();
                                    } else {
                                        handle_clone.unset_minimized();
                                        if let Some(seat_arc) =
                                            seat_container_inner.lock().unwrap().as_ref()
                                        {
                                            if let Some(seat) = seat_arc.lock().unwrap().as_ref() {
                                                handle_clone.activate(seat);
                                            }
                                        }
                                    }
                                    if let Some(conn) = conn_for_click.lock().unwrap().as_ref() {
                                        let _ = conn.flush();
                                    }
                                });

                                let app_id_clone = win.app_id.clone();
                                let handle_for_menu = win.handle.clone();
                                let windows_for_menu = windows_arc.clone();

                                btn.connect_button_press_event(move |_, event: &EventButton| {
                                    if event.button() == 3 {
                                        let menu = Menu::new();

                                        let mut same_app_handles: Vec<ZwlrForeignToplevelHandleV1> =
                                            Vec::new();
                                        if let Ok(wins) = windows_for_menu.lock() {
                                            for w in wins.iter() {
                                                if w.app_id == app_id_clone {
                                                    same_app_handles.push(w.handle.clone());
                                                }
                                            }
                                        }

                                        if same_app_handles.len() > 1 {
                                            let close_all = MenuItem::with_label(
                                                &crate::locales::LOCALE.close_all_windows.replace(
                                                    "{}",
                                                    &same_app_handles.len().to_string(),
                                                ),
                                            );
                                            let handles = same_app_handles.clone();
                                            close_all.connect_activate(move |_| {
                                                for h in &handles {
                                                    h.close();
                                                }
                                            });
                                            menu.append(&close_all);
                                        }

                                        let close_item = MenuItem::with_label(
                                            &crate::locales::LOCALE.close_window,
                                        );
                                        let h = handle_for_menu.clone();
                                        close_item.connect_activate(move |_| {
                                            h.close();
                                        });
                                        menu.append(&close_item);

                                        menu.append(&SeparatorMenuItem::new());

                                        let pin_item =
                                            MenuItem::with_label(&crate::locales::LOCALE.pin);
                                        let app_id_pin = app_id_clone.clone();
                                        pin_item.connect_activate(move |_| {
                                            toggle_pin_app(&app_id_pin, true);
                                        });
                                        menu.append(&pin_item);

                                        menu.show_all();
                                        menu.popup_at_pointer(Some(event));
                                        return glib::Propagation::Stop;
                                    }
                                    glib::Propagation::Proceed
                                });

                                task_box.pack_start(&btn, false, false, 0);
                            }

                            task_box.show_all();
                            pinned_box.show_all();
                        }
                    }
                }
            }
            glib::ControlFlow::Continue
        });
    }

    pub fn set_wayland_windows(&self, windows: Arc<Mutex<Vec<WindowHandle>>>) {
        *self.wayland_windows.lock().unwrap() = Some(windows);
    }

    pub fn set_wayland_seat(
        &self,
        seat: Arc<Mutex<Option<wayland_client::protocol::wl_seat::WlSeat>>>,
    ) {
        *self.wayland_seat.lock().unwrap() = Some(seat);
    }

    pub fn set_wayland_conn(&self, conn: wayland_client::Connection) {
        *self.wayland_conn.lock().unwrap() = Some(conn);
    }

    pub fn set_keyboard_layout(&self, layout: Arc<Mutex<String>>) {
        *self.keyboard_layout.lock().unwrap() = Some(layout);
    }

    pub fn show(&self) {
        self.window.show_all();
    }
}
