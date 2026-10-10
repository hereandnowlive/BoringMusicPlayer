// Small text-formatting helpers.

/** Formats a length in milliseconds as "3:07", or "1:02:45" past an hour. */
export function formatDuration(ms: number): string {
  const totalSeconds = Math.max(0, Math.floor(ms / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const ss = String(seconds).padStart(2, "0");

  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, "0")}:${ss}`;
  }
  return `${minutes}:${ss}`;
}

/**
 * Formats a number of bytes as "512 kB", "34.5 MB" or "1.23 GB".
 *
 * Uses steps of 1000, the same as the Files app on Ubuntu, so the numbers
 * agree with what the file manager shows for the same folder.
 */
export function formatSize(bytes: number): string {
  const units = ["bytes", "kB", "MB", "GB", "TB"];
  let value = Math.max(0, bytes);
  let unit = 0;
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000;
    unit += 1;
  }
  // Whole numbers for bytes and kB; three significant figures above that
  // ("1.23 GB", "34.5 MB", "512 MB").
  const digits = unit < 2 || value >= 100 ? 0 : value >= 10 ? 1 : 2;
  return `${value.toFixed(digits)} ${units[unit]}`;
}

/** The last part of a path: "/home/me/Music" becomes "Music". */
export function baseName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts.length > 0 ? parts[parts.length - 1] : path;
}

/** "1 song" / "1,234 songs". */
export function songCount(count: number): string {
  return `${count.toLocaleString()} ${count === 1 ? "song" : "songs"}`;
}
