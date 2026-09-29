// Release builds are GUI-only: no console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(windows))]
compile_error!("Grandium is a Windows app.");

mod apps;
mod clipboard;
mod file_search;
mod files;
mod icons;
mod launcher;
mod notes;
mod platform;
mod search;
mod settings;
mod setup;
mod snippets;
mod tray;

use std::sync::Arc;

use tauri::Manager;

fn main() {
    let icon_server = Arc::new(icons::IconServer::start());
    let serving_icons = icon_server.clone();

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
            move |_ctx, request, responder| serving_icons.handle(request, responder),
        )
        .register_asynchronous_uri_scheme_protocol(clipboard::SCHEME, |ctx, request, responder| {
            clipboard::serve_picture(ctx.app_handle(), request, responder)
        })
        .manage(icon_server)
        .manage(apps::AppIndex::default())
        .manage(file_search::FileSearch::default())
        .invoke_handler(tauri::generate_handler![
            launcher::app_status,
            launcher::hide_launcher,
            launcher::set_launcher_height,
            search::search,
            search::run_action,
            setup::setup_options,
            setup::save_setup,
            setup::pick_folder,
            snippets::snippet_draft,
            snippets::save_snippet,
        ])
        .on_window_event(launcher::on_window_event)
        .setup(|app| {
            let handle = app.handle();
            let data = files::data_dir(handle);
            let file = |name: &str| data.as_ref().map(|dir| dir.join(name));
            app.manage(settings::SettingsStore::load(file("settings.json")));
            let settings = app.state::<settings::SettingsStore>().get();
            app.manage(search::SearchState::load(file("usage.json")));
            app.manage(clipboard::ClipboardStore::load(
                file("clipboard"),
                settings.clipboard_limits(),
            ));
            app.manage(snippets::SnippetStore::load(file("snippets.json")));
            app.manage(notes::NoteStore::new(notes::notes_dir(handle)));
            app.manage(launcher::LauncherState::default());
            launcher::prepare_window(handle);

            // Problems here (another app owning the hotkey, say) show up in
            // the launcher; Grandium still starts.
            let _ = setup::apply(handle, None, &settings);
            tray::create(handle)?;
            clipboard::start_recording(handle);
            file_search::start(handle, &settings.extra_folders);
            // Show the launcher once at startup so it's clear Grandium is
            // running, unless Windows started it at sign-in.
            let at_sign_in = std::env::args().any(|arg| arg == platform::startup::BACKGROUND_ARG);
            if !at_sign_in || launcher::status(handle).setup_needed {
                launcher::show(handle);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to start Grandium");
}
