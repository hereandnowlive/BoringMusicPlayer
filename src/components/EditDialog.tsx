import { useEffect, useMemo, useState } from "react";
import * as api from "../api";
import { button, buttonDanger, buttonPrimary, cx } from "../classes";
import { songCount } from "../format";
import { albumTracks } from "../library";
import type { FileUpdateResult, Track } from "../types";
import { ContextMenu } from "./ContextMenu";
import { CoverArt } from "./CoverArt";

interface EditDialogProps {
  /** The track being edited. */
  track: Track;
  /** The whole library, used to find the other songs on the same album. */
  tracks: Track[];
  onClose: () => void;
}

/**
 * The "edit song" window. It does two jobs:
 *
 * - **Details:** change the title, artist and album stored in the file.
 * - **Cover art:** show the embedded cover at full size, replace it with a
 *   different image, remove it, or save a copy of it as an image file
 *   (a button, and also the cover's right-click menu).
 *
 * Artist, album and cover changes can be applied to the whole album at
 * once. A title is always just this one song's.
 *
 * Everything here rewrites the audio files, so nothing happens until the
 * user presses Save or picks an image, and removing a cover takes a second
 * click on the same button.
 */
export function EditDialog({ track, tracks, onClose }: EditDialogProps) {
  const album = useMemo(() => albumTracks(tracks, track), [tracks, track]);
  const hasAlbum = album.length > 1;

  // What is typed in the three boxes. They start as the track's current
  // values and are reset whenever those change (which is what happens after
  // a save: the library reloads and hands us the updated track).
  const [title, setTitle] = useState(track.title);
  const [artist, setArtist] = useState(track.artist);
  const [albumName, setAlbumName] = useState(track.album);
  useEffect(() => {
    setTitle(track.title);
    setArtist(track.artist);
    setAlbumName(track.album);
  }, [track.title, track.artist, track.album]);

  const [wholeAlbum, setWholeAlbum] = useState(false);
  const [busy, setBusy] = useState(false);
  const [confirmingRemove, setConfirmingRemove] = useState(false);
  const [result, setResult] = useState<FileUpdateResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Where a copy of the cover was last saved, to say so.
  const [savedTo, setSavedTo] = useState<string | null>(null);
  // Where the cover's right-click menu is open, or null when it is closed.
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);

  const targets = wholeAlbum && hasAlbum ? album : [track];
  const targetIds = targets.map((t) => t.id);

  // Which boxes differ from what is in the file. Only those are written.
  const titleChanged = title.trim() !== track.title;
  const artistChanged = artist.trim() !== track.artist;
  const albumChanged = albumName.trim() !== track.album;
  const anythingChanged = titleChanged || artistChanged || albumChanged;

  // Escape closes the dialog (but not in the middle of writing files).
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [busy, onClose]);

  /** Runs a file-changing action and shows how it went. */
  async function apply(action: () => Promise<FileUpdateResult>) {
    setBusy(true);
    setError(null);
    setResult(null);
    setSavedTo(null);
    setConfirmingRemove(false);
    try {
      setResult(await action());
      // The backend emits library:changed, the library reloads, and this
      // dialog re-renders with the updated track.
    } catch (e) {
      setError(api.errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  /** Writes the changed text boxes to the file(s). */
  async function saveDetails() {
    // `undefined` means "leave this tag alone" to the backend.
    const newArtist = artistChanged ? artist.trim() : undefined;
    const newAlbum = albumChanged ? albumName.trim() : undefined;
    const newTitle = titleChanged ? title.trim() : undefined;

    await apply(async () => {
      // This song gets every changed field, including its title...
      const own = await api.setTrackTags([track.id], {
        title: newTitle,
        artist: newArtist,
        album: newAlbum,
      });

      // ...and the rest of the album, if asked for, gets artist and album.
      const others = targetIds.filter((id) => id !== track.id);
      if (others.length === 0 || (!artistChanged && !albumChanged)) return own;
      const rest = await api.setTrackTags(others, { artist: newArtist, album: newAlbum });

      return {
        updated: own.updated + rest.updated,
        failures: [...own.failures, ...rest.failures],
      };
    });
  }

  async function chooseImage() {
    setConfirmingRemove(false);
    const imagePath = await api.pickImage();
    if (imagePath === null) return; // cancelled: nothing is changed
    await apply(() => api.setCoverArt(targetIds, imagePath));
  }

  /**
   * Saves a copy of this song's cover as an image file. The backend asks
   * where to put it. Unlike everything else here, this changes no music
   * file, so it does not go through `apply`.
   */
  async function saveCoverCopy() {
    setError(null);
    setResult(null);
    setSavedTo(null);
    setConfirmingRemove(false);
    try {
      setSavedTo(await api.saveCoverArt(track.id)); // null if cancelled
    } catch (e) {
      setError(api.errorMessage(e));
    }
  }

  /** One labelled text box of the details form. */
  const field = (label: string, value: string, onChange: (value: string) => void) => (
    <label className="flex flex-col gap-1">
      <span className="text-xs text-muted">{label}</span>
      <input
        type="text"
        // `select-text` turns text selection back on (the page as a whole
        // has it off; see index.html).
        className="rounded-md border border-edge bg-window px-2.5 py-1.75 select-text"
        value={value}
        disabled={busy}
        onChange={(event) => onChange(event.target.value)}
      />
    </label>
  );

  return (
    <div
      className="fixed inset-0 z-20 grid place-items-center bg-black/60 p-6"
      onClick={() => !busy && onClose()}
    >
      <div
        className="flex max-w-190 gap-6 rounded-xl border border-edge bg-panel p-6 shadow-[0_16px_48px_rgb(0_0_0/50%)]"
        role="dialog"
        aria-modal="true"
        aria-label="Edit song"
        // Clicks inside the dialog must not reach the backdrop and close it.
        onClick={(event) => event.stopPropagation()}
      >
        {/* ---- Left: the cover and its buttons ----
            The buttons are stacked and fill the column, so swapping
            "Remove cover" for "Confirm removal" does not move anything. */}
        <div className="flex flex-none flex-col gap-2">
          {/* Right-clicking the cover offers to save it. This replaces the
              webview's own menu, whose "Save Image As" does nothing here. */}
          <div
            onContextMenu={(event) => {
              event.preventDefault();
              setMenu({ x: event.clientX, y: event.clientY });
            }}
          >
            <CoverArt track={track} size={240} fullSize />
          </div>
          <button className={button} disabled={busy} onClick={() => void chooseImage()}>
            {track.hasArt ? "Replace cover…" : "Add cover…"}
          </button>
          {confirmingRemove ? (
            <button
              className={buttonDanger}
              disabled={busy}
              onClick={() => void apply(() => api.removeCoverArt(targetIds))}
            >
              Confirm removal
            </button>
          ) : (
            <button
              className={button}
              disabled={busy || !targets.some((t) => t.hasArt)}
              onClick={() => setConfirmingRemove(true)}
            >
              Remove cover
            </button>
          )}
          <button
            className={button}
            disabled={busy || !track.hasArt}
            onClick={() => void saveCoverCopy()}
          >
            Save cover as…
          </button>
        </div>

        {/* ---- Right: details, scope, save ---- */}
        <form
          // A fixed width (`w-100`, 400px), so the dialog does not resize
          // as its messages change.
          className="flex w-100 min-w-0 flex-col gap-3"
          // Pressing Enter in a text box submits the form, i.e. saves.
          onSubmit={(event) => {
            event.preventDefault();
            if (anythingChanged && !busy) void saveDetails();
          }}
        >
          {field("Title", title, setTitle)}
          {field("Artist", artist, setArtist)}
          {field("Album", albumName, setAlbumName)}

          <fieldset className="mt-1 flex flex-col gap-1.5" disabled={busy}>
            <legend className="mb-1.5 text-xs text-muted">
              Apply artist, album and cover changes to
            </legend>
            <label className="flex items-center gap-2">
              <input
                type="radio"
                className="accent-accent"
                name="scope"
                checked={!wholeAlbum}
                onChange={() => {
                  setWholeAlbum(false);
                  setConfirmingRemove(false);
                }}
              />
              This song only
            </label>
            <label className={cx("flex items-center gap-2", !hasAlbum && "text-muted")}>
              <input
                type="radio"
                className="accent-accent"
                name="scope"
                checked={wholeAlbum}
                disabled={!hasAlbum}
                onChange={() => {
                  setWholeAlbum(true);
                  setConfirmingRemove(false);
                }}
              />
              {hasAlbum
                ? `The whole album (${songCount(album.length)})`
                : "The whole album (no other songs found)"}
            </label>
          </fieldset>

          <p className="text-xs text-muted">
            Changes are saved inside the music {targets.length === 1 ? "file" : "files"}, so they
            show up in other players too.
          </p>

          {/* `min-h-10` leaves room for two lines, so messages appearing
              and disappearing do not move the buttons. */}
          <div
            className="flex min-h-10 flex-col gap-1 text-[13px] text-muted wrap-anywhere"
            aria-live="polite"
          >
            {busy && <span>Writing {songCount(targets.length)}…</span>}
            {confirmingRemove && (
              <span className="text-danger">
                This deletes the cover from {songCount(targets.length)}. Click again to confirm.
              </span>
            )}
            {error && <span className="text-danger">{error}</span>}
            {savedTo && <span>Cover saved to {savedTo}</span>}
            {result && (
              <>
                <span>Updated {songCount(result.updated)}.</span>
                {result.failures.map((failure) => (
                  <span key={failure} className="text-danger">
                    {failure}
                  </span>
                ))}
              </>
            )}
          </div>

          {/* `mt-auto` pushes the buttons to the bottom of the dialog. */}
          <div className="mt-auto flex gap-2">
            <button type="submit" className={buttonPrimary} disabled={busy || !anythingChanged}>
              Save details
            </button>
            <button type="button" className={button} disabled={busy} onClick={onClose}>
              Close
            </button>
          </div>
        </form>

        {menu && (
          <ContextMenu
            x={menu.x}
            y={menu.y}
            onClose={() => setMenu(null)}
            items={[
              {
                label: "Save cover as…",
                disabled: busy || !track.hasArt,
                onSelect: () => void saveCoverCopy(),
              },
            ]}
          />
        )}
      </div>
    </div>
  );
}
