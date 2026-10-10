import { useEffect, useState } from "react";
import * as api from "../api";
import type { PlayerState } from "../types";
import { useBackendEvent } from "./useBackendEvent";

/** What the UI shows before the backend has reported anything. */
const INITIAL_STATE: PlayerState = {
  status: "stopped",
  track: null,
  positionMs: 0,
  durationMs: 0,
  shuffle: false,
  volume: 0.7,
  queueLength: 0,
  queuePosition: 0,
  error: null,
};

/**
 * The player's live state (current song, position, volume...).
 *
 * The backend owns this state. The UI never changes it directly: it sends a
 * command (see api.ts) and the result arrives here as a `player:state` event.
 */
export function usePlayer(): PlayerState {
  const [state, setState] = useState<PlayerState>(INITIAL_STATE);

  // Ask once at startup, in case something was already playing (for example
  // after the UI reloads during development).
  useEffect(() => {
    api.getPlayerState().then(setState, () => {});
  }, []);

  useBackendEvent("player:state", setState);

  return state;
}
