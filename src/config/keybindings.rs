use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    SpawnTerminal,
    KillActive,
    ToggleFloating,
    ToggleDrawer,
    ToggleBar,
    ToggleLauncher,
    SpawnExternalLauncher,
    SwitchWorkspace(usize),
    MoveToWorkspace(usize),
    FocusNext,
    FocusPrev,
    SwapMaster,
    IncreaseMasterRatio,
    DecreaseMasterRatio,
    BrightnessUp,
    BrightnessDown,
    VolumeUp,
    VolumeDown,
    VolumeMute,
    Quit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBinding {
    pub modifiers: Vec<String>,
    pub key: String,
    pub action: Action,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyConfig {
    pub bindings: Vec<KeyBinding>,
}

impl Default for KeyConfig {
    fn default() -> Self {
        let mut bindings = Vec::new();

        // Terminal: Super + Return
        bindings.push(KeyBinding {
            modifiers: vec!["Super".into()],
            key: "Return".into(),
            action: Action::SpawnTerminal,
        });

        // Kill window: Super + Q
        bindings.push(KeyBinding {
            modifiers: vec!["Super".into()],
            key: "q".into(),
            action: Action::KillActive,
        });

        // Application Launcher: Super + d and Super + Space
        bindings.push(KeyBinding {
            modifiers: vec!["Super".into()],
            key: "d".into(),
            action: Action::ToggleLauncher,
        });
        bindings.push(KeyBinding {
            modifiers: vec!["Super".into()],
            key: "space".into(),
            action: Action::ToggleLauncher,
        });

        // Quick Settings Drawer: Super + s
        bindings.push(KeyBinding {
            modifiers: vec!["Super".into()],
            key: "s".into(),
            action: Action::ToggleDrawer,
        });

        // External Launcher (rofi/fuzzel/wofi): Super + Shift + d
        bindings.push(KeyBinding {
            modifiers: vec!["Super".into(), "Shift".into()],
            key: "d".into(),
            action: Action::SpawnExternalLauncher,
        });

        // Toggle Floating: Super + Shift + Space
        bindings.push(KeyBinding {
            modifiers: vec!["Super".into(), "Shift".into()],
            key: "space".into(),
            action: Action::ToggleFloating,
        });

        // Focus navigation: Super + j / k
        bindings.push(KeyBinding {
            modifiers: vec!["Super".into()],
            key: "j".into(),
            action: Action::FocusNext,
        });
        bindings.push(KeyBinding {
            modifiers: vec!["Super".into()],
            key: "k".into(),
            action: Action::FocusPrev,
        });

        // Swap with master: Super + Shift + Return
        bindings.push(KeyBinding {
            modifiers: vec!["Super".into(), "Shift".into()],
            key: "Return".into(),
            action: Action::SwapMaster,
        });

        // Split ratio: Super + h / l
        bindings.push(KeyBinding {
            modifiers: vec!["Super".into()],
            key: "h".into(),
            action: Action::DecreaseMasterRatio,
        });
        bindings.push(KeyBinding {
            modifiers: vec!["Super".into()],
            key: "l".into(),
            action: Action::IncreaseMasterRatio,
        });

        // Workspaces 1..9
        for i in 1..=9 {
            bindings.push(KeyBinding {
                modifiers: vec!["Super".into()],
                key: i.to_string(),
                action: Action::SwitchWorkspace(i),
            });
            bindings.push(KeyBinding {
                modifiers: vec!["Super".into(), "Shift".into()],
                key: i.to_string(),
                action: Action::MoveToWorkspace(i),
            });
        }

        // Multimedia keys
        bindings.push(KeyBinding {
            modifiers: vec![],
            key: "XF86MonBrightnessUp".into(),
            action: Action::BrightnessUp,
        });
        bindings.push(KeyBinding {
            modifiers: vec![],
            key: "XF86MonBrightnessDown".into(),
            action: Action::BrightnessDown,
        });
        bindings.push(KeyBinding {
            modifiers: vec![],
            key: "XF86AudioRaiseVolume".into(),
            action: Action::VolumeUp,
        });
        bindings.push(KeyBinding {
            modifiers: vec![],
            key: "XF86AudioLowerVolume".into(),
            action: Action::VolumeDown,
        });
        bindings.push(KeyBinding {
            modifiers: vec![],
            key: "XF86AudioMute".into(),
            action: Action::VolumeMute,
        });

        // Quit: Super + Shift + Q
        bindings.push(KeyBinding {
            modifiers: vec!["Super".into(), "Shift".into()],
            key: "Q".into(),
            action: Action::Quit,
        });

        Self { bindings }
    }
}
