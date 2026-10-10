# Boring Music Player

A personal desktop music player for local files, in the spirit of Clementine
and MusicBee.

- Build a library from any number of folders
- Shows the cover art embedded in each file
- Embeds new cover art into files, or replaces what is there (one song or a whole album), no external tagger needed
- Edits title, artist and album, saved into the files
- Double-click to play; plays on through the list, in order or shuffled
- Play / pause, stop, next, previous, seek, volume
- Shows title, artist, album, length, time played and time remaining
- Search and sortable columns
- Reopens on the song it was closed on
- Playlists: make your own lists and drag songs into them

The UI is TypeScript (React), styled with [Tailwind CSS](https://tailwindcss.com). Everything that touches files or sound is Rust.
[Tauri](https://tauri.app) joins the two into one desktop app.

---

## 1. One-time setup

**Do this first. The app will not compile on this machine until you do.**
Tauri needs the development headers for GTK, WebKit and ALSA, and installing
them needs your password:

```bash
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev libasound2-dev librsvg2-dev \
                 libayatana-appindicator3-dev libxdo-dev libssl-dev \
                 build-essential curl wget file
```

Already in place:

| Tool | Status |
|---|---|
| Rust 1.99 | Installed with [rustup](https://rustup.rs) into `~/.cargo`. Open a **new terminal** so `cargo` is on your `PATH`. |
| Node 24 + npm | Was already installed. |
| JavaScript packages | Installed in `node_modules/`. Recreate any time with `npm install`. |

## 2. Everyday commands

Run these from the project root (this folder).

| Command | What it does |
|---|---|
| `npm run app` | **Run the app for development.** Edits to `src/` appear instantly; edits to `src-tauri/` trigger a rebuild and restart. The first run compiles ~500 Rust crates and takes a few minutes. |
| `npm run app:build` | Build the optimised app and its installers (`.deb`, `.rpm` and AppImage). Each is copied to `installer/` with the version in its name, for example **`installer/boring-music-player_1.3.0_amd64.deb`**; the build prints how to install or run each one. (The bare program is `src-tauri/target/release/music-player`, and Tauri's originals are under `src-tauri/target/release/bundle/`.) |
| `cd src-tauri && cargo test` | Run the Rust unit tests. |
| `npm run typecheck` | Type-check the TypeScript. |
| `cd src-tauri && cargo clippy` | Rust linter. Its suggestions are a good way to learn idiomatic Rust. |

To try the tag and cover art code against your own files without risking them (it
works on temporary copies):

```bash
cd src-tauri
SAMPLE_MUSIC=/path/to/some/songs cargo test real_files -- --ignored --nocapture
```

## 3. Using it

- **The sidebar** has "All songs" at the top, then two sections that fold away when you click their heading: **Folders**, and **Playlists** at the bottom. Each section is at most half the sidebar tall and scrolls past that. Whether each is open is remembered.
- **Add folder** is the **+** button in the Folders heading. It picks a folder, which is scanned in the background, sub-folders included.
- **Add MP3 player** is the player-shaped button next to it. It adds a portable player (pick its drive, e.g. `/media/you/PLAYER`). It appears in the sidebar with a player icon and lists its songs like any folder. To copy songs to it, select them in the list (Ctrl-click and Shift-click select several) and **drag them onto the player**: it lights up while you drag. Songs are filed as `Music/Artist/Album/` on the player, and ones it already has are not copied again; a message names them. The player disappears from the sidebar when unplugged and comes back (and is rescanned) when plugged in again.
- Each sidebar entry shows its number of songs and, on the right, the combined size of those songs (other files in the folder are not counted).
- **Click a folder or a playlist** in the sidebar to show only its songs; **All songs** shows everything except what is on MP3 players (those are copies). The choice is remembered the next time the app opens.
- **Playlists.** The **+** in the Playlists heading asks for a name and makes an empty playlist. To fill it, select songs in the list and **drag them onto the playlist**, the same way as onto an MP3 player; a song already in it is not added twice. A playlist lists its songs in the order they were added, and playing from it queues just those songs. To take songs out, select them while the playlist is showing, right-click and choose **Remove from playlist**. The **×** beside a playlist deletes it (after a second click to confirm); the songs themselves are not touched.
- **Songs that are already there are refused.** While you drag, a playlist or MP3 player that already has every song you are holding turns red and says so, and the label you are dragging turns red over it. If it has only some of them, it says how many will be added. An MP3 player counts a song as present when it holds one with the same title, artist and (within two seconds) length.
- **Compare.** The ticked-list button on a playlist or MP3 player switches compare mode on for it. Every song in the list then shows a tick if that playlist or player already has it, or an empty ring if not, and the toolbar says how many do. It keeps working as you move between folders and search. Click the button again, or **Stop comparing** in the toolbar, to leave.
- **Double-click** a song to play it. The songs currently listed, in the order listed, become the queue, so searching or sorting first changes what plays next.
- **The last song played is remembered.** The app reopens with it loaded in the player, stopped at its start, and centred in the list if it is in the folder that is open. Press play to carry on from it through the same queue as before.
- **Right-click** a song and choose **Edit details and cover…**, or click the big cover in the bottom-left corner, to change its title, artist or album, or to replace or remove its embedded cover. Artist, album and cover changes can be applied to the whole album.
- **Save cover as…** writes a copy of a song's embedded cover to an image file, at its original size. It is a button in the edit dialog, and in the menu you get by right-clicking the cover there or the big cover in the bottom-left corner.
- **Double-click** the cover in the bottom-left corner, or any empty part of the player bar, to jump to it in the list (if the current search is not hiding it).
- **Space** toggles play/pause wherever the focus is in the window, except while typing in a text box or while a dialog is open. **← →** go to the previous or next song in the queue, and **Shift+← →** jump 5 seconds back or forward in the song. **↑ ↓** change the volume. These work the same way as Space. In the list, **Ctrl+↑ ↓** move the selection and **Enter** plays it.
- **Previous** restarts the song if more than 5 seconds have played; otherwise it goes to the song before.
- **Shuffle never runs out.** With shuffle on, the queue is a ring: every song plays once, then the same shuffled order starts again, and Previous on the first song goes round to the last. With shuffle off, playback stops after the last song and Previous on the first song restarts it.
- Click a column header to sort: ascending, descending, then back to the default (artist, album, track).
- **Rescan** (the circular arrow in the Folders heading) picks up files added, changed or deleted outside the app. This also happens on every start.

---

## 4. How it fits together

```
┌──────────────────── one desktop window ────────────────────┐
│                                                            │
│   UI  (src/)   TypeScript + React, drawn in a webview      │
│        │  ▲                                                │
│ invoke │  │ events            <img src="art://…">          │
│        ▼  │                           │                    │
│   Backend  (src-tauri/src/)   Rust    ▼                    │
│   commands.rs ──┬── db.rs ───────── library.db (SQLite)    │
│                 ├── scanner.rs ──── scanner thread         │
│                 ├── player.rs ───── player thread → sound  │
│                 ├── tags.rs ─────── your audio files       │
│                 └── artwork.rs ──── thumbnail cache        │
└────────────────────────────────────────────────────────────┘
```

There are exactly three ways the two halves talk. If you understand these,
you can trace anything:

1. **Commands (UI → Rust).** The UI calls a function in `src/api.ts`, which
   calls `invoke("some_command", {...})`. Tauri runs the Rust function of the
   same name in `commands.rs` and returns its result as JSON.
2. **Events (Rust → UI).** Rust emits a named event with some data; the UI
   listens with the `useBackendEvent` hook. There are six:

   | Event | Data | Meaning |
   |---|---|---|
   | `player:state` | `PlayerState` | Sent ~5×/second while playing, and on every change |
   | `scan:progress` | `ScanProgress` | A scan is running |
   | `scan:finished` | `ScanSummary` | A scan ended |
   | `copy:progress` | `CopyProgress` | Songs are being copied to an MP3 player |
   | `library:changed` | none | Reload the track, folder and playlist lists |
   | `playlists:changed` | none | Reload just the playlists |

3. **Cover art URLs.** Images are too big to push through JSON, so the UI
   just uses `<img src="art://localhost/42?size=64">` and Rust answers that
   request with image bytes, like a tiny web server (`art_protocol.rs`).

### Worked example: double-clicking a song

1. `LibraryTable.tsx` calls `onPlay(index)`.
2. `App.tsx` → `api.playTracks(ids of the visible songs, index)`.
3. `commands.rs::play_tracks` sends `Command::PlayQueue` down a channel.
4. The player thread (`player.rs`) receives it, hands the ids to `queue.rs`,
   looks the track up in `db.rs`, decodes the file and starts playback.
5. It publishes a new `PlayerState`; `lib.rs` emits it as `player:state`.
6. `usePlayer.ts` receives it, React re-renders, the player bar updates.

The UI **never** decides what is playing. It sends a request and then draws
whatever state the backend reports. That single direction of truth is why
there are no "UI says playing but nothing is" bugs.

### Two background threads

Both follow the same pattern: a thread that owns some state and sleeps on a
channel until it is sent a message.

- **Player thread** (`player.rs`) owns the sound device and the queue. It also
  wakes every 200 ms by itself to notice that a song ended and start the next.
- **Scanner thread** (`scanner.rs`) walks the folders. It is incremental: a
  file whose size and modification time are unchanged is not opened again.

---

## 5. File map

```
custom music player/
├── README.md               ← you are here
├── CLAUDE.md               instructions for Claude Code: how I want it to work with me
├── skills.md               Claude's guide to the code: map, rules, playbooks (loaded by CLAUDE.md)
├── Todo.txt                my running list of things to build
├── package.json            npm scripts and JavaScript dependencies
├── index.html              the page the webview loads; just a mount point for React
├── vite.config.ts          build tool config for the UI (React and Tailwind plugins)
├── tsconfig.json           TypeScript compiler settings
├── .github/workflows/
│   └── release.yml         GitHub Action: builds Linux and Windows installers and publishes a release
├── scripts/
│   ├── copy-installer.mjs  copies the built installers to installer/ after app:build
│   └── make-icon.py        builds the app icon artwork from the clipart
├── installer/              the built installers land here (not in git)
├── old-icons/              previous app icons, kept as a backup (not in git)
│
├── src/                    ── THE UI (TypeScript) ──────────────────────────
│   ├── main.tsx            entry point: mounts <App />
│   ├── App.tsx             top-level layout; owns search, sort and dialog state
│   ├── api.ts              ★ every call to and from the backend, typed
│   ├── types.ts            ★ data shapes; mirrors src-tauri/src/models.rs
│   ├── library.ts          pure functions: views, filter, sort, "same album", "already there?"
│   ├── format.ts           pure functions: "3:07", "1,234 songs"
│   ├── styles.css          Tailwind setup: the colour theme and a few base rules
│   ├── classes.ts          Tailwind class strings shared by several components (buttons)
│   ├── hooks/
│   │   ├── useBackendEvent.ts   subscribe to one backend event
│   │   ├── usePlayer.ts         the live player state
│   │   ├── useLibrary.ts        tracks, folders, scan status + actions
│   │   ├── useLibraryView.ts    what the sidebar has selected (all / a folder / a playlist); remembered between runs
│   │   ├── useSavedFlag.ts      an on/off value remembered between runs (sidebar sections open or closed)
│   │   ├── useStartupReveal.ts  at startup, scrolls the list to the song the app reopened on
│   │   └── useTrackDrag.ts      dragging songs from the list onto an MP3 player or a playlist
│   └── components/
│       ├── Sidebar.tsx          All songs, the Folders section, scan status, the Playlists section
│       ├── SidebarSection.tsx   one collapsible section of the sidebar (heading + contents)
│       ├── ScrollList.tsx       a scrolling list that keeps its items clear of the scrollbar
│       ├── SidebarEntry.tsx     one line of the sidebar: select, compare, remove, drop songs on
│       ├── PlaylistDialog.tsx   the small "new playlist" window
│       ├── LibraryTable.tsx     the virtualised song list + right-click menu
│       ├── NowPlaying.tsx       the big cover (bottom-left) with title/artist/album
│       ├── PlayerBar.tsx        transport buttons, progress bar, volume
│       ├── SeekBar.tsx          progress bar with elapsed / remaining time
│       ├── EditDialog.tsx       edit title/artist/album; replace / remove cover art
│       ├── CoverArt.tsx         one cover image, or a placeholder
│       ├── ContextMenu.tsx      the app's own right-click menu
│       └── Icons.tsx            inline SVG icons
│
└── src-tauri/              ── THE BACKEND (Rust) ───────────────────────────
    ├── Cargo.toml          Rust dependencies, each with a note on why
    ├── tauri.conf.json     window size, app name/id, security policy, bundling
    ├── build.rs            build-time hook Tauri requires; nothing to edit
    ├── capabilities/
    │   └── default.json    which built-in Tauri features the UI may use
    ├── icons/              app icons; source.png is built from clipart.png by scripts/make-icon.py
    └── src/
        ├── main.rs         3-line entry point; calls lib.rs
        ├── lib.rs          ★ startup: opens the database, starts the threads,
        │                     registers commands. The table of contents.
        ├── commands.rs     ★ every function the UI can call. Thin wrappers.
        ├── models.rs       ★ data shapes sent to the UI; mirrors src/types.ts
        ├── error.rs        the one error type used everywhere
        ├── db.rs           SQLite: schema and every SQL query in the project
        ├── scanner.rs      finds audio files; keeps the database in step with disk
        ├── tags.rs         reads and writes tags and embedded cover art (lofty)
        ├── wav_repair.rs   fixes WAV files whose tags are hidden behind junk bytes
        ├── artwork.rs      image resizing; the thumbnail cache
        ├── art_protocol.rs answers the UI's art:// image requests
        ├── device.rs       copies songs onto an MP3 player (Music/Artist/Album/)
        ├── player.rs       the player thread: decoding, playback, transport
        ├── queue.rs        play order: linear and shuffle. No audio, pure logic
        └── test_support.rs helpers for unit tests (makes tiny WAV/PNG files)
```

★ = start here. `api.ts` ↔ `commands.rs` and `types.ts` ↔ `models.rs` are the
two seams between the languages. **Nothing checks that those pairs agree**, so
when you change one side, change the other.

Not in git (see `.gitignore`), all safe to delete and regenerate:
`node_modules/`, `dist/`, `src-tauri/target/` (large), `src-tauri/gen/`.

### Where the app keeps its data

| What | Where | Safe to delete? |
|---|---|---|
| Library database | `~/.local/share/com.mitchell.musicplayer/library.db` | Yes. You lose the folder list, **your playlists**, the volume and shuffle settings and the last song played; re-add folders and it rebuilds. |
| Thumbnails | `~/.cache/com.mitchell.musicplayer/thumbnails/` | Yes, always. Rebuilt on demand. |

Apart from your playlists and settings, the database is only a cache of what is in your files. **The only time the app
changes your music files is when you edit a song's details or cover art in
the edit dialog.** Dragging songs onto an MP3 player only *adds* files to
the player; nothing is ever deleted or overwritten there. (If that file is a WAV with the fault described in
`wav_repair.rs`, the edit also tidies its layout; the audio is untouched.)

---

## 6. Recipes

**Add a new backend command**

1. Write a `#[tauri::command]` function in `commands.rs`.
2. Add its name to the `generate_handler![...]` list in `lib.rs`.
3. Add a typed wrapper in `src/api.ts`. Call that from a component.

**Store another tag (say, genre)**

1. `tags.rs`: add the field to `TrackTags`, fill it in `read_tags`.
2. `db.rs`: add a new `if version < 2 { ALTER TABLE tracks ADD COLUMN ... }`
   block in `migrate` (never edit the existing block), then add the column to
   `TRACK_COLUMNS`, `row_to_track` and the `upsert_tracks` SQL.
3. `models.rs` and `src/types.ts`: add the field to `Track` in both.
4. Show it: add a column in `LibraryTable.tsx` (a width in `ROW_GRID`, a header in `COLUMNS` and a cell in the row).

   Existing rows keep the column's default until their file changes. To
   refill everything, delete `library.db` and re-add your folders.

**Support another file extension**

Add it to `AUDIO_EXTENSIONS` in `tags.rs`. It will only play if the decoder
supports the format (see limitations below).

**Change how "next" is chosen** → `queue.rs`, and it has tests to extend.

**Change how something looks** → the Tailwind classes in that component's `className`. Colours are defined once, in the `@theme` block of `styles.css`; button styles shared between components are in `classes.ts`.

---

## 7. Reading the Rust

The code is commented for someone who knows C# or TypeScript but is new to
Rust. Every file starts with what it is for and links to the relevant Rust
Book chapters; every function says what it does.

Suggested reading order, easiest first:

1. `models.rs`: structs and enums, nothing else.
2. `queue.rs`: methods, `Option`, `Vec`, and a full set of tests. No libraries.
3. `error.rs`: how `Result` and `?` replace exceptions.
4. `db.rs`: a struct with methods; closures; `Mutex`.
5. `tags.rs`, `artwork.rs`: using libraries; traits.
6. `scanner.rs`: your first thread and channel (small).
7. `player.rs`: the same pattern, larger; an enum of commands and `match`.
8. `commands.rs`, `lib.rs`: how it is all wired to Tauri.

Rough translations from C#:

| Rust | C# equivalent |
|---|---|
| `Option<T>` (`Some(x)` / `None`) | `T?`, but you are forced to handle the null case |
| `Result<T, E>` and `?` | exceptions: `?` is "rethrow to my caller if this failed" |
| `struct` + `impl` block | a class's fields + its methods |
| `trait` | `interface` |
| `enum` with data, + `match` | a closed set of record types + `switch` pattern matching |
| `&T` / `&mut T` | a read-only / exclusive borrowed reference; no GC, the compiler checks lifetimes |
| `Arc<T>` | a normal shared object reference (reference-counted, thread-safe) |
| `Mutex<T>` | `lock (obj) { ... }`, except the data lives *inside* the lock |
| `mpsc::channel` | `System.Threading.Channels.Channel<T>` |
| `Box<dyn Fn(...)>` | a delegate (`Action<...>` / `Func<...>`) |
| `#[derive(...)]`, `#[tauri::command]` | attributes that generate code |
| `cargo` / `Cargo.toml` / crates | `dotnet` / `.csproj` / NuGet packages |

Learning resources:

- [The Rust Book](https://doc.rust-lang.org/book/): the standard introduction. Chapters 3–10 cover most of this codebase; chapter 16 covers the threads.
- [Rust for C#/.NET Developers](https://microsoft.github.io/rust-for-dotnet-devs/latest/): Microsoft's side-by-side guide.
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/): short runnable snippets per feature.
- [Tauri docs](https://tauri.app/start/), especially [Calling Rust from the frontend](https://tauri.app/develop/calling-rust/).
- Library docs: [rodio](https://docs.rs/rodio) (audio), [lofty](https://docs.rs/lofty) (tags), [rusqlite](https://docs.rs/rusqlite) (SQLite), [image](https://docs.rs/image).
- UI side: [React](https://react.dev/learn), [Tailwind CSS](https://tailwindcss.com/docs/styling-with-utility-classes) (styling), [TanStack Virtual](https://tanstack.com/virtual/latest) (the song list).

Tip: `cargo doc --open` (in `src-tauri/`) builds browsable documentation for
this project *and* the exact versions of every library it uses.

---

## 8. Limitations and what is not built

**Formats.** Plays MP3, FLAC, M4A (AAC and Apple Lossless), Ogg Vorbis, WAV
and AIFF. **Opus and WMA are not supported** (the decoder library has no
support for them), so files of those types are ignored by the scanner.

**Editing.** Title, artist, album and cover art can be edited. Other tags
(album artist, track number, year, genre…) are read-only. Note that albums are
grouped by album artist when a file has one, so renaming the artist does not
change which songs count as "the whole album".

**Memory.** Each song is read fully into memory when it starts (see the
comment in `player.rs::load` for why). Fine for normal tracks; a multi-hour
lossless file will use that much RAM while it plays.

**Playlists.** A playlist remembers songs by their place in the library, not by a copy. A song whose file is moved or renamed is seen as a new song after the next scan and drops out of its playlists; so does every song of a folder that is removed from the library. Playlists cannot be renamed or reordered yet, and are not exported as `.m3u` files.

**Not built:** an album/artist browser, gapless playback, repeat
modes, keyboard media keys / desktop "now playing" integration, a graphic
equaliser, watching folders for changes while running (use Rescan).

**What has and has not been tested.** The backend has 64 unit tests. The whole
app was also driven by hand through every feature above, using copies of real
MP3, M4A and FLAC files and a read-only scan of a real 1,800-song folder (about
3 seconds). Not tested: actually *hearing* the output (tests ran muted),
playback of Ogg and AIFF files, the installers from `npm run app:build`,
Windows and macOS, and libraries far beyond a few thousand songs.

## 9. Releasing a new version

"Releasing" here just means building a fresh installer and installing it
over the old one. There is no auto-updater: the installed app stays as it is
until you do this.

1. **Check it works.**

   ```bash
   npm run typecheck
   (cd src-tauri && cargo test)
   npm run app          # try the change by hand, then close the window
   ```

2. **Bump the version number** (optional but tidy). It appears in three
   files; keep them the same:

   | File | Line | Matters for |
   |---|---|---|
   | `src-tauri/tauri.conf.json` | `"version": "…"` | **The one that counts**: the version the installer and the system's package manager see |
   | `src-tauri/Cargo.toml` | `version = "…"` | Label only |
   | `package.json` | `"version": "…"` | Label only |

3. **Build.** Takes a few minutes; the last line printed is the installer's path.

   ```bash
   npm run app:build
   ```

4. **Install or upgrade.** This replaces whatever version is installed, even
   one with the same version number.

   ```bash
   sudo dpkg -i installer/boring-music-player_1.3.0_amd64.deb   # use the version you built
   ```

**Your data survives upgrades.** The library database and thumbnails live in
your home folder (see "Where the app keeps its data"), not in the installed
package.

**If the release changes the database tables**, add a new
`if version < N { ... }` block to `migrate` in `src-tauri/src/db.rs` and never
edit an existing block. The new version then upgrades an old database the
first time it opens. That number is the database's own schema version (it is
3 at the time of writing) and has nothing to do with the app version above.

**To uninstall:** `sudo apt remove music-player`. Your data folders are left
behind; delete them by hand if you want them gone.

### Publishing a release on GitHub

`.github/workflows/release.yml` builds the Linux packages (`.deb`, `.rpm`,
AppImage) and the Windows installers (`.msi`, `-setup.exe`) on GitHub's own
machines and publishes them as a release tagged `v<version>`.

1. Bump the version (step 2 above), commit and push.
2. On GitHub: **Actions → Release → Run workflow**.

The run fails at once if that version's tag already exists. The release stays
a private draft until both builds succeed, so a failed run publishes nothing.

### Other operating systems

The Windows build is made by the GitHub workflow above but the app has never
been run or tested there. macOS is not set up at all. Tauri can build for
both, and nothing in the code is Linux-only, but each version must be built
*on* that operating system.

- `"targets": "all"` in `src-tauri/tauri.conf.json` makes each system build
  its own installer types.
- `scripts/copy-installer.mjs` collects the Linux and Windows installers. It
  does not look for a macOS `.dmg`; add a line to `KINDS` for that.
- The builds will be unsigned, so Windows and macOS will warn before opening
  them. Removing the warnings needs paid developer certificates.

## 10. Troubleshooting

| Symptom | Cause / fix |
|---|---|
| Build fails with `Package webkit2gtk-4.1 was not found` (or `alsa`, `gtk+-3.0`…) | The step 1 `apt install` has not been run. |
| `cargo: command not found` | Open a new terminal, or run `source ~/.cargo/env`. |
| `Port 1420 is already in use` | Another `npm run app` is still running. |
| "No sound output available" | No default audio device. Check the system sound settings, then press play again. |
| A song is skipped with an error strip | The file is missing, unreadable or in an unsupported format. The message names the file. |
| A changed tag does not show up | Press Rescan. |
| An edit fails with "written but cannot be read back" | The file's internal layout is damaged in a way the app does not know how to fix, so it refused to pretend the edit worked. Re-exporting or converting the file usually cures it. (One such fault, junk bytes between the sections of a WAV file, *is* fixed automatically: see `wav_repair.rs`.) |
| The list looks wrong after changing the database code | Delete `library.db` (path above) and re-add your folders. This also deletes your playlists. |
