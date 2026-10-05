use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use smithay::backend::input::{
    Event, GestureBeginEvent, GestureSwipeUpdateEvent, InputEvent,
    KeyboardKeyEvent, PointerButtonEvent, PointerMotionEvent,
};
use smithay::backend::libinput::LibinputInputBackend;
use smithay::reexports::calloop::EventLoop;
use smithay::reexports::input::{Libinput, LibinputInterface};
use smithay::reexports::wayland_server::Display;
use smithay::utils::Point;
use wayland_server::ListeningSocket;

use crate::config::Config;
use crate::state::{ClientState, MaterialWmState};

struct LibinputSessionInterface;

impl LibinputInterface for LibinputSessionInterface {
    fn open_restricted(&mut self, path: &std::path::Path, flags: i32) -> Result<std::os::unix::io::OwnedFd, i32> {
        use std::os::unix::fs::OpenOptionsExt;
        let mut options = std::fs::OpenOptions::new();
        options.custom_flags(flags);
        options.read(true);
        options.write(true);

        options
            .open(path)
            .map(std::os::unix::io::OwnedFd::from)
            .map_err(|e| e.raw_os_error().unwrap_or(13)) // 13 = EACCES
    }

    fn close_restricted(&mut self, fd: std::os::unix::io::OwnedFd) {
        drop(fd);
    }
}

pub fn run_drm(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("Initializing KMS/DRM, Udev, and Libinput bare-metal backend");

    let mut event_loop: EventLoop<MaterialWmState> = EventLoop::try_new()?;
    let mut display: Display<MaterialWmState> = Display::new()?;
    let dh = display.handle();

    let mut state = MaterialWmState::new(dh.clone(), config);

    // Initialize Libinput
    let mut libinput_ctx = Libinput::new_with_udev(LibinputSessionInterface);
    if let Err(e) = libinput_ctx.udev_assign_seat("seat0") {
        tracing::warn!("Failed to assign seat0 to libinput: {:?}. Running with default seat.", e);
    }

    let input_backend = LibinputInputBackend::new(libinput_ctx);

    // Register Libinput into Calloop event loop
    event_loop
        .handle()
        .insert_source(input_backend, move |event, _, state| match event {
            InputEvent::Keyboard { event } => {
                let keycode = event.key_code();
                let key_state = event.state();
                let time = event.time_msec();
                let serial = smithay::utils::SERIAL_COUNTER.next_serial();
                state.on_keyboard_key(keycode.into(), key_state, serial, time);
            }
            InputEvent::PointerMotion { event } => {
                let delta_x = event.delta_x();
                let delta_y = event.delta_y();
                let cur = state.pointer_location;
                let new_x = (cur.x + delta_x).clamp(0.0, state.screen_size.w as f64);
                let new_y = (cur.y + delta_y).clamp(0.0, state.screen_size.h as f64);
                let new_pos = Point::from((new_x, new_y));
                let time = event.time_msec();
                let serial = smithay::utils::SERIAL_COUNTER.next_serial();
                state.on_pointer_motion(new_pos, serial, time);
            }
            InputEvent::PointerButton { event } => {
                let button = event.button_code();
                let btn_state = event.state();
                let time = event.time_msec();
                let serial = smithay::utils::SERIAL_COUNTER.next_serial();
                state.on_pointer_button(button, btn_state, serial, time);
            }
            InputEvent::PointerAxis { event } => {
                let v_scroll = smithay::backend::input::PointerAxisEvent::amount(&event, smithay::backend::input::Axis::Vertical)
                    .or_else(|| smithay::backend::input::PointerAxisEvent::amount_v120(&event, smithay::backend::input::Axis::Vertical))
                    .unwrap_or(0.0);
                let h_scroll = smithay::backend::input::PointerAxisEvent::amount(&event, smithay::backend::input::Axis::Horizontal)
                    .or_else(|| smithay::backend::input::PointerAxisEvent::amount_v120(&event, smithay::backend::input::Axis::Horizontal))
                    .unwrap_or(0.0);
                let time = event.time_msec();
                state.on_pointer_axis(h_scroll, v_scroll, time);
            }
            InputEvent::GestureSwipeBegin { event } => {
                let fingers = event.fingers();
                let active_ws = state.workspaces.active_idx;
                state.gesture_tracker.on_gesture_swipe_begin(fingers, active_ws);
            }
            InputEvent::GestureSwipeUpdate { event } => {
                let delta_x = event.delta_x();
                let delta_y = event.delta_y();
                state.gesture_tracker.on_gesture_swipe_update(delta_x, delta_y);
                state.needs_redraw = true;
            }
            InputEvent::GestureSwipeEnd { .. } => {
                if let Some(target_ws) = state.gesture_tracker.on_gesture_swipe_end(state.workspaces.workspaces.len()) {
                    state.execute_action(crate::config::keybindings::Action::SwitchWorkspace(target_ws));
                }
                state.needs_redraw = true;
            }
            _ => {}
        })
        .map_err(|e| format!("Failed to register libinput source: {:?}", e))?;

    let listener = ListeningSocket::bind_auto("wayland", 1..32)?;
    let socket_name = listener
        .socket_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "wayland-1".to_string());
    std::env::set_var("WAYLAND_DISPLAY", &socket_name);
    tracing::info!("material-wm listening on bare-metal WAYLAND_DISPLAY={}", socket_name);

    // Main DRM/KMS Event Loop
    while state.running.load(Ordering::SeqCst) {
        let now = Instant::now();
        state.tick(now);

        // Check for new clients connecting
        if let Some(stream) = listener.accept()? {
            tracing::info!("Accepted new Wayland client on DRM backend");
            let _ = display.handle().insert_client(stream, Arc::new(ClientState::default()));
        }

        display.dispatch_clients(&mut state)?;
        display.flush_clients()?;

        let _ = event_loop.dispatch(Some(std::time::Duration::from_millis(4)), &mut state);
    }

    Ok(())
}
