//! The Rust half of the music player ("the backend").
//!
//! This file is the table of contents and the startup sequence. It declares
//! the modules (one per file in this folder) and wires them together in
//! [`run`]. For a tour of what each module is for and how they relate, see
//! `README.md` in the project root.
//!
//! Learning links:
//! - Modules and `mod` / `use`:
//!   <https://doc.rust-lang.org/book/ch07-00-managing-growing-projects-with-packages-crates-and-modules.html>
//! - Closures (the `move |...| { ... }` blocks below):
//!   <https://doc.rust-lang.org/book/ch13-01-closures.html>
//! - Tauri's Rust side: <https://tauri.app/develop/>

// Each `mod` line pulls in the file of the same name.
mod art_protocol;
mod artwork;
mod commands;
mod db;
mod device;
mod error;
mod models;
mod player;
mod queue;
mod scanner;
mod tags;
mod wav_repair;

// Test-only helpers; not compiled into the real app.
#[cfg(test)]
mod test_support;

use std::fs;
use std::sync::Arc;

use tauri::{Emitter, Manager};

use crate::artwork::ThumbCache;
use crate::commands::AppState;
use crate::db::Db;

/// Builds and runs the application. Returns only when the window is closed.
pub fn run() {
    tauri::Builder::default()
        // Native "choose a folder / choose a file" dialogs for the UI.
        .plugin(tauri_plugin_dialog::init())
        // Answer `art://...` image requests from the UI (see art_protocol.rs).
        // Reading and resizing a cover is slow-ish, so each request is
        // handled on a background thread and answered through `responder`.
        .register_asynchronous_uri_scheme_protocol("art", |context, request, responder| {
            let app = context.app_handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                responder.respond(art_protocol::handle(&app, &request));
            });
        })
        // Runs once, before the window appears.
        .setup(|app| {
            // Where the app keeps its own files. On Linux these are
            //   ~/.local/share/com.mitchell.musicplayer/   (data)
            //   ~/.cache/com.mitchell.musicplayer/         (cache)
            let data_dir = app.path().app_data_dir()?;
            let cache_dir = app.path().app_cache_dir()?;
            fs::create_dir_all(&data_dir)?;

            let db = Arc::new(Db::open(&data_dir.join("library.db"))?);
            let thumbs = ThumbCache::new(cache_dir.join("thumbnails"))?;

            // Start the player thread. Whenever its state changes it calls
            // this closure, which forwards the state to the UI as an event.
            let handle = app.handle().clone();
            let player = player::spawn_player(Arc::clone(&db), move |state| {
                let _ = handle.emit("player:state", state);
            });

            // Start the scanner thread, forwarding its reports the same way.
            let progress_handle = app.handle().clone();
            let finished_handle = app.handle().clone();
            let scanner = scanner::spawn_scanner(
                Arc::clone(&db),
                move |progress| {
                    let _ = progress_handle.emit("scan:progress", progress);
                },
                move |summary| {
                    let _ = finished_handle.emit("scan:finished", summary);
                },
            );

            // Pick up any files added, changed or deleted since last time.
            scanner.request_scan();

            // Watch for MP3 players being plugged in or pulled out. Either
            // way the UI reloads its folder list (it hides devices that are
            // not connected); a newly connected one is also scanned, since
            // its contents may have changed while it was away.
            let watcher_handle = app.handle().clone();
            let watcher_scanner = scanner.clone();
            device::spawn_watcher(Arc::clone(&db), move |change| {
                if change == device::DeviceChange::Connected {
                    watcher_scanner.request_scan();
                }
                let _ = watcher_handle.emit(commands::EVENT_LIBRARY_CHANGED, ());
            });

            // Make these available to every command (see `AppState`).
            app.manage(AppState {
                db,
                player,
                scanner,
                thumbs,
            });
            Ok(())
        })
        // The list of commands the UI is allowed to call. A command missing
        // from this list fails at runtime with "command not found".
        .invoke_handler(tauri::generate_handler![
            commands::list_tracks,
            commands::list_folders,
            commands::add_folder,
            commands::remove_folder,
            commands::rescan_library,
            commands::list_playlists,
            commands::create_playlist,
            commands::delete_playlist,
            commands::add_to_playlist,
            commands::remove_from_playlist,
            commands::get_setting,
            commands::set_setting,
            commands::play_tracks,
            commands::toggle_pause,
            commands::stop,
            commands::next_track,
            commands::previous_track,
            commands::seek,
            commands::set_volume,
            commands::set_shuffle,
            commands::get_player_state,
            commands::set_cover_art,
            commands::remove_cover_art,
            commands::save_cover_art,
            commands::set_track_tags,
            commands::copy_to_device,
        ])
        // `generate_context!` embeds tauri.conf.json and the built UI.
        .run(tauri::generate_context!())
        .expect("error while running the application");
}
