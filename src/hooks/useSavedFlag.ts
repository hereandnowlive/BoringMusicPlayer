import { useCallback, useEffect, useState } from "react";
import * as api from "../api";

/**
 * An on/off value that is remembered between runs of the app, such as
 * whether a section of the sidebar is open.
 *
 * It starts as `initial`, switches to the saved value a moment later (once
 * the backend has answered), and is saved again every time it is changed.
 * A failure to load or save is ignored: the worst that happens is that the
 * value is not remembered.
 *
 * Returns the value and a function to set it, like React's `useState`.
 */
export function useSavedFlag(key: string, initial: boolean): [boolean, (value: boolean) => void] {
  const [value, setValue] = useState(initial);

  useEffect(() => {
    api.getSetting(key).then(
      (saved) => {
        if (saved !== null) setValue(saved === "true");
      },
      () => {},
    );
  }, [key]);

  const set = useCallback(
    (next: boolean) => {
      setValue(next);
      api.setSetting(key, String(next)).catch(() => {});
    },
    [key],
  );

  return [value, set];
}
