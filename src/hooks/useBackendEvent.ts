import { useEffect, useRef } from "react";
import { subscribe, type BackendEvents } from "../api";

/**
 * Runs `handler` every time the backend emits `event`, for as long as the
 * component using this hook is on screen.
 *
 * The handler is kept in a ref so that passing a new function on each render
 * (which is what inline arrow functions do) does not tear down and recreate
 * the subscription.
 *
 * Custom hooks: https://react.dev/learn/reusing-logic-with-custom-hooks
 */
export function useBackendEvent<K extends keyof BackendEvents>(
  event: K,
  handler: (payload: BackendEvents[K]) => void,
): void {
  const latestHandler = useRef(handler);
  useEffect(() => {
    latestHandler.current = handler;
  });

  useEffect(() => {
    // `subscribe` returns the function that stops listening; returning it
    // from the effect makes React call it when the component goes away.
    return subscribe(event, (payload) => latestHandler.current(payload));
  }, [event]);
}
