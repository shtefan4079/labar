use glib;
use gtk::prelude::*;
use gtk::{
    Box, Button, ComboBoxText, Image, Label, Orientation, Scale, ScrolledWindow, Window, WindowType,
};
use gtk_layer_shell::LayerShell;
use serde_json::Value;
use std::cell::RefCell;
use std::process::Command;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::utils::exec_command;

#[derive(Debug, Clone)]
struct AudioDevice {
    id: i64,
    name: String,
    description: String,
    volume: f64,
    is_muted: bool,
    is_default: bool,
}

#[derive(Debug, Clone)]
struct AudioStream {
    id: i64,
    app_name: String,
    icon_name: String,
    volume: f64,
    is_muted: bool,
}

pub struct AudioMixerPopup {
    window: Window,
    backdrop: Window,
    output_combo: ComboBoxText,
    input_combo: ComboBoxText,
    master_slider: Scale,
    streams_box: Box,
    content_box: Box,
    trigger_button: Arc<Mutex<Option<gtk::Widget>>>,
    updating_ui: Rc<RefCell<bool>>,
    sinks: Rc<RefCell<Vec<AudioDevice>>>,
    sources: Rc<RefCell<Vec<AudioDevice>>>,
}

impl AudioMixerPopup {
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
        window.set_title("Volume");
        window.set_default_size(400, 500);
        window.set_decorated(false);
        window.set_resizable(false);
        window.set_widget_name("audio-window");

        let main_box = Box::new(Orientation::Vertical, 0);
        window.add(&main_box);

        let header_box = Box::new(Orientation::Horizontal, 12);
        header_box.set_widget_name("audio-header");
        header_box.set_margin_start(16);
        header_box.set_margin_end(16);
        header_box.set_margin_top(16);
        header_box.set_margin_bottom(16);

        let title = Label::new(Some(&crate::locales::LOCALE.audio_title));
        title.set_widget_name("header-title");
        header_box.pack_start(&title, false, false, 0);

        let spacer = Box::new(Orientation::Horizontal, 0);
        header_box.pack_start(&spacer, true, true, 0);

        let refresh_btn =
            Button::from_icon_name(Some("view-refresh-symbolic"), gtk::IconSize::Button);
        refresh_btn.set_widget_name("refresh-button");
        header_box.pack_start(&refresh_btn, false, false, 0);

        main_box.pack_start(&header_box, false, false, 0);

        let scroll = ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        main_box.pack_start(&scroll, true, true, 0);

        let content_box = Box::new(Orientation::Vertical, 16);
        content_box.set_margin_start(16);
        content_box.set_margin_end(16);
        content_box.set_margin_bottom(16);
        scroll.add(&content_box);

        let out_label = Label::new(Some(&crate::locales::LOCALE.output_device));
        out_label.set_halign(gtk::Align::Start);
        out_label.set_widget_name("section-label");
        content_box.pack_start(&out_label, false, false, 0);

        let output_combo = ComboBoxText::new();
        content_box.pack_start(&output_combo, false, false, 0);

        let master_slider = Scale::with_range(Orientation::Horizontal, 0.0, 150.0, 1.0);
        master_slider.set_value(100.0);
        content_box.pack_start(&master_slider, false, false, 0);

        let in_label = Label::new(Some(&crate::locales::LOCALE.input_device));
        in_label.set_halign(gtk::Align::Start);
        in_label.set_widget_name("section-label");
        content_box.pack_start(&in_label, false, false, 0);

        let input_combo = ComboBoxText::new();
        content_box.pack_start(&input_combo, false, false, 0);

        content_box.pack_start(
            &gtk::Separator::new(Orientation::Horizontal),
            false,
            false,
            8,
        );

        let app_label = Label::new(Some(&crate::locales::LOCALE.apps_label));
        app_label.set_halign(gtk::Align::Start);
        app_label.set_widget_name("section-label");
        content_box.pack_start(&app_label, false, false, 0);

        let streams_box = Box::new(Orientation::Vertical, 10);
        content_box.pack_start(&streams_box, false, false, 0);

        let provider = gtk::CssProvider::new();
        provider.load_from_data(include_bytes!("audio.css")).ok();
        gtk::StyleContext::add_provider_for_screen(
            &gdk::Screen::default().unwrap(),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        let instance = AudioMixerPopup {
            window,
            backdrop,
            output_combo,
            input_combo,
            master_slider,
            streams_box,
            content_box,
            trigger_button: Arc::new(Mutex::new(None)),
            updating_ui: Rc::new(RefCell::new(false)),
            sinks: Rc::new(RefCell::new(Vec::new())),
            sources: Rc::new(RefCell::new(Vec::new())),
        };

        let win_for_backdrop = instance.window.clone();
        let backdrop_for_click = instance.backdrop.clone();
        instance.backdrop.connect_button_press_event(move |_, _| {
            win_for_backdrop.hide();
            backdrop_for_click.hide();
            glib::Propagation::Stop
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

        let updating_clone = instance.updating_ui.clone();
        let sinks_clone = instance.sinks.clone();
        instance.output_combo.connect_changed(move |combo| {
            if *updating_clone.borrow() {
                return;
            }

            if let Some(active_id) = combo.active_id() {
                let sinks = sinks_clone.borrow();
                if let Some(sink) = sinks.iter().find(|s| s.description == active_id.as_str()) {
                    let cmd = format!("pactl set-default-sink '{}'", sink.name);
                    let _ = Command::new("sh").arg("-c").arg(&cmd).status();
                }
            }
        });

        let updating_clone2 = instance.updating_ui.clone();
        let sources_clone = instance.sources.clone();
        instance.input_combo.connect_changed(move |combo| {
            if *updating_clone2.borrow() {
                return;
            }

            if let Some(active_id) = combo.active_id() {
                let sources = sources_clone.borrow();
                if let Some(source) = sources.iter().find(|s| s.description == active_id.as_str()) {
                    let cmd = format!("pactl set-default-source '{}'", source.name);
                    let _ = Command::new("sh").arg("-c").arg(&cmd).status();
                }
            }
        });

        let updating_clone3 = instance.updating_ui.clone();
        instance.master_slider.connect_value_changed(move |scale| {
            if *updating_clone3.borrow() {
                return;
            }

            let val = scale.value() as i32;
            let cmd = format!("pactl set-sink-volume @DEFAULT_SINK@ {}%", val);
            let _ = Command::new("sh").arg("-c").arg(&cmd).status();
        });

        let streams_box_clone = instance.streams_box.clone();
        let output_combo_clone = instance.output_combo.clone();
        let input_combo_clone = instance.input_combo.clone();
        let master_slider_clone = instance.master_slider.clone();
        let updating_clone4 = instance.updating_ui.clone();
        let sinks_clone2 = instance.sinks.clone();
        let sources_clone2 = instance.sources.clone();

        refresh_btn.connect_clicked(move |_| {
            Self::do_refresh(
                &streams_box_clone,
                &output_combo_clone,
                &input_combo_clone,
                &master_slider_clone,
                &updating_clone4,
                &sinks_clone2,
                &sources_clone2,
            );
        });

        instance
    }

    fn do_refresh(
        streams_box: &Box,
        output_combo: &ComboBoxText,
        input_combo: &ComboBoxText,
        master_slider: &Scale,
        updating_ui: &Rc<RefCell<bool>>,
        sinks_rc: &Rc<RefCell<Vec<AudioDevice>>>,
        sources_rc: &Rc<RefCell<Vec<AudioDevice>>>,
    ) {
        *updating_ui.borrow_mut() = true;

        let (sinks, sources, streams) = Self::fetch_data();

        *sinks_rc.borrow_mut() = sinks.clone();
        *sources_rc.borrow_mut() = sources.clone();

        output_combo.remove_all();
        let mut active_sink_idx = 0;
        for (i, sink) in sinks.iter().enumerate() {
            output_combo.append(Some(&sink.description), &sink.description);
            if sink.is_default {
                active_sink_idx = i;
            }
        }
        if !sinks.is_empty() {
            output_combo.set_active(Some(active_sink_idx as u32));
            master_slider.set_value(sinks[active_sink_idx].volume);
        }

        input_combo.remove_all();
        let mut active_source_idx = 0;
        for (i, source) in sources.iter().enumerate() {
            input_combo.append(Some(&source.description), &source.description);
            if source.is_default {
                active_source_idx = i;
            }
        }
        if !sources.is_empty() {
            input_combo.set_active(Some(active_source_idx as u32));
        }

        streams_box.foreach(|w| streams_box.remove(w));

        if streams.is_empty() {
            let label = Label::new(Some(&crate::locales::LOCALE.no_audio_apps));
            label.set_widget_name("no-apps-label");
            streams_box.pack_start(&label, false, false, 10);
        } else {
            for stream in streams {
                let row = Box::new(Orientation::Horizontal, 10);
                row.set_widget_name("app-row");

                let icon = Image::from_icon_name(Some(&stream.icon_name), gtk::IconSize::Menu);
                icon.set_pixel_size(24);
                row.pack_start(&icon, false, false, 0);

                let name = Label::new(Some(&stream.app_name));
                name.set_widget_name("app-name");
                name.set_max_width_chars(20);
                name.set_ellipsize(pango::EllipsizeMode::End);
                name.set_halign(gtk::Align::Start);
                row.pack_start(&name, false, false, 0);

                let slider = Scale::with_range(Orientation::Horizontal, 0.0, 150.0, 1.0);
                slider.set_hexpand(true);
                slider.set_value(stream.volume);

                let stream_id = stream.id;
                slider.connect_value_changed(move |scale| {
                    let val = scale.value() as i32;
                    let cmd = format!("pactl set-sink-input-volume {} {}%", stream_id, val);
                    let _ = Command::new("sh").arg("-c").arg(&cmd).status();
                });

                row.pack_start(&slider, true, true, 0);
                streams_box.add(&row);
            }
        }

        streams_box.show_all();
        *updating_ui.borrow_mut() = false;
    }

    fn fetch_data() -> (Vec<AudioDevice>, Vec<AudioDevice>, Vec<AudioStream>) {
        let mut sinks = Vec::new();
        let mut sources = Vec::new();
        let mut streams = Vec::new();

        let default_sink = exec_command("pactl get-default-sink").trim().to_string();
        let default_source = exec_command("pactl get-default-source").trim().to_string();

        let sinks_json = exec_command("pactl -f json list sinks");
        if let Ok(json) = serde_json::from_str::<Value>(&sinks_json) {
            if let Some(arr) = json.as_array() {
                for item in arr {
                    let id = item["index"].as_i64().unwrap_or(0);
                    let name = item["name"].as_str().unwrap_or("").to_string();
                    let description = item["description"]
                        .as_str()
                        .unwrap_or("Unknown")
                        .to_string();
                    let is_muted = item["mute"].as_bool().unwrap_or(false);
                    let is_default = name == default_sink;

                    let mut volume = 100.0;
                    if let Some(vol_obj) = item["volume"].as_object() {
                        if let Some((_, first_channel)) = vol_obj.iter().next() {
                            if let Some(val_str) = first_channel["value_percent"].as_str() {
                                volume = val_str.trim_end_matches('%').parse().unwrap_or(100.0);
                            }
                        }
                    }

                    sinks.push(AudioDevice {
                        id,
                        name,
                        description,
                        volume,
                        is_muted,
                        is_default,
                    });
                }
            }
        }

        let sources_json = exec_command("pactl -f json list sources");
        if let Ok(json) = serde_json::from_str::<Value>(&sources_json) {
            if let Some(arr) = json.as_array() {
                for item in arr {
                    let name = item["name"].as_str().unwrap_or("").to_string();

                    if name.contains(".monitor") {
                        continue;
                    }

                    let id = item["index"].as_i64().unwrap_or(0);
                    let description = item["description"]
                        .as_str()
                        .unwrap_or("Unknown")
                        .to_string();
                    let is_muted = item["mute"].as_bool().unwrap_or(false);
                    let is_default = name == default_source;

                    let mut volume = 100.0;
                    if let Some(vol_obj) = item["volume"].as_object() {
                        if let Some((_, first_channel)) = vol_obj.iter().next() {
                            if let Some(val_str) = first_channel["value_percent"].as_str() {
                                volume = val_str.trim_end_matches('%').parse().unwrap_or(100.0);
                            }
                        }
                    }

                    sources.push(AudioDevice {
                        id,
                        name,
                        description,
                        volume,
                        is_muted,
                        is_default,
                    });
                }
            }
        }

        let inputs_json = exec_command("pactl -f json list sink-inputs");
        if let Ok(json) = serde_json::from_str::<Value>(&inputs_json) {
            if let Some(arr) = json.as_array() {
                for item in arr {
                    let id = item["index"].as_i64().unwrap_or(0);
                    let is_muted = item["mute"].as_bool().unwrap_or(false);

                    let mut app_name = "Unknown App".to_string();
                    let mut icon_name = "audio-x-generic".to_string();

                    if let Some(props) = item["properties"].as_object() {
                        if let Some(name) = props.get("application.name").and_then(|v| v.as_str()) {
                            app_name = name.to_string();
                        } else if let Some(name) = props.get("media.name").and_then(|v| v.as_str())
                        {
                            app_name = name.to_string();
                        }

                        if let Some(icon) =
                            props.get("application.icon_name").and_then(|v| v.as_str())
                        {
                            icon_name = icon.to_string();
                        }
                    }

                    let mut volume = 100.0;
                    if let Some(vol_obj) = item["volume"].as_object() {
                        if let Some((_, first_channel)) = vol_obj.iter().next() {
                            if let Some(val_str) = first_channel["value_percent"].as_str() {
                                volume = val_str.trim_end_matches('%').parse().unwrap_or(100.0);
                            }
                        }
                    }

                    streams.push(AudioStream {
                        id,
                        app_name,
                        icon_name,
                        volume,
                        is_muted,
                    });
                }
            }
        }

        (sinks, sources, streams)
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
            Self::do_refresh(
                &self.streams_box,
                &self.output_combo,
                &self.input_combo,
                &self.master_slider,
                &self.updating_ui,
                &self.sinks,
                &self.sources,
            );

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
