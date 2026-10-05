pub mod drm;
pub mod winit;

use crate::config::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendType {
    Auto,
    Winit,
    Drm,
}

pub fn run(backend_type: BackendType, config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let selected = match backend_type {
        BackendType::Winit => BackendType::Winit,
        BackendType::Drm => BackendType::Drm,
        BackendType::Auto => {
            if std::env::var("WAYLAND_DISPLAY").is_ok() || std::env::var("DISPLAY").is_ok() {
                tracing::info!("Host display server detected. Running in Winit windowed mode.");
                BackendType::Winit
            } else {
                tracing::info!("No host display detected. Running bare-metal DRM/KMS mode.");
                BackendType::Drm
            }
        }
    };

    match selected {
        BackendType::Winit => winit::run_winit(config),
        BackendType::Drm => drm::run_drm(config),
        BackendType::Auto => unreachable!(),
    }
}
