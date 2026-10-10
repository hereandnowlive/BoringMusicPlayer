//! Serves cover art to the UI through a custom URL scheme.
//!
//! The UI shows a cover with a plain `<img>` tag whose address looks like
//!
//! ```text
//! art://localhost/42?size=96&v=1700000000000
//!                 ^^      ^^   ^^^^^^^^^^^^^
//!                 |       |    the file's modification time. Not read here;
//!                 |       |    it only makes the URL change when the file
//!                 |       |    does, so the webview never shows a stale cover.
//!                 |       thumbnail size in pixels (leave out for the original)
//!                 track id
//! ```
//!
//! The webview asks the app for that address, `lib.rs` routes the request to
//! [`handle`], and we answer with image bytes just like a web server would.
//! This avoids pushing large images through the JSON command channel.
//!
//! (On Windows the same thing is spelled `http://art.localhost/42`. The
//! `convertFileSrc` helper used in `src/api.ts` picks the right form.)
//!
//! Learning link:
//! - Tauri custom protocols:
//!   <https://docs.rs/tauri/latest/tauri/struct.Builder.html#method.register_asynchronous_uri_scheme_protocol>

use std::path::Path;

use tauri::http::{header, Request, Response, StatusCode};
use tauri::{AppHandle, Manager};

use crate::commands::AppState;
use crate::error::{AppError, AppResult};
use crate::tags::{self, CoverArt};

/// Smallest and largest thumbnail the UI may ask for, in pixels.
const MIN_THUMBNAIL: u32 = 16;
const MAX_THUMBNAIL: u32 = 1024;

/// Answers one `art://` request.
pub fn handle(app: &AppHandle, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    match find_cover(app, request) {
        Ok(Some(cover)) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, cover.mime)
            // Safe to cache forever: a changed file gets a different URL.
            .header(header::CACHE_CONTROL, "max-age=31536000, immutable")
            .body(cover.data),
        Ok(None) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new()),
        Err(error) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(error.to_string().into_bytes()),
    }
    // Building a response only fails for malformed headers, and ours are
    // fixed strings, so this cannot happen in practice.
    .expect("response headers are valid")
}

/// Works out which picture a request is asking for and loads it.
/// `Ok(None)` means "that track has no cover".
fn find_cover(app: &AppHandle, request: &Request<Vec<u8>>) -> AppResult<Option<CoverArt>> {
    let uri = request.uri();

    // The path is "/42"; strip the slash and read the number.
    let track_id: i64 = uri
        .path()
        .trim_start_matches('/')
        .parse()
        .map_err(|_| AppError::Other(format!("Bad artwork address: {uri}")))?;

    let state = app.state::<AppState>();
    let Some(track) = state.db.get_track(track_id)? else {
        return Ok(None);
    };

    match requested_size(uri.query()) {
        // A thumbnail: resized, cached on disk, always JPEG.
        Some(size) => {
            let size = size.clamp(MIN_THUMBNAIL, MAX_THUMBNAIL);
            let jpeg = state.thumbs.thumbnail(&track, size)?;
            Ok(jpeg.map(|data| CoverArt {
                data,
                mime: "image/jpeg".into(),
            }))
        }
        // The original picture, exactly as embedded in the file.
        None => tags::read_cover(Path::new(&track.path)),
    }
}

/// Pulls the `size` value out of a query string such as `size=96&v=123`.
fn requested_size(query: Option<&str>) -> Option<u32> {
    query?
        .split('&')
        .find_map(|pair| pair.strip_prefix("size="))?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_is_read_from_the_query_string() {
        assert_eq!(requested_size(Some("size=96&v=5")), Some(96));
        assert_eq!(requested_size(Some("v=5&size=200")), Some(200));
        assert_eq!(requested_size(Some("v=5")), None);
        assert_eq!(requested_size(Some("size=big")), None);
        assert_eq!(requested_size(None), None);
    }
}
