//! Plain data types that travel between Rust and the TypeScript UI.
//!
//! Every struct here has a twin in `src/types.ts`. **If you change a field
//! here, change it there too** - nothing checks that the two agree.
//!
//! `#[derive(Serialize)]` generates the code that turns a struct into JSON.
//! `#[serde(rename_all = "camelCase")]` renames `duration_ms` to
//! `durationMs` in that JSON, so each language keeps its own naming style.
//!
//! Learning links:
//! - Structs: <https://doc.rust-lang.org/book/ch05-00-structs.html>
//! - Enums and `Option`: <https://doc.rust-lang.org/book/ch06-00-enums.html>
//! - serde attributes: <https://serde.rs/attributes.html>

use serde::Serialize;

/// One song in the library. Mirrors a row of the `tracks` table.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    /// Database id. Stays the same for as long as the file stays in the library.
    pub id: i64,
    /// Absolute path of the audio file.
    pub path: String,
    /// Which library folder this file was found under.
    pub folder_id: i64,
    pub title: String,
    /// Empty string when the file has no artist tag.
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    /// `Option<T>` is Rust's "nullable": `Some(3)` or `None`.
    pub track_no: Option<u32>,
    pub disc_no: Option<u32>,
    pub year: Option<i32>,
    pub duration_ms: i64,
    /// Whether the file has at least one embedded picture.
    pub has_art: bool,
    /// File modification time (seconds since 1970). The UI appends it to
    /// artwork URLs so a changed file is never shown with stale cached art.
    pub mtime: i64,
}

/// A folder the user added to the library.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: i64,
    pub path: String,
    pub track_count: i64,
    /// Combined size, in bytes, of the audio files in `track_count`. Other
    /// files in the folder (pictures, playlists...) are not counted, so
    /// this is the size of the music, not of the folder on disk.
    pub size_bytes: i64,
    /// True for a portable music player (an "MP3 player"). It is scanned
    /// and listed like any folder, and songs can also be copied onto it.
    pub is_device: bool,
    /// Whether the folder can be reached right now. False for a device
    /// that is unplugged, or a drive that is not mounted.
    ///
    /// The database cannot know this, so it always says `true`; the real
    /// answer is filled in by `Folder::check_connected` in `commands.rs`
    /// just before folders are sent to the UI.
    pub connected: bool,
}

impl Folder {
    /// Returns this folder with `connected` set by looking at the disk.
    pub fn check_connected(mut self) -> Self {
        self.connected = std::path::Path::new(&self.path).is_dir();
        self
    }
}

/// A list of songs the user put together by hand.
///
/// A playlist does not hold copies of songs, only the ids of tracks in the
/// library, in the order they were added. A song leaves a playlist by
/// itself when it leaves the library (its file is deleted, or its folder
/// is removed).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    /// Ids of the playlist's tracks, in playlist order. The UI already has
    /// every track's details (from `list_tracks`), so ids are all it needs.
    pub track_ids: Vec<i64>,
}

/// Result of adding songs to a playlist.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistAddResult {
    /// How many songs were added.
    pub added: usize,
    /// How many were left out because the playlist already had them.
    pub already_present: usize,
}

/// What the player is doing right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")] // sent as "stopped" / "playing" / "paused"
pub enum PlaybackStatus {
    Stopped,
    Playing,
    Paused,
}

/// Snapshot of the player, sent to the UI a few times a second while a song
/// plays (event name: `player:state`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerState {
    pub status: PlaybackStatus,
    /// The current song, or `None` if nothing has been played yet.
    pub track: Option<Track>,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub shuffle: bool,
    /// 0.0 (silent) to 1.0 (full).
    pub volume: f32,
    /// How many songs are in the play queue, and where we are in it (0-based).
    pub queue_length: usize,
    pub queue_position: usize,
    /// Set when the last attempt to play something failed.
    pub error: Option<String>,
}

/// Progress of a library scan (event name: `scan:progress`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    /// The folder being scanned.
    pub folder: String,
    /// Files handled so far / files found in this folder.
    pub done: usize,
    pub total: usize,
}

/// Result of a library scan (event name: `scan:finished`).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    /// New files added to the library.
    pub added: usize,
    /// Files whose tags were re-read because the file changed on disk.
    pub updated: usize,
    /// Library entries removed because the file is gone.
    pub removed: usize,
    /// Files that looked like audio but could not be read.
    pub failed: usize,
    /// Problems worth telling the user about (e.g. a folder that is missing).
    pub errors: Vec<String>,
}

/// Progress of copying songs to a device (event name: `copy:progress`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyProgress {
    /// Files finished so far / files to handle in total.
    pub done: usize,
    pub total: usize,
    /// Name of the file being copied now ("" once everything is done).
    pub current: String,
}

/// Result of copying songs to a device.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyResult {
    /// Files newly written to the device.
    pub copied: usize,
    /// Titles of the songs left alone because the device already has them.
    pub already_present: Vec<String>,
    /// One message per file that could not be copied.
    pub failures: Vec<String>,
}

/// Result of changing several files at once (cover art or text tags).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtUpdateResult {
    /// How many files were changed.
    pub updated: usize,
    /// One message per file that could not be changed.
    pub failures: Vec<String>,
}
