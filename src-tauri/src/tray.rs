//! The tray icon: opening Grandium without the keyboard, settings, pausing
//! clipboard history, and quitting.

use grandium_core::DEFAULT_HOTKEY;
use tauri::menu::{CheckMenuItem, CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::settings::SettingsStore;
use crate::{launcher, setup};

const TRAY_ID: &str = "grandium";

/// Menu items that change from elsewhere too.
struct TrayItems {
    pause_clipboard: CheckMenuItem<tauri::Wry>,
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let settings = app.state::<SettingsStore>().get();
    let pause_clipboard =
        CheckMenuItemBuilder::with_id("pause-clipboard", "Pause clipboard history")
            .checked(settings.clipboard_paused)
            .build(app)?;
    let open = MenuItemBuilder::with_id("open", "Open Grandium").build(app)?;
    let settings_item = MenuItemBuilder::with_id("settings", "Settings…").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit Grandium").build(app)?;
    let menu = MenuBuilder::new(app)
        .item(&open)
        .item(&settings_item)
        .item(&pause_clipboard)
        .separator()
        .item(&quit)
        .build()?;

    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(tooltip(app))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => launcher::show(app),
            "settings" => setup::open(app),
            "quit" => app.exit(0),
            "pause-clipboard" => {
                let settings = app
                    .state::<SettingsStore>()
                    .update(|settings| settings.clipboard_paused = !settings.clipboard_paused);
                sync_clipboard_paused(app, settings.clipboard_paused);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                launcher::show(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    app.manage(TrayItems { pause_clipboard });
    Ok(())
}

/// Keeps the tray's check mark in step when pausing from the launcher.
pub fn sync_clipboard_paused(app: &AppHandle, paused: bool) {
    if let Some(items) = app.try_state::<TrayItems>() {
        let _ = items.pause_clipboard.set_checked(paused);
    }
}

fn tooltip(app: &AppHandle) -> String {
    if launcher::status(app).hotkey_error.is_some() {
        "Grandium (click to open)".to_string()
    } else {
        format!("Grandium ({DEFAULT_HOTKEY})")
    }
}
