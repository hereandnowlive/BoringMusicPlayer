import { useCallback, useEffect, useState } from "react";
import * as api from "../api";
import { ALL_SONGS, type LibraryView } from "../library";
import type { Folder } from "../types";
import type { Library } from "./useLibrary";

/** Name under which the view is saved in the backend's settings. */
const SETTING_KEY = "library_filter";

/** Marks a saved playlist view, as in "playlist:7". */
const PLAYLIST_PREFIX = "playlist:";

/**
 * Whether a folder appears in the sidebar. Everything does except an MP3
 * player that is not plugged in: it stays in the library (so plugging it
 * back in brings it straight back) but is hidden until then.
 */
export function isShown(folder: Folder): boolean {
  return !folder.isDevice || folder.connected;
}

/** The text a view is saved as: "all", "12" (a folder) or "playlist:7". */
function toSaved(view: LibraryView): string {
  if (view.kind === "folder") return String(view.id);
  if (view.kind === "playlist") return PLAYLIST_PREFIX + view.id;
  return "all";
}

/** The reverse of `toSaved`. Anything unrecognised means "All songs". */
function fromSaved(saved: string | null): LibraryView {
  if (saved === null) return ALL_SONGS;
  const isPlaylist = saved.startsWith(PLAYLIST_PREFIX);
  const id = Number(isPlaylist ? saved.slice(PLAYLIST_PREFIX.length) : saved);
  // `Number("all")` is not a positive integer, so it falls through.
  if (!Number.isInteger(id) || id <= 0) return ALL_SONGS;
  return { kind: isPlaylist ? "playlist" : "folder", id };
}

/**
 * Which sidebar entry is selected, and so what the song list shows: all
 * songs, one folder or one playlist.
 *
 * The choice is saved through the backend each time it changes and read
 * back when the app starts, so the player reopens showing the same list.
 *
 * Returns the current view and a function to change it, in the same shape
 * as React's own `useState`, followed by whether the saved view has been
 * read back yet. Until it has, the view is still the "All songs" every
 * start begins with, which may not be the one that was saved.
 */
export function useLibraryView(
  library: Library,
  onError: (error: unknown) => void,
): [LibraryView, (view: LibraryView) => void, boolean] {
  const [view, setView] = useState<LibraryView>(ALL_SONGS);
  const [restored, setRestored] = useState(false);

  // Restore the saved view once, at startup.
  useEffect(() => {
    api
      .getSetting(SETTING_KEY)
      .then((saved) => setView(fromSaved(saved)), onError)
      // Whether or not it worked, there is nothing more to wait for.
      .finally(() => setRestored(true));
  }, [onError]);

  // If what is selected is not available (the folder or playlist was
  // removed, or it is an MP3 player that has been unplugged), fall back to
  // "All songs". Wait for the library to load first, or everything would
  // look "missing".
  const selectionIsGone =
    !library.loading &&
    ((view.kind === "folder" &&
      !library.folders.some((folder) => folder.id === view.id && isShown(folder))) ||
      (view.kind === "playlist" &&
        !library.playlists.some((playlist) => playlist.id === view.id)));
  useEffect(() => {
    if (selectionIsGone) setView(ALL_SONGS);
  }, [selectionIsGone]);

  const select = useCallback(
    (next: LibraryView) => {
      setView(next);
      api.setSetting(SETTING_KEY, toSaved(next)).catch(onError);
    },
    [onError],
  );

  return [view, select, restored];
}
