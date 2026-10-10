//! Audio playback.
//!
//! # The shape of this file
//!
//! All sound is produced by one dedicated thread, the "player thread". The
//! rest of the app never touches audio directly. Instead it sends
//! [`Command`]s (play this, pause, next...) down a channel, and the player
//! thread reports back by calling a function with a fresh [`PlayerState`]
//! whenever something changes.
//!
//! ```text
//!   UI click -> commands.rs -> PlayerHandle::send(Command) --channel--> player thread
//!   UI update <- "player:state" event <- on_state(&PlayerState) <------ player thread
//! ```
//!
//! Why a dedicated thread? Two reasons. The audio output device cannot be
//! moved between threads, so something has to own it for the whole life of
//! the app. And the thread needs to wake up regularly on its own, to notice
//! that a song has finished and start the next one, even when the UI is idle.
//!
//! The thread's state lives in the private [`Engine`] struct. Because only
//! that one thread ever touches an `Engine`, none of its fields need locks.
//!
//! # Libraries
//!
//! `rodio` plays the audio. Note that rodio has its own type called
//! `Player` (one stream of sound that can be paused, sought and so on); in
//! this file it is always written out as `rodio::Player` and the variable
//! holding it is called `sink`.
//!
//! Learning links:
//! - Channels: <https://doc.rust-lang.org/book/ch16-02-message-passing.html>
//! - Enums that carry data (how `Command` works):
//!   <https://doc.rust-lang.org/book/ch06-01-defining-an-enum.html>
//! - `match`: <https://doc.rust-lang.org/book/ch06-02-match.html>
//! - rodio: <https://docs.rs/rodio>

use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink};

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::models::{PlaybackStatus, PlayerState, Track};
use crate::queue::Queue;

/// How often the player thread wakes up by itself to check whether the song
/// has ended and to send the UI a new position.
const TICK: Duration = Duration::from_millis(200);

/// "Previous" restarts the current song if it has played for longer than
/// this, and goes to the song before it otherwise.
const RESTART_THRESHOLD: Duration = Duration::from_secs(5);

/// Stop trying after this many songs in a row fail to play (for example
/// when the drive holding the music has been unplugged).
const MAX_FAILURES_IN_A_ROW: usize = 10;

const DEFAULT_VOLUME: f32 = 0.7;

/// Names under which the player's settings are saved in the database.
const SETTING_VOLUME: &str = "volume";
const SETTING_SHUFFLE: &str = "shuffle";

/// Names under which the "session" is saved, so the app can reopen on the
/// song it was closed on: the id of that song, and the queue it belonged to
/// (track ids separated by commas, in the order the UI listed them).
const SETTING_LAST_TRACK: &str = "last_track";
const SETTING_LAST_QUEUE: &str = "last_queue";

/// Everything the player thread can be asked to do.
#[derive(Debug)]
pub enum Command {
    /// Replace the queue with these tracks and start playing the one at
    /// `start_index`.
    PlayQueue {
        track_ids: Vec<i64>,
        start_index: usize,
    },
    /// Pause if playing, resume if paused, start the current song if stopped.
    TogglePause,
    Stop,
    Next,
    Previous,
    /// Jump to a position within the current song.
    Seek(Duration),
    /// 0.0 to 1.0.
    SetVolume(f32),
    SetShuffle(bool),
}

/// The rest of the app's connection to the player thread. Cheap to clone;
/// every clone talks to the same thread.
#[derive(Clone)]
pub struct PlayerHandle {
    tx: Sender<Command>,
    /// The most recent state published by the player thread, kept so that
    /// [`PlayerHandle::state`] can answer without asking the thread.
    latest: Arc<Mutex<PlayerState>>,
}

impl PlayerHandle {
    /// Queues a command for the player thread and returns immediately.
    pub fn send(&self, command: Command) {
        // Fails only if the player thread is gone (app shutting down).
        let _ = self.tx.send(command);
    }

    /// The player's most recently published state.
    pub fn state(&self) -> PlayerState {
        self.latest
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

/// Starts the player thread and returns the handle used to control it.
///
/// `on_state` is called from the player thread every time the state changes
/// and a few times a second during playback.
pub fn spawn_player<F>(db: Arc<Db>, on_state: F) -> PlayerHandle
where
    F: Fn(&PlayerState) + Send + 'static,
{
    let (tx, rx) = mpsc::channel();

    // Restore what the user had last time.
    let volume = db
        .get_setting(SETTING_VOLUME)
        .ok()
        .flatten()
        .and_then(|text| text.parse::<f32>().ok())
        .map(|volume| volume.clamp(0.0, 1.0))
        .unwrap_or(DEFAULT_VOLUME);
    let shuffle = db.get_setting(SETTING_SHUFFLE).ok().flatten().as_deref() == Some("true");

    // Bring back the song the app was closed on, and its queue. The song is
    // shown as the current one but stays stopped: nothing is decoded and no
    // sound device is opened until the user presses play.
    let mut queue = Queue::new(shuffle);
    let current = restore_last_session(&db, &mut queue);
    // `as_ref()` turns `&Option<Track>` into `Option<&Track>`, so `map` can
    // look inside without taking the track out of `current`.
    // <https://doc.rust-lang.org/std/option/enum.Option.html#method.as_ref>
    let saved_track_id = current.as_ref().map(|track| track.id);

    let engine = Engine {
        db,
        on_state: Box::new(on_state),
        // This first state is what the UI receives when it asks at startup
        // (`get_player_state`), so the restored song has to be in it.
        latest: Arc::new(Mutex::new(PlayerState {
            status: PlaybackStatus::Stopped,
            track: current.clone(),
            position_ms: 0,
            duration_ms: duration_of(current.as_ref()),
            shuffle,
            volume,
            queue_length: queue.len(),
            queue_position: queue.position(),
            error: None,
        })),
        device: None,
        sink: None,
        queue,
        current,
        status: PlaybackStatus::Stopped,
        volume,
        error: None,
        settings_changed: false,
        queue_changed: false,
        saved_track_id,
    };
    let latest = Arc::clone(&engine.latest);

    // `move` hands ownership of `engine` and `rx` to the new thread.
    thread::spawn(move || engine.run(rx));

    PlayerHandle { tx, latest }
}

/// Reloads the song that was current when the app last ran, and the queue
/// it came from, into `queue`. Returns the song's library details, or
/// `None` if nothing was saved or the song has since left the library.
///
/// The other half of this is [`Engine::save_session_if_changed`].
///
/// Inside a function that returns `Option`, the `?` operator means "if
/// this is `None`, return `None` right now", just as it returns the `Err`
/// in a function that returns `Result`. The chains below also use
/// `.ok()` (turn a `Result` into an `Option`, dropping the error) and
/// `.flatten()` (turn `Option<Option<T>>` into `Option<T>`).
/// <https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html#where-the--operator-can-be-used>
fn restore_last_session(db: &Db, queue: &mut Queue) -> Option<Track> {
    let saved_id = db.get_setting(SETTING_LAST_TRACK).ok().flatten()?;
    let track_id = saved_id.parse::<i64>().ok()?;
    let track = db.get_track(track_id).ok().flatten()?;

    // The saved queue is text like "12,7,31". Anything that is not a number
    // is skipped (`filter_map` keeps only the `Some` results).
    let saved_queue = db.get_setting(SETTING_LAST_QUEUE).ok().flatten();
    let mut track_ids: Vec<i64> = saved_queue
        .as_deref()
        .unwrap_or_default()
        .split(',')
        .filter_map(|id| id.parse().ok())
        .collect();

    // Find the song's place in its queue. If the queue was lost, or does
    // not contain the song, fall back to a queue of just that song.
    let start_index = match track_ids.iter().position(|&id| id == track_id) {
        Some(index) => index,
        None => {
            track_ids = vec![track_id];
            0
        }
    };
    queue.set_tracks(track_ids, start_index);
    Some(track)
}

/// A track's length in the form `PlayerState` wants it, or 0 for no track.
fn duration_of(track: Option<&Track>) -> u64 {
    track.map(|track| track.duration_ms.max(0) as u64).unwrap_or(0)
}

/// The player thread's private state.
struct Engine {
    db: Arc<Db>,
    /// Called with every new state. `Box<dyn Fn...>` means "some function,
    /// we do not know which one at compile time", like a delegate in C#.
    on_state: Box<dyn Fn(&PlayerState) + Send>,
    latest: Arc<Mutex<PlayerState>>,

    /// The open sound card connection. Opened the first time something is
    /// played, then kept for the life of the app.
    device: Option<MixerDeviceSink>,
    /// The song currently loaded, as a rodio stream. `None` when stopped.
    sink: Option<rodio::Player>,

    queue: Queue,
    /// Library details of the current song.
    current: Option<Track>,
    status: PlaybackStatus,
    volume: f32,
    error: Option<String>,
    /// True when volume or shuffle changed and has not been saved yet.
    settings_changed: bool,
    /// True when the queue was replaced and has not been saved yet.
    queue_changed: bool,
    /// Id of the song last written to the database as "the last song
    /// played", used to notice that the current song is a different one.
    saved_track_id: Option<i64>,
}

impl Engine {
    /// The player thread's main loop. Runs until the app exits.
    fn run(mut self, rx: Receiver<Command>) {
        loop {
            // Wait for a command, but for no longer than one tick.
            match rx.recv_timeout(TICK) {
                Ok(command) => {
                    self.handle(command);
                    self.publish();
                }
                Err(RecvTimeoutError::Timeout) => {
                    // Nothing asked of us: do the periodic housekeeping.
                    let was_playing = self.status == PlaybackStatus::Playing;
                    self.advance_if_song_ended();
                    self.save_settings_if_changed();
                    self.save_session_if_changed();
                    if was_playing {
                        self.publish(); // the position moved
                    }
                }
                // Every PlayerHandle is gone: the app is closing. Save
                // anything changed since the last tick before leaving.
                Err(RecvTimeoutError::Disconnected) => {
                    self.save_settings_if_changed();
                    self.save_session_if_changed();
                    break;
                }
            }
        }
    }

    /// Carries out one command from the UI.
    fn handle(&mut self, command: Command) {
        // A new action clears whatever error the last one left behind.
        self.error = None;

        match command {
            Command::PlayQueue {
                track_ids,
                start_index,
            } => {
                self.queue.set_tracks(track_ids, start_index);
                self.queue_changed = true;
                self.start_current();
            }

            Command::TogglePause => match self.status {
                PlaybackStatus::Playing => {
                    if let Some(sink) = &self.sink {
                        sink.pause();
                    }
                    self.status = PlaybackStatus::Paused;
                }
                PlaybackStatus::Paused => {
                    if let Some(sink) = &self.sink {
                        sink.play();
                    }
                    self.status = PlaybackStatus::Playing;
                }
                PlaybackStatus::Stopped => self.start_current(),
            },

            Command::Stop => self.stop(),

            Command::Next => {
                // On the last song of a linear queue there is no next one;
                // keep playing. (A shuffled queue wraps round instead.)
                if self.queue.advance().is_some() {
                    self.start_current();
                }
            }

            Command::Previous => {
                // Early in a song, "previous" means the song before. Later
                // on (or on the first song of a linear queue) it means
                // "from the top".
                if self.position() <= RESTART_THRESHOLD {
                    self.queue.go_back();
                }
                self.start_current();
            }

            Command::Seek(position) => {
                if let Some(sink) = &self.sink {
                    if let Err(error) = sink.try_seek(position) {
                        self.error = Some(format!("Could not seek in this file: {error}"));
                    }
                }
            }

            Command::SetVolume(volume) => {
                self.volume = volume.clamp(0.0, 1.0);
                if let Some(sink) = &self.sink {
                    sink.set_volume(loudness(self.volume));
                }
                self.settings_changed = true;
            }

            Command::SetShuffle(shuffle) => {
                self.queue.set_shuffle(shuffle);
                self.settings_changed = true;
            }
        }
    }

    /// Called every tick: if the song that was playing has run out, move on
    /// to the next one, or stop at the end of the queue. (Only a linear
    /// queue has an end; see `queue.rs`.)
    fn advance_if_song_ended(&mut self) {
        if self.status != PlaybackStatus::Playing {
            return;
        }
        // rodio's `empty()` becomes true once the sound we gave it has
        // finished.
        let ended = self.sink.as_ref().is_none_or(|sink| sink.empty());
        if !ended {
            return;
        }

        self.error = None;
        if self.queue.advance().is_some() {
            self.start_current();
        } else {
            self.stop();
        }
    }

    /// Starts playing the queue's current song from the beginning. A song
    /// that cannot be played is skipped, and the next one is tried.
    fn start_current(&mut self) {
        // Check for a working sound device first. Without one *every* song
        // would fail, and we do not want to race through the whole queue.
        if let Err(error) = self.open_device() {
            self.error = Some(error.to_string());
            self.stop();
            return;
        }

        let mut failures = 0;
        loop {
            let Some(track_id) = self.queue.current() else {
                self.stop(); // the queue is empty
                return;
            };

            match self.load(track_id) {
                Ok(track) => {
                    self.current = Some(track);
                    self.status = PlaybackStatus::Playing;
                    return;
                }
                Err(error) => {
                    failures += 1;
                    self.error = Some(error.to_string());
                    if failures >= MAX_FAILURES_IN_A_ROW || self.queue.advance().is_none() {
                        self.stop();
                        return;
                    }
                }
            }
        }
    }

    /// Looks up a track and begins playing it, replacing whatever was
    /// playing. Returns the track's details on success.
    fn load(&mut self, track_id: i64) -> AppResult<Track> {
        let track = self
            .db
            .get_track(track_id)?
            .ok_or_else(|| AppError::Other("That song is no longer in the library".into()))?;

        // Read the whole file into memory and decode from there, instead of
        // streaming from disk. That costs some RAM (a FLAC is typically
        // 20-40 MB) but it means the file on disk is free while the song
        // plays, so embedding new cover art into the song you are listening
        // to cannot disturb playback.
        let bytes = fs::read(&track.path)
            .map_err(|error| AppError::Audio(format!("Could not open {}: {error}", track.path)))?;
        let byte_len = bytes.len() as u64;

        let mut builder = Decoder::builder()
            .with_data(Cursor::new(bytes))
            // Knowing the length is what makes seeking possible.
            .with_byte_len(byte_len)
            .with_seekable(true);
        // The file extension helps the decoder guess the format quickly.
        if let Some(extension) = Path::new(&track.path).extension().and_then(|e| e.to_str()) {
            builder = builder.with_hint(extension);
        }
        let source = builder
            .build()
            .map_err(|error| AppError::Audio(format!("Could not play {}: {error}", track.path)))?;

        let device = self.open_device()?;
        let sink = rodio::Player::connect_new(device.mixer());
        sink.set_volume(loudness(self.volume));
        sink.append(source);

        // Assigning drops the previous `rodio::Player`, which silences it.
        self.sink = Some(sink);
        Ok(track)
    }

    /// Returns the sound output device, opening it on first use.
    fn open_device(&mut self) -> AppResult<&MixerDeviceSink> {
        if self.device.is_none() {
            let device = DeviceSinkBuilder::open_default_sink()
                .map_err(|error| AppError::Audio(format!("No sound output available: {error}")))?;
            self.device = Some(device);
        }
        // The `if` above guarantees there is a device by this point.
        Ok(self.device.as_ref().expect("device was just opened"))
    }

    /// Stops playback. The current song stays selected, so pressing play
    /// afterwards starts it again from the beginning.
    fn stop(&mut self) {
        self.sink = None;
        self.status = PlaybackStatus::Stopped;
    }

    /// How far into the current song we are.
    fn position(&self) -> Duration {
        self.sink
            .as_ref()
            .map(|sink| sink.get_pos())
            .unwrap_or_default()
    }

    /// Writes volume and shuffle to the database if they changed.
    ///
    /// This runs on the tick rather than straight away because dragging the
    /// volume slider sends dozens of changes a second, and each save is a
    /// disk write.
    fn save_settings_if_changed(&mut self) {
        if !self.settings_changed {
            return;
        }
        self.settings_changed = false;
        // Losing a saved setting is harmless, so errors are ignored.
        let _ = self
            .db
            .set_setting(SETTING_VOLUME, &self.volume.to_string());
        let _ = self
            .db
            .set_setting(SETTING_SHUFFLE, &self.queue.shuffle().to_string());
    }

    /// Remembers the current song and its queue in the database, so the
    /// next launch can reopen on them (see [`restore_last_session`]).
    ///
    /// Like the settings above, this is saved as things change, on the
    /// tick, and not at the moment the app closes: a desktop app can be
    /// killed without warning, and what was saved a moment ago survives
    /// that. Each part is only written when it differs from what is
    /// already saved, so an idle tick costs nothing.
    fn save_session_if_changed(&mut self) {
        if self.queue_changed {
            self.queue_changed = false;
            // Turn [12, 7, 31] into "12,7,31".
            let ids: Vec<String> = self
                .queue
                .tracks()
                .iter()
                .map(|id| id.to_string())
                .collect();
            let _ = self.db.set_setting(SETTING_LAST_QUEUE, &ids.join(","));
        }

        let current_id = self.current.as_ref().map(|track| track.id);
        if current_id != self.saved_track_id {
            self.saved_track_id = current_id;
            if let Some(track_id) = current_id {
                let _ = self
                    .db
                    .set_setting(SETTING_LAST_TRACK, &track_id.to_string());
            }
        }
    }

    /// Builds a snapshot of the current state, stores it for
    /// [`PlayerHandle::state`], and hands it to the `on_state` callback.
    fn publish(&mut self) {
        let state = PlayerState {
            status: self.status,
            track: self.current.clone(),
            position_ms: self.position().as_millis() as u64,
            duration_ms: duration_of(self.current.as_ref()),
            shuffle: self.queue.shuffle(),
            volume: self.volume,
            queue_length: self.queue.len(),
            queue_position: self.queue.position(),
            error: self.error.clone(),
        };

        *self
            .latest
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = state.clone();
        (self.on_state)(&state);
    }
}

/// Converts the slider position (0.0 to 1.0) into the amplification factor
/// given to rodio.
///
/// Hearing is roughly logarithmic: halving the amplitude sounds like much
/// less than "half as loud". Squaring the slider value is a cheap
/// approximation that makes the slider feel even along its whole length.
fn loudness(slider: f32) -> f32 {
    slider * slider
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ScannedFile;
    use crate::tags::TrackTags;
    use crate::test_support::write_wav;

    /// An in-memory library holding three songs. Returns it with the songs'
    /// ids, in the order the library lists them.
    fn library_of_three() -> (Db, Vec<i64>) {
        let db = Db::open_in_memory().unwrap();
        let folder = db.add_folder("/music", false).unwrap();
        let files: Vec<ScannedFile> = ["a", "b", "c"]
            .iter()
            .map(|name| ScannedFile {
                path: format!("/music/{name}.mp3"),
                mtime: 1,
                size: 100,
                tags: TrackTags {
                    title: name.to_string(),
                    duration_ms: 3000,
                    ..TrackTags::default()
                },
            })
            .collect();
        db.upsert_tracks(folder.id, &files).unwrap();
        let ids = db.list_tracks().unwrap().iter().map(|t| t.id).collect();
        (db, ids)
    }

    #[test]
    fn with_nothing_saved_nothing_is_restored() {
        let (db, _) = library_of_three();
        let mut queue = Queue::new(false);

        assert_eq!(restore_last_session(&db, &mut queue), None);
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn the_last_song_is_restored_at_its_place_in_the_queue() {
        let (db, ids) = library_of_three();
        let saved_queue = format!("{},{},{}", ids[0], ids[1], ids[2]);
        db.set_setting(SETTING_LAST_QUEUE, &saved_queue).unwrap();
        db.set_setting(SETTING_LAST_TRACK, &ids[1].to_string()).unwrap();
        let mut queue = Queue::new(false);

        let track = restore_last_session(&db, &mut queue).unwrap();

        assert_eq!(track.id, ids[1]);
        assert_eq!(queue.current(), Some(ids[1]));
        assert_eq!(queue.len(), 3);
        assert_eq!(queue.advance(), Some(ids[2])); // "next" carries on down the list
    }

    #[test]
    fn a_song_missing_from_the_saved_queue_is_queued_by_itself() {
        let (db, ids) = library_of_three();
        db.set_setting(SETTING_LAST_QUEUE, &format!("{},junk", ids[0])).unwrap();
        db.set_setting(SETTING_LAST_TRACK, &ids[2].to_string()).unwrap();
        let mut queue = Queue::new(false);

        let track = restore_last_session(&db, &mut queue).unwrap();

        assert_eq!(track.id, ids[2]);
        assert_eq!(queue.tracks(), &[ids[2]]);
    }

    #[test]
    fn a_song_no_longer_in_the_library_is_not_restored() {
        let (db, ids) = library_of_three();
        db.set_setting(SETTING_LAST_TRACK, &ids[0].to_string()).unwrap();
        db.delete_tracks(&[ids[0]]).unwrap();
        let mut queue = Queue::new(false);

        assert_eq!(restore_last_session(&db, &mut queue), None);
        assert_eq!(queue.len(), 0);
    }

    /// The decoding half of `load`, without needing a sound card: make sure
    /// a file read into memory can be decoded and reports samples.
    #[test]
    fn a_file_in_memory_can_be_decoded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tone.wav");
        write_wav(&path, 1);

        let bytes = fs::read(&path).unwrap();
        let byte_len = bytes.len() as u64;
        let source = Decoder::builder()
            .with_data(Cursor::new(bytes))
            .with_byte_len(byte_len)
            .with_seekable(true)
            .with_hint("wav")
            .build()
            .unwrap();

        // One second at 8000 Hz mono is 8000 samples.
        assert_eq!(source.count(), 8000);
    }

    #[test]
    fn loudness_curve_keeps_the_ends_fixed() {
        assert_eq!(loudness(0.0), 0.0);
        assert_eq!(loudness(1.0), 1.0);
        assert!(loudness(0.5) < 0.5);
    }
}
