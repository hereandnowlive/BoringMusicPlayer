//! The functions the UI can call.
//!
//! Each `#[tauri::command]` function here can be invoked from TypeScript by
//! name: `invoke("add_folder", { path })` in `src/api.ts` runs
//! [`add_folder`] below. Arguments arrive as JSON and are converted for us
//! (TypeScript's `trackIds` becomes Rust's `track_ids`); the return value is
//! converted back to JSON. An `Err` makes the TypeScript promise reject.
//!
//! These functions are deliberately thin. They check arguments, call into
//! the modules that do the real work, and report the outcome. If you are
//! looking for *how* something works, follow the call into `db.rs`,
//! `player.rs`, `scanner.rs`, `tags.rs` or `artwork.rs`.
//!
//! **Adding a command:** write the function here, add its name to the
//! `generate_handler!` list in `lib.rs`, and add a typed wrapper for it in
//! `src/api.ts`.
//!
//! A note on `#[tauri::command(async)]`: plain commands run on the main
//! (window) thread, which is fine when they finish instantly. Anything slow
//! (reading thousands of rows, rewriting audio files) is marked `(async)` so
//! Tauri runs it on a background thread and the window stays responsive.
//!
//! Learning links:
//! - Tauri commands: <https://tauri.app/develop/calling-rust/>
//! - Tauri state (`State<AppState>`): <https://tauri.app/develop/state-management/>
//! - Events sent to the UI: <https://tauri.app/develop/calling-frontend/>

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::artwork::{self, ThumbCache};
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::device;
use crate::models::{
    ArtUpdateResult, CopyResult, Folder, PlayerState, Playlist, PlaylistAddResult, Track,
};
use crate::player::{Command, PlayerHandle};
use crate::scanner::{self, ScanHandle};
use crate::tags::{self, TextEdit};

/// Name of the event that tells the UI to reload the track and folder lists.
pub const EVENT_LIBRARY_CHANGED: &str = "library:changed";

/// The long-lived objects every command needs. One instance is created at
/// startup in `lib.rs` and Tauri hands a reference to any command that asks
/// for `State<AppState>`.
pub struct AppState {
    /// `Arc` because the player and scanner threads hold the database too.
    pub db: Arc<Db>,
    pub player: PlayerHandle,
    pub scanner: ScanHandle,
    pub thumbs: ThumbCache,
}

// ===================================================================== library

/// Every track in the library.
#[tauri::command(async)]
pub fn list_tracks(state: State<'_, AppState>) -> AppResult<Vec<Track>> {
    state.db.list_tracks()
}

/// The folders that make up the library.
#[tauri::command]
pub fn list_folders(state: State<'_, AppState>) -> AppResult<Vec<Folder>> {
    let folders = state.db.list_folders()?;
    // Fill in whether each folder can be reached right now (an unplugged
    // device cannot).
    Ok(folders.into_iter().map(Folder::check_connected).collect())
}

/// Adds a folder to the library and starts scanning it in the background.
/// `is_device` marks it as a portable music player that songs can be copied
/// onto.
#[tauri::command]
pub fn add_folder(
    state: State<'_, AppState>,
    path: String,
    is_device: bool,
) -> AppResult<Folder> {
    // `canonicalize` resolves symlinks and `..`, giving one true spelling of
    // the path, and fails if the path does not exist.
    let folder = fs::canonicalize(&path)
        .map_err(|_| AppError::Other(format!("Folder not found: {path}")))?;
    if !folder.is_dir() {
        return Err(AppError::Other(format!("Not a folder: {path}")));
    }

    // If this exact folder is already in the library there is nothing to
    // do; if it overlaps one, `find_existing` returns an error.
    if let Some(existing) = find_existing(&folder, &state.db.list_folders()?)? {
        return Ok(existing.check_connected());
    }

    let folder_text = folder
        .to_str()
        .ok_or_else(|| AppError::Other("That folder's name is not valid text".into()))?;
    let added = state.db.add_folder(folder_text, is_device)?;
    state.scanner.request_scan();
    Ok(added.check_connected())
}

/// Checks a folder about to be added against the folders already in the
/// library.
///
/// - Returns `Ok(Some(folder))` if it is already there (same path).
/// - Returns an error if it is inside, or contains, a library folder. If
///   both /Music and /Music/Rock were in the library, each scan would claim
///   the files in Rock for itself and the two would keep undoing each
///   other's work.
/// - Returns `Ok(None)` if it is new and overlaps nothing.
///
/// `Path::starts_with` compares whole path components, so `/Music2` does
/// not count as being inside `/Music`.
fn find_existing(folder: &Path, library: &[Folder]) -> AppResult<Option<Folder>> {
    for existing in library {
        let existing_path = Path::new(&existing.path);
        if folder == existing_path {
            return Ok(Some(existing.clone()));
        }
        if folder.starts_with(existing_path) {
            return Err(AppError::Other(format!(
                "That folder is already covered by {}",
                existing.path
            )));
        }
        if existing_path.starts_with(folder) {
            return Err(AppError::Other(format!(
                "That folder contains {}, which is already in the library. \
                 Remove that one first.",
                existing.path
            )));
        }
    }
    Ok(None)
}

/// Removes a folder and its tracks from the library. Nothing on disk is
/// deleted.
#[tauri::command]
pub fn remove_folder(app: AppHandle, state: State<'_, AppState>, folder_id: i64) -> AppResult<()> {
    state.db.remove_folder(folder_id)?;
    let _ = app.emit(EVENT_LIBRARY_CHANGED, ());
    Ok(())
}

/// Starts a background scan of every library folder. Progress and results
/// arrive as `scan:progress` and `scan:finished` events.
#[tauri::command]
pub fn rescan_library(state: State<'_, AppState>) {
    state.scanner.request_scan();
}

// ===================================================================== devices

/// Name of the event that reports progress while songs are being copied.
pub const EVENT_COPY_PROGRESS: &str = "copy:progress";

/// Copies the given tracks onto a device (a library folder added as an MP3
/// player), then rescans so they show up in its list. The copying itself
/// is in `device.rs`.
///
/// Tracks that already live on that device are left out, so dragging a
/// song from the device's own list onto it does nothing.
#[tauri::command(async)]
pub fn copy_to_device(
    app: AppHandle,
    state: State<'_, AppState>,
    folder_id: i64,
    track_ids: Vec<i64>,
) -> AppResult<CopyResult> {
    let folder = state
        .db
        .get_folder(folder_id)?
        .ok_or_else(|| AppError::Other("That folder is no longer in the library".into()))?;
    if !folder.is_device {
        return Err(AppError::Other(format!(
            "{} is not a music player, so songs cannot be copied to it",
            folder.path
        )));
    }
    let root = Path::new(&folder.path);
    if !root.is_dir() {
        return Err(AppError::Other(format!(
            "{} is not connected",
            folder.path
        )));
    }

    // What the device holds already, as of its last scan.
    let on_device: Vec<Track> = state
        .db
        .list_tracks()?
        .into_iter()
        .filter(|track| track.folder_id == folder_id)
        .collect();

    // Sort the dragged songs into "copy these" and "the device has these".
    // Ids that no longer exist, and songs dragged from the device's own
    // list, are quietly dropped.
    let mut to_copy = Vec::new();
    let mut already_present = Vec::new();
    for track_id in track_ids {
        let Some(track) = state.db.get_track(track_id)? else {
            continue;
        };
        if track.folder_id == folder_id {
            continue;
        }
        if device::is_on_device(&track, &on_device) {
            already_present.push(track.title);
        } else {
            to_copy.push(track);
        }
    }

    let mut result = device::copy_tracks(root, &to_copy, &|progress| {
        let _ = app.emit(EVENT_COPY_PROGRESS, progress);
    });
    // Put the ones recognised above in front of any `copy_tracks` found.
    already_present.append(&mut result.already_present);
    result.already_present = already_present;

    if result.copied > 0 {
        state.scanner.request_scan();
    }
    Ok(result)
}

// =================================================================== playlists

/// Name of the event that tells the UI to reload just the playlists. (A
/// `library:changed` reloads them too, along with everything else.)
pub const EVENT_PLAYLISTS_CHANGED: &str = "playlists:changed";

/// The longest playlist name allowed, in characters.
const MAX_PLAYLIST_NAME: usize = 100;

/// Every playlist, with the ids of its tracks.
#[tauri::command]
pub fn list_playlists(state: State<'_, AppState>) -> AppResult<Vec<Playlist>> {
    state.db.list_playlists()
}

/// Creates an empty playlist and returns it.
#[tauri::command]
pub fn create_playlist(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> AppResult<Playlist> {
    let name = clean_playlist_name(&name)?;
    let playlist = state.db.create_playlist(name)?;
    let _ = app.emit(EVENT_PLAYLISTS_CHANGED, ());
    Ok(playlist)
}

/// Tidies a playlist name typed by the user and checks it is usable.
///
/// The returned `&str` is a slice of the `&str` passed in (the same text
/// minus the spaces at its ends), not a copy, which is why no lifetime has
/// to be written: with one reference in and one out, Rust assumes the
/// output borrows from the input.
/// <https://doc.rust-lang.org/book/ch10-03-lifetime-syntax.html#lifetime-elision>
fn clean_playlist_name(name: &str) -> AppResult<&str> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::Other("Give the playlist a name".into()));
    }
    // `chars().count()` counts characters; `len()` would count bytes, and
    // an accented letter or an emoji is several bytes.
    if name.chars().count() > MAX_PLAYLIST_NAME {
        return Err(AppError::Other(format!(
            "A playlist name can be at most {MAX_PLAYLIST_NAME} characters"
        )));
    }
    Ok(name)
}

/// Deletes a playlist. The songs in it are not affected.
#[tauri::command]
pub fn delete_playlist(
    app: AppHandle,
    state: State<'_, AppState>,
    playlist_id: i64,
) -> AppResult<()> {
    state.db.delete_playlist(playlist_id)?;
    let _ = app.emit(EVENT_PLAYLISTS_CHANGED, ());
    Ok(())
}

/// Adds tracks to the end of a playlist. Ones it already has are skipped
/// and counted in the result.
#[tauri::command]
pub fn add_to_playlist(
    app: AppHandle,
    state: State<'_, AppState>,
    playlist_id: i64,
    track_ids: Vec<i64>,
) -> AppResult<PlaylistAddResult> {
    let added = state.db.add_to_playlist(playlist_id, &track_ids)?;
    let _ = app.emit(EVENT_PLAYLISTS_CHANGED, ());
    Ok(PlaylistAddResult {
        added,
        already_present: track_ids.len() - added,
    })
}

/// Takes tracks out of a playlist. They stay in the library and on disk.
#[tauri::command]
pub fn remove_from_playlist(
    app: AppHandle,
    state: State<'_, AppState>,
    playlist_id: i64,
    track_ids: Vec<i64>,
) -> AppResult<()> {
    state.db.remove_from_playlist(playlist_id, &track_ids)?;
    let _ = app.emit(EVENT_PLAYLISTS_CHANGED, ());
    Ok(())
}

// ==================================================================== settings

/// Reads a small saved value by name (for example which folder was last
/// selected in the sidebar). Returns `None` if it was never saved.
///
/// Settings live in the `settings` table of the library database. The
/// player's own settings (`volume`, `shuffle`) are stored there too, but
/// those are read and written by `player.rs`, not through these commands.
#[tauri::command]
pub fn get_setting(state: State<'_, AppState>, key: String) -> AppResult<Option<String>> {
    state.db.get_setting(&key)
}

/// Saves a small value by name, replacing any previous value.
#[tauri::command]
pub fn set_setting(state: State<'_, AppState>, key: String, value: String) -> AppResult<()> {
    state.db.set_setting(&key, &value)
}

// ====================================================================== player
//
// These all just forward to the player thread. None of them returns
// anything: the result shows up in the next `player:state` event.

/// Plays `track_ids[start_index]` and queues the rest of the list after it.
/// The UI passes the songs currently shown, in the order shown.
#[tauri::command]
pub fn play_tracks(state: State<'_, AppState>, track_ids: Vec<i64>, start_index: usize) {
    state.player.send(Command::PlayQueue {
        track_ids,
        start_index,
    });
}

#[tauri::command]
pub fn toggle_pause(state: State<'_, AppState>) {
    state.player.send(Command::TogglePause);
}

#[tauri::command]
pub fn stop(state: State<'_, AppState>) {
    state.player.send(Command::Stop);
}

#[tauri::command]
pub fn next_track(state: State<'_, AppState>) {
    state.player.send(Command::Next);
}

#[tauri::command]
pub fn previous_track(state: State<'_, AppState>) {
    state.player.send(Command::Previous);
}

#[tauri::command]
pub fn seek(state: State<'_, AppState>, position_ms: u64) {
    state
        .player
        .send(Command::Seek(Duration::from_millis(position_ms)));
}

#[tauri::command]
pub fn set_volume(state: State<'_, AppState>, volume: f32) {
    state.player.send(Command::SetVolume(volume));
}

#[tauri::command]
pub fn set_shuffle(state: State<'_, AppState>, shuffle: bool) {
    state.player.send(Command::SetShuffle(shuffle));
}

/// The player's current state. The UI calls this once at startup; after that
/// it relies on `player:state` events.
#[tauri::command]
pub fn get_player_state(state: State<'_, AppState>) -> PlayerState {
    state.player.state()
}

// ==================================================== editing files (tags, art)

/// Embeds the image at `image_path` as the cover of each of the given
/// tracks. **This rewrites those audio files.**
#[tauri::command(async)]
pub fn set_cover_art(
    app: AppHandle,
    state: State<'_, AppState>,
    track_ids: Vec<i64>,
    image_path: String,
) -> AppResult<ArtUpdateResult> {
    // Decode and (if needed) shrink the image once, then reuse the result
    // for every file.
    let cover = artwork::prepare_for_embedding(Path::new(&image_path))?;

    let result = update_each_track(&state, &track_ids, |track| {
        tags::write_cover(Path::new(&track.path), &cover)
    });
    let _ = app.emit(EVENT_LIBRARY_CHANGED, ());
    Ok(result)
}

/// Removes all embedded pictures from each of the given tracks. **This
/// rewrites those audio files.**
#[tauri::command(async)]
pub fn remove_cover_art(
    app: AppHandle,
    state: State<'_, AppState>,
    track_ids: Vec<i64>,
) -> AppResult<ArtUpdateResult> {
    let result = update_each_track(&state, &track_ids, |track| {
        tags::remove_cover(Path::new(&track.path))
    });
    let _ = app.emit(EVENT_LIBRARY_CHANGED, ());
    Ok(result)
}

/// Saves a copy of a track's embedded cover as an image file of its own,
/// in a place the user picks. Returns the path it was saved to, or `None`
/// if the user cancelled. The audio file is only read, never changed.
///
/// The picture is written exactly as it is stored in the song (same
/// format, full size), not the shrunken thumbnail the list shows.
///
/// Unlike the other dialogs in the app, which the UI opens (see
/// `pickFolder` in `src/api.ts`), this one is opened from Rust. The file
/// name to suggest depends on the picture's type (.jpg or .png), which is
/// only known once the cover has been read out of the file, and that
/// happens here.
///
/// This is the app's replacement for the webview's own "Save Image As"
/// menu entry, which does nothing in a Tauri window on Linux.
///
/// Learning link: opening dialogs from Rust,
/// <https://tauri.app/plugin/dialog/#rust>
#[tauri::command(async)]
pub fn save_cover_art(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    track_id: i64,
) -> AppResult<Option<String>> {
    let track = state
        .db
        .get_track(track_id)?
        .ok_or_else(|| AppError::Other("That song is no longer in the library".into()))?;
    let cover = tags::read_cover(Path::new(&track.path))?
        .ok_or_else(|| AppError::Other("This song has no cover to save".into()))?;

    // Suggest "<album>.jpg", or the song's title if it has no album.
    let extension = cover.file_extension();
    let name = if track.album.is_empty() {
        &track.title
    } else {
        &track.album
    };
    let suggested = format!("{}.{extension}", device::safe_name(name, "cover"));

    // `blocking_save_file` waits here until the dialog is closed. That is
    // fine because this command is `(async)`, so it is not running on the
    // window's own thread. It gives `None` if the user cancelled.
    //
    // `let ... else` is "unwrap this, or else leave the function":
    // <https://doc.rust-lang.org/rust-by-example/flow_control/let_else.html>
    let Some(chosen) = app
        .dialog()
        .file()
        .set_parent(&window)
        .set_title("Save cover as")
        .set_file_name(suggested)
        .add_filter("Image", &[extension])
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let chosen = chosen
        .into_path()
        .map_err(|error| AppError::Other(format!("Cannot save there: {error}")))?;

    let destination = cover_destination(chosen, extension)?;
    fs::write(&destination, &cover.data)?;
    Ok(Some(destination.display().to_string()))
}

/// Decides the exact file a cover is written to, given what the user chose
/// in the save dialog.
///
/// A name typed without an extension ("front") gets the picture's own
/// ("front.jpg"), so the file opens in an image viewer. The dialog has
/// already asked before replacing the file the user *chose*, but it knows
/// nothing about the name with the extension added, so if a file of that
/// name exists this refuses to overwrite it.
fn cover_destination(chosen: PathBuf, extension: &str) -> AppResult<PathBuf> {
    if chosen.extension().is_some() {
        return Ok(chosen);
    }
    let with_extension = chosen.with_extension(extension);
    if with_extension.exists() {
        return Err(AppError::Other(format!(
            "{} already exists. Choose a different name.",
            with_extension.display()
        )));
    }
    Ok(with_extension)
}

/// Changes the title, artist and/or album of each of the given tracks. A
/// field left out (`None`) is not touched; an empty string clears it.
/// **This rewrites those audio files.**
#[tauri::command(async)]
pub fn set_track_tags(
    app: AppHandle,
    state: State<'_, AppState>,
    track_ids: Vec<i64>,
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
) -> AppResult<ArtUpdateResult> {
    let edit = TextEdit {
        title,
        artist,
        album,
    };
    let result = update_each_track(&state, &track_ids, |track| {
        tags::write_text(Path::new(&track.path), &edit)
    });
    let _ = app.emit(EVENT_LIBRARY_CHANGED, ());
    Ok(result)
}

/// Shared by the commands above: runs `change` on each track's file,
/// then brings the database and thumbnail cache up to date for that track.
/// One file failing (read-only, missing...) does not stop the others; its
/// message is collected in the result instead.
fn update_each_track(
    state: &AppState,
    track_ids: &[i64],
    change: impl Fn(&Track) -> AppResult<()>,
) -> ArtUpdateResult {
    let mut result = ArtUpdateResult::default();

    for &track_id in track_ids {
        // This closure exists so `?` can be used for the three steps; the
        // `match` below then handles success or failure in one place.
        let outcome = (|| -> AppResult<()> {
            let track = state
                .db
                .get_track(track_id)?
                .ok_or_else(|| AppError::Other(format!("Track {track_id} is not in the library")))?;
            change(&track).map_err(|error| AppError::Other(format!("{}: {error}", track.path)))?;
            scanner::refresh_track(&state.db, &track)?;
            state.thumbs.forget(track_id);
            Ok(())
        })();

        match outcome {
            Ok(()) => result.updated += 1,
            Err(error) => result.failures.push(error.to_string()),
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library() -> Vec<Folder> {
        vec![Folder {
            id: 1,
            path: "/home/me/Music".into(),
            track_count: 0,
            size_bytes: 0,
            is_device: false,
            connected: true,
        }]
    }

    #[test]
    fn a_new_unrelated_folder_is_accepted() {
        let found = find_existing(Path::new("/mnt/usb/Albums"), &library()).unwrap();
        assert_eq!(found, None);
    }

    #[test]
    fn the_same_folder_is_returned_rather_than_added_twice() {
        let found = find_existing(Path::new("/home/me/Music"), &library()).unwrap();
        assert_eq!(found.map(|folder| folder.id), Some(1));
    }

    #[test]
    fn a_folder_inside_a_library_folder_is_refused() {
        let error = find_existing(Path::new("/home/me/Music/Rock"), &library()).unwrap_err();
        assert!(error.to_string().contains("already covered"));
    }

    #[test]
    fn a_folder_containing_a_library_folder_is_refused() {
        let error = find_existing(Path::new("/home/me"), &library()).unwrap_err();
        assert!(error.to_string().contains("Remove that one first"));
    }

    #[test]
    fn a_cover_name_with_an_extension_is_used_as_chosen() {
        let chosen = PathBuf::from("/tmp/pictures/front.png");
        assert_eq!(cover_destination(chosen.clone(), "jpg").unwrap(), chosen);
    }

    #[test]
    fn a_cover_name_without_an_extension_is_given_one() {
        let dir = tempfile::tempdir().unwrap();
        let saved = cover_destination(dir.path().join("front"), "jpg").unwrap();
        assert_eq!(saved, dir.path().join("front.jpg"));
    }

    #[test]
    fn adding_the_extension_never_overwrites_another_file() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("front.jpg"), b"someone else's picture").unwrap();

        let error = cover_destination(dir.path().join("front"), "jpg").unwrap_err();
        assert!(error.to_string().contains("already exists"));
    }

    #[test]
    fn playlist_names_are_trimmed_and_must_not_be_empty_or_huge() {
        assert_eq!(clean_playlist_name("  Running  ").unwrap(), "Running");
        assert!(clean_playlist_name("   ").is_err());
        assert!(clean_playlist_name(&"x".repeat(MAX_PLAYLIST_NAME)).is_ok());
        assert!(clean_playlist_name(&"x".repeat(MAX_PLAYLIST_NAME + 1)).is_err());
    }

    #[test]
    fn a_similarly_named_sibling_is_not_an_overlap() {
        let found = find_existing(Path::new("/home/me/Music2"), &library()).unwrap();
        assert_eq!(found, None);
    }
}
