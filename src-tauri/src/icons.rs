//! Serves icons to the UI from a custom `icon` URL scheme. The UI asks
//! for `convertFileSrc(key, "icon")` and gets a PNG, read from Windows the
//! first time and kept in memory after that. The key is an app's ID, or
//! `file:<path>` or `folder:<path>`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

use grandium_core::file_index;
use percent_encoding::percent_decode_str;
use tauri::http::{header, HeaderValue, Request, Response, StatusCode};
use tauri::UriSchemeResponder;

use crate::platform::{self, Com};

pub const SCHEME: &str = "icon";
/// Pixels; sharp at the UI's 32px on displays scaled up to 200%.
const SIZE: u32 = 64;

/// Icons read so far, by shared key. Failures are kept too, so a broken
/// icon isn't retried on every keystroke.
type Cache = Arc<Mutex<HashMap<String, Option<Vec<u8>>>>>;

pub struct IconServer {
    requests: Sender<(String, UriSchemeResponder)>,
    warm: Sender<String>,
}

impl IconServer {
    /// Starts two threads that read icons, so the UI never waits on the
    /// shell: one answers the UI, the other reads icons ahead of time.
    pub fn start() -> Self {
        let cache = Cache::default();

        let (requests, incoming) = mpsc::channel::<(String, UriSchemeResponder)>();
        let serving = cache.clone();
        std::thread::spawn(move || {
            let _com = Com::init();
            for (key, responder) in incoming {
                responder.respond(match icon(&serving, &key) {
                    Some(png) => png_response(png),
                    None => not_found(),
                });
            }
        });

        let (warm, to_warm) = mpsc::channel::<String>();
        std::thread::spawn(move || {
            let _com = Com::init();
            for key in to_warm {
                icon(&cache, &key);
            }
        });

        Self { requests, warm }
    }

    pub fn handle(&self, request: Request<Vec<u8>>, responder: UriSchemeResponder) {
        let path = request.uri().path().trim_start_matches('/');
        let id = percent_decode_str(path).decode_utf8_lossy().into_owned();
        let _ = self.requests.send((id, responder));
    }

    /// Reads these icons in the background, so they're ready when shown.
    pub fn warm(&self, keys: impl IntoIterator<Item = String>) {
        for key in keys {
            let _ = self.warm.send(key);
        }
    }
}

/// The icon for `key`, from the cache or read now.
fn icon(cache: &Cache, key: &str) -> Option<Vec<u8>> {
    let (shared, source) = source(key);
    if let Some(known) = cache.lock().unwrap().get(&shared) {
        return known.clone();
    }
    // Read without holding the lock: the other thread may need it.
    let png = match source {
        Source::App(id) => platform::app_icon(id, SIZE),
        Source::File(path) => platform::file_icon(path, SIZE),
    }
    .ok();
    cache.lock().unwrap().insert(shared, png.clone());
    png
}

enum Source<'a> {
    App(&'a str),
    File(&'a str),
}

/// What `key` asks for, and the key its icon is kept under: files of the
/// same type share one icon.
fn source(key: &str) -> (String, Source<'_>) {
    let file = |path, is_dir| {
        let shared = file_index::icon_group(Path::new(path), is_dir);
        (
            shared.unwrap_or_else(|| key.to_string()),
            Source::File(path),
        )
    };
    if let Some(path) = key.strip_prefix("file:") {
        file(path, false)
    } else if let Some(path) = key.strip_prefix("folder:") {
        file(path, true)
    } else {
        (key.to_string(), Source::App(key))
    }
}

pub fn png_response(png: Vec<u8>) -> Response<Vec<u8>> {
    let mut response = Response::new(png);
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("max-age=3600"),
    );
    response
}

pub fn not_found() -> Response<Vec<u8>> {
    let mut response = Response::new(Vec::new());
    *response.status_mut() = StatusCode::NOT_FOUND;
    response
}
