# skills.md

A working guide to this codebase for Claude: where things live, the rules the code depends on, and step-by-step playbooks for common changes. [README.md](README.md) is the longer, human-oriented tour; read its sections 4 to 6 when you need more depth than this file gives.

## What this is

A desktop music player for local files, built with Tauri 2. Linux only (`.deb`); never built or tested on Windows or macOS.

- **UI**: `src/`, TypeScript + React 19, styled with Tailwind CSS 4, built by Vite, drawn in a webview.
- **Backend**: `src-tauri/src/`, Rust. Everything that touches files, the database or sound.
- **Storage**: SQLite (`library.db`), which is only a cache of what is in the audio files.

## Commands

Run from the project root unless noted.

| Command | Use |
|---|---|
| `npm run typecheck` | Type-check the UI. Run after any change under `src/`. |
| `cd src-tauri && cargo test` | Rust unit tests (64, plus 1 ignored that needs `SAMPLE_MUSIC`). Run after any Rust change. |
| `cd src-tauri && cargo clippy` | Rust linter. Run after any Rust change. |
| `npm run app` | Run the app in development. Opens a window and keeps running, so start it in the background. Uses port 1420. |
| `npm run app:build` | Release build; copies the installer to `installer/boring-music-player_<version>_amd64.deb`. Takes minutes. |
| `npm run clean` | Deletes `src-tauri/target` (all Rust build output). Safe; the next build takes minutes. |

There are no UI tests and no test runner for TypeScript. `npm run typecheck` is the only automated check on that side.

## How the two halves talk

There are exactly three channels. Every feature uses one or more of them.

1. **Commands (UI → Rust).** A wrapper in `src/api.ts` calls `invoke("name", {...})`, which runs the `#[tauri::command]` function of the same name in `commands.rs`. Arguments are camelCase in TypeScript and snake_case in Rust; Tauri converts.
2. **Events (Rust → UI).** Rust calls `emit`; the UI listens with the `useBackendEvent` hook. The events are `player:state`, `scan:progress`, `scan:finished`, `copy:progress`, `library:changed` and `playlists:changed`, typed by `BackendEvents` in `src/api.ts`.
3. **Cover art (`art://`).** `<img src="art://localhost/<trackId>?size=64&v=<mtime>">`, answered by `art_protocol.rs`. Build these URLs with `artUrl()` in `src/api.ts`.

## Where things live

### Backend (`src-tauri/src/`)

| File | Responsibility |
|---|---|
| `lib.rs` | Startup: opens the database, starts the threads, registers commands and the `art://` protocol. |
| `commands.rs` | Every function the UI can call, plus `AppState`. Thin wrappers only. |
| `models.rs` | Structs sent to the UI as JSON. |
| `error.rs` | `AppError` and `AppResult<T>`, used by every fallible function. |
| `db.rs` | The `Db` struct, the schema, migrations, and all SQL in the project. |
| `scanner.rs` | Scanner thread; incremental scan by size and modification time. |
| `player.rs` | Player thread; owns the audio device and the queue. Driven by the `Command` enum. |
| `queue.rs` | Play order, linear and shuffle. Pure logic, no audio. |
| `tags.rs` | Reads and writes tags and embedded cover art with `lofty`. Holds `AUDIO_EXTENSIONS`. |
| `wav_repair.rs` | Repairs WAV files whose tag chunks sit behind junk bytes. |
| `artwork.rs` | Image resizing for embedding; `ThumbCache`. |
| `art_protocol.rs` | Handler for `art://` requests. |
| `device.rs` | Copies songs to an MP3 player; the plug/unplug watcher thread. No Tauri types. |
| `test_support.rs` | Test-only helpers that generate tiny WAV and PNG files. |

### UI (`src/`)

| File | Responsibility |
|---|---|
| `App.tsx` | Top-level layout. Owns state shared across the screen: search, sort, notice strip, open dialog. |
| `api.ts` | Every backend call, event type, and dialog helper. |
| `types.ts` | Data shapes received from the backend. |
| `library.ts` | Pure functions: `LibraryView` and `tracksInView`, search, sort, same-album grouping, and `membershipTest` (does a playlist or device already have a song). |
| `format.ts` | Pure formatting helpers (durations, counts). |
| `styles.css` | Tailwind entry point: the `@theme` colours (`window`, `panel`, `raised`, `edge`, `fg`, `muted`, `accent`, `on-accent`, `danger`) and a few base rules (focus ring, pointer cursor, disabled buttons). No component styles. |
| `classes.ts` | The `cx()` helper for conditional classes, and class strings shared by several components (`button`, `buttonPrimary`, `iconButton`, ...). |
| `hooks/` | `useLibrary` (tracks, folders, playlists, scan and copy status), `usePlayer` (live player state), `useLibraryView` (what the sidebar has selected: all, a folder or a playlist; persisted), `useSavedFlag` (a remembered on/off value), `useStartupReveal` (scrolls to the restored song once at startup), `useTrackDrag` (drag songs to a device or a playlist), `useBackendEvent`. |
| `components/` | `Sidebar` (built from `SidebarSection`, `ScrollList` and `SidebarEntry`), `PlaylistDialog`, `LibraryTable` (virtualised list and right-click menu), `NowPlaying`, `PlayerBar`, `SeekBar`, `EditDialog`, `CoverArt`, `ContextMenu` (the shared right-click menu), `Icons`. |

## Rules the code depends on

Follow these unless I say otherwise. If a task seems to need breaking one, raise it first.

- **The two seams must be changed together.** `api.ts` ↔ `commands.rs` and `types.ts` ↔ `models.rs` are kept in step by hand; nothing checks them. Every `models.rs` struct uses `#[serde(rename_all = "camelCase")]`.
- **The backend owns the truth.** The UI never decides what is playing. It sends a command, then draws whatever `player:state` reports. Player commands return nothing.
- **Components never call Tauri directly.** They go through `src/api.ts`.
- **All SQL stays in `db.rs`.** Other modules call methods on `Db`.
- **Migrations are append-only.** To change the schema, add a new `if version < N` block to `migrate` in `db.rs` and bump `user_version`. Never edit an existing block. The schema is at version 3.
- **Commands stay thin.** Real work goes in the module that owns it. Mark a command `#[tauri::command(async)]` if it is slow (many rows, file rewrites), so it does not block the window.
- **Fallible Rust functions return `AppResult<T>`.** `AppError::Other(message)` is for text shown to the user as is.
- **Audio files are only modified by the edit dialog** (`set_cover_art`, `remove_cover_art`, `set_track_tags`). Copying to a device only adds files. Do not add another code path that writes to the user's music without asking me.
- **After changing a file's tags or art**, call `scanner::refresh_track` and `thumbs.forget(track_id)`, then emit `library:changed`. `update_each_track` in `commands.rs` already does the first two.
- **A playlist holds track ids, not copies or paths.** Rows in `playlist_tracks` are removed by `ON DELETE CASCADE` when a track or playlist goes, so a moved or renamed file drops out of its playlists at the next scan. A playlist holds each track once. Unlike the database's other tables, playlists are not a cache: they cannot be rebuilt from the files.
- **What the list shows is a `LibraryView`** (`library.ts`): all songs, one folder or one playlist. `tracksInView` turns it into tracks.
- **"Is this song already on the device?" is decided in two places that must agree**: `is_on_device` in `device.rs` (used when copying) and `membershipTest` in `library.ts` (used for the red drag refusal and compare mode). Both match on title, artist and length within `SAME_SONG_TOLERANCE_MS`.
- **A device is a library folder with `is_device` set.** It is scanned like any folder. "All songs" excludes device folders, and album grouping stays within one folder.
- **Worker threads follow one pattern**: a thread that owns its state and sleeps on an `mpsc` channel, reached through a cloneable handle (`PlayerHandle`, `ScanHandle`). Use it for any new background work.
- **Styling is Tailwind utilities, not custom CSS.** Do not add component rules to `styles.css`; it holds only the theme and element-wide base rules. Write class names out in full (never build them from pieces), since Tailwind finds them by scanning the source as text. Each constant in `classes.ts` is a complete look; do not combine two that set the same property.
- **Pure logic goes in pure modules** (`queue.rs`, `library.ts`, `format.ts`) so it can be tested without audio, files or React.

## Playbooks

### Add a backend command

1. Write the `#[tauri::command]` function in `commands.rs`, in the matching section.
2. Add it to `generate_handler![...]` in `lib.rs`. If it is missing, the call fails at runtime with "command not found".
3. Add a typed wrapper in `src/api.ts`.
4. If it returns a new shape, add the struct to `models.rs` and the interface to `types.ts`.

### Add a backend event

1. Add a name constant next to `EVENT_LIBRARY_CHANGED` in `commands.rs` and `emit` it.
2. Add the event and its payload type to `BackendEvents` in `src/api.ts`.
3. Listen with `useBackendEvent`, usually inside a hook in `src/hooks/`.

### Store another tag or track field

1. `tags.rs`: add it to `TrackTags` and fill it in `read_tags`.
2. `db.rs`: new migration block, then update `TRACK_COLUMNS`, `row_to_track` and the `upsert_tracks` SQL. The column order in `TRACK_COLUMNS` must match the indexes in `row_to_track`.
3. `models.rs` and `types.ts`: add the field to `Track` in both.
4. To show it: add a column in `LibraryTable.tsx` (a width in `ROW_GRID`, an entry in `COLUMNS`, a cell in the row).

### Add a table (as playlists did)

1. `db.rs`: new migration block with `CREATE TABLE`, then methods on `Db` and a `row_to_*` function.
2. `models.rs` and `types.ts`: the new struct and interface.
3. Commands, `generate_handler!`, and `api.ts` wrappers, as above.
4. Emit an event after writes so the UI reloads: `library:changed` reloads everything; a narrower event (like `playlists:changed`) reloads just the new data.

### Add a persisted setting

Use `api.getSetting` / `api.setSetting` with a string key, as `useLibraryView.ts` does, or `useSavedFlag` for a plain on/off value. No schema change is needed. The keys `volume`, `shuffle`, `last_track` and `last_queue` belong to `player.rs`; `library_filter` belongs to `useLibraryView.ts`; `sidebar_folders_open` and `sidebar_playlists_open` belong to `Sidebar.tsx`.

### Change playback behaviour

- Which song comes next: `queue.rs`. Extend its tests.
- A new transport action: add a variant to `Command` in `player.rs`, handle it in `Engine::handle`, then add the command, handler entry and `api.ts` wrapper.
- A new field in the player state: `PlayerState` in `models.rs` and `types.ts`, filled in `Engine::publish`, with a default in `INITIAL_STATE` in `usePlayer.ts`.

### Add a UI component

1. Create it in `src/components/`. Take data and callbacks as props.
2. Put shared state in `App.tsx` or a hook, not in the component.
3. Style it with Tailwind utility classes in `className`, using the theme colours (`bg-panel`, `text-muted`, `border-edge`...). Reuse the button constants in `classes.ts`; use `cx()` for conditional classes.
4. Add new icons to `Icons.tsx` as inline SVG.

### Support another file extension

Add it to `AUDIO_EXTENSIONS` in `tags.rs`. It only plays if `rodio`'s symphonia decoders support the format. Opus and WMA are not supported.

## Before finishing a change

1. Run `npm run typecheck` if anything under `src/` changed.
2. Run `cargo test` and `cargo clippy` in `src-tauri/` if any Rust changed.
3. Add or extend unit tests for new Rust logic. Tests live in a `#[cfg(test)] mod tests` block at the bottom of the same file, with sentence-style names, and build their fixtures with `test_support.rs` and `tempfile`.
4. Update the file map in README section 5 if a file was added, removed or renamed, and the tables in this file to match.
5. Update the README's event table, recipes or limitations if the change affects them.
6. Follow [CLAUDE.md](CLAUDE.md): document the Rust, list the changed files, and raise design concerns.

## Things that are easy to get wrong

- **Type names differ across the seam in one place**: Rust's `ArtUpdateResult` is TypeScript's `FileUpdateResult`. Only the field names matter to JSON.
- **The app version lives in three files** that should be kept the same: `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and `package.json`. Only `tauri.conf.json` affects the installer.
- **The last song and its queue are restored at startup** by `restore_last_session` in `player.rs`, as a stopped player with a track. Do not assume "stopped" means "no track".
- **The database schema version is unrelated to the app version.**
- **The webview's built-in right-click menu is for web pages.** Entries such as "Save Image As" do nothing in the Tauri window, so `main.tsx` turns that menu off in production builds (text boxes excepted); it still appears in development. Where a right-click should do something, call `event.preventDefault()` in `onContextMenu` and show `ContextMenu` instead.
- **One dialog is opened from Rust**: `save_cover_art` in `commands.rs` shows its own save dialog, because the suggested file name depends on the image type. The other dialogs are opened by the UI through `api.ts`.
- **Some Tailwind sizes are tied to TypeScript constants**: `h-11` on rows is `ROW_HEIGHT`, `h-8.75` on the list header is `HEADER_HEIGHT` plus 1px, and `size-50` on the big cover is `COVER_SIZE`. Change both together.
- **The base font size is `text-sm` (14px), set on `<body>` in `index.html`**, along with the window-level classes (`select-none`, `overflow-hidden`, `scheme-dark`).
- **rodio has its own `Player` type.** In `player.rs` it is always written `rodio::Player` and the variable is called `sink`.
- **Each song is read fully into memory when it starts** (see `Engine::load`). Keep that in mind for anything touching long files.
- **Device file names must be FAT-safe.** Use `safe_name` in `device.rs` for anything written to a device.
- **A missing folder does not wipe its tracks.** The scanner deliberately skips unreachable folders so an unplugged drive keeps its library entries.
- **Overlapping library folders are refused** by `find_existing` in `commands.rs`.
- **The Tauri capability list** in `src-tauri/capabilities/default.json` only allows `core:default` and `dialog:allow-open`. A new plugin or built-in API needs an entry there, and new URL schemes need the CSP in `tauri.conf.json` updated.
- **App data paths**: database at `~/.local/share/com.mitchell.musicplayer/library.db`, thumbnails at `~/.cache/com.mitchell.musicplayer/thumbnails/`. Both are safe to delete. Never test against these; use `Db::open_in_memory()` and temporary folders.

## Planned work

[Todo.txt](Todo.txt) holds my running list. The larger items are an options button and a "compare" view for MP3 players and playlists. Check it before proposing new features, since an idea may already be listed.
