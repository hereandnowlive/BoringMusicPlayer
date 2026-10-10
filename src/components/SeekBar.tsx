import { useEffect, useRef, useState, type PointerEvent } from "react";
import { cx } from "../classes";
import { formatDuration } from "../format";

interface SeekBarProps {
  positionMs: number;
  durationMs: number;
  /** False when nothing is loaded, which makes the bar inert. */
  enabled: boolean;
  /** Called with the new position when the user clicks or drags. */
  onSeek: (positionMs: number) => void;
}

/** Classes shared by the two time readouts. `tabular-nums` makes every
 *  digit the same width, so the text does not jiggle as the time counts. */
const TIME = "flex-none text-xs text-muted tabular-nums";

/**
 * The progress bar: time played on the left, time remaining and total
 * length on the right, and a bar between them that can be clicked or
 * dragged to jump around in the song.
 */
export function SeekBar({ positionMs, durationMs, enabled, onSeek }: SeekBarProps) {
  const barRef = useRef<HTMLDivElement>(null);

  // While the user is dragging (and for the instant after letting go, until
  // the backend confirms the jump) the bar shows this position instead of
  // the real one. Otherwise the handle would fight the user's pointer.
  const [override, setOverride] = useState<number | null>(null);
  const dragging = useRef(false);

  // A new position from the backend after a drag has ended means the seek
  // has taken effect, so go back to showing the real position.
  useEffect(() => {
    if (!dragging.current) setOverride(null);
  }, [positionMs]);

  const shownMs = Math.min(override ?? positionMs, durationMs);
  const fraction = durationMs > 0 ? shownMs / durationMs : 0;

  /** Converts a pointer's horizontal position into a time in the song. */
  function timeAt(event: PointerEvent): number {
    const bar = barRef.current!.getBoundingClientRect();
    const x = Math.min(Math.max(event.clientX - bar.left, 0), bar.width);
    return (x / bar.width) * durationMs;
  }

  function onPointerDown(event: PointerEvent<HTMLDivElement>) {
    if (!enabled || durationMs <= 0) return;
    dragging.current = true;
    // "Capture" keeps the move/up events coming to this element even when
    // the pointer strays outside it during the drag.
    event.currentTarget.setPointerCapture(event.pointerId);
    setOverride(timeAt(event));
  }

  function onPointerMove(event: PointerEvent<HTMLDivElement>) {
    if (dragging.current) setOverride(timeAt(event));
  }

  function onPointerUp(event: PointerEvent<HTMLDivElement>) {
    if (!dragging.current) return;
    dragging.current = false;
    onSeek(timeAt(event)); // seek once, on release, not continuously
  }

  return (
    <div className="flex items-center gap-2.5">
      <span className={`${TIME} min-w-9.5`}>{formatDuration(shownMs)}</span>

      <div
        ref={barRef}
        // The clickable area (`h-4.5`, 18px) is taller than the visible
        // line, to be easy to hit. `group` lets the handle inside react
        // to this element being hovered.
        // https://tailwindcss.com/docs/hover-focus-and-other-states#styling-based-on-parent-state
        className={cx(
          "group relative h-4.5 flex-1 rounded-sm",
          enabled ? "cursor-pointer" : "cursor-default",
        )}
        role="slider"
        aria-label="Position in song"
        aria-valuemin={0}
        aria-valuemax={Math.round(durationMs / 1000)}
        aria-valuenow={Math.round(shownMs / 1000)}
        aria-valuetext={`${formatDuration(shownMs)} of ${formatDuration(durationMs)}`}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={() => {
          dragging.current = false;
          setOverride(null);
        }}
      >
        <div className="absolute inset-x-0 top-1.75 h-1 overflow-hidden rounded-xs bg-edge">
          <div className="h-full bg-accent" style={{ width: `${fraction * 100}%` }} />
        </div>
        {/* The round handle, hidden until the bar is hovered.
            `-ml-1.5` (half its width) centres it on the position. */}
        <div
          className={cx(
            "absolute top-0.75 -ml-1.5 size-3 rounded-full bg-fg opacity-0",
            enabled && "group-hover:opacity-100",
          )}
          style={{ left: `${fraction * 100}%` }}
        />
      </div>

      <span className={`${TIME} min-w-23 text-right`} title="Time remaining / total length">
        {enabled ? `-${formatDuration(durationMs - shownMs)} / ${formatDuration(durationMs)}` : "0:00"}
      </span>
    </div>
  );
}
