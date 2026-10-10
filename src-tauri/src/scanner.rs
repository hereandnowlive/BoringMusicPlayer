//! Finds the audio files in the library folders and keeps the database in
//! step with what is on disk.
//!
//! A scan is *incremental*: a file whose size and modification time match
//! what the database remembers is skipped without being opened. So the first
//! scan of a big folder takes a while, and later scans take a moment.
//!
//! Scans run on their own background thread so the window never freezes.
//! The thread sleeps until someone calls [`ScanHandle::request_scan`]. This
//! "worker thread fed by a channel" pattern is the same one `player.rs` uses.
//!
//! Learning links:
//! - Threads: <https://doc.rust-lang.org/book/ch16-01-threads.html>
//! - Channels (message passing between threads):
//!   <https://doc.rust-lang.org/book/ch16-02-message-passing.html>
//! - `Arc` (sharing one value between threads):
//!   <https://doc.rust-lang.org/book/ch16-03-shared-state.html#atomic-reference-counting-with-arct>
//! - walkdir: <https://docs.rs/walkdir>

use std::collections::HashSet;
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread;
use std::time::UNIX_EPOCH;

use walkdir::{DirEntry, WalkDir};

use crate::db::{Db, ScannedFile};
use crate::error::{AppError, AppResult};
use crate::models::{Folder, ScanProgress, ScanSummary, Track};
use crate::tags;

/// How many files to read before writing them to the database in one go.
const BATCH_SIZE: usize = 200;

/// How many files to handle between progress reports to the UI.
const PROGRESS_EVERY: usize = 25;

/// Lets the rest of the app ask the scanner thread for a scan. Cloning it
/// gives another handle to the same thread.
#[derive(Clone)]
pub struct ScanHandle {
    tx: Sender<()>,
}

impl ScanHandle {
    /// Asks for a scan of every library folder and returns immediately. If a
    /// scan is already running, another one runs right after it (so a folder
    /// added halfway through is not missed).
    pub fn request_scan(&self) {
        // `send` only fails if the scanner thread is gone, which only
        // happens while the app is shutting down.
        let _ = self.tx.send(());
    }
}

/// Starts the scanner thread.
///
/// `on_progress` and `on_finished` are called from that thread; `lib.rs`
/// passes closures that forward the values to the UI as events. Taking
/// callbacks, rather than emitting events here, keeps this file free of any
/// Tauri code.
pub fn spawn_scanner<P, F>(db: Arc<Db>, on_progress: P, on_finished: F) -> ScanHandle
where
    // `Send + 'static` is what the compiler needs in order to let a closure
    // move to another thread: it must not hold references to anything that
    // might disappear while the thread is still running.
    P: Fn(ScanProgress) + Send + 'static,
    F: Fn(ScanSummary) + Send + 'static,
{
    let (tx, rx) = mpsc::channel::<()>();

    thread::spawn(move || {
        // `recv()` blocks until a request arrives, and returns `Err` once
        // every `Sender` is gone, which ends the loop and the thread.
        while rx.recv().is_ok() {
            // Several requests may have queued up; one scan covers them all.
            while rx.try_recv().is_ok() {}
            on_finished(scan_all(&db, &on_progress));
        }
    });

    ScanHandle { tx }
}

/// Scans every library folder and adds up the results. A folder that fails
/// (say, an unplugged drive) is reported in `errors` and does not stop the
/// others from being scanned.
pub fn scan_all(db: &Db, on_progress: &dyn Fn(ScanProgress)) -> ScanSummary {
    let mut total = ScanSummary::default();

    let folders = match db.list_folders() {
        Ok(folders) => folders,
        Err(error) => {
            total.errors.push(error.to_string());
            return total;
        }
    };

    for folder in folders {
        // An MP3 player that is not plugged in is normal, not a problem to
        // report. Its songs stay in the database until it comes back.
        if folder.is_device && !Path::new(&folder.path).is_dir() {
            continue;
        }

        match scan_folder(db, &folder, on_progress) {
            Ok(summary) => {
                total.added += summary.added;
                total.updated += summary.updated;
                total.removed += summary.removed;
                total.failed += summary.failed;
                total.errors.extend(summary.errors);
            }
            Err(error) => total.errors.push(error.to_string()),
        }
    }

    total
}

/// Brings the database up to date with one folder on disk: adds new files,
/// re-reads changed ones and forgets ones that were deleted.
pub fn scan_folder(
    db: &Db,
    folder: &Folder,
    on_progress: &dyn Fn(ScanProgress),
) -> AppResult<ScanSummary> {
    let root = Path::new(&folder.path);

    // If the folder itself is missing (external drive not plugged in), stop
    // here. Carrying on would see "no files" and wipe the folder's tracks
    // from the library.
    if !root.is_dir() {
        return Err(AppError::Other(format!(
            "Folder not found, skipped: {}",
            folder.path
        )));
    }

    let mut summary = ScanSummary::default();
    let known = db.known_files(folder.id)?;
    let (files, walk_was_complete) = find_audio_files(root);

    let mut seen: HashSet<String> = HashSet::with_capacity(files.len());
    let mut batch: Vec<ScannedFile> = Vec::with_capacity(BATCH_SIZE);

    for (index, path) in files.iter().enumerate() {
        if index % PROGRESS_EVERY == 0 {
            on_progress(ScanProgress {
                folder: folder.path.clone(),
                done: index,
                total: files.len(),
            });
        }

        // Paths are stored as text. A file name that is not valid UTF-8
        // (very rare) cannot be stored faithfully, so it is skipped.
        let Some(path_text) = path.to_str() else {
            summary.failed += 1;
            continue;
        };
        seen.insert(path_text.to_string());

        let Ok(metadata) = path.metadata() else {
            summary.failed += 1;
            continue;
        };
        let mtime = modified_millis(&metadata);
        let size = metadata.len() as i64;

        // Unchanged since the last scan? Then there is nothing to do.
        let previous = known.get(path_text);
        if previous.is_some_and(|k| k.mtime == mtime && k.size == size) {
            continue;
        }

        match tags::read_tags(path) {
            Ok(tags) => {
                if previous.is_some() {
                    summary.updated += 1;
                } else {
                    summary.added += 1;
                }
                batch.push(ScannedFile {
                    path: path_text.to_string(),
                    mtime,
                    size,
                    tags,
                });
            }
            Err(_) => summary.failed += 1,
        }

        if batch.len() >= BATCH_SIZE {
            db.upsert_tracks(folder.id, &batch)?;
            batch.clear();
        }
    }
    db.upsert_tracks(folder.id, &batch)?;

    // Anything the database knows about that we did not see on disk has been
    // deleted or moved. Only trust that if the whole folder could be read.
    if walk_was_complete {
        let gone: Vec<i64> = known
            .iter()
            .filter(|(path, _)| !seen.contains(*path))
            .map(|(_, known_file)| known_file.id)
            .collect();
        db.delete_tracks(&gone)?;
        summary.removed = gone.len();
    } else {
        summary.errors.push(format!(
            "Part of {} could not be read, so missing files were not removed from the library",
            folder.path
        ));
    }

    on_progress(ScanProgress {
        folder: folder.path.clone(),
        done: files.len(),
        total: files.len(),
    });

    Ok(summary)
}

/// Re-reads one file's tags into the database. Used right after the app
/// itself changes a file (embedding cover art), so the library is correct
/// without waiting for a full scan.
pub fn refresh_track(db: &Db, track: &Track) -> AppResult<()> {
    let path = Path::new(&track.path);
    let metadata = path.metadata()?;
    let file = ScannedFile {
        path: track.path.clone(),
        mtime: modified_millis(&metadata),
        size: metadata.len() as i64,
        tags: tags::read_tags(path)?,
    };
    db.upsert_tracks(track.folder_id, &[file])
}

/// Lists every audio file under `root`, including in sub-folders.
///
/// The second value is `false` if some part of the folder could not be read
/// (for example a sub-folder without permission), meaning the list may be
/// incomplete.
fn find_audio_files(root: &Path) -> (Vec<PathBuf>, bool) {
    let mut files = Vec::new();
    let mut complete = true;

    let walker = WalkDir::new(root)
        .follow_links(true)
        .into_iter()
        // `filter_entry` also stops walkdir from descending into a rejected
        // folder, so hidden folders are skipped entirely.
        .filter_entry(|entry| !is_hidden(entry));

    for entry in walker {
        match entry {
            Ok(entry) => {
                if entry.file_type().is_file() && tags::is_audio_file(entry.path()) {
                    files.push(entry.into_path());
                }
            }
            Err(_) => complete = false,
        }
    }

    (files, complete)
}

/// True for files and folders whose name starts with a dot, such as the
/// `.Trash-1000` folder on external drives (which would otherwise bring
/// deleted songs back into the library). The scanned folder itself is never
/// treated as hidden.
fn is_hidden(entry: &DirEntry) -> bool {
    entry.depth() > 0
        && entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with('.'))
}

/// A file's modification time in milliseconds since 1970 (0 if the
/// filesystem cannot say).
fn modified_millis(metadata: &Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|since_epoch| since_epoch.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tags::CoverArt;
    use crate::test_support::{tiny_png, write_wav};
    use std::fs;

    /// A progress callback that does nothing.
    fn quiet(_: ScanProgress) {}

    #[test]
    fn scan_adds_skips_and_removes() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("album")).unwrap();
        fs::create_dir(dir.path().join(".hidden")).unwrap();
        write_wav(&dir.path().join("one.wav"), 1);
        write_wav(&dir.path().join("album/two.wav"), 1);
        write_wav(&dir.path().join(".hidden/secret.wav"), 1);
        fs::write(dir.path().join("cover.jpg"), "not audio").unwrap();
        fs::write(dir.path().join("broken.mp3"), "not really an mp3").unwrap();

        let db = Db::open_in_memory().unwrap();
        let folder = db.add_folder(dir.path().to_str().unwrap(), false).unwrap();

        // First scan: both real songs are added, the broken one is counted.
        let first = scan_folder(&db, &folder, &quiet).unwrap();
        assert_eq!((first.added, first.updated, first.removed), (2, 0, 0));
        assert_eq!(first.failed, 1);
        let mut titles: Vec<String> =
            db.list_tracks().unwrap().into_iter().map(|t| t.title).collect();
        titles.sort();
        assert_eq!(titles, ["one", "two"]);

        // Second scan with nothing changed: nothing to do.
        let second = scan_folder(&db, &folder, &quiet).unwrap();
        assert_eq!((second.added, second.updated, second.removed), (0, 0, 0));

        // Delete one file: it leaves the library.
        fs::remove_file(dir.path().join("one.wav")).unwrap();
        let third = scan_folder(&db, &folder, &quiet).unwrap();
        assert_eq!((third.added, third.updated, third.removed), (0, 0, 1));
        assert_eq!(db.list_tracks().unwrap().len(), 1);
    }

    #[test]
    fn a_missing_folder_does_not_wipe_its_tracks() {
        let dir = tempfile::tempdir().unwrap();
        write_wav(&dir.path().join("song.wav"), 1);
        let db = Db::open_in_memory().unwrap();
        let folder = db.add_folder(dir.path().to_str().unwrap(), false).unwrap();
        scan_folder(&db, &folder, &quiet).unwrap();

        // Simulate an unplugged drive by pointing the scan somewhere that
        // does not exist.
        let unplugged = Folder {
            path: dir.path().join("nope").to_string_lossy().into_owned(),
            ..folder
        };
        assert!(scan_folder(&db, &unplugged, &quiet).is_err());
        assert_eq!(db.list_tracks().unwrap().len(), 1);
    }

    #[test]
    fn refresh_picks_up_newly_embedded_art() {
        let dir = tempfile::tempdir().unwrap();
        let song = dir.path().join("song.wav");
        write_wav(&song, 1);
        let db = Db::open_in_memory().unwrap();
        let folder = db.add_folder(dir.path().to_str().unwrap(), false).unwrap();
        scan_folder(&db, &folder, &quiet).unwrap();
        let before = db.list_tracks().unwrap().remove(0);
        assert!(!before.has_art);

        let cover = CoverArt { data: tiny_png([9, 9, 9]), mime: "image/png".into() };
        tags::write_cover(&song, &cover).unwrap();
        refresh_track(&db, &before).unwrap();

        let after = db.get_track(before.id).unwrap().unwrap();
        assert!(after.has_art);
        assert_eq!(after.id, before.id);
    }
}
