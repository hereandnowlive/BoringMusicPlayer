import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../api";
import type {
  CopyProgress,
  CopyResult,
  Folder,
  Playlist,
  PlaylistAddResult,
  ScanProgress,
  ScanSummary,
  Track,
} from "../types";
import { useBackendEvent } from "./useBackendEvent";

/** While a scan runs, refresh the list at most this often (milliseconds). */
const RELOAD_INTERVAL_DURING_SCAN = 2000;

export interface Library {
  tracks: Track[];
  folders: Folder[];
  playlists: Playlist[];
  /** True until the first load from the backend has finished. */
  loading: boolean;
  /** Progress of the scan that is running, or null when idle. */
  scan: ScanProgress | null;
  /** What the most recent scan did, or null if none has finished yet. */
  lastScan: ScanSummary | null;
  /** Progress of a copy to a device that is running, or null when idle. */
  copy: CopyProgress | null;
  /**
   * Opens the folder picker and adds the chosen folder. Pass `true` to add
   * it as a portable music player that songs can be copied onto.
   */
  addFolder: (isDevice?: boolean) => Promise<void>;
  /** Copies tracks onto a device. Resolves to null if it could not start. */
  copyToDevice: (folderId: number, trackIds: number[]) => Promise<CopyResult | null>;
  removeFolder: (folderId: number) => Promise<void>;
  rescan: () => Promise<void>;
  /** Adds tracks to a playlist. Resolves to null if it failed. */
  addToPlaylist: (playlistId: number, trackIds: number[]) => Promise<PlaylistAddResult | null>;
  removeFromPlaylist: (playlistId: number, trackIds: number[]) => Promise<void>;
  deletePlaylist: (playlistId: number) => Promise<void>;
}

/**
 * Loads the library (tracks, folders and playlists) and keeps it up to date
 * as the backend scans folders, changes files or edits playlists.
 *
 * `onError` is called with anything that goes wrong, so the caller decides
 * how to show it.
 */
export function useLibrary(onError: (error: unknown) => void): Library {
  const [tracks, setTracks] = useState<Track[]>([]);
  const [folders, setFolders] = useState<Folder[]>([]);
  const [playlists, setPlaylists] = useState<Playlist[]>([]);
  const [loading, setLoading] = useState(true);
  const [scan, setScan] = useState<ScanProgress | null>(null);
  const [lastScan, setLastScan] = useState<ScanSummary | null>(null);
  const [copy, setCopy] = useState<CopyProgress | null>(null);

  // Counts reloads so that, if two overlap, a slow older one cannot
  // overwrite the result of a newer one.
  const reloadCounter = useRef(0);
  const lastReloadTime = useRef(0);

  const reload = useCallback(async () => {
    const thisReload = ++reloadCounter.current;
    lastReloadTime.current = Date.now();
    try {
      const [newTracks, newFolders, newPlaylists] = await Promise.all([
        api.listTracks(),
        api.listFolders(),
        api.listPlaylists(),
      ]);
      if (thisReload !== reloadCounter.current) return; // a newer reload started
      setTracks(newTracks);
      setFolders(newFolders);
      setPlaylists(newPlaylists);
    } catch (error) {
      onError(error);
    } finally {
      setLoading(false);
    }
  }, [onError]);

  // First load.
  useEffect(() => {
    void reload();
  }, [reload]);

  useBackendEvent("library:changed", () => void reload());

  // A playlist was edited. The songs themselves are unchanged, so only the
  // (small) playlist list is fetched again, not the whole library.
  useBackendEvent("playlists:changed", () => {
    api.listPlaylists().then(setPlaylists, onError);
  });

  useBackendEvent("scan:progress", (progress) => {
    setScan(progress);
    // Let songs appear while a long scan is still going, without reloading
    // the whole list on every progress tick.
    if (Date.now() - lastReloadTime.current > RELOAD_INTERVAL_DURING_SCAN) {
      void reload();
    }
  });

  useBackendEvent("scan:finished", (summary) => {
    setScan(null);
    setLastScan(summary);
    void reload();
  });

  useBackendEvent("copy:progress", setCopy);

  const addFolder = useCallback(
    async (isDevice = false) => {
      try {
        const path = await api.pickFolder(
          isDevice ? "Choose your MP3 player (or its music folder)" : undefined,
        );
        if (path === null) return; // dialog cancelled
        await api.addFolder(path, isDevice);
        await reload(); // show the folder immediately; its songs follow as the scan runs
      } catch (error) {
        onError(error);
      }
    },
    [reload, onError],
  );

  const copyToDevice = useCallback(
    async (folderId: number, trackIds: number[]) => {
      try {
        // The backend rescans afterwards, which is what makes the copied
        // songs appear in the device's list.
        return await api.copyToDevice(folderId, trackIds);
      } catch (error) {
        onError(error);
        return null;
      } finally {
        setCopy(null);
      }
    },
    [onError],
  );

  const removeFolder = useCallback(
    async (folderId: number) => {
      try {
        await api.removeFolder(folderId); // the backend then emits library:changed
      } catch (error) {
        onError(error);
      }
    },
    [onError],
  );

  const rescan = useCallback(async () => {
    try {
      await api.rescanLibrary();
    } catch (error) {
      onError(error);
    }
  }, [onError]);

  // The three playlist actions do not update the list themselves: the
  // backend emits `playlists:changed` when it is done, handled above.
  const addToPlaylist = useCallback(
    async (playlistId: number, trackIds: number[]) => {
      try {
        return await api.addToPlaylist(playlistId, trackIds);
      } catch (error) {
        onError(error);
        return null;
      }
    },
    [onError],
  );

  const removeFromPlaylist = useCallback(
    async (playlistId: number, trackIds: number[]) => {
      try {
        await api.removeFromPlaylist(playlistId, trackIds);
      } catch (error) {
        onError(error);
      }
    },
    [onError],
  );

  const deletePlaylist = useCallback(
    async (playlistId: number) => {
      try {
        await api.deletePlaylist(playlistId);
      } catch (error) {
        onError(error);
      }
    },
    [onError],
  );

  return {
    tracks,
    folders,
    playlists,
    loading,
    scan,
    lastScan,
    copy,
    addFolder,
    copyToDevice,
    removeFolder,
    rescan,
    addToPlaylist,
    removeFromPlaylist,
    deletePlaylist,
  };
}
