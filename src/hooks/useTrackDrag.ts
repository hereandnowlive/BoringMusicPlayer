import { useCallback, useEffect, useRef, useState } from "react";

/** A drag of songs out of the list that is in progress. */
export interface TrackDrag {
  trackIds: number[];
  /** Text for the label that follows the pointer. */
  label: string;
  /** Where the pointer is, in window coordinates. */
  x: number;
  y: number;
  /** The drop target under the pointer, if it is over one. */
  hoverTarget: DropTarget | null;
}

/**
 * Somewhere dragged songs can be dropped: a device folder (the songs are
 * copied onto it) or a playlist (they are added to it).
 */
export interface DropTarget {
  kind: "folder" | "playlist";
  id: number;
}

/**
 * The attribute that marks an element as a place songs can be dropped. Its
 * value says which target it is, as written by `dropTargetValue`. The
 * sidebar puts it on connected devices and on playlists.
 */
export const DROP_TARGET_ATTRIBUTE = "data-drop-target";

/** The attribute value for a target: "folder:3" or "playlist:7". */
export function dropTargetValue(target: DropTarget): string {
  return `${target.kind}:${target.id}`;
}

/** Whether two targets (either may be absent) are the same one. */
export function sameTarget(a: DropTarget | null, b: DropTarget | null): boolean {
  return a !== null && b !== null && a.kind === b.kind && a.id === b.id;
}

/** Finds the drop target under a point in the window, if there is one. */
function targetAt(x: number, y: number): DropTarget | null {
  const element = document.elementFromPoint(x, y)?.closest(`[${DROP_TARGET_ATTRIBUTE}]`);
  const [kind, idText] = (element?.getAttribute(DROP_TARGET_ATTRIBUTE) ?? "").split(":");
  const id = Number(idText);
  if ((kind !== "folder" && kind !== "playlist") || !Number.isInteger(id)) return null;
  return { kind, id };
}

/**
 * Dragging songs from the list onto a device or a playlist in the sidebar.
 *
 * This is done with plain pointer events (move, up) instead of the
 * browser's built-in drag-and-drop feature. The built-in one behaves
 * differently in each webview and is awkward to control; following the
 * pointer ourselves is a few lines and works the same everywhere.
 *
 * How it fits together:
 * - `LibraryTable` notices a press-and-move on a row and calls `start`.
 * - While `drag` is non-null, this hook follows the pointer, works out
 *   which drop target (if any) is under it, and `App` draws the label.
 * - Releasing over a target calls `onDrop`; releasing anywhere else, or
 *   pressing Escape, cancels.
 *
 * Pointer events: https://developer.mozilla.org/docs/Web/API/Pointer_events
 */
export function useTrackDrag(onDrop: (target: DropTarget, trackIds: number[]) => void) {
  const [drag, setDrag] = useState<TrackDrag | null>(null);

  // The window listeners below are set up once per drag, but must call the
  // latest `onDrop`; a ref gives them that without re-subscribing.
  const latestOnDrop = useRef(onDrop);
  useEffect(() => {
    latestOnDrop.current = onDrop;
  });

  const start = useCallback((trackIds: number[], label: string, x: number, y: number) => {
    setDrag({ trackIds, label, x, y, hoverTarget: null });
  }, []);

  const dragging = drag !== null;
  useEffect(() => {
    if (!dragging) return;

    const onMove = (event: PointerEvent) => {
      const { clientX: x, clientY: y } = event;
      setDrag((current) => current && { ...current, x, y, hoverTarget: targetAt(x, y) });
    };

    const onUp = (event: PointerEvent) => {
      const target = targetAt(event.clientX, event.clientY);
      // Read the dragged ids from state as it is now, then end the drag.
      setDrag((current) => {
        if (current && target !== null) {
          // Deferred so the drop handler does not run inside a state update.
          const { trackIds } = current;
          window.setTimeout(() => latestOnDrop.current(target, trackIds), 0);
        }
        return null;
      });
    };

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setDrag(null);
    };

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [dragging]);

  return { drag, start };
}
