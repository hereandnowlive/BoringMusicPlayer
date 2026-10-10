// The one place where the UI talks to the Rust backend.
//
// Components never call Tauri directly; they call the functions below. That
// keeps command names and argument shapes in a single file, with types.
//
// Three kinds of thing live here:
//   1. Commands  - the UI asks Rust to do something   (invoke)
//   2. Events    - Rust tells the UI something changed (subscribe)
//   3. Helpers   - cover art URLs and native file dialogs
//
// Each command matches a `#[tauri::command]` function of the same name in
// src-tauri/src/commands.rs.
//
// Docs: https://tauri.app/develop/calling-rust/

import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  CopyProgress,
  CopyResult,
  FileUpdateResult,
  Folder,
  PlayerState,
  Playlist,
  PlaylistAddResult,
  ScanProgress,
  ScanSummary,
  Track,
} from "./types";

// ---------------------------------------------------------------- 1. Commands

export const listTracks = () => invoke<Track[]>("list_tracks");
export const listFolders = () => invoke<Folder[]>("list_folders");
/** Adds a library folder. `isDevice` marks it as a portable music player. */
export const addFolder = (path: string, isDevice: boolean) =>
  invoke<Folder>("add_folder", { path, isDevice });
/** Copies tracks onto a device folder. Progress arrives as `copy:progress` events. */
export const copyToDevice = (folderId: number, trackIds: number[]) =>
  invoke<CopyResult>("copy_to_device", { folderId, trackIds });
export const removeFolder = (folderId: number) => invoke<void>("remove_folder", { folderId });
export const rescanLibrary = () => invoke<void>("rescan_library");

export const listPlaylists = () => invoke<Playlist[]>("list_playlists");
/** Creates an empty playlist. Rejects if the name is empty or already used. */
export const createPlaylist = (name: string) => invoke<Playlist>("create_playlist", { name });
/** Deletes a playlist. Its songs stay in the library. */
export const deletePlaylist = (playlistId: number) =>
  invoke<void>("delete_playlist", { playlistId });
/** Adds tracks to the end of a playlist; ones it already has are skipped. */
export const addToPlaylist = (playlistId: number, trackIds: number[]) =>
  invoke<PlaylistAddResult>("add_to_playlist", { playlistId, trackIds });
/** Takes tracks out of a playlist. They stay in the library. */
export const removeFromPlaylist = (playlistId: number, trackIds: number[]) =>
  invoke<void>("remove_from_playlist", { playlistId, trackIds });

/** Reads a small value saved by `setSetting`; null if never saved. */
export const getSetting = (key: string) => invoke<string | null>("get_setting", { key });
/** Saves a small value that should survive closing the app. */
export const setSetting = (key: string, value: string) =>
  invoke<void>("set_setting", { key, value });

/** Plays `trackIds[startIndex]` and queues the rest of the list after it. */
export const playTracks = (trackIds: number[], startIndex: number) =>
  invoke<void>("play_tracks", { trackIds, startIndex });
export const togglePause = () => invoke<void>("toggle_pause");
export const stop = () => invoke<void>("stop");
export const nextTrack = () => invoke<void>("next_track");
export const previousTrack = () => invoke<void>("previous_track");
export const seek = (positionMs: number) =>
  invoke<void>("seek", { positionMs: Math.round(positionMs) });
export const setVolume = (volume: number) => invoke<void>("set_volume", { volume });
export const setShuffle = (shuffle: boolean) => invoke<void>("set_shuffle", { shuffle });
export const getPlayerState = () => invoke<PlayerState>("get_player_state");

/** Embeds an image file as the cover of the given tracks. Rewrites the audio files. */
export const setCoverArt = (trackIds: number[], imagePath: string) =>
  invoke<FileUpdateResult>("set_cover_art", { trackIds, imagePath });
/** Removes embedded pictures from the given tracks. Rewrites the audio files. */
export const removeCoverArt = (trackIds: number[]) =>
  invoke<FileUpdateResult>("remove_cover_art", { trackIds });

/**
 * Asks where to save a copy of the track's embedded cover, then writes it
 * there as an image file. Resolves to the path it was saved to, or null if
 * the user cancelled. The backend opens the "save as" dialog itself.
 */
export const saveCoverArt = (trackId: number) =>
  invoke<string | null>("save_cover_art", { trackId });

/** Text tags to change. Leave a field out to keep it; "" clears it. */
export interface TagChanges {
  title?: string;
  artist?: string;
  album?: string;
}
/** Changes title/artist/album of the given tracks. Rewrites the audio files. */
export const setTrackTags = (trackIds: number[], changes: TagChanges) =>
  invoke<FileUpdateResult>("set_track_tags", { trackIds, ...changes });

// ------------------------------------------------------------------ 2. Events

/** Every event the backend emits, with the type of data it carries. */
export interface BackendEvents {
  "player:state": PlayerState;
  "scan:progress": ScanProgress;
  "scan:finished": ScanSummary;
  "copy:progress": CopyProgress;
  /** Tracks or folders changed outside of a scan; reload the lists. */
  "library:changed": null;
  /** A playlist was created, deleted or had songs added or removed. */
  "playlists:changed": null;
}

/**
 * Listens for a backend event. Returns a function that stops listening.
 *
 * Tauri's `listen` is asynchronous (it resolves to the "stop" function a
 * moment later). This wrapper hides that, so callers get a stop function
 * straight away, which is what React effects need.
 */
export function subscribe<K extends keyof BackendEvents>(
  event: K,
  handler: (payload: BackendEvents[K]) => void,
): () => void {
  let stopped = false;
  let unlisten: (() => void) | undefined;

  void listen<BackendEvents[K]>(event, (e) => handler(e.payload)).then((fn) => {
    if (stopped) fn(); // stop was requested before listening had even begun
    else unlisten = fn;
  });

  return () => {
    stopped = true;
    unlisten?.();
  };
}

// ----------------------------------------------------------------- 3. Helpers

/**
 * The address of a track's embedded cover, for use as an <img> src, or null
 * if the track has none. Pass `size` (pixels) for a small cached thumbnail;
 * leave it out for the original image.
 *
 * These `art://` addresses are answered by src-tauri/src/art_protocol.rs.
 */
export function artUrl(track: Track, size?: number): string | null {
  if (!track.hasArt) return null;
  // `v` changes whenever the file does, so a new cover is never hidden by
  // an old cached image.
  const query = size ? `?size=${size}&v=${track.mtime}` : `?v=${track.mtime}`;
  return convertFileSrc(String(track.id), "art") + query;
}

/** Shows the native "choose a folder" dialog. Resolves to null if cancelled. */
export async function pickFolder(title = "Add a music folder"): Promise<string | null> {
  return await open({ directory: true, multiple: false, title });
}

/** Shows the native "choose a file" dialog for images. Null if cancelled. */
export async function pickImage(): Promise<string | null> {
  return await open({
    multiple: false,
    title: "Choose cover art",
    filters: [{ name: "Images", extensions: ["jpg", "jpeg", "png", "webp", "bmp", "gif"] }],
  });
}

/** Turns whatever a failed command threw into a message for the user. */
export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return String(error);
}
