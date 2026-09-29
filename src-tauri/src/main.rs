// Release builds are GUI-only: no console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod launcher;
mod tray;

fn main() {
    tauri::Builder::default()
        // Must be the first plugin: starting Grandium again just opens the
        // launcher of the copy that's already running.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            launcher::show(app)
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(launcher::on_hotkey)
                .build(),
        )
        .manage(launcher::LauncherState::default())
        .invoke_handler(tauri::generate_handler![
            launcher::app_status,
            launcher::hide_launcher,
            launcher::set_launcher_height,
        ])
        .on_window_event(launcher::on_window_event)
        .setup(|app| {
            let app = app.handle();
            launcher::register_hotkey(app);
            tray::create(app)?;
            // Show the launcher once at startup so it's clear Grandium is running.
            launcher::show(app);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to start Grandium");
}
