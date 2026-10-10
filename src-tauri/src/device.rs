//! Copying songs onto a portable music player.
//!
//! A "device" is just a library folder with a flag on it (see
//! `Folder::is_device`). It is scanned and listed like any other folder;
//! the only extra thing it can do is receive copies of songs, which is what
//! this file implements.
//!
//! Songs are filed the way players and their companion software usually
//! arrange them:
//!
//! ```text
//! <player>/Music/<Artist>/<Album>/<original file name>
//! ```
//!
//! (`Music/` is used if the player has such a folder; otherwise the songs
//! go straight under the folder that was added to the library.)
//!
//! It also holds the "watcher": a small background thread that notices a
//! device being plugged in or pulled out (see [`spawn_watcher`]).
//!
//! This file knows nothing about Tauri. `commands.rs` looks up the folder
//! and tracks, then calls [`copy_tracks`]; `lib.rs` starts the watcher.
//!
//! Learning links:
//! - Files and I/O: <https://doc.rust-lang.org/std/fs/index.html>
//! - `Path` / `PathBuf`: <https://doc.rust-lang.org/std/path/index.html>

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::db::Db;
use crate::models::{CopyProgress, CopyResult, Track};

/// How often the watcher checks whether devices are plugged in.
const WATCH_INTERVAL: Duration = Duration::from_secs(2);

/// Two songs with the same title and artist are treated as the same
/// recording if their lengths differ by no more than this (milliseconds).
/// Different encodings of one song differ by a fraction of a second; a
/// live version or a remix differs by much more.
const SAME_SONG_TOLERANCE_MS: i64 = 2000;

/// Characters that cannot appear in file names on FAT, the filesystem
/// nearly every music player uses. They are fine on Linux, so songs in the
/// library can contain them.
const FORBIDDEN_ON_FAT: &[char] = &['"', '*', '/', ':', '<', '>', '?', '\\', '|'];

/// What happened to the set of connected devices between two checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceChange {
    /// At least one device was plugged in.
    Connected,
    /// Devices were only unplugged.
    Disconnected,
}

/// Starts a background thread that checks every couple of seconds which
/// device folders can be reached, and calls `on_change` when that changes.
///
/// A plugged-in device is simply a folder that exists (the system creates
/// `/media/<user>/<name>` when it mounts the device and removes it again
/// when the device goes), so there is nothing cleverer to do than look.
pub fn spawn_watcher<F>(db: Arc<Db>, on_change: F)
where
    F: Fn(DeviceChange) + Send + 'static,
{
    thread::spawn(move || {
        let mut connected = connected_devices(&db);
        loop {
            thread::sleep(WATCH_INTERVAL);
            let now = connected_devices(&db);
            if now == connected {
                continue;
            }
            // `is_subset` is true when nothing new appeared, i.e. devices
            // were only removed.
            let change = if now.is_subset(&connected) {
                DeviceChange::Disconnected
            } else {
                DeviceChange::Connected
            };
            connected = now;
            on_change(change);
        }
    });
}

/// The ids of the device folders that exist on disk right now.
fn connected_devices(db: &Db) -> HashSet<i64> {
    db.list_folders()
        .unwrap_or_default()
        .into_iter()
        .filter(|folder| folder.is_device && Path::new(&folder.path).is_dir())
        .map(|folder| folder.id)
        .collect()
}

/// Whether the device already holds this song, judged by what the song
/// *is* (title, artist, length) and not by its file name, because the same
/// song is often stored under different names in different places.
///
/// `on_device` is the device's track list from the library database.
pub fn is_on_device(track: &Track, on_device: &[Track]) -> bool {
    let same_text = |a: &str, b: &str| a.trim().to_lowercase() == b.trim().to_lowercase();
    on_device.iter().any(|other| {
        same_text(&track.title, &other.title)
            && same_text(&track.artist, &other.artist)
            && (track.duration_ms - other.duration_ms).abs() <= SAME_SONG_TOLERANCE_MS
    })
}

/// Copies `tracks` onto the device whose library folder is `device_root`.
///
/// - A song whose destination file already exists is skipped and listed in
///   the result. (`commands.rs` filters out songs the device has under
///   another name before calling this; see [`is_on_device`].)
/// - A song that fails is reported in the result and the rest carry on,
///   except when the device is full, where there is no point continuing.
/// - `on_progress` is called before each file and once at the end.
pub fn copy_tracks(
    device_root: &Path,
    tracks: &[Track],
    on_progress: &dyn Fn(CopyProgress),
) -> CopyResult {
    let music_dir = music_dir(device_root);
    let mut result = CopyResult::default();

    for (index, track) in tracks.iter().enumerate() {
        let source = Path::new(&track.path);
        let file_name = source
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();

        on_progress(CopyProgress {
            done: index,
            total: tracks.len(),
            current: file_name.clone(),
        });

        let destination = destination_for(&music_dir, track, &file_name);
        if destination.exists() {
            result.already_present.push(track.title.clone());
            continue;
        }

        match copy_file(source, &destination) {
            Ok(()) => result.copied += 1,
            Err(error) => {
                result.failures.push(format!("{file_name}: {error}"));
                if error.kind() == ErrorKind::StorageFull {
                    break; // every remaining file would fail the same way
                }
            }
        }
    }

    on_progress(CopyProgress {
        done: tracks.len(),
        total: tracks.len(),
        current: String::new(),
    });
    result
}

/// Where music goes on the device: its `Music` folder if it has one
/// (whatever the capitalisation), otherwise the device folder itself.
fn music_dir(device_root: &Path) -> PathBuf {
    let existing = fs::read_dir(device_root).ok().and_then(|entries| {
        entries
            .flatten()
            .find(|entry| {
                entry.path().is_dir()
                    && entry.file_name().to_string_lossy().eq_ignore_ascii_case("music")
            })
            .map(|entry| entry.path())
    });
    existing.unwrap_or_else(|| device_root.to_path_buf())
}

/// The full path a track should have on the device:
/// `<music dir>/<Artist>/<Album>/<file name>`.
fn destination_for(music_dir: &Path, track: &Track, file_name: &str) -> PathBuf {
    // Prefer the album artist, so a compilation stays in one folder.
    let artist = if !track.album_artist.is_empty() {
        &track.album_artist
    } else {
        &track.artist
    };

    music_dir
        .join(safe_name(artist, "Unknown Artist"))
        .join(safe_name(&track.album, "Unknown Album"))
        .join(safe_name(file_name, "Unknown Song"))
}

/// Makes `name` usable as a file or folder name on a FAT-formatted device:
/// forbidden characters become `_`, and leading/trailing spaces and
/// trailing dots (which FAT silently drops) are removed. `fallback` is used
/// if nothing is left.
///
/// `pub(crate)` makes it visible to the rest of this project but not
/// outside it; `commands.rs` uses it to suggest a file name for a saved
/// cover. <https://doc.rust-lang.org/reference/visibility-and-privacy.html>
pub(crate) fn safe_name(name: &str, fallback: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if FORBIDDEN_ON_FAT.contains(&c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_end_matches('.').trim_end();

    if cleaned.is_empty() {
        fallback.to_string()
    } else {
        cleaned.to_string()
    }
}

/// Copies one file, creating the destination's folders as needed.
///
/// The data is written to a temporary `.part` file which is renamed into
/// place only once it is complete and has been flushed to the device. Two
/// reasons: a copy interrupted half-way (cable pulled) leaves no
/// broken-but-plausible song behind, and by the time the UI says "copied"
/// the data really is on the device, not waiting in the computer's memory.
fn copy_file(source: &Path, destination: &Path) -> io::Result<()> {
    if let Some(folder) = destination.parent() {
        fs::create_dir_all(folder)?;
    }

    let mut part_name = destination.as_os_str().to_owned();
    part_name.push(".part");
    let part_path = PathBuf::from(part_name);

    // This closure does the fallible steps so that, whichever one fails,
    // the clean-up below runs.
    let write = || -> io::Result<()> {
        let mut input = File::open(source)?;
        let mut output = File::create(&part_path)?;
        io::copy(&mut input, &mut output)?;
        output.sync_all()?; // wait until the device has actually stored it
        fs::rename(&part_path, destination)
    };

    let outcome = write();
    if outcome.is_err() {
        let _ = fs::remove_file(&part_path); // do not leave a partial file
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::write_wav;
    use std::cell::RefCell;

    /// A track with the given file and tags; the other fields do not matter.
    fn track(path: &Path, artist: &str, album: &str) -> Track {
        Track {
            id: 1,
            path: path.to_string_lossy().into_owned(),
            folder_id: 1,
            title: "Song".into(),
            artist: artist.into(),
            album: album.into(),
            album_artist: String::new(),
            track_no: None,
            disc_no: None,
            year: None,
            duration_ms: 1000,
            has_art: false,
            mtime: 0,
        }
    }

    #[test]
    fn songs_are_filed_by_artist_and_album_under_the_music_folder() {
        let dir = tempfile::tempdir().unwrap();
        let library = dir.path().join("library");
        let device = dir.path().join("device");
        fs::create_dir_all(&library).unwrap();
        fs::create_dir_all(device.join("MUSIC")).unwrap(); // odd capitalisation

        let tagged = library.join("one.wav");
        let untagged = library.join("two.wav");
        write_wav(&tagged, 1);
        write_wav(&untagged, 1);
        let tracks = [track(&tagged, "Metric", "Fantasies"), track(&untagged, "", "")];

        let progress = RefCell::new(Vec::new());
        let result = copy_tracks(&device, &tracks, &|p| progress.borrow_mut().push(p.done));

        assert_eq!(result, CopyResult { copied: 2, already_present: vec![], failures: vec![] });
        let copied = device.join("MUSIC/Metric/Fantasies/one.wav");
        assert_eq!(fs::read(&copied).unwrap(), fs::read(&tagged).unwrap());
        assert!(device.join("MUSIC/Unknown Artist/Unknown Album/two.wav").exists());
        assert!(!device.join("MUSIC/Metric/Fantasies/one.wav.part").exists());
        assert_eq!(*progress.borrow(), [0, 1, 2]);

        // Copying the same songs again changes nothing.
        let again = copy_tracks(&device, &tracks, &|_| {});
        assert_eq!(again.copied, 0);
        assert_eq!(again.already_present, ["Song", "Song"]);
    }

    #[test]
    fn a_device_without_a_music_folder_gets_the_songs_at_its_top_level() {
        let dir = tempfile::tempdir().unwrap();
        let song = dir.path().join("song.wav");
        let device = dir.path().join("device");
        write_wav(&song, 1);
        fs::create_dir_all(&device).unwrap();

        copy_tracks(&device, &[track(&song, "A", "B")], &|_| {});
        assert!(device.join("A/B/song.wav").exists());
    }

    #[test]
    fn a_missing_song_is_reported_and_the_others_still_copy() {
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("good.wav");
        write_wav(&good, 1);
        let device = dir.path().join("device");
        fs::create_dir_all(&device).unwrap();
        let tracks = [
            track(&dir.path().join("gone.wav"), "A", "B"),
            track(&good, "A", "B"),
        ];

        let result = copy_tracks(&device, &tracks, &|_| {});
        assert_eq!((result.copied, result.already_present.len()), (1, 0));
        assert_eq!(result.failures.len(), 1);
        assert!(result.failures[0].starts_with("gone.wav"));
        assert!(!device.join("A/B/gone.wav.part").exists());
    }

    #[test]
    fn a_song_is_recognised_on_the_device_whatever_its_file_is_called() {
        let mut library_song = track(Path::new("/music/Savant - Rolling Stone.mp3"), "Savant", "");
        library_song.title = "Rolling Stone".into();
        library_song.duration_ms = 200_000;

        // Same song on the device: other file name, other capitals, and a
        // slightly different length from being encoded separately.
        let mut device_song = track(Path::new("/player/Music/# - Rolling Stone.mp3"), "SAVANT", "x");
        device_song.title = "rolling stone ".into();
        device_song.duration_ms = 200_900;
        assert!(is_on_device(&library_song, &[device_song.clone()]));

        // Same title and artist but a very different length: another version.
        device_song.duration_ms = 320_000;
        assert!(!is_on_device(&library_song, &[device_song.clone()]));

        // Same title by someone else.
        device_song.duration_ms = 200_000;
        device_song.artist = "Bob Dylan".into();
        assert!(!is_on_device(&library_song, &[device_song]));

        assert!(!is_on_device(&library_song, &[]));
    }

    #[test]
    fn names_are_made_safe_for_fat() {
        assert_eq!(safe_name("AC/DC", "x"), "AC_DC");
        assert_eq!(safe_name("What? Why: \"Because\"", "x"), "What_ Why_ _Because_");
        assert_eq!(safe_name("  R.E.M. ", "x"), "R.E.M");
        assert_eq!(safe_name("...", "Unknown Album"), "Unknown Album");
        assert_eq!(safe_name("", "Unknown Artist"), "Unknown Artist");
        assert_eq!(safe_name("Sigur Rós", "x"), "Sigur Rós"); // accents are fine
    }
}
