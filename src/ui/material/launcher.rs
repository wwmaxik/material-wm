use std::fs;
use std::path::PathBuf;
use std::process::Command;

use pangocairo::cairo::{self, ImageSurface};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::gles::GlesTexture;
use smithay::output::Output;
use smithay::utils::{Point, Transform};

use crate::render_helpers::memory::MemoryBuffer;
use crate::render_helpers::primary_gpu_texture::PrimaryGpuTextureRenderElement;
use crate::render_helpers::renderer::NiriRenderer;
use crate::render_helpers::solid_color::{SolidColorBuffer, SolidColorRenderElement};
use crate::render_helpers::texture::{TextureBuffer, TextureRenderElement};
use crate::ui::material::audio::AudioManager;
use crate::ui::material::theme::M3Colors;
use crate::utils::output_size;

#[derive(Debug, Clone)]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    pub exec: String,
    pub comment: String,
    pub icon: String,
    pub terminal: bool,
}

pub struct MaterialAppLauncher {
    pub is_open: bool,
    pub progress: f64,
    pub target_progress: f64,
    pub search_query: String,
    pub selected_idx: usize,
    pub scroll_idx: usize,
    pub apps: Vec<AppEntry>,
    dirty: bool,
    cached_buffer: Option<TextureBuffer<GlesTexture>>,
    backdrop_buffer: Option<SolidColorBuffer>,
}

impl MaterialAppLauncher {
    pub const CARD_W: f64 = 520.0;
    pub const CARD_H: f64 = 460.0;

    pub fn new() -> Self {
        let apps = Self::scan_applications();

        Self {
            is_open: false,
            progress: 0.0,
            target_progress: 0.0,
            search_query: String::new(),
            selected_idx: 0,
            scroll_idx: 0,
            apps,
            dirty: true,
            cached_buffer: None,
            backdrop_buffer: None,
        }
    }

    pub fn toggle(&mut self, audio: &AudioManager) {
        if self.is_open {
            self.close(audio);
        } else {
            self.open(audio);
        }
    }

    pub fn open(&mut self, audio: &AudioManager) {
        if !self.is_open {
            self.is_open = true;
            self.target_progress = 1.0;
            self.search_query.clear();
            self.selected_idx = 0;
            self.scroll_idx = 0;
            self.dirty = true;
            audio.play_click();
        }
    }

    pub fn close(&mut self, audio: &AudioManager) {
        if self.is_open {
            self.is_open = false;
            self.target_progress = 0.0;
            self.dirty = true;
            audio.play_click();
        }
    }

    pub fn is_animating(&self) -> bool {
        (self.target_progress - self.progress).abs() > 0.005
    }

    pub fn on_scroll(&mut self, delta_y: f64, audio: &AudioManager) -> bool {
        if !self.is_open {
            return false;
        }

        let filtered_count = self.filtered_apps().len();
        let max_visible = 6;
        if filtered_count <= max_visible {
            return false;
        }

        let max_scroll = filtered_count - max_visible;
        if delta_y > 0.0 {
            // Scroll down
            if self.scroll_idx < max_scroll {
                self.scroll_idx += 1;
                if self.selected_idx < self.scroll_idx {
                    self.selected_idx = self.scroll_idx;
                }
                self.dirty = true;
                audio.play_tick();
                return true;
            }
        } else if delta_y < 0.0 {
            // Scroll up
            if self.scroll_idx > 0 {
                self.scroll_idx -= 1;
                if self.selected_idx >= self.scroll_idx + max_visible {
                    self.selected_idx = self.scroll_idx + max_visible - 1;
                }
                self.dirty = true;
                audio.play_tick();
                return true;
            }
        }

        false
    }

    pub fn filtered_apps(&self) -> Vec<&AppEntry> {
        let query = self.search_query.trim().to_lowercase();
        if query.is_empty() {
            return self.apps.iter().collect();
        }

        self.apps
            .iter()
            .filter(|app| {
                app.name.to_lowercase().contains(&query)
                    || app.comment.to_lowercase().contains(&query)
                    || app.id.to_lowercase().contains(&query)
            })
            .collect()
    }

    pub fn on_key_down(&mut self, key_name: &str, audio: &AudioManager) -> bool {
        if !self.is_open {
            return false;
        }

        let filtered_count = self.filtered_apps().len();

        match key_name {
            "Escape" => {
                self.close(audio);
                true
            }
            "Return" | "KP_Enter" => {
                let filtered = self.filtered_apps();
                if let Some(app) = filtered.get(self.selected_idx) {
                    Self::launch_app(app);
                    self.close(audio);
                }
                true
            }
            "Up" => {
                if self.selected_idx > 0 {
                    self.selected_idx -= 1;
                    if self.selected_idx < self.scroll_idx {
                        self.scroll_idx = self.selected_idx;
                    }
                    self.dirty = true;
                    audio.play_tick();
                }
                true
            }
            "Down" => {
                if filtered_count > 0 && self.selected_idx + 1 < filtered_count {
                    self.selected_idx += 1;
                    let max_visible = 6;
                    if self.selected_idx >= self.scroll_idx + max_visible {
                        self.scroll_idx = self.selected_idx - max_visible + 1;
                    }
                    self.dirty = true;
                    audio.play_tick();
                }
                true
            }
            "Page_Up" => {
                let step = 5;
                self.selected_idx = self.selected_idx.saturating_sub(step);
                self.scroll_idx = self.scroll_idx.saturating_sub(step);
                self.dirty = true;
                audio.play_tick();
                true
            }
            "Page_Down" => {
                let step = 5;
                if filtered_count > 0 {
                    self.selected_idx = (self.selected_idx + step).min(filtered_count - 1);
                    let max_visible = 6;
                    if filtered_count > max_visible {
                        let max_scroll = filtered_count - max_visible;
                        self.scroll_idx = (self.scroll_idx + step).min(max_scroll);
                    }
                }
                self.dirty = true;
                audio.play_tick();
                true
            }
            "BackSpace" => {
                if self.search_query.pop().is_some() {
                    self.selected_idx = 0;
                    self.scroll_idx = 0;
                    self.dirty = true;
                    audio.play_tick();
                }
                true
            }
            _ => false,
        }
    }

    pub fn on_char_input(&mut self, ch: char, audio: &AudioManager) -> bool {
        if !self.is_open || ch.is_control() {
            return false;
        }

        self.search_query.push(ch);
        self.selected_idx = 0;
        self.dirty = true;
        audio.play_tick();
        true
    }

    pub fn on_pointer_click(
        &mut self,
        x: f64,
        y: f64,
        screen_w: f64,
        screen_h: f64,
        audio: &AudioManager,
    ) -> bool {
        if !self.is_open {
            return false;
        }

        let card_x = (screen_w - Self::CARD_W) / 2.0;
        let card_y = (screen_h - Self::CARD_H) / 2.0;

        // If clicked outside card, close
        if x < card_x || x > card_x + Self::CARD_W || y < card_y || y > card_y + Self::CARD_H {
            self.close(audio);
            return true;
        }

        // List starts after search pill (search_y: 20, search_h: 44, gap: 16 -> 80)
        let list_y = card_y + 80.0;
        let item_h = 54.0;
        if y >= list_y {
            let row = ((y - list_y) / item_h) as usize;
            if row < 6 {
                let app_idx = self.scroll_idx + row;
                let filtered = self.filtered_apps();
                if let Some(app) = filtered.get(app_idx) {
                    Self::launch_app(app);
                    self.close(audio);
                    return true;
                }
            }
        }

        true
    }

    pub fn render<R: NiriRenderer>(
        &mut self,
        renderer: &mut R,
        output: &Output,
        colors: &M3Colors,
        push: &mut dyn FnMut(crate::niri::OutputRenderElements<R>),
    ) {
        let diff = self.target_progress - self.progress;
        if diff.abs() > 0.005 {
            self.progress += diff * 0.28;
        } else {
            self.progress = self.target_progress;
        }

        if !self.is_open && self.progress < 0.01 {
            return;
        }

        let scale = output.current_scale().fractional_scale();
        let size = output_size(output);
        let screen_w = size.w;
        let screen_h = size.h;

        let slide_y = (1.0 - self.progress) * -30.0;
        let card_x = (screen_w - Self::CARD_W) / 2.0;
        let card_y = (screen_h - Self::CARD_H) / 2.0 + slide_y;

        // 1. Darkened backdrop scrim over entire screen
        let backdrop = self.backdrop_buffer.get_or_insert_with(|| {
            SolidColorBuffer::new(size, [0.0, 0.0, 0.0, 0.55])
        });
        backdrop.resize(size);
        let scrim_elem = SolidColorRenderElement::from_buffer(
            backdrop,
            (0., 0.),
            (self.progress * 0.55) as f32,
            Kind::Unspecified,
        );
        push(scrim_elem.into());

        // 2. Render Modal Card
        if self.cached_buffer.is_none() || self.dirty {
            self.dirty = false;

            let card_w = Self::CARD_W;
            let card_h = Self::CARD_H;
            let shadow_pad = 24.0;
            let total_w = card_w + shadow_pad * 2.0;
            let total_h = card_h + shadow_pad * 2.0;

            let phys_w = (total_w * scale).round() as i32;
            let phys_h = (total_h * scale).round() as i32;

            if let Ok(surface) = ImageSurface::create(cairo::Format::ARgb32, phys_w, phys_h) {
                if let Ok(cr) = cairo::Context::new(&surface) {
                    cr.scale(scale, scale);

                    let ox = shadow_pad;
                    let oy = shadow_pad;

                    // Elevation drop shadow
                    for (pad, alpha) in [(20.0, 0.06), (14.0, 0.12), (6.0, 0.22)] {
                        Self::set_source_color(&cr, [0.0, 0.0, 0.0, alpha]);
                        Self::draw_rounded_rect(
                            &cr,
                            ox - pad,
                            oy - pad + 8.0,
                            card_w + pad * 2.0,
                            card_h + pad * 2.0,
                            28.0 + pad,
                        );
                        let _ = cr.fill();
                    }

                    // Card background & outline
                    Self::set_source_color(&cr, colors.surface_container);
                    Self::draw_rounded_rect(&cr, ox, oy, card_w, card_h, 28.0);
                    let _ = cr.fill();

                    Self::set_source_color(&cr, colors.outline_variant);
                    Self::draw_rounded_rect(&cr, ox, oy, card_w, card_h, 28.0);
                    cr.set_line_width(1.2);
                    let _ = cr.stroke();

                    // Search input capsule pill
                    let search_x = ox + 20.0;
                    let search_y = oy + 20.0;
                    let search_w = card_w - 40.0;
                    let search_h = 44.0;

                    Self::set_source_color(&cr, colors.surface_container_highest);
                    Self::draw_rounded_rect(&cr, search_x, search_y, search_w, search_h, 22.0);
                    let _ = cr.fill();

                    // Search icon circle badge
                    Self::set_source_color(&cr, colors.primary);
                    cr.arc(search_x + 22.0, search_y + 22.0, 12.0, 0.0, 2.0 * std::f64::consts::PI);
                    let _ = cr.fill();

                    // Search glass icon
                    Self::set_source_color(&cr, colors.on_primary);
                    cr.arc(search_x + 21.0, search_y + 21.0, 4.0, 0.0, 2.0 * std::f64::consts::PI);
                    cr.set_line_width(1.6);
                    let _ = cr.stroke();
                    cr.move_to(search_x + 24.0, search_y + 24.0);
                    cr.line_to(search_x + 27.5, search_y + 27.5);
                    let _ = cr.stroke();

                    // Search query text or placeholder
                    if self.search_query.is_empty() {
                        Self::draw_text(
                            &cr,
                            "Поиск приложений и утилит...",
                            "sans 13px",
                            colors.on_surface_variant,
                            search_x + 46.0,
                            search_y + 13.0,
                        );
                    } else {
                        let query_display = format!("{}|", self.search_query);
                        Self::draw_text(
                            &cr,
                            &query_display,
                            "sans medium 13px",
                            colors.on_surface,
                            search_x + 46.0,
                            search_y + 13.0,
                        );
                    }

                    // Application list
                    let list_y = search_y + search_h + 16.0;
                    let item_h = 50.0;
                    let filtered = self.filtered_apps();
                    let max_visible = 6;
                    let has_scroll = filtered.len() > max_visible;
                    let list_w = if has_scroll { search_w - 14.0 } else { search_w };

                    for i in 0..max_visible {
                        let app_idx = self.scroll_idx + i;
                        if app_idx >= filtered.len() {
                            break;
                        }

                        let app = filtered[app_idx];
                        let item_y = list_y + i as f64 * (item_h + 4.0);
                        let is_selected = app_idx == self.selected_idx;

                        // Selection pill background
                        if is_selected {
                            Self::set_source_color(&cr, colors.primary_container);
                            Self::draw_rounded_rect(&cr, search_x, item_y, list_w, item_h, 18.0);
                            let _ = cr.fill();
                        } else {
                            Self::set_source_color(&cr, colors.surface_container_high);
                            Self::draw_rounded_rect(&cr, search_x, item_y, list_w, item_h, 18.0);
                            let _ = cr.fill();
                        }

                        // Pastel avatar circle badge
                        let avatar_x = search_x + 24.0;
                        let avatar_y = item_y + 25.0;
                        let avatar_color = Self::pastel_color(&app.name);
                        Self::set_source_color(&cr, avatar_color);
                        cr.arc(avatar_x, avatar_y, 16.0, 0.0, 2.0 * std::f64::consts::PI);
                        let _ = cr.fill();

                        // Initial letter
                        let initial = app
                            .name
                            .chars()
                            .next()
                            .map(|c| c.to_uppercase().to_string())
                            .unwrap_or_else(|| "A".into());
                        Self::draw_text(
                            &cr,
                            &initial,
                            "sans bold 13px",
                            [0.1, 0.1, 0.15, 0.95],
                            avatar_x - 5.5,
                            avatar_y - 8.5,
                        );

                        // App Name
                        let title_color = if is_selected {
                            colors.on_primary_container
                        } else {
                            colors.on_surface
                        };
                        Self::draw_text(
                            &cr,
                            &app.name,
                            "sans bold 13px",
                            title_color,
                            search_x + 52.0,
                            item_y + 8.5,
                        );

                        // Subtitle / Comment
                        let sub_color = if is_selected {
                            colors.on_primary_container
                        } else {
                            colors.on_surface_variant
                        };
                        let comment_trunc = if app.comment.len() > 45 {
                            format!("{}...", &app.comment[..42])
                        } else {
                            app.comment.clone()
                        };
                        Self::draw_text(
                            &cr,
                            &comment_trunc,
                            "sans 10px",
                            sub_color,
                            search_x + 52.0,
                            item_y + 27.5,
                        );
                    }

                    // Vertical scrollbar pill
                    if has_scroll {
                        let scrollbar_x = search_x + search_w - 6.0;
                        let track_y = list_y;
                        let track_h = max_visible as f64 * (item_h + 4.0) - 4.0;
                        let thumb_h = (track_h * (max_visible as f64 / filtered.len() as f64)).max(22.0);
                        let max_scroll = (filtered.len() - max_visible) as f64;
                        let scroll_ratio = if max_scroll > 0.0 {
                            self.scroll_idx as f64 / max_scroll
                        } else {
                            0.0
                        };
                        let thumb_y = track_y + scroll_ratio * (track_h - thumb_h);

                        // Track
                        Self::set_source_color(&cr, [1.0, 1.0, 1.0, 0.06]);
                        Self::draw_rounded_rect(&cr, scrollbar_x, track_y, 4.0, track_h, 2.0);
                        let _ = cr.fill();

                        // Thumb pill
                        Self::set_source_color(&cr, colors.primary);
                        Self::draw_rounded_rect(&cr, scrollbar_x, thumb_y, 4.0, thumb_h, 2.0);
                        let _ = cr.fill();
                    }

                    drop(cr);
                    surface.flush();
                    if let Ok(data) = surface.take_data() {
                        let buffer = MemoryBuffer::new(
                            data.to_vec(),
                            Fourcc::Argb8888,
                            (phys_w, phys_h),
                            scale,
                            Transform::Normal,
                        );
                        if let Ok(tex_buf) =
                            TextureBuffer::from_memory_buffer(renderer.as_gles_renderer(), &buffer)
                        {
                            self.cached_buffer = Some(tex_buf);
                        }
                    }
                }
            }
        }

        if let Some(buffer) = self.cached_buffer.clone() {
            let elem = TextureRenderElement::from_texture_buffer(
                buffer,
                Point::new(card_x - 24.0, card_y - 24.0),
                self.progress as f32,
                None,
                None,
                Kind::Unspecified,
            );
            push(PrimaryGpuTextureRenderElement(elem).into());
        }
    }

    fn pastel_color(name: &str) -> [f64; 4] {
        let palette = [
            [0.82, 0.74, 1.00, 1.0], // Lavender
            [1.00, 0.76, 0.76, 1.0], // Peach / Rose
            [0.68, 0.88, 0.84, 1.0], // Mint
            [0.72, 0.82, 1.00, 1.0], // Sky Blue
            [0.96, 0.92, 0.65, 1.0], // Lemon
            [0.80, 0.94, 0.75, 1.0], // Pistachio
        ];
        let hash = name.bytes().fold(0usize, |acc, b| acc.wrapping_add(b as usize));
        palette[hash % palette.len()]
    }

    fn launch_app(app: &AppEntry) {
        let mut clean_exec = app.exec.clone();
        for field in &["%f", "%F", "%u", "%U", "%d", "%D", "%n", "%N", "%k", "%v", "%m", "%%"] {
            clean_exec = clean_exec.replace(field, "");
        }
        let clean_exec = clean_exec.trim().to_string();

        tracing::info!("Launching application: {} (exec: {})", app.name, clean_exec);

        if app.terminal {
            let terms = ["kitty", "alacritty", "foot", "wezterm", "gnome-terminal", "xterm"];
            let term = terms
                .iter()
                .find(|t| Command::new("which").arg(t).output().map(|o| o.status.success()).unwrap_or(false))
                .unwrap_or(&"kitty");
            let _ = Command::new(term).arg("-e").arg("sh").arg("-c").arg(&clean_exec).spawn();
        } else {
            let _ = Command::new("sh").arg("-c").arg(&clean_exec).spawn();
        }
    }

    fn scan_applications() -> Vec<AppEntry> {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        let search_dirs = [
            PathBuf::from("/usr/share/applications"),
            PathBuf::from("/usr/local/share/applications"),
            PathBuf::from(&home).join(".local/share/applications"),
            PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        ];

        let mut apps = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        for dir in search_dirs {
            if !dir.exists() {
                continue;
            }
            let entries = match fs::read_dir(&dir) {
                Ok(e) => e,
                Err(_) => continue,
            };

            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("desktop") {
                    continue;
                }

                let id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();

                if seen_ids.contains(&id) {
                    continue;
                }

                let content = match fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                let mut in_desktop_entry = false;
                let mut name = None;
                let mut exec = None;
                let mut comment = None;
                let mut icon = None;
                let mut nodisplay = false;
                let mut terminal = false;

                for line in content.lines() {
                    let line = line.trim();
                    if line.starts_with('[') && line.ends_with(']') {
                        in_desktop_entry = line == "[Desktop Entry]";
                        continue;
                    }

                    if !in_desktop_entry {
                        continue;
                    }

                    if let Some((k, v)) = line.split_once('=') {
                        let k = k.trim();
                        let v = v.trim();
                        match k {
                            "Name" if name.is_none() => name = Some(v.to_string()),
                            "Exec" if exec.is_none() => exec = Some(v.to_string()),
                            "Comment" if comment.is_none() => comment = Some(v.to_string()),
                            "Icon" if icon.is_none() => icon = Some(v.to_string()),
                            "NoDisplay" => nodisplay = v.eq_ignore_ascii_case("true"),
                            "Terminal" => terminal = v.eq_ignore_ascii_case("true"),
                            _ => {}
                        }
                    }
                }

                if nodisplay {
                    continue;
                }

                if let (Some(n), Some(e)) = (name, exec) {
                    seen_ids.insert(id.clone());
                    apps.push(AppEntry {
                        id,
                        name: n,
                        exec: e,
                        comment: comment.unwrap_or_default(),
                        icon: icon.unwrap_or_default(),
                        terminal,
                    });
                }
            }
        }

        apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        apps
    }

    fn set_source_color(cr: &cairo::Context, c: [f64; 4]) {
        cr.set_source_rgba(c[0], c[1], c[2], c[3]);
    }

    fn draw_rounded_rect(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
        let r = r.min(w / 2.0).min(h / 2.0);
        let deg = std::f64::consts::PI / 180.0;
        cr.new_sub_path();
        cr.arc(x + w - r, y + r, r, -90.0 * deg, 0.0 * deg);
        cr.arc(x + w - r, y + h - r, r, 0.0 * deg, 90.0 * deg);
        cr.arc(x + r, y + h - r, r, 90.0 * deg, 180.0 * deg);
        cr.arc(x + r, y + r, r, 180.0 * deg, 270.0 * deg);
        cr.close_path();
    }

    fn draw_text(
        cr: &cairo::Context,
        text: &str,
        font_desc: &str,
        color: [f64; 4],
        x: f64,
        y: f64,
    ) {
        Self::set_source_color(cr, color);
        let font = pango::FontDescription::from_string(font_desc);
        let layout = pangocairo::functions::create_layout(cr);
        layout.set_font_description(Some(&font));
        layout.set_text(text);
        cr.move_to(x, y);
        pangocairo::functions::show_layout(cr, &layout);
    }
}
