use lazy_static::lazy_static;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::sync::Mutex;

#[derive(Deserialize, Debug, Clone)]
pub struct Localization {
    pub search_placeholder: String,
    pub pinned_label: String,
    pub nothing_found: String,
    pub pin: String,
    pub unpin: String,
    pub pin_to_taskbar: String,
    pub show_desktop_tooltip: String,
    pub close_all_windows: String,
    pub close_window: String,
    pub wifi_title: String,
    pub wifi_disabled: String,
    pub wifi_no_networks: String,
    pub wifi_connected: String,
    pub wifi_saved: String,
    pub signal: String,
    pub forget: String,
    pub disconnect: String,
    pub connect: String,
    pub enter_password: String,
    pub cancel: String,
    pub connecting_to: String,
    pub password_placeholder: String,
    pub audio_title: String,
    pub output_device: String,
    pub input_device: String,
    pub apps_label: String,
    pub no_audio_apps: String,
}

impl Default for Localization {
    fn default() -> Self {
        Localization {
            search_placeholder: "Search apps...".to_string(),
            pinned_label: "Pinned".to_string(),
            nothing_found: "Nothing found".to_string(),
            pin: "Pin".to_string(),
            unpin: "Unpin".to_string(),
            pin_to_taskbar: "Pin to taskbar".to_string(),
            show_desktop_tooltip: "Show Desktop".to_string(),
            close_all_windows: "Close all windows ({})".to_string(),
            close_window: "Close window".to_string(),
            wifi_title: "WiFi".to_string(),
            wifi_disabled: "WiFi disabled".to_string(),
            wifi_no_networks: "No networks found".to_string(),
            wifi_connected: "✔ Connected".to_string(),
            wifi_saved: "Saved".to_string(),
            signal: "Signal".to_string(),
            forget: "Forget".to_string(),
            disconnect: "Disconnect".to_string(),
            connect: "Connect".to_string(),
            enter_password: "Enter password".to_string(),
            cancel: "Cancel".to_string(),
            connecting_to: "Connecting to: {}".to_string(),
            password_placeholder: "Password".to_string(),
            audio_title: "Audio".to_string(),
            output_device: "Output Device".to_string(),
            input_device: "Input Device".to_string(),
            apps_label: "Applications".to_string(),
            no_audio_apps: "No apps using audio".to_string(),
        }
    }
}

lazy_static! {
    pub static ref LOCALE: Localization = {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());

        let config_path = format!("{}/.config/labar/current_local.json", home);

        let path_to_load = if std::path::Path::new(&config_path).exists() {
            config_path
        } else {
            "locales/current_local.json".to_string()
        };

        if let Ok(content) = fs::read_to_string(&path_to_load) {
            if let Ok(loc) = serde_json::from_str::<Localization>(&content) {
                return loc;
            } else {
                eprintln!("Failed to parse localization file: {}", path_to_load);
            }
        } else {
            eprintln!("Localization file not found: {}", path_to_load);
        }

        Localization::default()
    };
}
