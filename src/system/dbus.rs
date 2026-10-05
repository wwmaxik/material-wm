use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use zbus::blocking::connection::Builder;
use zbus::blocking::Connection;
use zbus::interface;

#[derive(Debug, Clone)]
pub struct NotificationItem {
    pub id: u32,
    pub app_name: String,
    pub summary: String,
    pub body: String,
    pub icon: String,
    pub timestamp: Instant,
    pub duration_secs: u32,
}

pub struct NotificationDaemon {
    notifications: Arc<Mutex<Vec<NotificationItem>>>,
    next_id: Arc<Mutex<u32>>,
}

#[interface(name = "org.freedesktop.Notifications")]
impl NotificationDaemon {
    fn notify(
        &mut self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        _actions: Vec<String>,
        _hints: HashMap<String, zbus::zvariant::Value<'_>>,
        expire_timeout: i32,
    ) -> u32 {
        let mut id_guard = self.next_id.lock().unwrap();
        let id = if replaces_id != 0 {
            replaces_id
        } else {
            *id_guard += 1;
            *id_guard
        };

        let duration = if expire_timeout > 0 {
            (expire_timeout / 1000) as u32
        } else {
            5
        };

        let item = NotificationItem {
            id,
            app_name,
            summary,
            body,
            icon: app_icon,
            timestamp: Instant::now(),
            duration_secs: duration,
        };

        let mut notifs = self.notifications.lock().unwrap();
        if let Some(pos) = notifs.iter().position(|n| n.id == id) {
            notifs[pos] = item;
        } else {
            notifs.push(item);
        }

        id
    }

    fn close_notification(&mut self, id: u32) {
        let mut notifs = self.notifications.lock().unwrap();
        notifs.retain(|n| n.id != id);
    }

    fn get_capabilities(&self) -> Vec<&str> {
        vec!["body", "actions", "icon-static"]
    }

    fn get_server_information(&self) -> (&str, &str, &str, &str) {
        ("material-wm", "Material-WM Project", "0.1.0", "1.2")
    }
}

pub struct DbusManager {
    pub notifications: Arc<Mutex<Vec<NotificationItem>>>,
    pub wifi_ssid: Arc<Mutex<String>>,
    pub wifi_enabled: Arc<Mutex<bool>>,
    pub bt_enabled: Arc<Mutex<bool>>,
}

impl DbusManager {
    pub fn new() -> Self {
        let notifications = Arc::new(Mutex::new(Vec::new()));
        let next_id = Arc::new(Mutex::new(1u32));
        let wifi_ssid = Arc::new(Mutex::new("Wi-Fi".to_string()));
        let wifi_enabled = Arc::new(Mutex::new(true));
        let bt_enabled = Arc::new(Mutex::new(false));

        // Start session bus notification server thread
        {
            let notifs_clone = notifications.clone();
            let next_id_clone = next_id.clone();
            thread::spawn(move || {
                let daemon = NotificationDaemon {
                    notifications: notifs_clone,
                    next_id: next_id_clone,
                };

                match Builder::session() {
                    Ok(builder) => match builder.name("org.freedesktop.Notifications") {
                        Ok(builder) => match builder.serve_at("/org/freedesktop/Notifications", daemon) {
                            Ok(builder) => match builder.build() {
                                Ok(_conn) => {
                                    tracing::info!("Registered internal org.freedesktop.Notifications server");
                                    loop {
                                        thread::park();
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to build Notifications D-Bus connection: {}", e);
                                }
                            },
                            Err(e) => {
                                tracing::warn!("Failed to serve Notifications D-Bus interface: {}", e);
                            }
                        },
                        Err(e) => {
                            tracing::warn!("Failed to request org.freedesktop.Notifications name: {}", e);
                        }
                    },
                    Err(e) => {
                        tracing::warn!("Failed to connect to D-Bus session bus: {}", e);
                    }
                }
            });
        }

        // Query system bus for NetworkManager and BlueZ in background thread
        {
            let _wifi_ssid_clone = wifi_ssid.clone();
            let wifi_enabled_clone = wifi_enabled.clone();
            let bt_enabled_clone = bt_enabled.clone();

            thread::spawn(move || {
                if let Ok(conn) = Connection::system() {
                    // Check NetworkManager WirelessEnabled
                    if let Ok(reply) = conn.call_method(
                        Some("org.freedesktop.NetworkManager"),
                        "/org/freedesktop/NetworkManager",
                        Some("org.freedesktop.DBus.Properties"),
                        "Get",
                        &("org.freedesktop.NetworkManager", "WirelessEnabled"),
                    ) {
                        if let Ok(val) = reply.body().deserialize::<zbus::zvariant::Value>() {
                            if let zbus::zvariant::Value::Bool(b) = val {
                                *wifi_enabled_clone.lock().unwrap() = b;
                            }
                        }
                    }

                    // Check BlueZ Bluetooth adapter
                    if let Ok(reply) = conn.call_method(
                        Some("org.bluez"),
                        "/org/bluez/hci0",
                        Some("org.freedesktop.DBus.Properties"),
                        "Get",
                        &("org.bluez.Adapter1", "Powered"),
                    ) {
                        if let Ok(val) = reply.body().deserialize::<zbus::zvariant::Value>() {
                            if let zbus::zvariant::Value::Bool(b) = val {
                                *bt_enabled_clone.lock().unwrap() = b;
                            }
                        }
                    }
                }
            });
        }

        Self {
            notifications,
            wifi_ssid,
            wifi_enabled,
            bt_enabled,
        }
    }

    /// Retrieve active notifications, filtering out expired ones
    pub fn active_notifications(&self) -> Vec<NotificationItem> {
        let mut notifs = self.notifications.lock().unwrap();
        let now = Instant::now();
        notifs.retain(|n| now.duration_since(n.timestamp).as_secs() < n.duration_secs as u64);
        notifs.clone()
    }
}
