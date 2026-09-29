//! Serves app icons to the UI from a custom `icon` URL scheme. The UI asks
//! for `convertFileSrc(appId, "icon")` and gets a PNG, read from Windows the
//! first time and kept in memory after that.

use std::collections::HashMap;
use std::sync::mpsc::{self, Sender};

use percent_encoding::percent_decode_str;
use tauri::http::{header, HeaderValue, Request, Response, StatusCode};
use tauri::UriSchemeResponder;

use crate::platform::{self, Com};

pub const SCHEME: &str = "icon";
/// Pixels; sharp at the UI's 32px on displays scaled up to 200%.
const SIZE: u32 = 64;

pub struct IconServer {
    requests: Sender<(String, UriSchemeResponder)>,
}

impl IconServer {
    /// Starts the thread that reads icons. All shell calls happen on that
    /// one thread, so the UI never waits on them.
    pub fn start() -> Self {
        let (requests, incoming) = mpsc::channel::<(String, UriSchemeResponder)>();
        std::thread::spawn(move || {
            let _com = Com::init();
            // Failures are cached too, so a broken icon isn't retried on every keystroke.
            let mut cache: HashMap<String, Option<Vec<u8>>> = HashMap::new();
            for (id, responder) in incoming {
                let png = cache
                    .entry(id)
                    .or_insert_with_key(|id| platform::app_icon(id, SIZE).ok());
                responder.respond(match png {
                    Some(png) => png_response(png.clone()),
                    None => not_found(),
                });
            }
        });
        Self { requests }
    }

    pub fn handle(&self, request: Request<Vec<u8>>, responder: UriSchemeResponder) {
        let path = request.uri().path().trim_start_matches('/');
        let id = percent_decode_str(path).decode_utf8_lossy().into_owned();
        let _ = self.requests.send((id, responder));
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
