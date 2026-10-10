// Pure functions for searching and sorting the track list. No React and no
// backend calls here, which keeps this logic easy to follow and to test.

import { dropTargetValue, type DropTarget } from "./hooks/useTrackDrag";
import type { Folder, Playlist, Track } from "./types";

/**
 * What the song list is showing: everything, one library folder, or one
 * playlist. The sidebar chooses it.
 *
 * This is a "discriminated union": several object shapes that share a
 * `kind` field. Checking `view.kind` tells TypeScript which shape it is, so
 * `view.id` is only reachable where it exists. (Rust's enums-with-data are
 * the same idea.)
 * https://www.typescriptlang.org/docs/handbook/2/narrowing.html#discriminated-unions
 */
export type LibraryView =
  | { kind: "all" }
  | { kind: "folder"; id: number }
  | { kind: "playlist"; id: number };

/** The view every start begins with. */
export const ALL_SONGS: LibraryView = { kind: "all" };

/** Which column the list is sorted by. "default" is artist / album / track. */
export type SortKey = "default" | "title" | "artist" | "album" | "duration";

export interface Sort {
  key: SortKey;
  descending: boolean;
}

/**
 * Compares text the way a person would: ignoring case and accents, and
 * treating digits as numbers ("Track 2" before "Track 10").
 */
const collator = new Intl.Collator(undefined, { sensitivity: "base", numeric: true });

/**
 * Returns the tracks a view shows, before searching and sorting.
 *
 * - "All songs" is everything except what is on devices: the songs on an
 *   MP3 player are copies, and listing them too would show (and shuffle)
 *   every copied song twice.
 * - A folder is the tracks scanned from it.
 * - A playlist is its tracks in playlist order, wherever they live.
 */
export function tracksInView(
  tracks: Track[],
  tracksById: ReadonlyMap<number, Track>,
  view: LibraryView,
  deviceFolderIds: ReadonlySet<number>,
  playlists: Playlist[],
): Track[] {
  switch (view.kind) {
    case "all":
      if (deviceFolderIds.size === 0) return tracks;
      return tracks.filter((track) => !deviceFolderIds.has(track.folderId));
    case "folder":
      return tracks.filter((track) => track.folderId === view.id);
    case "playlist": {
      const playlist = playlists.find((p) => p.id === view.id);
      if (!playlist) return [];
      // Look each id up; one that is momentarily unknown (the two lists
      // are loaded separately) is skipped. `flatMap` with "one item or
      // none" is a map and a filter in one step.
      return playlist.trackIds.flatMap((id) => {
        const track = tracksById.get(id);
        return track ? [track] : [];
      });
    }
  }
}

/**
 * Returns the tracks matching the search text. Every word typed must appear
 * somewhere in the title, artist or album.
 */
export function filterTracks(tracks: Track[], search: string): Track[] {
  const words = search.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return tracks;

  return tracks.filter((track) => {
    const haystack = `${track.title}\n${track.artist}\n${track.album}`.toLowerCase();
    return words.every((word) => haystack.includes(word));
  });
}

/**
 * Returns the tracks sorted by the chosen column.
 *
 * The backend already delivers tracks in artist / album / disc / track
 * order, and a playlist's tracks arrive in playlist order; "default" keeps
 * whichever it is given. JavaScript's sort is "stable" (equal items keep their relative
 * order), so sorting by, say, album leaves each album's songs in track order.
 */
export function sortTracks(tracks: Track[], sort: Sort): Track[] {
  if (sort.key === "default") {
    return sort.descending ? [...tracks].reverse() : tracks;
  }

  const compare = comparators[sort.key];
  const sorted = [...tracks].sort(compare);
  return sort.descending ? sorted.reverse() : sorted;
}

const comparators: Record<Exclude<SortKey, "default">, (a: Track, b: Track) => number> = {
  title: (a, b) => collator.compare(a.title, b.title),
  artist: (a, b) => collator.compare(a.artist, b.artist),
  album: (a, b) => collator.compare(a.album, b.album),
  duration: (a, b) => a.durationMs - b.durationMs,
};

// ------------------------------------------------- "is this song already there?"
//
// Used in two places: while songs are being dragged (a playlist or MP3
// player that already has them shows a refusal), and in compare mode (every
// row of the list is marked as on the playlist or player, or not).

/**
 * Two songs with the same title and artist count as the same recording if
 * their lengths differ by no more than this. Must match
 * `SAME_SONG_TOLERANCE_MS` in src-tauri/src/device.rs, which makes the
 * real decision when songs are copied.
 */
const SAME_SONG_TOLERANCE_MS = 2000;

/** Title and artist, tidied so that case and stray spaces do not matter. */
function songKey(track: Track): string {
  return `${track.title.trim().toLowerCase()}\n${track.artist.trim().toLowerCase()}`;
}

/**
 * Builds a function that answers "does this playlist or MP3 player already
 * have this song?". The preparation is done once here, so the function
 * returned is fast enough to call for every row of a long list.
 *
 * - A playlist has a song if that exact track is in it.
 * - An MP3 player has a song if the track is one of its own files, or if
 *   it holds a song with the same title, artist and (near enough) length.
 *   Its copy usually has a different file name, so the file is not what is
 *   compared. This mirrors `is_on_device` in src-tauri/src/device.rs.
 */
export function membershipTest(
  target: DropTarget,
  tracks: Track[],
  playlists: Playlist[],
): (track: Track) => boolean {
  if (target.kind === "playlist") {
    const ids = new Set(playlists.find((p) => p.id === target.id)?.trackIds);
    return (track) => ids.has(track.id);
  }

  // The lengths of every song on the device, grouped by title and artist.
  const lengths = new Map<string, number[]>();
  for (const track of tracks) {
    if (track.folderId !== target.id) continue;
    const key = songKey(track);
    const sameSong = lengths.get(key);
    if (sameSong) sameSong.push(track.durationMs);
    else lengths.set(key, [track.durationMs]);
  }
  return (track) =>
    track.folderId === target.id ||
    (lengths.get(songKey(track)) ?? []).some(
      (length) => Math.abs(length - track.durationMs) <= SAME_SONG_TOLERANCE_MS,
    );
}

/**
 * For each place the dragged songs could be dropped (every connected MP3
 * player and every playlist), how many of them it already has. The keys
 * are `dropTargetValue` strings ("folder:3", "playlist:7").
 */
export function countAlreadyPresent(
  dragged: Track[],
  tracks: Track[],
  folders: Folder[],
  playlists: Playlist[],
): Map<string, number> {
  const targets: DropTarget[] = [
    ...folders
      .filter((folder) => folder.isDevice && folder.connected)
      .map((folder): DropTarget => ({ kind: "folder", id: folder.id })),
    ...playlists.map((playlist): DropTarget => ({ kind: "playlist", id: playlist.id })),
  ];

  const counts = new Map<string, number>();
  for (const target of targets) {
    const has = membershipTest(target, tracks, playlists);
    counts.set(dropTargetValue(target), dragged.filter(has).length);
  }
  return counts;
}

/**
 * The tracks that belong to the same album as `track` (including itself).
 * Used to offer "apply this cover to the whole album".
 *
 * Two tracks are on the same album when the album name matches and so does
 * the album artist (falling back to the track artist), so two different
 * albums that happen to share a name like "Greatest Hits" stay separate.
 * They must also be in the same library folder, so that editing an album
 * in the library does not silently rewrite the copies on an MP3 player
 * (or the other way round).
 */
export function albumTracks(tracks: Track[], track: Track): Track[] {
  if (!track.album) return [track];
  const owner = (t: Track) => (t.albumArtist || t.artist).toLowerCase();
  const album = track.album.toLowerCase();
  const artist = owner(track);
  return tracks.filter(
    (t) =>
      t.folderId === track.folderId && t.album.toLowerCase() === album && owner(t) === artist,
  );
}
