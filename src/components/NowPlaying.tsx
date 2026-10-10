import { useRef, useState } from "react";
import type { Track } from "../types";
import { ContextMenu } from "./ContextMenu";
import { CoverArt } from "./CoverArt";

/** Width and height of the cover, in pixels. Must match `size-50` on the
 *  button below (Tailwind sizes are in steps of 4px: 50 x 4 = 200). */
const COVER_SIZE = 200;

/** How long a click waits for a second click, in milliseconds. */
const DOUBLE_CLICK_MS = 250;

/** Classes for the artist and album lines under the title. */
const LINE = "truncate text-xs text-white/80";

interface NowPlayingProps {
  /** The song loaded in the player, or null if there is none. */
  track: Track | null;
  /** Open the edit dialog (details and cover art) for a track. */
  onEdit: (track: Track) => void;
  /** Scroll the song list to a track. */
  onReveal: (track: Track) => void;
  /** Save a copy of a track's cover as an image file. */
  onSaveCover: (track: Track) => void;
}

/**
 * The large cover in the bottom-left corner of the window, with the
 * current song's title, artist and album written across its lower edge.
 *
 * It sits at the foot of the left-hand column, beside the player bar, and
 * not inside it. That is what lets the cover be big without making the
 * player bar tall.
 *
 * One click opens the edit dialog; a double-click shows the song in the
 * list. A double-click starts with a click, so the single-click action
 * waits a moment to see whether a second click follows.
 *
 * A right-click opens a menu with the same actions plus "Save cover as…".
 * It replaces the webview's own menu, whose "Save Image As" does nothing
 * in this kind of window.
 */
export function NowPlaying({ track, onEdit, onReveal, onSaveCover }: NowPlayingProps) {
  const clickTimer = useRef<number | undefined>(undefined);
  // Where the right-click menu is open, or null when it is closed.
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);

  function onClick() {
    if (!track) return;
    window.clearTimeout(clickTimer.current);
    clickTimer.current = window.setTimeout(() => onEdit(track), DOUBLE_CLICK_MS);
  }

  function onDoubleClick() {
    window.clearTimeout(clickTimer.current); // it was not a single click
    if (track) onReveal(track);
  }

  return (
    // The menu is a sibling of the button, not a child: a button may not
    // contain other buttons. The empty tag `<>` groups the two without
    // adding an element. Docs: https://react.dev/reference/react/Fragment
    <>
      <button
        // `relative` so the text can be positioned on top of the cover.
        // `disabled:opacity-100` undoes the fade every disabled button
        // gets (styles.css): the placeholder should not look washed out.
        className="relative mb-3 size-50 flex-none self-center overflow-hidden rounded-md enabled:hover:brightness-110 disabled:opacity-100"
        disabled={track === null}
        title={
          track ? "Click to edit details and cover; double-click to show in the library" : undefined
        }
        onClick={onClick}
        onDoubleClick={onDoubleClick}
        onContextMenu={(event) => {
          event.preventDefault(); // replace the webview's own menu
          if (track) setMenu({ x: event.clientX, y: event.clientY });
        }}
      >
        <CoverArt track={track} size={COVER_SIZE} />

        {/* The text lies on top of the cover, over a background that fades
            from clear to nearly black so it is readable on any cover. */}
        <div className="absolute inset-x-0 bottom-0 bg-[linear-gradient(transparent,rgb(0_0_0/85%)_55%)] px-2 pt-7 pb-2 text-center text-white">
          <div className="truncate text-[13px] font-semibold">
            {track ? track.title : "Nothing playing"}
          </div>
          {track && <div className={LINE}>{track.artist || "Unknown artist"}</div>}
          {track?.album && <div className={LINE}>{track.album}</div>}
        </div>
      </button>

      {menu && track && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
          items={[
            { label: "Show in the library", onSelect: () => onReveal(track) },
            { label: "Edit details and cover…", onSelect: () => onEdit(track) },
            {
              label: "Save cover as…",
              disabled: !track.hasArt,
              onSelect: () => onSaveCover(track),
            },
          ]}
        />
      )}
    </>
  );
}
