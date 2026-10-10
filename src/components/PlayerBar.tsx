import { useState, type MouseEvent } from "react";
import * as api from "../api";
import { cx, iconButton, iconButtonActive, playButton } from "../classes";
import type { PlayerState, Track } from "../types";
import {
  NextIcon,
  PauseIcon,
  PlayIcon,
  PreviousIcon,
  ShuffleIcon,
  StopIcon,
  VolumeIcon,
} from "./Icons";
import { SeekBar } from "./SeekBar";

interface PlayerBarProps {
  player: PlayerState;
  /** Scroll the song list to a track. */
  onReveal: (track: Track) => void;
  onError: (error: unknown) => void;
}

/**
 * The strip along the bottom of the window, to the right of the cover: the
 * transport buttons, the progress bar and the volume slider. (What is
 * playing is shown by `NowPlaying.tsx`.)
 */
export function PlayerBar({ player, onReveal, onError }: PlayerBarProps) {
  const isPlaying = player.status === "playing";
  const hasTrack = player.track !== null;

  // While the slider is being dragged it shows its own value, so it follows
  // the pointer exactly instead of waiting for the backend to echo each step.
  const [draggedVolume, setDraggedVolume] = useState<number | null>(null);
  const volume = draggedVolume ?? player.volume;

  /**
   * Double-clicking an empty part of the bar shows the current song in the
   * list, like double-clicking the cover. Double-clicks that land on a
   * control (a button, the progress bar, the volume slider) are ignored,
   * so skipping two songs quickly does not also jump the list.
   */
  function onBarDoubleClick(event: MouseEvent) {
    const target = event.target as HTMLElement;
    if (target.closest("button, input, [role=slider]")) return;
    if (player.track) onReveal(player.track);
  }

  /** Runs a backend command, reporting failure instead of ignoring it. */
  const run = (command: Promise<void>) => void command.catch(onError);

  return (
    <footer
      // Three columns: spacer | transport | volume. The outer two are equal
      // widths, which keeps the transport centred.
      // The bar gets the "hand" cursor while there is a song for a
      // double-click to jump to.
      className={cx(
        "col-start-2 grid grid-cols-[minmax(0,1fr)_minmax(300px,3fr)_minmax(0,1fr)] items-center gap-5 border-t border-edge bg-panel px-4 py-2.5",
        hasTrack && "cursor-pointer",
      )}
      onDoubleClick={onBarDoubleClick}
    >
      {/* An empty first column balances the volume column on the right, so
          the buttons stay centred. */}
      <div />

      {/* ---- Centre: buttons and progress ---- */}
      <div className="flex min-w-0 flex-col gap-1">
        <div className="flex items-center justify-center gap-1.5">
          <button
            className={player.shuffle ? iconButtonActive : iconButton}
            title={player.shuffle ? "Shuffle is on" : "Shuffle is off"}
            aria-pressed={player.shuffle}
            onClick={() => run(api.setShuffle(!player.shuffle))}
          >
            <ShuffleIcon />
          </button>
          <button
            className={iconButton}
            title="Previous"
            disabled={!hasTrack}
            onClick={() => run(api.previousTrack())}
          >
            <PreviousIcon size={24} />
          </button>
          <button
            className={playButton}
            title={isPlaying ? "Pause" : "Play"}
            disabled={!hasTrack}
            onClick={() => run(api.togglePause())}
          >
            {isPlaying ? <PauseIcon size={26} /> : <PlayIcon size={26} />}
          </button>
          <button
            className={iconButton}
            title="Stop"
            disabled={player.status === "stopped"}
            onClick={() => run(api.stop())}
          >
            <StopIcon size={24} />
          </button>
          <button
            className={iconButton}
            title="Next"
            disabled={!hasTrack}
            onClick={() => run(api.nextTrack())}
          >
            <NextIcon size={24} />
          </button>
        </div>

        <SeekBar
          positionMs={player.status === "stopped" ? 0 : player.positionMs}
          durationMs={player.durationMs}
          enabled={player.status !== "stopped"}
          onSeek={(ms) => run(api.seek(ms))}
        />
      </div>

      {/* ---- Right: volume ---- */}
      <div className="flex items-center justify-end gap-2 text-muted">
        <VolumeIcon />
        <input
          type="range"
          className="w-27.5 accent-accent"
          min={0}
          max={1}
          step={0.01}
          value={volume}
          aria-label="Volume"
          title={`Volume ${Math.round(volume * 100)}%`}
          onChange={(event) => {
            const value = Number(event.target.value);
            setDraggedVolume(value);
            run(api.setVolume(value));
          }}
          onPointerUp={() => setDraggedVolume(null)}
          onBlur={() => setDraggedVolume(null)}
        />
      </div>
    </footer>
  );
}
