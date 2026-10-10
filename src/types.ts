// The shapes of the data exchanged with the Rust backend.
//
// Every type here mirrors a struct in src-tauri/src/models.rs. If you change
// a field there, change it here too: nothing checks that the two agree.
// (Rust's snake_case fields arrive here as camelCase.)

/** One song in the library. */
export interface Track {
  id: number;
  /** Absolute path of the audio file. */
  path: string;
  folderId: number;
  title: string;
  /** Empty string when the file has no artist tag. */
  artist: string;
  album: string;
  albumArtist: string;
  trackNo: number | null;
  discNo: number | null;
  year: number | null;
  durationMs: number;
  /** Whether the file has an embedded picture. */
  hasArt: boolean;
  /** File modification time; used to keep cover art URLs fresh. */
  mtime: number;
}

/** A folder the user added to the library. */
export interface Folder {
  id: number;
  path: string;
  trackCount: number;
  /** Combined size of the folder's songs, in bytes (other files not counted). */
  sizeBytes: number;
  /** True for a portable music player that songs can be copied onto. */
  isDevice: boolean;
  /** False while the folder cannot be reached (device unplugged). */
  connected: boolean;
}

/** A hand-made list of songs. Holds track ids, not copies of the songs. */
export interface Playlist {
  id: number;
  name: string;
  /** Ids of its tracks, in playlist order (the order they were added). */
  trackIds: number[];
}

/** Result of adding songs to a playlist. */
export interface PlaylistAddResult {
  added: number;
  /** How many were left out because the playlist already had them. */
  alreadyPresent: number;
}

export type PlaybackStatus = "stopped" | "playing" | "paused";

/** Snapshot of the player, sent several times a second during playback. */
export interface PlayerState {
  status: PlaybackStatus;
  track: Track | null;
  positionMs: number;
  durationMs: number;
  shuffle: boolean;
  /** 0 to 1. */
  volume: number;
  queueLength: number;
  queuePosition: number;
  /** Set when the last attempt to play something failed. */
  error: string | null;
}

/** Progress of a running library scan. */
export interface ScanProgress {
  folder: string;
  done: number;
  total: number;
}

/** What a finished library scan did. */
export interface ScanSummary {
  added: number;
  updated: number;
  removed: number;
  failed: number;
  errors: string[];
}

/** Progress of copying songs to a device. */
export interface CopyProgress {
  done: number;
  total: number;
  /** Name of the file being copied now. */
  current: string;
}

/** Result of copying songs to a device. */
export interface CopyResult {
  copied: number;
  /** Titles of the songs that were not copied because the device has them. */
  alreadyPresent: string[];
  failures: string[];
}

/** Result of changing several files at once (cover art or text tags). */
export interface FileUpdateResult {
  updated: number;
  failures: string[];
}
