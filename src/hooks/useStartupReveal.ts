import { useEffect, useRef, useState } from "react";
import * as api from "../api";

/**
 * Shows, once, the song the app reopened on.
 *
 * The backend remembers the last song played and starts up with it loaded
 * (stopped) in the player; see `restore_last_session` in
 * src-tauri/src/player.rs. This hook asks which song that is and, as soon
 * as the song list is ready, calls `onReveal` with its id so the list can
 * scroll to it.
 *
 * `listReady` should become true when the list shows what it is going to
 * show: the library has loaded and the saved folder selection is restored.
 * Revealing earlier would look for the song in the wrong list.
 *
 * It runs once per launch. Later song changes do not move the list.
 */
export function useStartupReveal(listReady: boolean, onReveal: (trackId: number) => void): void {
  // `undefined` until the backend has answered; then the restored song's
  // id, or `null` if the app started with no song.
  const [restoredId, setRestoredId] = useState<number | null | undefined>(undefined);
  // A ref, not state: flipping it must not cause another render.
  // Docs: https://react.dev/reference/react/useRef
  const done = useRef(false);

  useEffect(() => {
    api.getPlayerState().then(
      (state) => setRestoredId(state.track?.id ?? null),
      () => setRestoredId(null),
    );
  }, []);

  useEffect(() => {
    if (done.current || !listReady || restoredId === undefined) return;
    done.current = true;
    if (restoredId !== null) onReveal(restoredId);
  }, [listReady, restoredId, onReveal]);
}
