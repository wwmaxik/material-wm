# material-wm

<div align="center">
  <h3>Monolithic Material You (crDroid-style) Wayland Desktop Environment</h3>
  <p>Written entirely in Rust using the modern <a href="https://github.com/Smithay/smithay">Smithay</a> framework.</p>
</div>

---

## 🌟 Highlights

- **Aesthetic Fusion:** Material You (Android 13/14 crDroid ROM) visual language combined with Apple HIG human-centric interaction, fluid interruptible animations, and acoustic touch feedback.
- **Self-Contained & Monolithic:** No external status bars (waybar), app launchers (rofi/fuzzel), or notification daemons needed. Everything is integrated into a single high-performance binary.
- **Strict Damage Tracking:** Only redraws damaged rects via `smithay::backend::renderer::damage::OutputDamageTracker`. Zero wasted GPU cycles.
- **Hardware Optimized:** Tailored for low-power Intel Braswell architecture (Pentium N3700, Intel HD Gen8 LP) with zero AVX requirements (`-C target-cpu=silvermont`).
- **crDroid Quick Settings Drawer:** 2-column large pill cards, thick capsule sliders for Brightness & Volume with embedded icons, live date/time, battery status, and power actions.
- **Native Top Bar:** Dynamic workspace capsules with expansion physics, active window title centering, and status quick chip.
- **Built-in App Launcher:** Instant desktop entry indexing (`/usr/share/applications`, `~/.local/share/applications`), fuzzy search, harmonic M3 pastel avatar badges, and clean category mapping.
- **Desktop Wallpaper:** Native high-quality background scaling with procedural Material You dark gradient fallback.
- **Tiling & Floating:** Flexible Master-Stack layout with inner/outer gaps, dynamic ratio adjustment, floating window toggle, and interactive Super+drag / Super+resize.
- **Multi-backend:** Seamless Winit backend for windowed testing inside existing sessions (KDE/GNOME/X11), and bare-metal KMS/DRM + Libinput backend for direct TTY execution.

---

## ⌨️ Default Keybindings

| Keybinding | Action |
|---|---|
| `Super + Return` | Launch Terminal (`kitty` / configured terminal) |
| `Super + d` / `Super + Space` | Toggle Material You App Launcher |
| `Super + s` | Toggle crDroid Quick Settings Drawer |
| `Super + q` | Close Focused Window |
| `Super + Shift + Space` | Toggle Window Floating / Tiling Mode |
| `Super + 1..9` | Switch to Workspace 1–9 |
| `Super + Shift + 1..9` | Move Focused Window to Workspace 1–9 |
| `Super + j` / `Super + k` | Focus Next / Previous Window |
| `Super + h` / `Super + l` | Adjust Master Split Ratio |
| `Super + Left Click + Drag` | Move Floating Window |
| `Super + Right Click + Drag`| Resize Floating Window |
| `Super + Shift + q` | Quit `material-wm` |

---

## 🚀 Building & Running

### Requirements
- Rust stable (edition 2021)
- `libwayland-dev`, `libxkbcommon-dev`, `libgbm-dev`, `libinput-dev`, `libudev-dev`, `libasound2-dev`

### Compilation
```bash
# Build release binary (uses -C target-cpu=silvermont)
cargo build --release
```

### Running inside existing desktop (Windowed / Testing)
```bash
cargo run --release
```

### Running on Bare Metal (DRM/KMS from TTY)
```bash
./target/release/material-wm --backend drm
```

---

## 🎨 Configuration

Configuration is loaded from `~/.config/material-wm/config.toml` (or fallback `./config.toml`).

```toml
terminal = "kitty"
bar_height = 32
inner_gap = 8
outer_gap = 12
master_ratio = 0.55
wallpaper_path = "~/Изображения/wallpapers/mashina4k.jpg"

[theme]
primary = "#D0BCFF"
on_primary = "#381E72"
surface = "#141218"
surface_container = "#211F26"
surface_container_high = "#2B2930"
outline = "#938F99"
outline_variant = "#49454F"
error = "#F2B8B5"
```

---

## 📜 License
MIT License
