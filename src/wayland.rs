use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

use std::sync::{Arc, Mutex};
use std::thread;
pub use wayland_client;
use wayland_client::globals::GlobalListContents;
pub use wayland_client::protocol::{wl_keyboard, wl_output, wl_registry, wl_seat, wl_surface};
use wayland_client::EventQueue;

use memmap2::MmapOptions;
use std::os::unix::io::FromRawFd;
use xkbcommon::xkb;

pub use wayland_protocols_wlr::foreign_toplevel::v1::client as wlr_foreign_toplevel;

use wlr_foreign_toplevel::{
    zwlr_foreign_toplevel_handle_v1::{self, ZwlrForeignToplevelHandleV1},
    zwlr_foreign_toplevel_manager_v1::{self, ZwlrForeignToplevelManagerV1},
};

#[derive(Clone, Debug)]
pub struct WindowHandle {
    pub id: String,
    pub title: String,
    pub app_id: String,
    pub minimized: bool,
    pub activated: bool,
    pub handle: ZwlrForeignToplevelHandleV1,
}

impl WindowHandle {
    pub fn set_minimized(&self) {
        self.handle.set_minimized();
    }

    pub fn unset_minimized(&self) {
        self.handle.unset_minimized();
    }

    pub fn activate(&self, seat: &wl_seat::WlSeat) {
        self.handle.activate(seat);
    }

    pub fn close(&self) {
        self.handle.close();
    }
}

#[derive(Debug, Clone)]
pub enum UiEvent {
    Refresh,
    KeyboardLayout(String),
}

#[derive(Debug)]
pub struct WaylandClient {
    pub conn: Connection,
    pub(crate) event_queue: Arc<Mutex<EventQueue<AppData>>>,
    pub(crate) qh: QueueHandle<AppData>,
    pub windows: Arc<Mutex<Vec<WindowHandle>>>,
    pub seat: Arc<Mutex<Option<wl_seat::WlSeat>>>,
    pub keyboard_layout: Arc<Mutex<String>>,
}

pub struct SendXkbContext(pub xkb::Context);
unsafe impl Send for SendXkbContext {}

pub struct SendXkbState(pub xkb::State);
unsafe impl Send for SendXkbState {}

pub struct AppData {
    pub windows: Arc<Mutex<Vec<WindowHandle>>>,
    pub manager: Option<ZwlrForeignToplevelManagerV1>,
    pub seat: Option<wl_seat::WlSeat>,
    pub keyboard_layout: Arc<Mutex<String>>,
    pub ui_sender: glib::Sender<UiEvent>,
    pub xkb_context: SendXkbContext,
    pub xkb_state: Option<SendXkbState>,
    pub layout_names: Vec<String>,
}

impl WaylandClient {
    pub fn new(ui_sender: glib::Sender<UiEvent>) -> Self {
        eprintln!("[Wayland] Connecting...");
        let conn = Connection::connect_to_env().expect("Failed to connect to Wayland");
        eprintln!("[Wayland] Connected. Initializing Registry...");

        let (globals, event_queue) = wayland_client::globals::registry_queue_init::<AppData>(&conn)
            .expect("Failed registry init");
        let qh = event_queue.handle();

        let windows = Arc::new(Mutex::new(Vec::new()));
        let seat = Arc::new(Mutex::new(None));
        let keyboard_layout = Arc::new(Mutex::new("US".to_string()));

        let seat_clone = seat.clone();

        let xkb_context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);

        let mut app_data = AppData {
            windows: windows.clone(),
            manager: None,
            seat: None,
            keyboard_layout: keyboard_layout.clone(),
            ui_sender,
            xkb_context: SendXkbContext(xkb_context),
            xkb_state: None,
            layout_names: Vec::new(),
        };

        eprintln!("[Wayland] Binding Globals...");
        match globals.bind::<ZwlrForeignToplevelManagerV1, _, _>(&qh, 1..=3, ()) {
            Ok(manager) => {
                eprintln!("[Wayland] Bound Foreign Toplevel Manager successfully.");
                app_data.manager = Some(manager);
            }
            Err(e) => eprintln!("[Wayland] FAILED to bind Foreign Toplevel Manager: {:?}", e),
        }

        match globals.bind::<wl_seat::WlSeat, _, _>(&qh, 1..=1, ()) {
            Ok(s) => {
                eprintln!("[Wayland] Bound WlSeat.");
                *seat_clone.lock().unwrap() = Some(s);
            }
            Err(e) => eprintln!("[Wayland] FAILED to bind WlSeat: {:?}", e),
        }

        let event_queue_arc = Arc::new(Mutex::new(event_queue));
        let event_queue_clone = event_queue_arc.clone();

        thread::spawn(move || {
            let mut event_queue_locked = event_queue_clone.lock().unwrap();
            eprintln!("[Wayland] Event Loop started.");
            loop {
                if let Err(e) = event_queue_locked.blocking_dispatch(&mut app_data) {
                    eprintln!("[Wayland] Dispatch error: {:?}", e);
                    break;
                }
            }
        });

        WaylandClient {
            conn,
            event_queue: event_queue_arc,
            qh,
            windows,
            seat,
            keyboard_layout,
        }
    }
}

fn parse_state(state_bytes: &[u8]) -> (bool, bool) {
    let mut activated = false;
    let mut minimized = false;

    for chunk in state_bytes.chunks(4) {
        if chunk.len() == 4 {
            let val = u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);

            if val == 2 {
                activated = true;
            }
            if val == 1 {
                minimized = true;
            }
        }
    }

    (activated, minimized)
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for AppData {
    fn event(
        state: &mut AppData,
        _proxy: &ZwlrForeignToplevelManagerV1,
        event: zwlr_foreign_toplevel_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<AppData>,
    ) {
        match event {
            zwlr_foreign_toplevel_manager_v1::Event::Toplevel { toplevel } => {
                let id = format!("{:?}", toplevel.id());
                eprintln!("[Wayland] New toplevel found: ID={}", id);
                state.windows.lock().unwrap().push(WindowHandle {
                    id,
                    title: "".into(),
                    app_id: "".into(),
                    minimized: false,
                    activated: false,
                    handle: toplevel,
                });
                state.ui_sender.send(UiEvent::Refresh).ok();
            }
            _ => {}
        }
    }

    fn event_created_child(
        opcode: u16,
        _qh: &QueueHandle<AppData>,
    ) -> std::sync::Arc<dyn wayland_client::backend::ObjectData + 'static> {
        if opcode == 0 {
            // Opcode 0 is the `toplevel` event which creates a ZwlrForeignToplevelHandleV1
            // We return the ObjectData created by the queue handle for this interface and user data
            _qh.make_data::<ZwlrForeignToplevelHandleV1, ()>(())
        } else {
            // This should not happen for this interface
            panic!("Unknown opcode causing child creation")
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for AppData {
    fn event(
        _state: &mut AppData,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<AppData>,
    ) {
        if let wl_seat::Event::Capabilities { capabilities } = event {
            if let wayland_client::WEnum::Value(caps) = capabilities {
                if caps.contains(wl_seat::Capability::Keyboard) {
                    eprintln!("[Wayland] Seat has keyboard, getting keyboard...");
                    seat.get_keyboard(qh, ());
                }
            }
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for AppData {
    fn event(
        state: &mut AppData,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<AppData>,
    ) {
        match event {
            wl_keyboard::Event::Keymap { format, fd, size } => {
                if format == wayland_client::WEnum::Value(wl_keyboard::KeymapFormat::XkbV1) {
                    eprintln!("[Wayland] Keymap received. Size: {}", size);

                    // fd is OwnedFd, convert to File to use with Mmap
                    let file = std::fs::File::from(fd);

                    let keymap_string = unsafe {
                        let mmap = MmapOptions::new()
                            .len(size as usize)
                            .map(&file)
                            .expect("Failed to mmap keymap");
                        String::from_utf8_lossy(&mmap[..size as usize - 1]).into_owned()
                        // remove null terminator?
                    };

                    eprintln!(
                        "[Wayland] Keymap Header: {}",
                        &keymap_string.chars().take(500).collect::<String>()
                    );

                    // access context from wrapper
                    if let Some(keymap) = xkb::Keymap::new_from_string(
                        &state.xkb_context.0,
                        keymap_string.clone(),
                        xkb::KEYMAP_FORMAT_TEXT_V1,
                        xkb::KEYMAP_COMPILE_NO_FLAGS,
                    ) {
                        // Extract layout names
                        state.layout_names.clear();

                        // Try to parse short codes from "xkb_symbols { include "..." }"
                        let mut short_codes = Vec::new();
                        if let Some(start) = keymap_string.find("xkb_symbols") {
                            if let Some(include_start) = keymap_string[start..].find("include") {
                                // keymap_string[start + include_start..] starts with "include"
                                let substr = &keymap_string[start + include_start..];
                                if let Some(quote_start) = substr.find('"') {
                                    if let Some(quote_end) = substr[quote_start + 1..].find('"') {
                                        let include_str =
                                            &substr[quote_start + 1..quote_start + 1 + quote_end];
                                        eprintln!(
                                            "[Wayland] Found include string: {}",
                                            include_str
                                        );

                                        // Parse include string: "pc+us+ru:2+inet(evdev)"
                                        for part in include_str.split('+') {
                                            // Remove variant (e.g. "ru(winkeys)" -> "ru")
                                            let code = part.split('(').next().unwrap_or("").trim();
                                            // Handle group index (e.g. "ru:2" -> "ru")
                                            let code = code.split(':').next().unwrap_or("").trim();

                                            match code {
                                                "pc" | "evdev" | "inet" | "base" | "aliases"
                                                | "empty" | "complete" => continue,
                                                _ => {
                                                    if !code.is_empty() {
                                                        short_codes.push(code.to_uppercase());
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        eprintln!("[Wayland] Parsed short codes: {:?}", short_codes);

                        let num_layouts = keymap.num_layouts();
                        for i in 0..num_layouts {
                            if i < short_codes.len() as u32 {
                                state.layout_names.push(short_codes[i as usize].clone());
                            } else {
                                let name = keymap.layout_get_name(i).to_string();
                                let short_name = &name[0..std::cmp::min(2, name.len())];
                                state.layout_names.push(short_name.to_uppercase());
                            }
                        }

                        state.xkb_state = Some(SendXkbState(xkb::State::new(&keymap)));
                        eprintln!(
                            "[Wayland] XKB State created. Layouts: {:?}",
                            state.layout_names
                        );
                    } else {
                        eprintln!("[Wayland] Failed to compile keymap");
                    }
                }
            }
            wl_keyboard::Event::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
                ..
            } => {
                if let Some(xkb_state_wrapper) = state.xkb_state.as_mut() {
                    let xkb_state = &mut xkb_state_wrapper.0;

                    xkb_state.update_mask(mods_depressed, mods_latched, mods_locked, 0, 0, group);

                    let layout_idx = group as usize;

                    let layout_name = if layout_idx < state.layout_names.len() {
                        state.layout_names[layout_idx].clone()
                    } else {
                        "??".to_string()
                    };

                    *state.keyboard_layout.lock().unwrap() = layout_name.clone();
                    state
                        .ui_sender
                        .send(UiEvent::KeyboardLayout(layout_name))
                        .ok();
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for AppData {
    fn event(
        state: &mut AppData,
        proxy: &ZwlrForeignToplevelHandleV1,
        event: zwlr_foreign_toplevel_handle_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<AppData>,
    ) {
        let mut windows = state.windows.lock().unwrap();
        let id = format!("{:?}", proxy.id());
        if let Some(win) = windows.iter_mut().find(|w| w.id == id) {
            match event {
                zwlr_foreign_toplevel_handle_v1::Event::Title { title } => {
                    win.title = title;
                    state.ui_sender.send(UiEvent::Refresh).ok();
                }
                zwlr_foreign_toplevel_handle_v1::Event::AppId { app_id } => {
                    eprintln!("[Wayland] Update AppId: {} -> {}", id, app_id);
                    win.app_id = app_id;
                    state.ui_sender.send(UiEvent::Refresh).ok();
                }
                zwlr_foreign_toplevel_handle_v1::Event::State { state: state_bytes } => {
                    let (activated, minimized) = parse_state(&state_bytes);

                    win.activated = activated;
                    win.minimized = minimized;
                    state.ui_sender.send(UiEvent::Refresh).ok();
                }
                zwlr_foreign_toplevel_handle_v1::Event::Closed => {
                    eprintln!("[Wayland] Window Closed: {}", id);
                    win.id = "CLOSED".to_string();
                    state.ui_sender.send(UiEvent::Refresh).ok();
                }
                _ => {}
            }
        }

        windows.retain(|w| w.id != "CLOSED");
    }
}

impl Dispatch<wl_registry::WlRegistry, wayland_client::globals::GlobalListContents> for AppData {
    fn event(
        _: &mut AppData,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &wayland_client::globals::GlobalListContents,
        _: &Connection,
        _: &QueueHandle<AppData>,
    ) {
    }
}
