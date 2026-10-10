import { useState } from "react";
import { iconButtonSubtle } from "../classes";
import { baseName, formatSize, songCount } from "../format";
import type { Library } from "../hooks/useLibrary";
import { isShown } from "../hooks/useLibraryView";
import { useSavedFlag } from "../hooks/useSavedFlag";
import { dropTargetValue, sameTarget, type DropTarget } from "../hooks/useTrackDrag";
import { ALL_SONGS, type LibraryView } from "../library";
import type { Folder, Playlist } from "../types";
import { DeviceIcon, PlusIcon, RefreshIcon } from "./Icons";
import { PlaylistDialog } from "./PlaylistDialog";
import { ScrollList } from "./ScrollList";
import { SidebarEntry, type DropState } from "./SidebarEntry";
import { SidebarSection } from "./SidebarSection";

/** The thin progress bars under the scan and copy messages. */
const PROGRESS = "h-1.5 w-full accent-accent";

/** The line shown in place of a list that has nothing in it yet. */
const HINT = "px-2 text-xs text-muted";

interface SidebarProps {
  library: Library;
  /** What the song list is showing: all songs, a folder or a playlist. */
  view: LibraryView;
  onSelect: (view: LibraryView) => void;
  /** How many songs are being dragged from the list right now (0 = none). */
  draggedCount: number;
  /** The drop target the dragged songs are over right now, if any. */
  dropHover: DropTarget | null;
  /**
   * For each drop target (keyed by `dropTargetValue`), how many of the
   * dragged songs it already has. See `countAlreadyPresent` in library.ts.
   */
  alreadyPresent: ReadonlyMap<string, number>;
  /** The MP3 player or playlist that compare mode is on for, if any. */
  compare: DropTarget | null;
  /** Switch compare mode to this target, or off with null. */
  onCompare: (target: DropTarget | null) => void;
}

/**
 * The left-hand panel. From the top:
 *
 * - "All songs".
 * - The **Folders** section: one entry per library folder. Its header
 *   holds the buttons to rescan, add an MP3 player and add a folder.
 * - The status of the current scan or copy.
 * - The **Playlists** section, at the bottom, with a "+" in its header
 *   that opens the new-playlist dialog.
 *
 * Clicking an entry shows (and queues from) just its songs. Both sections
 * fold away when their header is clicked, are at most half the sidebar
 * tall, and scroll past that; see `SidebarSection`.
 *
 * Songs dragged from the list can be dropped on an MP3 player (they are
 * copied to it) or on a playlist (they are added to it). While songs are
 * being dragged those entries light up and everything else dims. One that
 * already has every song being dragged shows a refusal, in red, instead.
 *
 * MP3 players and playlists also have a "compare" button, which makes the
 * song list mark which of its songs that player or playlist already has.
 */
export function Sidebar({
  library,
  view,
  onSelect,
  draggedCount,
  dropHover,
  alreadyPresent,
  compare,
  onCompare,
}: SidebarProps) {
  const { folders, playlists, scan, lastScan, copy } = library;
  const dragging = draggedCount > 0;

  // Whether each section is open, remembered between runs.
  const [foldersOpen, setFoldersOpen] = useSavedFlag("sidebar_folders_open", true);
  const [playlistsOpen, setPlaylistsOpen] = useSavedFlag("sidebar_playlists_open", true);
  // Whether the new-playlist dialog is showing.
  const [creatingPlaylist, setCreatingPlaylist] = useState(false);

  /** How many of the dragged songs a target already has. */
  function alreadyThere(target: DropTarget): number {
    return alreadyPresent.get(dropTargetValue(target)) ?? 0;
  }

  /** How an entry should look during a drag, given whether it takes drops. */
  function dropState(target: DropTarget | null): DropState {
    if (!dragging) return "none";
    if (target === null) return "dimmed";
    const hovered = sameTarget(dropHover, target);
    // Nothing new to add: refuse, in red.
    if (alreadyThere(target) >= draggedCount) return hovered ? "refused-hover" : "refused";
    return hovered ? "hover" : "ready";
  }

  /**
   * The line under the name of an entry that songs are being dragged to.
   * `verb` is what dropping does ("copy", "add"); `where` finishes the
   * sentence "Already ..." ("on this player", "in this playlist").
   */
  function dropPrompt(target: DropTarget, state: DropState, verb: string, where: string): string {
    const already = alreadyThere(target);
    if (state === "refused" || state === "refused-hover") return `Already ${where}`;

    // When it has some of them, say how many will actually go.
    const songs =
      already > 0
        ? `${draggedCount - already} of ${songCount(draggedCount)}`
        : songCount(draggedCount);
    return state === "hover" ? `Release to ${verb} ${songs}` : `Drop here to ${verb} ${songs}`;
  }

  /** The "compare" button of an MP3 player or a playlist. */
  function compareButton(target: DropTarget, what: string) {
    const active = sameTarget(compare, target);
    return {
      active,
      title: active
        ? "Stop comparing"
        : `Compare: mark the songs in the list that are already ${what}`,
      onToggle: () => onCompare(active ? null : target),
    };
  }

  function folderEntry(folder: Folder) {
    // Only a connected device can take dropped songs.
    const target: DropTarget | null =
      folder.isDevice && folder.connected ? { kind: "folder", id: folder.id } : null;
    const drop = dropState(target);
    // Whether the detail line is showing a drag prompt.
    const prompting = target !== null && drop !== "none";
    const count = songCount(folder.trackCount);

    return (
      <SidebarEntry
        key={folder.id}
        name={baseName(folder.path)}
        icon={folder.isDevice ? <DeviceIcon size={15} /> : undefined}
        title={folder.path}
        // A device changes its text during a drag, so it is obvious that
        // this is where songs can go. (Its icon is what says it is a
        // player; there is no room to say so in words as well.)
        detail={prompting ? dropPrompt(target, drop, "copy", "on this player") : count}
        // The prompt needs the whole line, so the size steps aside for it.
        trailing={prompting ? undefined : formatSize(folder.sizeBytes)}
        selected={view.kind === "folder" && view.id === folder.id}
        onSelect={() => onSelect({ kind: "folder", id: folder.id })}
        drop={drop}
        dropTarget={target ?? undefined}
        compare={target ? compareButton(target, "on this player") : undefined}
        remove={{
          title: "Remove this folder from the library",
          question: "Remove from library? Files stay on disk.",
          onConfirm: () => void library.removeFolder(folder.id),
        }}
      />
    );
  }

  function playlistEntry(playlist: Playlist) {
    const target: DropTarget = { kind: "playlist", id: playlist.id };
    const drop = dropState(target);

    return (
      <SidebarEntry
        key={playlist.id}
        name={playlist.name}
        title={playlist.name}
        detail={
          dragging
            ? dropPrompt(target, drop, "add", "in this playlist")
            : songCount(playlist.trackIds.length)
        }
        selected={view.kind === "playlist" && view.id === playlist.id}
        onSelect={() => onSelect({ kind: "playlist", id: playlist.id })}
        drop={drop}
        dropTarget={target}
        compare={compareButton(target, "in this playlist")}
        remove={{
          title: "Delete this playlist",
          question: "Delete playlist? Its songs stay in the library.",
          onConfirm: () => void library.deletePlaylist(playlist.id),
        }}
      />
    );
  }

  // Devices hold copies, so they are not part of "All songs".
  const ownFolders = folders.filter((folder) => !folder.isDevice);

  return (
    <>
      {/* `flex-1` takes whatever height the cover below leaves. */}
      <aside className="flex min-h-0 flex-1 flex-col gap-3 px-3 py-4">
        {/* A list of one, so the entry is a proper list item like the rest. */}
        <ul className="flex-none">
          <SidebarEntry
            name="All songs"
            detail={songCount(ownFolders.reduce((sum, f) => sum + f.trackCount, 0))}
            trailing={formatSize(ownFolders.reduce((sum, f) => sum + f.sizeBytes, 0))}
            selected={view.kind === "all"}
            onSelect={() => onSelect(ALL_SONGS)}
            drop={dropState(null)}
          />
        </ul>

        <SidebarSection
          title="Folders"
          open={foldersOpen}
          onToggle={() => setFoldersOpen(!foldersOpen)}
          // The folder actions are small icon buttons in the header, not
          // full-width buttons under the list: the section may only be half
          // the sidebar tall, and that height is better spent on folders.
          // Each button's `title` is its tooltip.
          action={
            <>
              <button
                className={iconButtonSubtle}
                disabled={folders.length === 0 || scan !== null}
                title="Rescan: look for new, changed and deleted files"
                onClick={() => void library.rescan()}
              >
                <RefreshIcon size={18} />
              </button>
              <button
                className={iconButtonSubtle}
                title="Add MP3 player… Songs can then be dragged onto it to copy them."
                onClick={() => void library.addFolder(true)}
              >
                <DeviceIcon size={16} />
              </button>
              <button
                className={iconButtonSubtle}
                title="Add folder…"
                onClick={() => void library.addFolder()}
              >
                <PlusIcon size={18} />
              </button>
            </>
          }
        >
          {/* `isShown` leaves out MP3 players that are not plugged in. */}
          {folders.some(isShown) ? (
            <ScrollList>{folders.filter(isShown).map(folderEntry)}</ScrollList>
          ) : (
            <p className={HINT}>No folders yet. Press + to add your music.</p>
          )}
        </SidebarSection>

        {/* ---- Scan status ----
            Capped in height (`max-h-20`), with its own scrollbar, so a scan
            that reports many unreadable files cannot push the lists out of
            the sidebar. `empty:hidden` removes it altogether while there is
            nothing to say, so it does not leave a gap. */}
        <div
          className="flex max-h-20 flex-none flex-col gap-1.5 overflow-y-auto px-2 text-xs text-muted empty:hidden"
          aria-live="polite"
        >
          {copy !== null ? (
            <>
              <div className="truncate">
                Copying {Math.min(copy.done + 1, copy.total)} / {copy.total}: {copy.current}
              </div>
              <progress className={PROGRESS} value={copy.done} max={Math.max(copy.total, 1)} />
            </>
          ) : scan !== null ? (
            <>
              <div>
                Scanning {baseName(scan.folder)}… {scan.done.toLocaleString()} /{" "}
                {scan.total.toLocaleString()}
              </div>
              <progress className={PROGRESS} value={scan.done} max={Math.max(scan.total, 1)} />
            </>
          ) : (
            lastScan !== null && (
              <>
                <div>
                  Last scan: {lastScan.added} added, {lastScan.updated} updated, {lastScan.removed}{" "}
                  removed
                  {lastScan.failed > 0 && `, ${lastScan.failed} unreadable`}
                </div>
                {lastScan.errors.map((error) => (
                  <div key={error} className="text-danger wrap-anywhere">
                    {error}
                  </div>
                ))}
              </>
            )
          )}
        </div>

        {/* `mt-auto` soaks up the spare height above this section, which is
            what puts Playlists at the bottom of the sidebar. */}
        <SidebarSection
          title="Playlists"
          className="mt-auto"
          open={playlistsOpen}
          onToggle={() => setPlaylistsOpen(!playlistsOpen)}
          action={
            <button
              className={iconButtonSubtle}
              title="New playlist…"
              onClick={() => setCreatingPlaylist(true)}
            >
              <PlusIcon size={18} />
            </button>
          }
        >
          {playlists.length > 0 ? (
            <ScrollList>{playlists.map(playlistEntry)}</ScrollList>
          ) : (
            <p className={HINT}>No playlists yet. Press + to make one, then drag songs onto it.</p>
          )}
        </SidebarSection>
      </aside>

      {creatingPlaylist && <PlaylistDialog onClose={() => setCreatingPlaylist(false)} />}
    </>
  );
}
