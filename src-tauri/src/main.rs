// Release builds are GUI-only: no console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(windows))]
compile_error!("Grandium is a Windows app.");

mod apps;
mod files;
mod icons;
mod launcher;
mod platform;
mod search;
mod settings;
mod tray;

use tauri::Manager;

fn main() {
    let icon_server = icons::IconServer::start();

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
        .register_asynchronous_uri_scheme_protocol(
            icons::SCHEME,
            move |_ctx, request, responder| icon_server.handle(request, responder),
        )
        .manage(apps::AppIndex::default())
        .invoke_handler(tauri::generate_handler![
            launcher::app_status,
            launcher::hide_launcher,
            launcher::set_launcher_height,
            search::search,
            search::run_action,
        ])
        .on_window_event(launcher::on_window_event)
        .setup(|app| {
            let handle = app.handle();
            let data = files::data_dir(handle);
            let file = |name: &str| data.as_ref().map(|dir| dir.join(name));
            app.manage(settings::SettingsStore::load(file("settings.json")));
            app.manage(search::SearchState::load(file("usage.json")));
            app.manage(launcher::LauncherState::new(handle));

            let settings = app.state::<settings::SettingsStore>().get();
            launcher::apply_open_with(handle, settings.open_with);
            tray::create(handle)?;
            // Show the launcher once at startup so it's clear Grandium is running.
            launcher::show(handle);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to start Grandium");
}
