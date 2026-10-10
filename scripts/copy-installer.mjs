// Copies every freshly built installer to an easy-to-find place:
//
//     installer/boring-music-player_1.3.0_amd64.deb        (Debian, Ubuntu, Mint)
//     installer/boring-music-player-1.3.0-1.x86_64.rpm     (Fedora, openSUSE)
//     installer/boring-music-player_1.3.0_amd64.AppImage   (any Linux, no install)
//     installer/boring-music-player_1.3.0_x64_en-us.msi    (Windows)
//     installer/boring-music-player_1.3.0_x64-setup.exe    (Windows)
//
// Tauri writes each installer deep inside src-tauri/target/, one folder per
// type, under a name with spaces in it ("Boring Music Player_1.3.0_amd64.deb").
// This script runs automatically at the end of `npm run app:build` (see
// "scripts" in package.json) and copies out whichever types were built, under
// the same name in lower case with hyphens for the spaces. Which types get
// built is decided by "targets" in src-tauri/tauri.conf.json and by the
// operating system doing the build; a type that was not built is skipped.
//
// It can also be run by itself:  node scripts/copy-installer.mjs

import { copyFileSync, mkdirSync, readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, extname, join } from "node:path";
import { fileURLToPath } from "node:url";

// The project root is the folder above this script's folder.
const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const targetDir = join(root, "src-tauri", "target");
const outDir = join(root, "installer");

// Each installer type Tauri can make that we collect: the folder it is written
// to inside ".../release/bundle/", and the file extension to look for there.
const KINDS = [
  { folder: "deb", extension: ".deb" },
  { folder: "rpm", extension: ".rpm" },
  { folder: "appimage", extension: ".AppImage" },
  { folder: "msi", extension: ".msi" },
  { folder: "nsis", extension: ".exe" },
];

// Old versions pile up in the bundle folders, so only files with the current
// version in their name are wanted. The version Tauri puts in the name is the
// one in tauri.conf.json.
const { version } = JSON.parse(
  readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8"),
);

/** The names in a folder, or an empty list if the folder does not exist. */
function namesIn(dir) {
  try {
    return readdirSync(dir);
  } catch {
    return [];
  }
}

// A normal build writes to target/release/bundle. A build for another system
// (`tauri build --target x86_64-pc-windows-msvc`) writes to
// target/<that target>/release/bundle instead, so look in both.
const bundleDirs = [
  join(targetDir, "release", "bundle"),
  ...namesIn(targetDir).map((name) => join(targetDir, name, "release", "bundle")),
];

// For each type, find the newest installer of the current version.
const found = [];
for (const { folder, extension } of KINDS) {
  const candidates = bundleDirs
    .flatMap((bundleDir) => {
      const dir = join(bundleDir, folder);
      return namesIn(dir).map((name) => ({ name, path: join(dir, name) }));
    })
    .filter(({ name }) => name.endsWith(extension) && name.includes(version))
    .map((file) => ({ ...file, modified: statSync(file.path).mtimeMs }))
    .sort((a, b) => b.modified - a.modified);

  if (candidates.length > 0) found.push(candidates[0]);
}

if (found.length === 0) {
  console.error(
    `No installer for version ${version} found under ${targetDir}. Run "npm run app:build" first.`,
  );
  process.exit(1);
}

mkdirSync(outDir, { recursive: true });

console.log(`\nInstallers copied to ${outDir}:`);
for (const { name, path } of found) {
  // "Boring Music Player_1.3.0_amd64.deb" -> "boring-music-player_1.3.0_amd64.deb"
  // The extension keeps its capitals, since ".AppImage" is written that way.
  const extension = extname(name);
  const outName = name.slice(0, -extension.length).toLowerCase().replaceAll(" ", "-") + extension;

  // copyFileSync keeps the file's permissions, so the AppImage stays runnable.
  copyFileSync(path, join(outDir, outName));
  console.log(`  ${outName}`);
}

// How to install or run whatever was copied.
const copied = (extension) => found.some(({ name }) => name.endsWith(extension));
console.log("");
if (copied(".deb")) console.log("Install the .deb with:   sudo dpkg -i installer/<name>.deb");
if (copied(".rpm")) console.log("Install the .rpm with:   sudo rpm -U installer/<name>.rpm");
if (copied(".AppImage")) console.log("Run the AppImage with:   ./installer/<name>.AppImage");
console.log("");
