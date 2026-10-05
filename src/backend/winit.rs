use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use smithay::backend::input::{
    Event, InputEvent, KeyboardKeyEvent, PointerButtonEvent,
};
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::surface::render_elements_from_surface_tree;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::winit::{self, WinitEvent};
use smithay::reexports::calloop::EventLoop;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Display;
use smithay::utils::{Point, Rectangle, Size, Transform};
use smithay::wayland::compositor::{with_surface_tree_downward, SurfaceAttributes, TraversalAction};
use smithay::wayland::seat::WaylandFocus;
use smithay::reexports::winit::platform::pump_events::PumpStatus;
use wayland_server::ListeningSocket;

use crate::config::Config;
use crate::state::{ClientState, MaterialRenderElement, MaterialWmState};

pub fn run_winit(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let mut event_loop: EventLoop<MaterialWmState> = EventLoop::try_new()?;
    let mut display: Display<MaterialWmState> = Display::new()?;
    let dh = display.handle();

    let mut state = MaterialWmState::new(dh.clone(), config);

    let (mut backend, mut winit_event_loop) = winit::init::<GlesRenderer>()?;
    backend.window().set_title("material-wm (Material You Desktop)");

    let win_size = backend.window_size();
    let logical_size = Size::from((win_size.w as i32, win_size.h as i32));
    state.screen_size = logical_size;
    state.gesture_tracker.screen_width = logical_size.w;

    let mut damage_tracker = OutputDamageTracker::new(win_size, 1.0, Transform::Flipped180);

    // Initialize Material You GLES SDF shaders on the backend renderer
    {
        let (renderer, _) = backend.bind()?;
        state.ui.init_shaders(renderer);
    }

    let listener = ListeningSocket::bind_auto("wayland", 1..32)?;
    let socket_name = listener
        .socket_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "wayland-1".to_string());
    std::env::set_var("WAYLAND_DISPLAY", &socket_name);
    tracing::info!("material-wm listening on WAYLAND_DISPLAY={}", socket_name);

    while state.running.load(Ordering::SeqCst) {
        let now = Instant::now();
        state.tick(now);

        let pump_status = winit_event_loop.dispatch_new_events(|event| match event {
            WinitEvent::Resized { size, .. } => {
                let logical = Size::from((size.w as i32, size.h as i32));
                state.screen_size = logical;
                state.gesture_tracker.screen_width = logical.w;
                damage_tracker = OutputDamageTracker::new(size, 1.0, Transform::Flipped180);
                state.needs_redraw = true;
            }
            WinitEvent::Input(input_event) => match input_event {
                InputEvent::Keyboard { event } => {
                    let keycode = event.key_code();
                    let key_state = event.state();
                    let time = event.time_msec();
                    let serial = smithay::utils::SERIAL_COUNTER.next_serial();
                    state.on_keyboard_key(keycode.into(), key_state, serial, time);
                }
                InputEvent::PointerMotionAbsolute { event } => {
                    let screen_size = state.screen_size;
                    let pos = smithay::backend::input::AbsolutePositionEvent::position_transformed(&event, screen_size);
                    let time = event.time_msec();
                    let serial = smithay::utils::SERIAL_COUNTER.next_serial();
                    state.on_pointer_motion(pos, serial, time);
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
                _ => {}
            },
            WinitEvent::Redraw => {
                state.needs_redraw = true;
            }
            _ => {}
        });

        match pump_status {
            PumpStatus::Continue => (),
            PumpStatus::Exit(_) => break,
        }

        // Check for new clients connecting
        if let Some(stream) = listener.accept()? {
            tracing::info!("Accepted new Wayland client");
            let _ = display.handle().insert_client(stream, Arc::new(ClientState::default()));
        }

        // Render Frame with Damage Tracking
        if state.needs_redraw {
            state.needs_redraw = false;

            let damage_to_submit = {
                let (renderer, mut framebuffer) = backend.bind()?;
                let screen_size = state.screen_size;

                let mut render_elements: Vec<MaterialRenderElement> = Vec::new();

                // 1. Application Launcher text overlay (front-most element)
                if let Some(text_elem) = state.ui.render_launcher_text(renderer, screen_size) {
                    render_elements.push(MaterialRenderElement::Memory(text_elem));
                }

                // 2. crDroid Quick Settings Drawer text overlay
                if let Some(drawer_text) = state.ui.render_drawer_text(renderer, screen_size) {
                    render_elements.push(MaterialRenderElement::Memory(drawer_text));
                }

                // 3. Client windows and top bar context
                let active_ws = state.workspaces.active_workspace();
                let focused = active_ws.focused_window.clone();
                let active_title = focused.as_ref().and_then(|w| {
                    w.wl_surface().and_then(|s| {
                        smithay::wayland::compositor::with_states(&s, |states| {
                            states.data_map.get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
                                .and_then(|d| d.lock().ok())
                                .and_then(|role| role.title.clone().or_else(|| role.app_id.clone()))
                        })
                    })
                });

                // Top bar text overlay (workspaces, active title, battery/clock)
                if let Some(bar_text) = state.ui.render_bar_text(renderer, screen_size, active_ws.id, active_title.as_deref()) {
                    render_elements.push(MaterialRenderElement::Memory(bar_text));
                }

                // 4. UI Overlay Shaders: Launcher, Drawer, and Top Bar
                let ui_elements = state.ui.render_ui(renderer, screen_size);
                for elem in ui_elements {
                    render_elements.push(MaterialRenderElement::Shader(elem));
                }

                // 5. Client windows in front-to-back order (space.elements().rev())
                for w in state.space.elements().rev() {
                    if !active_ws.windows.contains(w) {
                        continue;
                    }
                    let rect = state.space.element_bbox(w).unwrap_or_else(|| {
                        Rectangle::new(Point::from((0, 0)), (800, 600).into())
                    });
                    let is_focused = focused.as_ref() == Some(w);

                    // Front-to-back for each window: border -> surfaces -> drop shadow
                    if let Some(border) = state.ui.render_window_border(rect, is_focused, 1.0) {
                        render_elements.push(MaterialRenderElement::Shader(border));
                    }

                    if let Some(surface) = w.wl_surface() {
                        let surf_elements = render_elements_from_surface_tree(
                            renderer,
                            &surface,
                            (rect.loc.x, rect.loc.y),
                            1.0,
                            1.0,
                            Kind::Unspecified,
                        );
                        for elem in surf_elements {
                            render_elements.push(MaterialRenderElement::Surface(elem));
                        }
                    }

                    if let Some(shadow) = state.ui.render_window_shadow(rect, 1.0) {
                        render_elements.push(MaterialRenderElement::Shader(shadow));
                    }
                }

                // 6. Desktop Wallpaper (bottom-most background element)
                if let Some(wallpaper_elem) = state.ui.render_wallpaper(renderer, screen_size) {
                    render_elements.push(MaterialRenderElement::Memory(wallpaper_elem));
                }

                let clear_color = state.ui.colors.surface_color();

                // Strict damage tracking: only redraw damaged regions
                let render_res = damage_tracker.render_output(
                    renderer,
                    &mut framebuffer,
                    0,
                    &render_elements,
                    clear_color,
                )?;

                render_res.damage.cloned()
            };

            // Submit damaged buffer to GPU
            backend.submit(damage_to_submit.as_deref())?;

            // Send frame callbacks to clients
            let elapsed_ms = state.start_time.elapsed().as_millis() as u32;
            let windows = state.workspaces.active_workspace().windows.clone();
            for w in &windows {
                if let Some(surface) = w.wl_surface() {
                    send_frames_surface_tree(&surface, elapsed_ms);
                }
            }
        }

        display.dispatch_clients(&mut state)?;
        display.flush_clients()?;

        // Calloop loop dispatch with minimal 2ms sleep to stay smooth and battery-friendly
        let _ = event_loop.dispatch(Some(std::time::Duration::from_millis(2)), &mut state);
    }

    Ok(())
}

fn send_frames_surface_tree(surface: &WlSurface, time: u32) {
    with_surface_tree_downward(
        surface,
        (),
        |_, _, &()| TraversalAction::DoChildren(()),
        |_surf, states, &()| {
            for callback in states
                .cached_state
                .get::<SurfaceAttributes>()
                .current()
                .frame_callbacks
                .drain(..)
            {
                callback.done(time);
            }
        },
        |_, _, &()| true,
    );
}
