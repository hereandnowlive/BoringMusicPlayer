import { useEffect, useState } from "react";
import * as api from "../api";
import { button, buttonPrimary } from "../classes";

interface PlaylistDialogProps {
  onClose: () => void;
}

/**
 * The small "new playlist" window: a name box with Create and Cancel.
 *
 * The playlist is made by the backend, which also refuses a name that is
 * empty or already used; that message is shown under the box and the
 * dialog stays open so the name can be corrected. On success the dialog
 * just closes: the backend's `playlists:changed` event is what makes the
 * new playlist appear in the sidebar.
 */
export function PlaylistDialog({ onClose }: PlaylistDialogProps) {
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Escape closes the dialog.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  async function create() {
    setBusy(true);
    setError(null);
    try {
      await api.createPlaylist(name);
      onClose();
    } catch (e) {
      setError(api.errorMessage(e));
      setBusy(false);
    }
  }

  return (
    <div
      className="fixed inset-0 z-20 grid place-items-center bg-black/60 p-6"
      onClick={onClose}
    >
      <form
        className="flex w-90 flex-col gap-3 rounded-xl border border-edge bg-panel p-6 shadow-[0_16px_48px_rgb(0_0_0/50%)]"
        role="dialog"
        aria-modal="true"
        aria-label="New playlist"
        // Clicks inside the dialog must not reach the backdrop and close it.
        onClick={(event) => event.stopPropagation()}
        // Pressing Enter in the text box submits the form, i.e. creates.
        onSubmit={(event) => {
          event.preventDefault();
          if (name.trim() && !busy) void create();
        }}
      >
        <label className="flex flex-col gap-1">
          <span className="text-xs text-muted">Name of the new playlist</span>
          <input
            type="text"
            className="rounded-md border border-edge bg-window px-2.5 py-1.75 select-text"
            value={name}
            maxLength={100}
            // Put the cursor in the box as soon as the dialog opens.
            autoFocus
            onChange={(event) => setName(event.target.value)}
          />
        </label>

        {/* `min-h-5` keeps one line free, so an error appearing does not
            move the buttons. */}
        <div className="min-h-5 text-[13px] text-danger wrap-anywhere" aria-live="polite">
          {error}
        </div>

        <div className="flex justify-end gap-2">
          <button type="button" className={button} onClick={onClose}>
            Cancel
          </button>
          <button type="submit" className={buttonPrimary} disabled={busy || !name.trim()}>
            Create
          </button>
        </div>
      </form>
    </div>
  );
}
