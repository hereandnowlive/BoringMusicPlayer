//! The library database (SQLite).
//!
//! The library lives in a single file, `library.db`, in the app's data
//! folder. Most of it is only a *cache* of what is in your music files:
//! the track list can always be rebuilt by a rescan. What cannot be
//! rebuilt, and so is lost if the file is deleted, is what the user made
//! here: the list of folders, the playlists and the settings.
//!
//! All SQL in the project is in this file. The rest of the backend calls the
//! methods on [`Db`] and never sees SQL.
//!
//! Learning links:
//! - rusqlite: <https://docs.rs/rusqlite>
//! - `Mutex` (why the connection is wrapped in one):
//!   <https://doc.rust-lang.org/book/ch16-03-shared-state.html>
//! - SQLite "upsert": <https://www.sqlite.org/lang_upsert.html>

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::{AppError, AppResult};
use crate::models::{Folder, Playlist, Track};
use crate::tags::TrackTags;

/// The column list used by every query that returns tracks, kept in one place
/// so it always matches [`row_to_track`].
const TRACK_COLUMNS: &str = "id, path, folder_id, title, artist, album, album_artist, \
     track_no, disc_no, year, duration_ms, has_art, mtime";

/// The start of every query that returns folders, kept in one place so it
/// always matches [`row_to_folder`]. The track count and the total size of
/// the folder's tracks are each worked out by a sub-query for each folder.
/// `SUM` of no rows is NULL, not 0, so `COALESCE` supplies the 0 for an
/// empty folder. <https://www.sqlite.org/lang_corefunc.html#coalesce>
const FOLDER_SELECT: &str = "SELECT f.id, f.path, f.is_device,
        (SELECT COUNT(*) FROM tracks t WHERE t.folder_id = f.id),
        (SELECT COALESCE(SUM(t.size), 0) FROM tracks t WHERE t.folder_id = f.id)
     FROM folders f";

/// What the scanner hands to [`Db::upsert_tracks`] for one audio file.
#[derive(Debug, Clone)]
pub struct ScannedFile {
    pub path: String,
    /// Modification time and size, used to detect that a file changed.
    pub mtime: i64,
    pub size: i64,
    pub tags: TrackTags,
}

/// What the database remembers about a file, used by the scanner to decide
/// whether it needs to read the file again.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KnownFile {
    pub id: i64,
    pub mtime: i64,
    pub size: i64,
}

/// Handle to the library database.
///
/// Several threads use the database (the UI commands, the scanner, the
/// player). A SQLite connection must only be used by one thread at a time,
/// so it sits inside a `Mutex`: whoever calls `lock()` gets exclusive access
/// until their guard goes out of scope.
pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    /// Opens (creating if needed) the database file at `path`.
    pub fn open(path: &Path) -> AppResult<Self> {
        Self::from_connection(Connection::open(path)?)
    }

    /// Opens a throwaway database that lives only in memory. Used by tests.
    #[cfg(test)]
    pub fn open_in_memory() -> AppResult<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    /// Shared setup for both ways of opening.
    fn from_connection(conn: Connection) -> AppResult<Self> {
        // SQLite ignores foreign keys unless asked. We rely on them so that
        // removing a folder also removes its tracks (ON DELETE CASCADE).
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Locks the connection for the duration of one method.
    ///
    /// `lock()` only fails if another thread panicked while holding the
    /// lock. The data is still fine for our purposes, so we carry on.
    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    // ----------------------------------------------------------------- folders

    /// Adds a library folder and returns it. `is_device` marks it as a
    /// portable music player. Adding a folder that is already there is not
    /// an error; the existing one is returned (unchanged).
    pub fn add_folder(&self, path: &str, is_device: bool) -> AppResult<Folder> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO folders (path, is_device) VALUES (?1, ?2)
             ON CONFLICT(path) DO NOTHING",
            params![path, is_device],
        )?;
        let folder = conn.query_row(
            &format!("{FOLDER_SELECT} WHERE f.path = ?1"),
            params![path],
            row_to_folder,
        )?;
        Ok(folder)
    }

    /// One folder by id, or `None` if it is not in the library.
    pub fn get_folder(&self, folder_id: i64) -> AppResult<Option<Folder>> {
        let folder = self
            .conn()
            .query_row(
                &format!("{FOLDER_SELECT} WHERE f.id = ?1"),
                params![folder_id],
                row_to_folder,
            )
            .optional()?;
        Ok(folder)
    }

    /// Removes a library folder and (through the foreign key) all its tracks.
    /// The files on disk are not touched.
    pub fn remove_folder(&self, folder_id: i64) -> AppResult<()> {
        self.conn()
            .execute("DELETE FROM folders WHERE id = ?1", params![folder_id])?;
        Ok(())
    }

    /// All library folders, with how many tracks each contains.
    pub fn list_folders(&self) -> AppResult<Vec<Folder>> {
        let conn = self.conn();
        // Ordinary folders first, then devices, each group alphabetically.
        let mut stmt = conn.prepare(&format!(
            "{FOLDER_SELECT} ORDER BY f.is_device, f.path COLLATE NOCASE"
        ))?;
        // `query_map` yields one `Result<Folder>` per row; `collect` gathers
        // them into a single `Result<Vec<Folder>>`, stopping at the first error.
        let folders = stmt
            .query_map([], row_to_folder)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(folders)
    }

    // ------------------------------------------------------------------ tracks

    /// Every track in the library, in artist / album / track order. Tracks
    /// with no artist tag are listed last instead of first.
    pub fn list_tracks(&self) -> AppResult<Vec<Track>> {
        let conn = self.conn();
        // `artist = ''` is 0 for tagged tracks and 1 for untagged ones, so
        // sorting by it first pushes the untagged ones to the end.
        let mut stmt = conn.prepare(&format!(
            "SELECT {TRACK_COLUMNS} FROM tracks
             ORDER BY artist = '', artist COLLATE NOCASE, album COLLATE NOCASE,
                      disc_no, track_no, title COLLATE NOCASE"
        ))?;
        let tracks = stmt
            .query_map([], row_to_track)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(tracks)
    }

    /// One track by id, or `None` if it is no longer in the library.
    pub fn get_track(&self, track_id: i64) -> AppResult<Option<Track>> {
        let conn = self.conn();
        let track = conn
            .query_row(
                &format!("SELECT {TRACK_COLUMNS} FROM tracks WHERE id = ?1"),
                params![track_id],
                row_to_track,
            )
            .optional()?; // turns the "no rows" error into Ok(None)
        Ok(track)
    }

    /// What we already know about the files in one folder, keyed by path.
    /// The scanner compares this with what is on disk.
    pub fn known_files(&self, folder_id: i64) -> AppResult<HashMap<String, KnownFile>> {
        let conn = self.conn();
        let mut stmt =
            conn.prepare("SELECT path, id, mtime, size FROM tracks WHERE folder_id = ?1")?;
        let rows = stmt.query_map(params![folder_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                KnownFile {
                    id: row.get(1)?,
                    mtime: row.get(2)?,
                    size: row.get(3)?,
                },
            ))
        })?;
        Ok(rows.collect::<Result<HashMap<_, _>, _>>()?)
    }

    /// Inserts new tracks and updates existing ones (matched by path), all in
    /// one transaction. Updating keeps the track's id, so a song that is
    /// playing or queued stays valid after its tags are re-read.
    pub fn upsert_tracks(&self, folder_id: i64, files: &[ScannedFile]) -> AppResult<()> {
        let mut conn = self.conn();
        // A transaction makes the batch all-or-nothing and, in SQLite, is
        // also dramatically faster than committing row by row.
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO tracks
                    (path, folder_id, title, artist, album, album_artist, track_no, disc_no,
                     year, duration_ms, has_art, mtime, size)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                 ON CONFLICT(path) DO UPDATE SET
                    folder_id = excluded.folder_id, title = excluded.title,
                    artist = excluded.artist, album = excluded.album,
                    album_artist = excluded.album_artist, track_no = excluded.track_no,
                    disc_no = excluded.disc_no, year = excluded.year,
                    duration_ms = excluded.duration_ms, has_art = excluded.has_art,
                    mtime = excluded.mtime, size = excluded.size",
            )?;
            for file in files {
                let t = &file.tags;
                stmt.execute(params![
                    file.path,
                    folder_id,
                    t.title,
                    t.artist,
                    t.album,
                    t.album_artist,
                    t.track_no,
                    t.disc_no,
                    t.year,
                    t.duration_ms,
                    t.has_art,
                    file.mtime,
                    file.size,
                ])?;
            }
        } // `stmt` borrows `tx`, so it has to be dropped before the commit.
        tx.commit()?;
        Ok(())
    }

    /// Removes tracks from the library (not from disk).
    pub fn delete_tracks(&self, track_ids: &[i64]) -> AppResult<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare("DELETE FROM tracks WHERE id = ?1")?;
            for id in track_ids {
                stmt.execute(params![id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    // --------------------------------------------------------------- playlists
    //
    // Two tables hold playlists (see `migrate`): `playlists` has one row per
    // playlist, and `playlist_tracks` has one row per song in a playlist,
    // saying which playlist, which track and at what position. This shape
    // is the usual way to store a many-to-many relationship (a playlist has
    // many tracks; a track can be in many playlists).

    /// Creates an empty playlist. Names must be unique (ignoring case), so
    /// that two entries in the sidebar never look the same.
    pub fn create_playlist(&self, name: &str) -> AppResult<Playlist> {
        let conn = self.conn();
        // `OR IGNORE` makes a clash with the UNIQUE rule on `name` insert
        // nothing instead of failing. `execute` returns the number of rows
        // it changed, so 0 means the name was taken.
        let inserted = conn.execute(
            "INSERT OR IGNORE INTO playlists (name) VALUES (?1)",
            params![name],
        )?;
        if inserted == 0 {
            return Err(AppError::Other(format!(
                "There is already a playlist called \"{name}\""
            )));
        }
        Ok(Playlist {
            id: conn.last_insert_rowid(),
            name: name.to_string(),
            track_ids: Vec::new(),
        })
    }

    /// Deletes a playlist. Its songs stay in the library; only the list
    /// goes (its `playlist_tracks` rows are removed by the foreign key).
    pub fn delete_playlist(&self, playlist_id: i64) -> AppResult<()> {
        self.conn()
            .execute("DELETE FROM playlists WHERE id = ?1", params![playlist_id])?;
        Ok(())
    }

    /// Every playlist, in name order, each with the ids of its tracks in
    /// playlist order.
    pub fn list_playlists(&self) -> AppResult<Vec<Playlist>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT id, name FROM playlists ORDER BY name COLLATE NOCASE")?;
        let mut playlists = stmt
            .query_map([], |row| {
                Ok(Playlist {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    track_ids: Vec::new(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Fill in each playlist's tracks. One small query per playlist is
        // simple, and fast enough for the handful of playlists a person has.
        let mut stmt = conn.prepare(
            "SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
        )?;
        // `&mut playlists` lends each playlist out for changing in turn:
        // <https://doc.rust-lang.org/book/ch08-01-vectors.html#iterating-over-the-values-in-a-vector>
        for playlist in &mut playlists {
            playlist.track_ids = stmt
                .query_map(params![playlist.id], |row| row.get(0))?
                .collect::<Result<Vec<i64>, _>>()?;
        }
        Ok(playlists)
    }

    /// Adds tracks to the end of a playlist, in the order given, and
    /// returns how many were added. A track the playlist already has is
    /// skipped (a playlist holds each song once), as is an id that is not
    /// in the library.
    pub fn add_to_playlist(&self, playlist_id: i64, track_ids: &[i64]) -> AppResult<usize> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;

        let exists: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM playlists WHERE id = ?1)",
            params![playlist_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(AppError::Other("That playlist no longer exists".into()));
        }

        // New songs go after the last one. In an empty playlist `MAX` is
        // NULL, which `COALESCE` turns into -1 so that counting starts at 0.
        let mut position: i64 = tx.query_row(
            "SELECT COALESCE(MAX(position), -1) FROM playlist_tracks WHERE playlist_id = ?1",
            params![playlist_id],
            |row| row.get(0),
        )?;

        let mut added = 0;
        {
            // Selecting FROM tracks (instead of inserting the id directly)
            // inserts nothing for an id that is not in the library. `OR
            // IGNORE` inserts nothing for a song the playlist already has.
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO playlist_tracks (playlist_id, track_id, position)
                 SELECT ?1, id, ?3 FROM tracks WHERE id = ?2",
            )?;
            for track_id in track_ids {
                if stmt.execute(params![playlist_id, track_id, position + 1])? == 1 {
                    position += 1;
                    added += 1;
                }
            }
        } // `stmt` borrows `tx`, so it has to be dropped before the commit.
        tx.commit()?;
        Ok(added)
    }

    /// Takes tracks out of a playlist. The songs stay in the library.
    pub fn remove_from_playlist(&self, playlist_id: i64, track_ids: &[i64]) -> AppResult<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx
                .prepare("DELETE FROM playlist_tracks WHERE playlist_id = ?1 AND track_id = ?2")?;
            for track_id in track_ids {
                stmt.execute(params![playlist_id, track_id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    // ---------------------------------------------------------------- settings

    /// Reads a saved setting (volume, shuffle...), or `None` if never saved.
    pub fn get_setting(&self, key: &str) -> AppResult<Option<String>> {
        let value = self
            .conn()
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?;
        Ok(value)
    }

    /// Saves a setting, replacing any previous value.
    pub fn set_setting(&self, key: &str, value: &str) -> AppResult<()> {
        self.conn().execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

/// Creates the tables, or upgrades them if the database was made by an older
/// version of the app.
///
/// SQLite stores a number called `user_version` in the file. We use it as
/// "which schema is this". To change the schema later: add another
/// `if version < N` block below that alters the tables and bumps the number.
/// Never edit an existing block, or databases already on disk will not match.
fn migrate(conn: &Connection) -> AppResult<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;

    if version < 1 {
        conn.execute_batch(
            "BEGIN;
             CREATE TABLE folders (
                 id   INTEGER PRIMARY KEY,
                 path TEXT NOT NULL UNIQUE
             );
             CREATE TABLE tracks (
                 id           INTEGER PRIMARY KEY,
                 path         TEXT NOT NULL UNIQUE,
                 folder_id    INTEGER NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
                 title        TEXT NOT NULL,
                 artist       TEXT NOT NULL,
                 album        TEXT NOT NULL,
                 album_artist TEXT NOT NULL,
                 track_no     INTEGER,
                 disc_no      INTEGER,
                 year         INTEGER,
                 duration_ms  INTEGER NOT NULL,
                 has_art      INTEGER NOT NULL,
                 mtime        INTEGER NOT NULL,
                 size         INTEGER NOT NULL
             );
             CREATE INDEX tracks_by_folder ON tracks(folder_id);
             CREATE TABLE settings (
                 key   TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );
             PRAGMA user_version = 1;
             COMMIT;",
        )?;
    }

    // Version 2: folders can be marked as portable music players.
    if version < 2 {
        conn.execute_batch(
            "BEGIN;
             ALTER TABLE folders ADD COLUMN is_device INTEGER NOT NULL DEFAULT 0;
             PRAGMA user_version = 2;
             COMMIT;",
        )?;
    }

    // Version 3: playlists.
    //
    // `COLLATE NOCASE` on the name makes its UNIQUE rule ignore case, so
    // "Running" and "running" count as the same name.
    //
    // In `playlist_tracks`, both columns that point at other tables say
    // `ON DELETE CASCADE`: deleting a playlist removes its rows, and so
    // does a track leaving the library. The PRIMARY KEY on the pair is what
    // stops one song being in the same playlist twice.
    // <https://www.sqlite.org/foreignkeys.html#fk_actions>
    if version < 3 {
        conn.execute_batch(
            "BEGIN;
             CREATE TABLE playlists (
                 id   INTEGER PRIMARY KEY,
                 name TEXT NOT NULL UNIQUE COLLATE NOCASE
             );
             CREATE TABLE playlist_tracks (
                 playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
                 track_id    INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
                 position    INTEGER NOT NULL,
                 PRIMARY KEY (playlist_id, track_id)
             );
             CREATE INDEX playlist_tracks_by_track ON playlist_tracks(track_id);
             PRAGMA user_version = 3;
             COMMIT;",
        )?;
    }

    Ok(())
}

/// Converts one result row into a [`Track`]. The numbers are column
/// positions and must match [`TRACK_COLUMNS`].
fn row_to_track(row: &Row) -> rusqlite::Result<Track> {
    Ok(Track {
        id: row.get(0)?,
        path: row.get(1)?,
        folder_id: row.get(2)?,
        title: row.get(3)?,
        artist: row.get(4)?,
        album: row.get(5)?,
        album_artist: row.get(6)?,
        track_no: row.get(7)?,
        disc_no: row.get(8)?,
        year: row.get(9)?,
        duration_ms: row.get(10)?,
        has_art: row.get(11)?,
        mtime: row.get(12)?,
    })
}

/// Converts one result row into a [`Folder`]. The column positions must
/// match [`FOLDER_SELECT`].
fn row_to_folder(row: &Row) -> rusqlite::Result<Folder> {
    Ok(Folder {
        id: row.get(0)?,
        path: row.get(1)?,
        is_device: row.get(2)?,
        track_count: row.get(3)?,
        size_bytes: row.get(4)?,
        connected: true, // see the note on `Folder::connected`
    })
}

// Unit tests live next to the code they test. `cargo test` runs them; they
// are left out of the real app. See
// <https://doc.rust-lang.org/book/ch11-01-writing-tests.html>.
#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, title: &str, mtime: i64) -> ScannedFile {
        ScannedFile {
            path: path.to_string(),
            mtime,
            size: 100,
            tags: TrackTags {
                title: title.to_string(),
                artist: "Artist".to_string(),
                duration_ms: 1000,
                ..TrackTags::default()
            },
        }
    }

    #[test]
    fn adding_a_folder_twice_returns_the_same_folder() {
        let db = Db::open_in_memory().unwrap();
        let first = db.add_folder("/music", false).unwrap();
        let second = db.add_folder("/music", false).unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(db.list_folders().unwrap().len(), 1);
    }

    #[test]
    fn upsert_updates_in_place_and_keeps_the_id() {
        let db = Db::open_in_memory().unwrap();
        let folder = db.add_folder("/music", false).unwrap();

        db.upsert_tracks(folder.id, &[file("/music/a.mp3", "Old title", 1)])
            .unwrap();
        let before = db.list_tracks().unwrap();

        db.upsert_tracks(folder.id, &[file("/music/a.mp3", "New title", 2)])
            .unwrap();
        let after = db.list_tracks().unwrap();

        assert_eq!(after.len(), 1);
        assert_eq!(after[0].id, before[0].id);
        assert_eq!(after[0].title, "New title");
        assert_eq!(after[0].mtime, 2);
    }

    #[test]
    fn a_folder_reports_the_total_size_of_its_tracks() {
        let db = Db::open_in_memory().unwrap();
        let folder = db.add_folder("/music", false).unwrap();
        assert_eq!(folder.size_bytes, 0); // empty, not an error

        // The `file` helper makes every file 100 bytes.
        db.upsert_tracks(folder.id, &[file("/music/a.mp3", "A", 1), file("/music/b.mp3", "B", 1)])
            .unwrap();

        let folder = db.get_folder(folder.id).unwrap().unwrap();
        assert_eq!(folder.track_count, 2);
        assert_eq!(folder.size_bytes, 200);
    }

    #[test]
    fn removing_a_folder_removes_its_tracks() {
        let db = Db::open_in_memory().unwrap();
        let keep = db.add_folder("/keep", false).unwrap();
        let drop = db.add_folder("/drop", false).unwrap();
        db.upsert_tracks(keep.id, &[file("/keep/a.mp3", "A", 1)]).unwrap();
        db.upsert_tracks(drop.id, &[file("/drop/b.mp3", "B", 1)]).unwrap();

        db.remove_folder(drop.id).unwrap();

        let tracks = db.list_tracks().unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].title, "A");
        assert_eq!(db.list_folders().unwrap()[0].track_count, 1);
    }

    #[test]
    fn tracks_are_listed_by_artist_with_untagged_ones_last() {
        let db = Db::open_in_memory().unwrap();
        let folder = db.add_folder("/music", false).unwrap();
        let mut untagged = file("/music/untagged.mp3", "Untagged", 1);
        untagged.tags.artist = String::new();
        let mut zebra = file("/music/z.mp3", "Z song", 1);
        zebra.tags.artist = "zebra".to_string();
        let mut apple = file("/music/a.mp3", "A song", 1);
        apple.tags.artist = "Apple".to_string();
        db.upsert_tracks(folder.id, &[untagged, zebra, apple]).unwrap();

        let titles: Vec<String> =
            db.list_tracks().unwrap().into_iter().map(|t| t.title).collect();
        assert_eq!(titles, ["A song", "Z song", "Untagged"]);
    }

    #[test]
    fn known_files_and_delete() {
        let db = Db::open_in_memory().unwrap();
        let folder = db.add_folder("/music", false).unwrap();
        db.upsert_tracks(
            folder.id,
            &[file("/music/a.mp3", "A", 5), file("/music/b.mp3", "B", 6)],
        )
        .unwrap();

        let known = db.known_files(folder.id).unwrap();
        assert_eq!(known.len(), 2);
        assert_eq!(known["/music/a.mp3"].mtime, 5);

        db.delete_tracks(&[known["/music/a.mp3"].id]).unwrap();
        assert_eq!(db.list_tracks().unwrap().len(), 1);
        assert!(db.get_track(known["/music/a.mp3"].id).unwrap().is_none());
    }

    #[test]
    fn devices_are_remembered_and_listed_after_ordinary_folders() {
        let db = Db::open_in_memory().unwrap();
        let player = db.add_folder("/media/player", true).unwrap();
        db.add_folder("/zzz/music", false).unwrap();

        assert!(player.is_device);
        assert_eq!(db.get_folder(player.id).unwrap(), Some(player));
        assert_eq!(db.get_folder(999).unwrap(), None);

        let paths: Vec<String> = db.list_folders().unwrap().into_iter().map(|f| f.path).collect();
        assert_eq!(paths, ["/zzz/music", "/media/player"]);
    }

    /// A library of three songs, for the playlist tests. Returns the
    /// songs' ids in the order they were added.
    fn three_songs(db: &Db) -> Vec<i64> {
        let folder = db.add_folder("/music", false).unwrap();
        let files = [
            file("/music/a.mp3", "A", 1),
            file("/music/b.mp3", "B", 1),
            file("/music/c.mp3", "C", 1),
        ];
        db.upsert_tracks(folder.id, &files).unwrap();
        // All three have the same artist and no album, so they are listed
        // by title: A, B, C.
        db.list_tracks().unwrap().iter().map(|t| t.id).collect()
    }

    #[test]
    fn playlists_keep_their_songs_in_the_order_added() {
        let db = Db::open_in_memory().unwrap();
        let ids = three_songs(&db);
        let playlist = db.create_playlist("Mix").unwrap();

        assert_eq!(db.add_to_playlist(playlist.id, &[ids[2], ids[0]]).unwrap(), 2);
        assert_eq!(db.add_to_playlist(playlist.id, &[ids[1]]).unwrap(), 1);

        let listed = db.list_playlists().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Mix");
        assert_eq!(listed[0].track_ids, vec![ids[2], ids[0], ids[1]]);
    }

    #[test]
    fn a_playlist_holds_each_song_once_and_only_songs_that_exist() {
        let db = Db::open_in_memory().unwrap();
        let ids = three_songs(&db);
        let playlist = db.create_playlist("Mix").unwrap();
        db.add_to_playlist(playlist.id, &[ids[0]]).unwrap();

        // ids[0] is there already and 9999 is not a track: only ids[1] goes in.
        let added = db.add_to_playlist(playlist.id, &[ids[0], 9999, ids[1]]).unwrap();

        assert_eq!(added, 1);
        assert_eq!(db.list_playlists().unwrap()[0].track_ids, vec![ids[0], ids[1]]);
    }

    #[test]
    fn playlist_names_are_unique_ignoring_case() {
        let db = Db::open_in_memory().unwrap();
        db.create_playlist("Running").unwrap();

        let error = db.create_playlist("running").unwrap_err();
        assert!(error.to_string().contains("already a playlist"));
    }

    #[test]
    fn songs_can_be_taken_out_of_a_playlist() {
        let db = Db::open_in_memory().unwrap();
        let ids = three_songs(&db);
        let playlist = db.create_playlist("Mix").unwrap();
        db.add_to_playlist(playlist.id, &ids).unwrap();

        db.remove_from_playlist(playlist.id, &[ids[1]]).unwrap();

        assert_eq!(db.list_playlists().unwrap()[0].track_ids, vec![ids[0], ids[2]]);
        assert_eq!(db.list_tracks().unwrap().len(), 3); // the song itself stays
    }

    #[test]
    fn a_song_leaving_the_library_leaves_its_playlists() {
        let db = Db::open_in_memory().unwrap();
        let ids = three_songs(&db);
        let playlist = db.create_playlist("Mix").unwrap();
        db.add_to_playlist(playlist.id, &ids).unwrap();

        db.delete_tracks(&[ids[0]]).unwrap();

        assert_eq!(db.list_playlists().unwrap()[0].track_ids, vec![ids[1], ids[2]]);
    }

    #[test]
    fn deleting_a_playlist_keeps_its_songs_in_the_library() {
        let db = Db::open_in_memory().unwrap();
        let ids = three_songs(&db);
        let playlist = db.create_playlist("Mix").unwrap();
        db.add_to_playlist(playlist.id, &ids).unwrap();

        db.delete_playlist(playlist.id).unwrap();

        assert!(db.list_playlists().unwrap().is_empty());
        assert_eq!(db.list_tracks().unwrap().len(), 3);
        // Adding to it now is an error, not a silent nothing.
        assert!(db.add_to_playlist(playlist.id, &ids).is_err());
    }

    #[test]
    fn settings_round_trip() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.get_setting("volume").unwrap(), None);
        db.set_setting("volume", "0.5").unwrap();
        db.set_setting("volume", "0.8").unwrap();
        assert_eq!(db.get_setting("volume").unwrap().as_deref(), Some("0.8"));
    }
}
