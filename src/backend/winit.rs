use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use smithay::backend::input::{
    Event, InputEvent, KeyboardKeyEvent, PointerButtonEvent,
};
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::surface::render_elements_from_surface_tree;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTarget};
use smithay::backend::winit::{self, WinitEvent};
use smithay::reexports::calloop::EventLoop;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Display;
use smithay::utils::{Logical, Point, Rectangle, Size, Transform};
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

    let mut damage_tracker = OutputDamageTracker::new(win_size, 1.0, Transform::Normal);

    let auto_screenshot = std::env::var("MATERIAL_WM_AUTOSCREENSHOT").is_ok();
    let mut screenshot_step: u32 = 0;

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
                damage_tracker = OutputDamageTracker::new(size, 1.0, Transform::Normal);
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

                // 1. APPLICATION LAUNCHER (Topmost modal layer when open)
                if state.ui.launcher.is_open || state.ui.launcher.open_progress.value() > 0.01 {
                    if let Some(text_elem) = state.ui.render_launcher_text(renderer, screen_size) {
                        render_elements.push(MaterialRenderElement::Memory(text_elem));
                    }
                    for elem in state.ui.render_launcher_shaders(screen_size) {
                        render_elements.push(MaterialRenderElement::Shader(elem));
                    }
                }

                // 2. QUICK SETTINGS DRAWER (Drawer overlay layer when open)
                if state.ui.drawer.is_open || state.ui.drawer.open_progress.value() > 0.01 {
                    if let Some(drawer_text) = state.ui.render_drawer_text(renderer, screen_size) {
                        render_elements.push(MaterialRenderElement::Memory(drawer_text));
                    }
                    for elem in state.ui.render_drawer_shaders(screen_size) {
                        render_elements.push(MaterialRenderElement::Shader(elem));
                    }
                }

                // 3. TOP STATUS BAR (Bar text on top of bar shaders)
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

                if let Some(bar_text) = state.ui.render_bar_text(renderer, screen_size, active_ws.id, active_title.as_deref()) {
                    render_elements.push(MaterialRenderElement::Memory(bar_text));
                }
                for elem in state.ui.render_bar_shaders(screen_size) {
                    render_elements.push(MaterialRenderElement::Shader(elem));
                }

                // 4. CLIENT WINDOWS (Front-to-back: focused window first)
                for w in state.space.elements().rev() {
                    if !active_ws.windows.contains(w) {
                        continue;
                    }
                    let rect = state.space.element_bbox(w).unwrap_or_else(|| {
                        Rectangle::new(Point::from((0, 0)), (800, 600).into())
                    });
                    let is_focused = focused.as_ref() == Some(w);

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

                // 5. DESKTOP WALLPAPER (Bottom-most background element)
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

                if auto_screenshot {
                    let scratch_dir = "/home/wwmaxik/.gemini/antigravity-cli/brain/4058d2a8-2b7a-4ee4-b02f-1867e76dc498/scratch";
                    match screenshot_step {
                        0 => {
                            let path = format!("{}/screenshot_desktop.png", scratch_dir);
                            capture_screenshot(renderer, &mut framebuffer, screen_size, &path);
                            state.ui.drawer.open(&state.ui.audio);
                            state.ui.drawer.open_progress.set_immediate(1.0);
                            state.ui.drawer.buffer_dirty = true;
                            state.needs_redraw = true;
                            screenshot_step = 1;
                        }
                        1 => {
                            let path = format!("{}/screenshot_drawer.png", scratch_dir);
                            capture_screenshot(renderer, &mut framebuffer, screen_size, &path);
                            state.ui.drawer.close(&state.ui.audio);
                            state.ui.drawer.open_progress.set_immediate(0.0);
                            state.ui.launcher.open(&state.ui.audio);
                            state.ui.launcher.open_progress.set_immediate(1.0);
                            state.ui.launcher.buffer_dirty = true;
                            state.needs_redraw = true;
                            screenshot_step = 2;
                        }
                        2 => {
                            let path = format!("{}/screenshot_launcher.png", scratch_dir);
                            capture_screenshot(renderer, &mut framebuffer, screen_size, &path);
                            state.running.store(false, Ordering::SeqCst);
                            screenshot_step = 3;
                        }
                        _ => {}
                    }
                }

                render_res.damage.cloned()
            };

            // Submit damaged buffer to GPU
            if let Err(e) = backend.submit(damage_to_submit.as_deref()) {
                tracing::warn!("backend.submit: {:?}", e);
            }

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

fn capture_screenshot(
    renderer: &mut GlesRenderer,
    framebuffer: &mut GlesTarget<'_>,
    size: Size<i32, Logical>,
    path: &str,
) {
    use smithay::backend::allocator::Fourcc;
    use smithay::backend::renderer::{ExportMem, Frame, Renderer};

    let region = Rectangle::new((0, 0).into(), (size.w, size.h).into());
    match renderer.copy_framebuffer(framebuffer, region, Fourcc::Abgr8888) {
        Ok(mapping) => match renderer.map_texture(&mapping) {
            Ok(slice) => {
                let w = size.w as u32;
                let h = size.h as u32;
                match image::save_buffer(path, slice, w, h, image::ExtendedColorType::Rgba8) {
                    Ok(_) => tracing::info!("Successfully captured screenshot to {}", path),
                    Err(e) => tracing::warn!("Failed to save screenshot image: {:?}", e),
                }
            }
            Err(e) => tracing::warn!("Failed to map texture for screenshot: {:?}", e),
        },
        Err(e) => tracing::warn!("Failed to copy framebuffer for screenshot: {:?}", e),
    }

    // Restore EGL surface binding after map_texture unbound it
    let _ = renderer.render(framebuffer, (1, 1).into(), Transform::Normal).map(|f| f.finish());
}
