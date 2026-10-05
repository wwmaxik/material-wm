pub mod backend;
pub mod config;
pub mod shell;
pub mod state;
pub mod system;
pub mod ui;

use backend::BackendType;
use config::Config;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize structured logging
    if let Ok(filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter("material_wm=info,smithay=info")
            .init();
    }

    println!("=======================================================");
    println!("   material-wm: Monolithic Material You Wayland DE     ");
    println!("=======================================================");

    let mut backend_type = BackendType::Auto;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--winit" => backend_type = BackendType::Winit,
            "--drm" => backend_type = BackendType::Drm,
            "--help" | "-h" => {
                println!("Usage: material-wm [OPTIONS]");
                println!();
                println!("Options:");
                println!("  --winit       Force nested windowed mode (Winit backend)");
                println!("  --drm         Force bare-metal KMS/DRM/Libinput mode");
                println!("  -h, --help    Show this help message");
                return Ok(());
            }
            other => {
                eprintln!("Unknown argument: {}", other);
            }
        }
    }

    // Load or generate default M3 configuration
    let config = Config::load();

    // Start desktop environment
    backend::run(backend_type, config)?;

    Ok(())
}
