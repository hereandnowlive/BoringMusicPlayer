import { useState } from "react";
import { artUrl } from "../api";
import type { Track } from "../types";
import { NoteIcon } from "./Icons";

interface CoverArtProps {
  /** The track whose embedded cover to show; null shows the placeholder. */
  track: Track | null;
  /** Width and height on screen, in pixels. */
  size: number;
  /**
   * Show the original image instead of a small cached thumbnail. Use for
   * large displays such as the cover art dialog.
   */
  fullSize?: boolean;
}

/**
 * A square showing a track's embedded cover, or a music-note placeholder
 * when the track has none (or the image fails to load).
 */
export function CoverArt({ track, size, fullSize = false }: CoverArtProps) {
  // Thumbnails are requested at twice the on-screen size so they stay sharp
  // on high-density displays.
  const url = track ? artUrl(track, fullSize ? undefined : size * 2) : null;

  // Remember which address failed, rather than a plain true/false, so that
  // the placeholder goes away by itself once the address changes (a new
  // song, or a new cover on the same song).
  const [failedUrl, setFailedUrl] = useState<string | null>(null);
  const showImage = url !== null && url !== failedUrl;

  return (
    <div
      className="grid flex-none place-items-center overflow-hidden rounded-sm bg-raised text-muted"
      style={{ width: size, height: size }}
    >
      {showImage ? (
        <img
          className="size-full object-cover"
          src={url}
          alt=""
          loading="lazy"
          draggable={false}
          onError={() => setFailedUrl(url)}
        />
      ) : (
        <NoteIcon size={Math.round(size * 0.5)} />
      )}
    </div>
  );
}
