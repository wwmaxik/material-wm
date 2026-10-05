pub mod backlight;
pub mod dbus;
pub mod power;

pub use backlight::BacklightManager;
pub use dbus::{DbusManager, NotificationItem};
pub use power::{BatteryInfo, PowerManager};

pub struct SystemState {
    pub power: PowerManager,
    pub backlight: BacklightManager,
    pub dbus: DbusManager,
}

impl SystemState {
    pub fn new() -> Self {
        Self {
            power: PowerManager,
            backlight: BacklightManager::new(),
            dbus: DbusManager::new(),
        }
    }

    pub fn poll_battery(&self) -> BatteryInfo {
        PowerManager::read_battery()
    }

    pub fn poll_brightness(&self) -> f32 {
        self.backlight.read_ratio()
    }

    pub fn set_brightness(&self, ratio: f32) {
        self.backlight.set_ratio(ratio);
    }
}
