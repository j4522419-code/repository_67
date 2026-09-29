//! The tray icon: opening Grandium without the keyboard, settings, choosing
//! which keys open it, pausing clipboard history, and quitting.

use grandium_core::settings::OpenWith;
use tauri::menu::{
    CheckMenuItem, CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, SubmenuBuilder,
};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::settings::SettingsStore;
use crate::{launcher, setup};

const TRAY_ID: &str = "grandium";

/// Menu items that change from elsewhere too.
struct TrayItems {
    open_with: Vec<(OpenWith, CheckMenuItem<tauri::Wry>)>,
    pause_clipboard: CheckMenuItem<tauri::Wry>,
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let current = app.state::<SettingsStore>().get().open_with;
    let choices: Vec<(OpenWith, CheckMenuItem<_>)> = OpenWith::ALL
        .iter()
        .map(|&option| {
            CheckMenuItemBuilder::with_id(format!("open-with:{}", option.label()), option.label())
                .checked(option == current)
                .build(app)
                .map(|item| (option, item))
        })
        .collect::<tauri::Result<_>>()?;
    let mut open_with = SubmenuBuilder::new(app, "Open Grandium with");
    for (_, item) in &choices {
        open_with = open_with.item(item);
    }

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
        .item(&open_with.build()?)
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
            id => {
                let choice = OpenWith::ALL
                    .into_iter()
                    .find(|option| id == format!("open-with:{}", option.label()));
                if let Some(choice) = choice {
                    let settings = app
                        .state::<SettingsStore>()
                        .update(|settings| settings.open_with = choice);
                    launcher::apply_open_with(app, settings.open_with);
                    sync_open_with(app, choice);
                }
            }
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
    app.manage(TrayItems {
        open_with: choices,
        pause_clipboard,
    });
    Ok(())
}

/// Keeps the tray's check mark in step when pausing from the launcher.
pub fn sync_clipboard_paused(app: &AppHandle, paused: bool) {
    if let Some(items) = app.try_state::<TrayItems>() {
        let _ = items.pause_clipboard.set_checked(paused);
    }
}

/// Checks the chosen "Open Grandium with" item (clicking a checked item
/// unchecks it, so every item is set) and updates the tooltip.
pub fn sync_open_with(app: &AppHandle, choice: OpenWith) {
    if let Some(items) = app.try_state::<TrayItems>() {
        for (option, item) in &items.open_with {
            let _ = item.set_checked(*option == choice);
        }
    }
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(tooltip(app)));
    }
}

fn tooltip(app: &AppHandle) -> String {
    let keys = launcher::status(app).keys;
    if keys.is_empty() {
        "Grandium (click to open)".to_string()
    } else {
        format!("Grandium ({})", keys.join(" or "))
    }
}
