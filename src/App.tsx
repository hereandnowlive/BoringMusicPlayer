import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import * as api from "./api";
import { buttonPrimary, cx, iconButtonSubtle, smallButton } from "./classes";
import { EditDialog } from "./components/EditDialog";
import { CloseIcon } from "./components/Icons";
import { LibraryTable, type RevealRequest } from "./components/LibraryTable";
import { NowPlaying } from "./components/NowPlaying";
import { PlayerBar } from "./components/PlayerBar";
import { Sidebar } from "./components/Sidebar";
import { baseName, songCount } from "./format";
import { useLibrary } from "./hooks/useLibrary";
import { useLibraryView } from "./hooks/useLibraryView";
import { usePlayer } from "./hooks/usePlayer";
import { useStartupReveal } from "./hooks/useStartupReveal";
import { dropTargetValue, useTrackDrag, type DropTarget } from "./hooks/useTrackDrag";
import {
  countAlreadyPresent,
  filterTracks,
  membershipTest,
  sortTracks,
  tracksInView,
  type LibraryView,
  type Sort,
  type SortKey,
} from "./library";
import type { Track } from "./types";

/** The keys that control playback from anywhere in the window. */
const PLAYBACK_KEYS = ["Space", "ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"];
/** How far Shift+Left and Shift+Right jump within the song. */
const KEY_SEEK_MS = 5000;
/** How much the up and down arrow keys change the volume (which is 0 to 1). */
const KEY_VOLUME_STEP = 0.05;

/** A message for the strip above the player bar. */
interface Notice {
  text: string;
  /** Errors are shown in red; other messages (a finished copy) are not. */
  isError: boolean;
}

/**
 * The top of the UI. It owns the state that more than one part of the
 * screen needs (library, player, search text, sort order, which dialog is
 * open) and passes each component the pieces it uses.
 *
 * Screen layout:
 *
 *   +------------+-----------------------------+
 *   | Sidebar    | search box       song count |
 *   | (folders)  | LibraryTable                |
 *   |            |                             |
 *   +------------+                             |
 *   | NowPlaying +-----------------------------+
 *   | (cover)    | PlayerBar                   |
 *   +------------+-----------------------------+
 */
export default function App() {
  // A message shown in a strip above the player bar until dismissed.
  const [notice, setNotice] = useState<Notice | null>(null);
  const reportError = useCallback(
    (error: unknown) => setNotice({ text: api.errorMessage(error), isError: true }),
    [],
  );

  const library = useLibrary(reportError);
  const player = usePlayer();
  // What the sidebar has selected, and so what the list shows: all songs,
  // one folder or one playlist. Remembered between runs.
  const [view, selectView, viewRestored] = useLibraryView(library, reportError);

  const [search, setSearch] = useState("");
  const [sort, setSort] = useState<Sort>({ key: "default", descending: false });
  // Id of the track whose edit dialog is open, if any.
  const [editTrackId, setEditTrackId] = useState<number | null>(null);
  // The latest "show this track in the list" request, if any.
  const [reveal, setReveal] = useState<RevealRequest | null>(null);

  // Show playback problems (unreadable file, no sound device...) as notices.
  useEffect(() => {
    if (player.error) setNotice({ text: player.error, isError: true });
  }, [player.error]);

  // `useMemo` recomputes these only when their inputs change, not on every
  // render. Docs: https://react.dev/reference/react/useMemo
  // Narrowing happens in three steps: the selected folder or playlist, then
  // the search text, then the sort order. What comes out is both what the list shows
  // and what gets queued when a song is played.
  const deviceFolderIds = useMemo(
    () => new Set(library.folders.filter((f) => f.isDevice).map((f) => f.id)),
    [library.folders],
  );
  const tracksById = useMemo(
    () => new Map(library.tracks.map((track) => [track.id, track])),
    [library.tracks],
  );
  const viewTracks = useMemo(
    () => tracksInView(library.tracks, tracksById, view, deviceFolderIds, library.playlists),
    [library.tracks, tracksById, view, deviceFolderIds, library.playlists],
  );
  const visibleTracks = useMemo(
    () => sortTracks(filterTracks(viewTracks, search), sort),
    [viewTracks, search, sort],
  );

  // Prefer the library's copy of the playing track: it is refreshed when
  // its tags or cover art change, whereas the player's copy dates from when the song
  // started.
  const playerTrackId = player.track?.id ?? null;
  const currentTrack: Track | null =
    (playerTrackId !== null ? tracksById.get(playerTrackId) : undefined) ?? player.track;
  const editTrack = editTrackId !== null ? tracksById.get(editTrackId) : undefined;

  /** Plays the song at `index` of the visible list, queueing the rest after it. */
  const playAt = useCallback(
    (index: number) => {
      api.playTracks(visibleTracks.map((track) => track.id), index).catch(reportError);
    },
    [visibleTracks, reportError],
  );

  /** Column header clicked: ascending, then descending, then back to default. */
  const changeSort = useCallback((key: SortKey) => {
    setSort((current) => {
      if (current.key !== key) return { key, descending: false };
      if (!current.descending) return { key, descending: true };
      return { key: "default", descending: false };
    });
  }, []);

  /** Songs were dropped on a device in the sidebar: copy them to it. */
  const dropOnDevice = useCallback(
    async (folderId: number, trackIds: number[]) => {
      const result = await library.copyToDevice(folderId, trackIds);
      if (result === null) return; // it failed to start; already reported

      const parts: string[] = [];
      if (result.copied > 0) parts.push(`Copied ${songCount(result.copied)} to the player.`);
      if (result.alreadyPresent.length > 0) {
        parts.push(`Already on the player, not copied: ${nameList(result.alreadyPresent)}.`);
      }
      if (parts.length === 0 && result.failures.length === 0) parts.push("Nothing to copy.");
      if (result.failures.length > 0) {
        parts.push(`${result.failures.length} failed: ${result.failures.join("; ")}`);
      }
      setNotice({ text: parts.join(" "), isError: result.failures.length > 0 });
    },
    [library],
  );
  /** Songs were dropped on a playlist in the sidebar: add them to it. */
  const dropOnPlaylist = useCallback(
    async (playlistId: number, trackIds: number[]) => {
      const result = await library.addToPlaylist(playlistId, trackIds);
      if (result === null) return; // it failed; already reported

      const name = library.playlists.find((p) => p.id === playlistId)?.name ?? "the playlist";
      const parts: string[] = [];
      if (result.added > 0) parts.push(`Added ${songCount(result.added)} to ${name}.`);
      if (result.alreadyPresent > 0) {
        parts.push(`${songCount(result.alreadyPresent)} already in ${name}, not added again.`);
      }
      setNotice({ text: parts.join(" ") || "Nothing to add.", isError: false });
    },
    [library],
  );

  // Dragging songs from the list to a device or a playlist. The list starts
  // the drag, this hook follows the pointer, and the sidebar shows where it
  // can land.
  const { drag, start: startDrag } = useTrackDrag((target: DropTarget, trackIds) => {
    if (target.kind === "folder") void dropOnDevice(target.id, trackIds);
    else void dropOnPlaylist(target.id, trackIds);
  });

  // Which of the dragged songs each drop target already has. The sidebar
  // uses it to refuse a drop that would add nothing. Worked out once per
  // drag: `drag.trackIds` is the same array for the whole of a drag, so
  // this does not rerun as the pointer moves.
  const draggedIds = drag?.trackIds;
  const alreadyPresent = useMemo(() => {
    if (!draggedIds) return new Map<string, number>();
    const dragged = draggedIds.flatMap((id) => tracksById.get(id) ?? []);
    return countAlreadyPresent(dragged, library.tracks, library.folders, library.playlists);
  }, [draggedIds, tracksById, library.tracks, library.folders, library.playlists]);

  // True while the dragged songs are over somewhere that has them all
  // already: the label following the pointer turns red as well.
  const dropRefused =
    drag !== null &&
    drag.hoverTarget !== null &&
    (alreadyPresent.get(dropTargetValue(drag.hoverTarget)) ?? 0) >= drag.trackIds.length;

  // ---- Compare mode ----
  // The MP3 player or playlist the list is being compared with, if any.
  // It is switched on and off by the "compare" button on a sidebar entry.
  const [compare, setCompare] = useState<DropTarget | null>(null);

  // Its name, for the toolbar. Also null if it is no longer there (the
  // playlist was deleted, the player unplugged).
  const compareName = useMemo(() => {
    if (compare === null) return null;
    if (compare.kind === "playlist") {
      return library.playlists.find((p) => p.id === compare.id)?.name ?? null;
    }
    const folder = library.folders.find((f) => f.id === compare.id && f.connected);
    return folder ? baseName(folder.path) : null;
  }, [compare, library.playlists, library.folders]);

  // Leave compare mode if what it was comparing with has gone.
  useEffect(() => {
    if (compare !== null && compareName === null && !library.loading) setCompare(null);
  }, [compare, compareName, library.loading]);

  // The ids of every track that the compared player or playlist already
  // has, or undefined when compare mode is off.
  const presentIds = useMemo(() => {
    if (compare === null || compareName === null) return undefined;
    const has = membershipTest(compare, library.tracks, library.playlists);
    return new Set(library.tracks.filter(has).map((track) => track.id));
  }, [compare, compareName, library.tracks, library.playlists]);
  const presentInView = presentIds
    ? visibleTracks.filter((track) => presentIds.has(track.id)).length
    : 0;

  // "Remove from playlist" in the list's right-click menu. It only exists
  // while a playlist is being shown. `useMemo` keeps the same function
  // between renders, so the (memoised) list is not redrawn for nothing.
  const { removeFromPlaylist } = library;
  const removeFromShownPlaylist = useMemo(() => {
    if (view.kind !== "playlist") return undefined;
    const playlistId = view.id;
    return (trackIds: number[]) => void removeFromPlaylist(playlistId, trackIds);
  }, [view, removeFromPlaylist]);

  const openEditor = useCallback((track: Track) => setEditTrackId(track.id), []);

  /** Saves a copy of a track's cover as an image file and says where. */
  const saveCover = useCallback(
    async (track: Track) => {
      try {
        const path = await api.saveCoverArt(track.id);
        if (path !== null) setNotice({ text: `Cover saved to ${path}`, isError: false });
      } catch (error) {
        reportError(error);
      }
    },
    [reportError],
  );
  // Asked for by the user (double-click on the cover or the player bar), so
  // the list glides there.
  const revealTrack = useCallback(
    (track: Track) => setReveal({ trackId: track.id, animate: true }),
    [],
  );

  // The app reopens on the song it was closed on. Once the list is showing
  // what it will show (songs loaded, saved folder or playlist selected),
  // centre that song in it. If it is not in what is open, nothing happens.
  const revealTrackId = useCallback((trackId: number) => setReveal({ trackId }), []);
  useStartupReveal(!library.loading && viewRestored, revealTrackId);

  // The keyboard handler below is attached once, so it reads the player's
  // state through a ref instead of being re-attached at every position tick.
  const playerRef = useRef(player);
  playerRef.current = player;
  // The volume last asked for with the keys. Holding a key down sends
  // presses faster than the backend reports back, so each step builds on
  // this, not on `player.volume`. It follows the backend the rest of the time.
  const keyVolume = useRef(player.volume);
  useEffect(() => {
    keyVolume.current = player.volume;
  }, [player.volume]);

  // Playback keys. They work wherever the focus is in the window, so they
  // still work after clicking a button (which leaves the focus on it). The
  // exceptions are typing in a text box and an open dialog.
  //   Space             play / pause
  //   Left / Right      previous / next song
  //   Shift+Left/Right  jump back / forward in the song
  //   Up / Down         volume
  // Ctrl+Up/Down is left alone: the list uses it to move the selection.
  useEffect(() => {
    const isPlaybackKey = (event: KeyboardEvent) => {
      if (!PLAYBACK_KEYS.includes(event.code)) return false;
      if (event.ctrlKey || event.altKey || event.metaKey) return false;
      if (document.querySelector("[role=dialog]")) return false;
      const target = event.target as HTMLElement;
      // The volume slider is an <input> too, but nothing is typed into it.
      return !target.closest("input:not([type=range]), textarea");
    };
    const run = (command: Promise<void>) => void command.catch(reportError);

    const onKeyDown = (event: KeyboardEvent) => {
      if (!isPlaybackKey(event)) return;
      // Otherwise the key scrolls the list, moves the focused slider, or
      // (space) "clicks" the focused button.
      event.preventDefault();
      const { status, positionMs, durationMs } = playerRef.current;

      switch (event.code) {
        case "Space":
          if (!event.repeat) run(api.togglePause()); // holding it toggles once
          break;
        case "ArrowLeft":
        case "ArrowRight": {
          const forward = event.code === "ArrowRight";
          if (event.shiftKey) {
            if (status === "stopped") break;
            const target = positionMs + (forward ? KEY_SEEK_MS : -KEY_SEEK_MS);
            run(api.seek(Math.min(Math.max(target, 0), durationMs)));
          } else if (!event.repeat) {
            // Holding the key does not race through the queue.
            run(forward ? api.nextTrack() : api.previousTrack());
          }
          break;
        }
        case "ArrowUp":
        case "ArrowDown": {
          if (event.shiftKey) break;
          const step = event.code === "ArrowUp" ? KEY_VOLUME_STEP : -KEY_VOLUME_STEP;
          const volume = Math.min(Math.max(keyVolume.current + step, 0), 1);
          keyVolume.current = volume;
          run(api.setVolume(volume));
          break;
        }
      }
    };
    // A focused button is clicked when space is released, not when it is
    // pressed, so the release has to be cancelled as well.
    const onKeyUp = (event: KeyboardEvent) => {
      if (isPlaybackKey(event)) event.preventDefault();
    };
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
    };
  }, [reportError]);

  return (
    // Two columns (the 250px left column, then everything else) and three
    // rows (the content, the optional notice strip, the player bar).
    <div className="grid h-full grid-cols-[250px_minmax(0,1fr)] grid-rows-[minmax(0,1fr)_auto_auto]">
      {/* The left column runs the full height of the window: the folder
          list above, the current song's cover below. `row-span-full`
          makes it span all three rows, so the player bar only occupies
          the space to its right. */}
      <div className="col-start-1 row-span-full flex min-h-0 flex-col border-r border-edge bg-panel">
        <Sidebar
          library={library}
          view={view}
          onSelect={selectView}
          draggedCount={drag?.trackIds.length ?? 0}
          dropHover={drag?.hoverTarget ?? null}
          alreadyPresent={alreadyPresent}
          compare={compare}
          onCompare={setCompare}
        />
        <NowPlaying
          track={currentTrack}
          onEdit={openEditor}
          onReveal={revealTrack}
          onSaveCover={(track) => void saveCover(track)}
        />
      </div>

      <main className="col-start-2 flex min-h-0 min-w-0 flex-col">
        {/* The toolbar sits above the list (`z-2`) and is opaque, so
            nothing scrolling in the list can show through or over it.

            `h-15` is a fixed, whole-pixel height. Left to size itself the
            toolbar can come out at a fractional height, which puts the top
            of the list on a fraction of a pixel; the browser then rounds
            the list's edge and its pinned header differently, leaving a
            one-pixel slit between them.

            The shadow extends the toolbar's background three pixels DOWN,
            over the top edge of the list, so even if the list is drawn a
            pixel off while scrolling, the seam is always covered. */}
        <header className="relative z-2 flex h-15 flex-none items-center gap-4 bg-window px-4 shadow-[0_3px_0_var(--color-window)]">
          <input
            type="search"
            className="max-w-105 flex-1 rounded-md border border-edge bg-panel px-3 py-1.75 select-text placeholder:text-muted"
            placeholder="Search title, artist or album"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
          />
          <span className="text-muted">
            {search.trim() && visibleTracks.length !== viewTracks.length
              ? `${visibleTracks.length.toLocaleString()} of ${songCount(viewTracks.length)}`
              : songCount(viewTracks.length)}
          </span>

          {/* Compare mode: says what the ticks in the list mean, and how
              to leave. `ml-auto` pushes it to the right-hand end. */}
          {presentIds && (
            <div className="ml-auto flex min-w-0 items-center gap-2 text-xs text-accent">
              <span className="truncate">
                {presentInView.toLocaleString()} of {songCount(visibleTracks.length)} already{" "}
                {compare?.kind === "playlist" ? "in" : "on"} {compareName}
              </span>
              <button className={smallButton} onClick={() => setCompare(null)}>
                Stop comparing
              </button>
            </div>
          )}
        </header>

        {visibleTracks.length > 0 ? (
          <LibraryTable
            tracks={visibleTracks}
            currentTrackId={playerTrackId}
            reveal={reveal}
            sort={sort}
            onSort={changeSort}
            onPlay={playAt}
            onEdit={openEditor}
            onDragStart={startDrag}
            onRemoveFromPlaylist={removeFromShownPlaylist}
            presentIds={presentIds}
          />
        ) : (
          <div className="flex flex-1 flex-col items-center justify-center gap-3.5 text-muted">
            {emptyMessage(library, search, view)}
          </div>
        )}
      </main>

      {notice && (
        <div
          className={cx(
            "col-start-2 flex items-center justify-between gap-3 border-t px-4 py-2 wrap-anywhere select-text",
            // Red for an error; amber for good news (a finished copy).
            notice.isError ? "border-danger bg-[#2a1b1d]" : "border-accent bg-[#2a2416]",
          )}
          role="alert"
        >
          <span>{notice.text}</span>
          <button className={iconButtonSubtle} title="Dismiss" onClick={() => setNotice(null)}>
            <CloseIcon size={16} />
          </button>
        </div>
      )}

      <PlayerBar player={player} onReveal={revealTrack} onError={reportError} />

      {/* The label that follows the pointer while songs are being dragged.
          `pointer-events-none` lets the pointer "see through" it to
          whatever is underneath, which is how drop targets are found. */}
      {drag && (
        <div
          className={cx(
            "pointer-events-none fixed z-30 max-w-65 truncate rounded-md px-2.5 py-1.5 text-[13px] font-semibold shadow-[0_4px_12px_rgb(0_0_0/40%)]",
            // Red over somewhere that already has every song being dragged.
            dropRefused ? "bg-danger text-white" : "bg-accent text-on-accent",
          )}
          style={{ left: drag.x + 14, top: drag.y + 14 }}
        >
          {drag.label}
        </div>
      )}

      {editTrack && (
        <EditDialog
          track={editTrack}
          tracks={library.tracks}
          onClose={() => setEditTrackId(null)}
        />
      )}
    </div>
  );
}

/** How many names `nameList` spells out before summarising the rest. */
const MAX_NAMES_SHOWN = 5;

/** "A, B, C" or, for long lists, "A, B, C, D, E and 7 more". */
function nameList(names: string[]): string {
  if (names.length <= MAX_NAMES_SHOWN) return names.join(", ");
  const shown = names.slice(0, MAX_NAMES_SHOWN).join(", ");
  return `${shown} and ${names.length - MAX_NAMES_SHOWN} more`;
}

/** What to show in place of the song list when there are no songs to show. */
function emptyMessage(
  library: ReturnType<typeof useLibrary>,
  search: string,
  view: LibraryView,
) {
  if (library.loading) return <p>Loading library…</p>;
  if (view.kind === "playlist" && !search.trim()) {
    return <p>This playlist is empty. Drag songs onto it in the sidebar to add them.</p>;
  }
  if (library.folders.length === 0) {
    return (
      <>
        <p>Your library is empty.</p>
        <button className={buttonPrimary} onClick={() => void library.addFolder()}>
          Add a music folder…
        </button>
      </>
    );
  }
  if (library.scan !== null) return <p>Scanning for music…</p>;
  if (search.trim()) return <p>No songs match “{search.trim()}”.</p>;
  if (view.kind === "folder") return <p>No music files were found in this folder.</p>;
  return <p>No music files were found in your library folders.</p>;
}
