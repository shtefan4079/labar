use gtk::prelude::*;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

mod audio;
mod launcher;
mod locales;
mod panel;
mod utils;
mod wayland;
mod wifi;

use audio::AudioMixerPopup;
use launcher::AppLauncher;
use panel::Labar;
use wifi::WiFiPopup;

fn main() {
    gtk::init().expect("Failed to initialize GTK");

    let launcher = Rc::new(AppLauncher::new());
    let wifi = Rc::new(WiFiPopup::new());
    let audio = Rc::new(AudioMixerPopup::new());

    let panel = Labar::new(launcher.clone(), wifi.clone(), audio.clone());

    let signal_flag = Arc::new(AtomicBool::new(false));
    let signal_flag_clone = signal_flag.clone();

    signal_hook::flag::register(signal_hook::consts::SIGUSR1, signal_flag_clone)
        .expect("Failed to register SIGUSR1 handler");

    let launcher_for_signal = launcher.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
        if signal_flag.swap(false, Ordering::Relaxed) {
            launcher_for_signal.toggle();
        }
        glib::ControlFlow::Continue
    });

    let (ui_sender, ui_receiver) = glib::MainContext::channel(glib::Priority::default());

    let wl_client = wayland::WaylandClient::new(ui_sender);

    panel.set_wayland_windows(wl_client.windows.clone());
    panel.set_wayland_seat(wl_client.seat.clone());
    panel.set_wayland_conn(wl_client.conn.clone());
    panel.set_keyboard_layout(wl_client.keyboard_layout.clone());
    panel.set_ui_receiver(ui_receiver);

    panel.show();

    gtk::main();
}
