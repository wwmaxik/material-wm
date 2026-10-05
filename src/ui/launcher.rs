use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement};
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::gles::element::PixelShaderElement;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::utils::{Logical, Point, Rectangle, Size, Transform};
use xkbcommon::xkb::Keysym;

use crate::config::theme::M3Colors;
use crate::config::Config;
use crate::shell::animation::AnimatedFloat;
use crate::ui::audio::AudioManager;
use crate::ui::shader::MaterialShaderPipeline;
use crate::ui::text::FontRenderer;

#[derive(Debug, Clone)]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    pub exec: String,
    pub comment: String,
    pub icon: String,
    pub terminal: bool,
}

#[derive(Debug, Clone)]
pub struct ExternalLauncher {
    pub name: String,
    pub path: PathBuf,
    pub command: String,
}

/// Detect popular external launchers installed on the system
pub fn find_external_launchers() -> Vec<ExternalLauncher> {
    let candidates = [
        ("rofi", "rofi -show drun -show-icons"),
        ("fuzzel", "fuzzel"),
        ("wofi", "wofi --show drun"),
        ("tofi", "tofi-drun --drun-launch=true"),
        ("dmenu", "dmenu_run"),
    ];

    let mut found = Vec::new();
    for (name, cmd) in candidates {
        if let Ok(output) = Command::new("which").arg(name).output() {
            if output.status.success() {
                let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path_str.is_empty() {
                    found.push(ExternalLauncher {
                        name: name.to_string(),
                        path: PathBuf::from(path_str),
                        command: cmd.to_string(),
                    });
                }
            }
        }
    }
    found
}

/// Scan standard XDG desktop directories for .desktop application entries
pub fn scan_applications() -> Vec<AppEntry> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let search_dirs = [
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
        PathBuf::from(&home).join(".local/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        PathBuf::from(&home).join(".local/share/flatpak/exports/share/applications"),
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

            if let Some(app) = parse_desktop_file(&path, &id) {
                seen_ids.insert(id);
                apps.push(app);
            }
        }
    }

    // Sort alphabetically by name
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
}

fn parse_desktop_file(path: &Path, id: &str) -> Option<AppEntry> {
    let content = fs::read_to_string(path).ok()?;
    let mut is_desktop_entry = false;
    let mut app_type = String::new();
    let mut name = String::new();
    let mut name_ru = String::new();
    let mut exec = String::new();
    let mut comment = String::new();
    let mut comment_ru = String::new();
    let mut icon = String::new();
    let mut terminal = false;
    let mut no_display = false;
    let mut hidden = false;

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            is_desktop_entry = line == "[Desktop Entry]";
            continue;
        }
        if !is_desktop_entry || line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            let v = v.trim();
            match k {
                "Type" => app_type = v.to_string(),
                "Name" => name = v.to_string(),
                "Name[ru]" => name_ru = v.to_string(),
                "Exec" => exec = v.to_string(),
                "Comment" => comment = v.to_string(),
                "Comment[ru]" => comment_ru = v.to_string(),
                "GenericName" if comment.is_empty() => comment = v.to_string(),
                "Icon" => icon = v.to_string(),
                "Terminal" => terminal = v.eq_ignore_ascii_case("true"),
                "NoDisplay" => no_display = v.eq_ignore_ascii_case("true"),
                "Hidden" => hidden = v.eq_ignore_ascii_case("true"),
                _ => {}
            }
        }
    }

    if app_type != "Application" || no_display || hidden || exec.is_empty() {
        return None;
    }

    let final_name = if !name_ru.is_empty() { name_ru } else if !name.is_empty() { name } else { id.to_string() };
    let final_comment = if !comment_ru.is_empty() { comment_ru } else { comment };

    // Sanitize Exec: remove %f, %F, %u, %U, etc.
    let sanitized_exec = sanitize_exec(&exec);

    Some(AppEntry {
        id: id.to_string(),
        name: final_name,
        exec: sanitized_exec,
        comment: final_comment,
        icon,
        terminal,
    })
}

fn sanitize_exec(exec: &str) -> String {
    let mut result = Vec::new();
    for part in exec.split_whitespace() {
        if part.starts_with('%') && part.len() == 2 {
            continue;
        }
        result.push(part);
    }
    result.join(" ")
}

pub const M3_AVATAR_PALETTES: [([f32; 4], [u8; 4]); 8] = [
    ([0.816, 0.737, 1.0, 1.0], [56, 30, 114, 255]),   // Pastel Violet (#D0BCFF / #381E72)
    ([0.659, 0.780, 0.980, 1.0], [6, 46, 111, 255]),   // Pastel Sky Blue (#A8C7FA / #062E6F)
    ([0.482, 0.816, 0.757, 1.0], [0, 55, 49, 255]),    // Pastel Teal (#7BD0C1 / #003731)
    ([0.659, 0.855, 0.710, 1.0], [27, 55, 33, 255]),   // Pastel Sage (#A8DAB5 / #1B3721)
    ([1.0, 0.843, 0.659, 1.0], [74, 40, 0, 255]),     // Pastel Peach (#FFD7A8 / #4A2800)
    ([1.0, 0.698, 0.722, 1.0], [86, 29, 37, 255]),    // Pastel Rose (#FFB2B8 / #561D25)
    ([0.910, 0.886, 0.533, 1.0], [60, 56, 0, 255]),    // Pastel Lemon (#E8E288 / #3C3800)
    ([0.800, 0.761, 0.863, 1.0], [51, 45, 65, 255]),   // Pastel Lavender (#CCC2DC / #332D41)
];

pub fn get_app_avatar_colors(app: &AppEntry) -> ([f32; 4], [u8; 4]) {
    let hash = app.id.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32));
    M3_AVATAR_PALETTES[(hash as usize) % M3_AVATAR_PALETTES.len()]
}

pub fn format_app_comment(app: &AppEntry) -> String {
    let comment = app.comment.trim();
    if !comment.is_empty() && !comment.contains('/') && !comment.contains('\\') && !comment.starts_with("env ") {
        if comment.len() > 48 {
            return format!("{}...", &comment[..45]);
        }
        return comment.to_string();
    }

    let name_lower = app.name.to_lowercase();
    let exec_lower = app.exec.to_lowercase();

    if app.terminal || exec_lower.contains("terminal") || exec_lower.contains("kitty") || exec_lower.contains("alacritty") {
        "Терминал и консоль".to_string()
    } else if name_lower.contains("game") || name_lower.contains("proton") || name_lower.contains("steam") || name_lower.contains("minecraft") || name_lower.contains("wine") || exec_lower.contains("steam") {
        "Игры и развлечения".to_string()
    } else if name_lower.contains("browser") || name_lower.contains("firefox") || name_lower.contains("chrome") || name_lower.contains("vpn") || name_lower.contains("tor") || name_lower.contains("amnezia") {
        "Интернет и сеть".to_string()
    } else if name_lower.contains("settings") || name_lower.contains("настройки") || name_lower.contains("control") {
        "Системные параметры".to_string()
    } else if name_lower.contains("code") || name_lower.contains("nvim") || name_lower.contains("git") || name_lower.contains("rust") || name_lower.contains("dev") || name_lower.contains("antigravity") {
        "Разработка и код".to_string()
    } else if name_lower.contains("file") || name_lower.contains("ark") || name_lower.contains("dolphin") || name_lower.contains("fm") {
        "Файлы и архивы".to_string()
    } else if name_lower.contains("audio") || name_lower.contains("music") || name_lower.contains("player") || name_lower.contains("video") {
        "Медиа и звук".to_string()
    } else {
        "Приложение системы".to_string()
    }
}

pub struct AppLauncher {
    pub is_open: bool,
    pub open_progress: AnimatedFloat,
    pub search_query: String,
    pub selected_index: usize,
    pub apps: Vec<AppEntry>,
    pub filtered_indices: Vec<usize>,
    pub width: i32,
    pub height: i32,
    pub scroll_offset: usize,
    pub text_buffer: Option<MemoryRenderBuffer>,
    pub buffer_dirty: bool,
}

impl AppLauncher {
    pub const VISIBLE_ITEMS: usize = 7;
    pub const ITEM_HEIGHT: i32 = 56;
    pub const CARD_WIDTH: i32 = 540;
    pub const CARD_HEIGHT: i32 = 530;

    pub fn new(config: &Config) -> Self {
        let apps = scan_applications();
        let total = apps.len();
        let filtered_indices: Vec<usize> = (0..total).collect();

        Self {
            is_open: false,
            open_progress: AnimatedFloat::new(0.0, config.animation_duration_ms),
            search_query: String::new(),
            selected_index: 0,
            apps,
            filtered_indices,
            width: Self::CARD_WIDTH,
            height: Self::CARD_HEIGHT,
            scroll_offset: 0,
            text_buffer: None,
            buffer_dirty: true,
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
        self.is_open = true;
        self.open_progress.retarget(1.0);
        self.search_query.clear();
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.update_filter();
        self.buffer_dirty = true;
        audio.play_pop();
    }

    pub fn close(&mut self, audio: &AudioManager) {
        self.is_open = false;
        self.open_progress.retarget(0.0);
        self.search_query.clear();
        self.buffer_dirty = true;
        audio.play_click();
    }

    pub fn update_animations(&mut self, now: Instant) -> bool {
        self.open_progress.is_animating(now)
    }

    pub fn update_filter(&mut self) {
        let q = self.search_query.trim().to_lowercase();
        if q.is_empty() {
            self.filtered_indices = (0..self.apps.len()).collect();
        } else {
            let mut prefix_matches = Vec::new();
            let mut contains_matches = Vec::new();

            for (idx, app) in self.apps.iter().enumerate() {
                let name_lower = app.name.to_lowercase();
                if name_lower.starts_with(&q) {
                    prefix_matches.push(idx);
                } else if name_lower.contains(&q)
                    || app.comment.to_lowercase().contains(&q)
                    || app.id.to_lowercase().contains(&q)
                {
                    contains_matches.push(idx);
                }
            }

            prefix_matches.extend(contains_matches);
            self.filtered_indices = prefix_matches;
        }

        self.selected_index = 0;
        self.scroll_offset = 0;
        self.buffer_dirty = true;
    }

    pub fn launcher_rect(&self, screen_size: Size<i32, Logical>) -> Rectangle<i32, Logical> {
        let x = (screen_size.w - self.width) / 2;
        let y = (screen_size.h - self.height) / 2;
        Rectangle::new(Point::from((x, y)), (self.width, self.height).into())
    }

    /// Handle keyboard input when launcher is active
    pub fn on_keyboard(&mut self, keysym: Keysym, terminal_cmd: &str, audio: &AudioManager) -> Option<String> {
        match keysym {
            Keysym::Escape => {
                self.close(audio);
                None
            }
            Keysym::Return => {
                if let Some(&app_idx) = self.filtered_indices.get(self.selected_index) {
                    let app = &self.apps[app_idx];
                    let exec = if app.terminal {
                        format!("{} -e {}", terminal_cmd, app.exec)
                    } else {
                        app.exec.clone()
                    };
                    self.close(audio);
                    Some(exec)
                } else {
                    None
                }
            }
            Keysym::Up => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                    if self.selected_index < self.scroll_offset {
                        self.scroll_offset = self.selected_index;
                    }
                    self.buffer_dirty = true;
                    audio.play_tick();
                }
                None
            }
            Keysym::Down => {
                if !self.filtered_indices.is_empty() && self.selected_index + 1 < self.filtered_indices.len() {
                    self.selected_index += 1;
                    if self.selected_index >= self.scroll_offset + Self::VISIBLE_ITEMS {
                        self.scroll_offset = self.selected_index - Self::VISIBLE_ITEMS + 1;
                    }
                    self.buffer_dirty = true;
                    audio.play_tick();
                }
                None
            }
            Keysym::BackSpace => {
                if !self.search_query.is_empty() {
                    self.search_query.pop();
                    self.update_filter();
                    audio.play_tick();
                }
                None
            }
            _ => {
                // Check if printable character
                let name = xkbcommon::xkb::keysym_get_name(keysym);
                if let Some(ch) = xkbcommon::xkb::keysym_to_utf8(keysym)
                    .chars()
                    .next()
                    .filter(|c| !c.is_control())
                {
                    self.search_query.push(ch);
                    self.update_filter();
                    audio.play_tick();
                } else if name.len() == 1 {
                    self.search_query.push_str(&name);
                    self.update_filter();
                    audio.play_tick();
                }
                None
            }
        }
    }

    /// Handle pointer down inside the launcher
    pub fn on_pointer_down(
        &mut self,
        pt: Point<f64, Logical>,
        screen_size: Size<i32, Logical>,
        terminal_cmd: &str,
        audio: &AudioManager,
    ) -> Option<String> {
        if self.open_progress.value() < 0.1 {
            return None;
        }

        let rect = self.launcher_rect(screen_size);
        let rel_x = pt.x as i32 - rect.loc.x;
        let rel_y = pt.y as i32 - rect.loc.y;

        // Click outside card closes launcher
        if rel_x < 0 || rel_x > rect.size.w || rel_y < 0 || rel_y > rect.size.h {
            self.close(audio);
            return None;
        }

        // Check if clicked inside application list (items start at y: 82)
        let items_start_y = 82;
        let items_end_y = items_start_y + (Self::VISIBLE_ITEMS as i32) * (Self::ITEM_HEIGHT + 4);

        if rel_y >= items_start_y && rel_y <= items_end_y {
            let row = (rel_y - items_start_y) / (Self::ITEM_HEIGHT + 4);
            let target_idx = self.scroll_offset + row as usize;
            if let Some(&app_idx) = self.filtered_indices.get(target_idx) {
                self.selected_index = target_idx;
                let app = &self.apps[app_idx];
                let exec = if app.terminal {
                    format!("{} -e {}", terminal_cmd, app.exec)
                } else {
                    app.exec.clone()
                };
                self.close(audio);
                return Some(exec);
            }
        }

        None
    }

    /// Render shader elements (card, search bar, item pills)
    pub fn render_shader_elements(
        &self,
        pipe: &MaterialShaderPipeline,
        screen_size: Size<i32, Logical>,
        colors: &M3Colors,
    ) -> Vec<PixelShaderElement> {
        let mut elements = Vec::new();
        let progress = self.open_progress.value();
        if progress < 0.01 {
            return elements;
        }

        let rect = self.launcher_rect(screen_size);

        // FRONT-TO-BACK ORDER (Topmost foreground elements first):

        // 1. Application items (Avatars & Pills)
        let search_x = rect.loc.x + 20;
        let search_y = rect.loc.y + 18;
        let search_w = rect.size.w - 40;
        let search_h = 48;
        let primary_color = M3Colors::hex_to_rgba(&colors.primary);

        let items_start_y = rect.loc.y + 82;
        let item_w = rect.size.w - 40;
        let item_h = Self::ITEM_HEIGHT;

        for (i, &app_idx) in self
            .filtered_indices
            .iter()
            .skip(self.scroll_offset)
            .take(Self::VISIBLE_ITEMS)
            .enumerate()
        {
            let current_idx = self.scroll_offset + i;
            let is_selected = current_idx == self.selected_index;
            let item_y = items_start_y + (i as i32) * (item_h + 4);
            let item_rect = Rectangle::new(Point::from((search_x, item_y)), (item_w, item_h).into());

            let app = &self.apps[app_idx];
            let (avatar_bg, _) = get_app_avatar_colors(app);

            // Left App Avatar / Initial badge (in front of item pill)
            let avatar_rect = Rectangle::new(Point::from((search_x + 10, item_y + 10)), (36, 36).into());
            let avatar_color = if is_selected {
                M3Colors::hex_to_rgba(&colors.on_primary)
            } else {
                avatar_bg
            };
            elements.push(pipe.create_pill_element(avatar_rect, 18.0, avatar_color, progress));

            // App item pill background
            if is_selected {
                elements.push(pipe.create_pill_element(item_rect, 18.0, primary_color, progress));
            } else {
                let hover_color = M3Colors::hex_to_rgba(&colors.surface_container_high);
                elements.push(pipe.create_pill_element(item_rect, 18.0, hover_color, progress * 0.7));
            }
        }

        // 2. Search icon indicator badge (in front of search bar)
        let badge_rect = Rectangle::new(Point::from((search_x + 10, search_y + 10)), (28, 28).into());
        elements.push(pipe.create_pill_element(badge_rect, 14.0, primary_color, progress * 0.9));

        // 3. Search Bar Pill
        let search_rect = Rectangle::new(Point::from((search_x, search_y)), (search_w, search_h).into());
        let search_color = M3Colors::hex_to_rgba(&colors.surface_container_high);
        elements.push(pipe.create_pill_element(search_rect, 24.0, search_color, progress));

        // 4. Card delicate border
        let border_color = M3Colors::hex_to_rgba(&colors.outline_variant);
        elements.push(pipe.create_border_element(rect, 28.0, 1.5, border_color, [0.0, 0.0, 0.0, 0.0], progress));

        // 5. Card background (SurfaceContainer)
        let card_color = M3Colors::hex_to_rgba(&colors.surface_container);
        elements.push(pipe.create_pill_element(rect, 28.0, card_color, progress));

        // 6. Card drop shadow (behind card)
        elements.push(pipe.create_shadow_element(rect, 28.0, 32.0, [0.0, 0.0, 0.0, 0.7], progress));

        // 7. Full-screen scrim backdrop (dim background behind launcher)
        let scrim_rect = Rectangle::new(Point::from((0, 0)), screen_size);
        elements.push(pipe.create_pill_element(scrim_rect, 0.0, [0.0, 0.0, 0.0, 0.55], progress));

        elements
    }

    /// Render rasterized text (Search prompt, app names, subtitles) onto a MemoryRenderBuffer
    pub fn render_text_element(
        &mut self,
        renderer: &mut GlesRenderer,
        font: &FontRenderer,
        screen_size: Size<i32, Logical>,
        colors: &M3Colors,
    ) -> Option<MemoryRenderBufferRenderElement<GlesRenderer>> {
        let progress = self.open_progress.value();
        if progress < 0.01 {
            return None;
        }

        let rect = self.launcher_rect(screen_size);
        let w = rect.size.w;
        let h = rect.size.h;

        if self.text_buffer.is_none() || self.buffer_dirty {
            let mut mem_buf = self.text_buffer.take().unwrap_or_else(|| {
                MemoryRenderBuffer::new(
                    Fourcc::Abgr8888,
                    (w, h),
                    1,
                    Transform::Normal,
                    None,
                )
            });

            let on_surface = M3Colors::hex_to_rgba_u8(&colors.on_surface);
            let on_primary = M3Colors::hex_to_rgba_u8(&colors.on_primary);
            let outline = M3Colors::hex_to_rgba_u8(&colors.outline);
            let primary = M3Colors::hex_to_rgba_u8(&colors.primary);

            let search_text = if self.search_query.is_empty() {
                "Поиск приложений и утилит..."
            } else {
                &self.search_query
            };
            let search_color = if self.search_query.is_empty() { outline } else { on_surface };
            let total_matches = self.filtered_indices.len();
            let has_query = !self.search_query.is_empty();

            let mut ctx = mem_buf.render();
            let _ = ctx.draw(|slice| {
                slice.fill(0); // clear with transparent

                // 1. Search Bar: Search icon \u{F0349} inside badge, prompt, match count
                font.draw_text(slice, w, h, "\u{F0349}", 37, 30, 16.0, on_primary);
                font.draw_text(slice, w, h, search_text, 68, 30, 15.0, search_color);
                if has_query {
                    let match_str = format!("({} найдено)", total_matches);
                    font.draw_text(slice, w, h, &match_str, 410, 31, 11.0, outline);
                }

                // 2. Draw application entries
                let items_start_y = 82;
                let item_h = Self::ITEM_HEIGHT;

                for (i, &app_idx) in self
                    .filtered_indices
                    .iter()
                    .skip(self.scroll_offset)
                    .take(Self::VISIBLE_ITEMS)
                    .enumerate()
                {
                    let current_idx = self.scroll_offset + i;
                    let is_selected = current_idx == self.selected_index;
                    let item_y = items_start_y + (i as i32) * (item_h + 4);

                    let app = &self.apps[app_idx];
                    let (_avatar_bg, avatar_fg) = get_app_avatar_colors(app);

                    // Initial letter inside avatar
                    let initial = app.name.chars().next().unwrap_or('?').to_uppercase().to_string();
                    let initial_color = if is_selected { primary } else { avatar_fg };
                    font.draw_centered_text(slice, w, h, &initial, 48, item_y + 20, 16.0, initial_color);

                    // Title
                    let title_color = if is_selected { on_primary } else { on_surface };
                    font.draw_text(slice, w, h, &app.name, 74, item_y + 16, 14.0, title_color);

                    // Subtitle / Comment
                    let sub_color = if is_selected { [on_primary[0], on_primary[1], on_primary[2], 210] } else { outline };
                    let sub_text = format_app_comment(app);
                    font.draw_text(slice, w, h, &sub_text, 74, item_y + 35, 11.0, sub_color);
                }

                Ok::<_, ()>(vec![Rectangle::from_size((w, h).into())])
            });
            drop(ctx);

            self.text_buffer = Some(mem_buf);
            self.buffer_dirty = false;
        }

        let mem_buf = self.text_buffer.as_ref()?;
        let loc = Point::from((rect.loc.x as f64, rect.loc.y as f64));
        MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            loc,
            mem_buf,
            Some(progress),
            None,
            None,
            Kind::Unspecified,
        )
        .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_exec() {
        assert_eq!(sanitize_exec("firefox-esr %u"), "firefox-esr");
        assert_eq!(sanitize_exec("kitty --class kitty %F"), "kitty --class kitty");
        assert_eq!(sanitize_exec("alacritty -e btop"), "alacritty -e btop");
    }

    #[test]
    fn test_launcher_filter() {
        let config = Config::default();
        let mut launcher = AppLauncher::new(&config);
        launcher.apps = vec![
            AppEntry {
                id: "kitty".into(),
                name: "Kitty Terminal".into(),
                exec: "kitty".into(),
                comment: "Fast, feature-rich terminal emulator".into(),
                icon: "kitty".into(),
                terminal: false,
            },
            AppEntry {
                id: "firefox".into(),
                name: "Firefox Web Browser".into(),
                exec: "firefox".into(),
                comment: "Browse the World Wide Web".into(),
                icon: "firefox".into(),
                terminal: false,
            },
            AppEntry {
                id: "btop".into(),
                name: "btop++".into(),
                exec: "btop".into(),
                comment: "Resource monitor".into(),
                icon: "btop".into(),
                terminal: true,
            },
        ];

        launcher.search_query = "kit".into();
        launcher.update_filter();
        assert_eq!(launcher.filtered_indices.len(), 1);
        assert_eq!(launcher.filtered_indices[0], 0);

        launcher.search_query = "web".into();
        launcher.update_filter();
        assert_eq!(launcher.filtered_indices.len(), 1);
        assert_eq!(launcher.filtered_indices[0], 1);

        launcher.search_query = "".into();
        launcher.update_filter();
        assert_eq!(launcher.filtered_indices.len(), 3);
    }

    #[test]
    fn test_external_launcher_detection() {
        let launchers = find_external_launchers();
        println!("Found external launchers: {:?}", launchers);
        // User system has rofi and fuzzel installed
        assert!(launchers.iter().any(|l| l.name == "rofi" || l.name == "fuzzel"));
    }
}
