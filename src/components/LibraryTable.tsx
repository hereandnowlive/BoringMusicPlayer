import {
  memo,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type MouseEvent,
  type PointerEvent,
} from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { cx } from "../classes";
import { formatDuration, songCount } from "../format";
import type { Sort, SortKey } from "../library";
import type { Track } from "../types";
import { ContextMenu } from "./ContextMenu";
import { CoverArt } from "./CoverArt";
import { CheckIcon } from "./Icons";

/** Height of one row in pixels. Must match `h-11` on the rows below
 *  (Tailwind sizes are in steps of 4px: 11 x 4 = 44). */
const ROW_HEIGHT = 44;

/** Height of the column header in pixels. Must match the header below:
 *  `h-8.75` is 35px, of which 1px is the overlap explained there. */
const HEADER_HEIGHT = 34;

/**
 * The column layout shared by the header and every row, so the two always
 * line up: cover | title | artist | album | length. To add a column, add
 * its width here (and its cell in the header and the row below).
 *
 * The square brackets are Tailwind's way of writing a one-off value; the
 * underscores stand for the spaces of the CSS `grid-template-columns`.
 * https://tailwindcss.com/docs/grid-template-columns#using-a-custom-value
 */
const ROW_GRID =
  "grid grid-cols-[32px_minmax(0,3fr)_minmax(0,2fr)_minmax(0,2fr)_64px] items-center gap-3 px-4";

/** The same layout with one more narrow column at the front, for the mark
 *  that compare mode puts on each row. */
const ROW_GRID_COMPARE =
  "grid grid-cols-[16px_32px_minmax(0,3fr)_minmax(0,2fr)_minmax(0,2fr)_64px] items-center gap-3 px-4";

/** One cell of a row: a single line, cut short with "…" if too long. */
const CELL = "truncate text-muted";

/** How far (in pixels) the pointer must move with the button held before a
 *  press on a row counts as a drag. */
const DRAG_THRESHOLD = 6;

/** The sortable columns, left to right. */
const COLUMNS: { key: SortKey; label: string; alignRight?: boolean }[] = [
  { key: "title", label: "Title" },
  { key: "artist", label: "Artist" },
  { key: "album", label: "Album" },
  { key: "duration", label: "Length", alignRight: true },
];

interface LibraryTableProps {
  /** The tracks to show, already filtered and sorted. */
  tracks: Track[];
  /** Id of the song that is loaded in the player, to highlight its row. */
  currentTrackId: number | null;
  /** Set (to a new object) to jump the list to a track. */
  reveal: RevealRequest | null;
  sort: Sort;
  onSort: (key: SortKey) => void;
  /** Play the track at this position in `tracks`. */
  onPlay: (index: number) => void;
  /** Open the edit dialog (details and cover art) for a track. */
  onEdit: (track: Track) => void;
  /**
   * The user started dragging these tracks (towards a device in the
   * sidebar). `label` describes them; `x`/`y` is where the pointer is.
   * From here on the drag is run by `useTrackDrag`.
   */
  onDragStart: (trackIds: number[], label: string, x: number, y: number) => void;
  /**
   * Take these tracks out of the playlist being shown. Only passed while a
   * playlist is being shown; it is what adds "Remove from playlist" to the
   * right-click menu.
   */
  onRemoveFromPlaylist?: (trackIds: number[]) => void;
  /**
   * Compare mode. When given, every row is marked: a tick if its track is
   * in this set (the playlist or MP3 player being compared with already
   * has it), an empty ring if not. Leave out for no marks.
   */
  presentIds?: ReadonlySet<number>;
}

/**
 * A request to scroll a track into view and select it. Every request is a
 * new object, which is how the list can tell that the same track has been
 * asked for a second time.
 */
export interface RevealRequest {
  trackId: number;
  /**
   * Glide to the track instead of jumping. Used when the user asks to see
   * the current song, so they can follow where the list is going. Left off
   * for the jump made when the app starts, which should just be in place.
   */
  animate?: boolean;
}

/** Where the right-click menu is open, and for which row. */
interface MenuState {
  x: number;
  y: number;
  index: number;
}

/**
 * The song list.
 *
 * A library can hold tens of thousands of songs, far too many to put on the
 * page at once. This list is "virtualised": only the rows currently in view
 * (plus a few either side) exist as elements, and each is positioned at the
 * place it would occupy in the full list.
 * Docs: https://tanstack.com/virtual/latest
 *
 * `memo` skips re-rendering when the props are unchanged, which matters
 * because the parent re-renders several times a second as the song position
 * updates. Docs: https://react.dev/reference/react/memo
 */
export const LibraryTable = memo(function LibraryTable({
  tracks,
  currentTrackId,
  reveal,
  sort,
  onSort,
  onPlay,
  onEdit,
  onDragStart,
  onRemoveFromPlaylist,
  presentIds,
}: LibraryTableProps) {
  const rowGrid = presentIds ? ROW_GRID_COMPARE : ROW_GRID;
  const scrollRef = useRef<HTMLDivElement>(null);

  // Several songs can be selected at once (Ctrl-click, Shift-click), so
  // the selection is a set of track ids. `anchorId` is the song most
  // recently clicked: Shift-click selects from it to the clicked song, and
  // the arrow keys move from it.
  const [selected, setSelected] = useState<ReadonlySet<number>>(new Set());
  const [anchorId, setAnchorId] = useState<number | null>(null);

  /** Makes one song the whole selection. */
  function selectOnly(trackId: number) {
    setSelected(new Set([trackId]));
    setAnchorId(trackId);
  }

  const [menu, setMenu] = useState<MenuState | null>(null);

  const virtualizer = useVirtualizer({
    count: tracks.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 12, // extra rows rendered above and below the visible area
    // The header sits inside the scrolling box, above the rows. These two
    // settings tell the virtualiser about it: where the rows really start,
    // and to keep a row scrolled into view clear of the pinned header.
    scrollMargin: HEADER_HEIGHT,
    scrollPaddingStart: HEADER_HEIGHT,
  });

  // Jump to a track when asked (double-click on the song in the player
  // bar). If the track is not in the list as currently shown, for example
  // because a search is hiding it, nothing happens.
  //
  // The effect depends only on `reveal`: it must run once per request, not
  // again every time the list or the virtualiser object changes.
  useEffect(() => {
    if (reveal === null) return;
    const index = tracks.findIndex((track) => track.id === reveal.trackId);
    if (index < 0) return;
    selectOnly(reveal.trackId);
    // "smooth" animates the scroll; "auto" jumps. People who have asked
    // their system to reduce animation get the jump either way.
    // https://developer.mozilla.org/docs/Web/CSS/@media/prefers-reduced-motion
    const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    virtualizer.scrollToIndex(index, {
      align: "center",
      behavior: reveal.animate && !reduceMotion ? "smooth" : "auto",
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reveal]);

  /**
   * Click: select just this song. Ctrl-click: add or remove it.
   * Shift-click: select everything from the last clicked song to this one.
   */
  function onRowClick(event: MouseEvent, index: number) {
    const trackId = tracks[index].id;

    if (event.shiftKey && anchorId !== null) {
      const anchorIndex = tracks.findIndex((track) => track.id === anchorId);
      if (anchorIndex >= 0) {
        const from = Math.min(anchorIndex, index);
        const to = Math.max(anchorIndex, index);
        setSelected(new Set(tracks.slice(from, to + 1).map((track) => track.id)));
        return; // the anchor stays put, so the range can be adjusted
      }
    }

    if (event.ctrlKey || event.metaKey) {
      const next = new Set(selected);
      if (!next.delete(trackId)) next.add(trackId); // `delete` is false if it was absent
      setSelected(next);
      setAnchorId(trackId);
      return;
    }

    selectOnly(trackId);
  }

  // Where the mouse button went down on a row, until it is released. A
  // press only becomes a drag once the pointer has moved a little, so that
  // ordinary clicks and double-clicks are not mistaken for drags.
  const press = useRef<{ x: number; y: number; track: Track } | null>(null);

  function onRowPointerDown(event: PointerEvent, track: Track) {
    if (event.button !== 0) return; // left button only
    press.current = { x: event.clientX, y: event.clientY, track };
  }

  function onRowPointerMove(event: PointerEvent) {
    const pressed = press.current;
    if (!pressed) return;
    const moved = Math.hypot(event.clientX - pressed.x, event.clientY - pressed.y);
    if (moved < DRAG_THRESHOLD) return;
    press.current = null; // the drag has begun; stop watching this press

    // Dragging one of the selected songs takes the whole selection along;
    // dragging any other song takes just that one.
    const { track } = pressed;
    let trackIds: number[];
    if (selected.has(track.id)) {
      // Keep them in the order they are listed.
      trackIds = tracks.filter((t) => selected.has(t.id)).map((t) => t.id);
    } else {
      trackIds = [track.id];
      selectOnly(track.id);
    }

    const label = trackIds.length === 1 ? track.title : songCount(trackIds.length);
    onDragStart(trackIds, label, event.clientX, event.clientY);
  }

  /** Keyboard: Ctrl+up/down moves the selection, Enter plays it. (The
   *  plain arrow keys are playback keys; see App.tsx.) */
  function onKeyDown(event: KeyboardEvent) {
    if (tracks.length === 0) return;
    const selectedIndex = tracks.findIndex((track) => track.id === anchorId);

    if (event.key === "Enter" && selectedIndex >= 0) {
      onPlay(selectedIndex);
    } else if (event.ctrlKey && (event.key === "ArrowDown" || event.key === "ArrowUp")) {
      event.preventDefault(); // do not also scroll the list
      const step = event.key === "ArrowDown" ? 1 : -1;
      const next = Math.min(Math.max(selectedIndex + step, 0), tracks.length - 1);
      selectOnly(tracks[next].id);
      virtualizer.scrollToIndex(next);
    }
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div
        ref={scrollRef}
        // `outline-none`: keyboard focus is shown on the selected row, not
        // around the whole list. `group` is what lets the rows inside
        // react to this element having focus (`group-focus-visible:`).
        className="group min-h-0 flex-1 overflow-y-scroll outline-none"
        tabIndex={0}
        role="listbox"
        aria-multiselectable="true"
        aria-label="Songs"
        onKeyDown={onKeyDown}
      >
        {/* The header is inside the scrolling box (pinned to its top by
            `position: sticky`) so that it is exactly as wide as the rows,
            whatever the width of the scrollbar.

            The header is deliberately placed one pixel ABOVE the top edge
            of the list (`-top-px -mt-px`) and made one pixel taller to
            compensate (`pt-px`, 35px high), so its text does not move.
            The list's edge and the pinned header can each be rounded to a
            different whole pixel, which left a one-pixel slit where
            scrolling rows showed through; overlapping the edge leaves no
            room for one. The shadow also paints background just above. */}
        <div
          className={cx(
            rowGrid,
            "sticky -top-px z-1 -mt-px h-8.75 border-b border-edge bg-window pt-px shadow-[0_-2px_0_var(--color-window)]",
          )}
        >
          {/* Empty cells above the compare marks and the covers. */}
          {presentIds && <span />}
          <span />
          {COLUMNS.map((column) => (
            <button
              key={column.key}
              className={cx(
                "flex items-center gap-1.5 text-xs font-semibold tracking-[0.04em] text-muted uppercase hover:text-fg",
                column.alignRight && "justify-end",
              )}
              tabIndex={-1}
              onClick={() => onSort(column.key)}
            >
              {column.label}
              {sort.key === column.key && (
                <span className="text-[9px] text-accent">{sort.descending ? "▼" : "▲"}</span>
              )}
            </button>
          ))}
        </div>

        {/* This inner box is as tall as ALL rows together, which is what
            gives the scrollbar its correct size. */}
        <div className="relative" style={{ height: virtualizer.getTotalSize() }}>
          {virtualizer.getVirtualItems().map((item) => {
            const track = tracks[item.index];
            const isSelected = selected.has(track.id);
            // The song that is loaded in the player has its title in the
            // accent colour.
            const isCurrent = track.id === currentTrackId;
            const isPresent = presentIds?.has(track.id) ?? false;
            const classes = cx(
              rowGrid,
              "absolute top-0 left-0 h-11 w-full cursor-pointer",
              // A selected row also gets an accent stripe on its left
              // edge while the list has keyboard focus.
              isSelected
                ? "bg-raised group-focus-visible:shadow-[inset_2px_0_0_var(--color-accent)]"
                : "hover:bg-panel",
            );

            return (
              <div
                key={track.id}
                className={classes}
                role="option"
                aria-selected={isSelected}
                onPointerDown={(event) => onRowPointerDown(event, track)}
                onPointerMove={onRowPointerMove}
                onPointerUp={() => (press.current = null)}
                // `item.start` is measured from the top of the scrolling box,
                // which includes the header; rows are placed inside
                // `.table-rows`, which starts below it.
                style={{ transform: `translateY(${item.start - HEADER_HEIGHT}px)` }}
                onClick={(event) => onRowClick(event, item.index)}
                onDoubleClick={() => onPlay(item.index)}
                onContextMenu={(event) => {
                  event.preventDefault(); // replace the webview's own menu
                  // Right-clicking outside the selection moves it here.
                  if (!selected.has(track.id)) selectOnly(track.id);
                  setMenu({ x: event.clientX, y: event.clientY, index: item.index });
                }}
              >
                {/* Compare mode: a tick for "already has it", an empty
                    ring for "does not". */}
                {presentIds &&
                  (isPresent ? (
                    <span className="text-accent" title="Already there">
                      <CheckIcon size={16} />
                    </span>
                  ) : (
                    <span
                      className="mx-auto size-2.5 rounded-full border border-muted/60"
                      title="Not there yet"
                    />
                  ))}
                <CoverArt track={track} size={32} />
                <span className={cx("truncate", isCurrent ? "font-semibold text-accent" : "text-fg")}>
                  {track.title}
                </span>
                <span className={CELL}>{track.artist || "Unknown artist"}</span>
                <span className={CELL}>{track.album}</span>
                {/* `tabular-nums`: digits all the same width, so the
                    lengths line up down the column. */}
                <span className={cx(CELL, "text-right tabular-nums")}>
                  {formatDuration(track.durationMs)}
                </span>
              </div>
            );
          })}
        </div>
      </div>

      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
          items={[
            { label: "Play", onSelect: () => onPlay(menu.index) },
            { label: "Edit details and cover…", onSelect: () => onEdit(tracks[menu.index]) },
            // Only in a playlist. It acts on the whole selection, which
            // always includes the row that was right-clicked. Spreading an
            // empty array adds nothing, so the entry is simply absent
            // everywhere else.
            ...(onRemoveFromPlaylist
              ? [
                  {
                    label:
                      selected.size > 1
                        ? `Remove ${songCount(selected.size)} from playlist`
                        : "Remove from playlist",
                    onSelect: () =>
                      onRemoveFromPlaylist(
                        tracks.filter((t) => selected.has(t.id)).map((t) => t.id),
                      ),
                  },
                ]
              : []),
          ]}
        />
      )}
    </div>
  );
});
