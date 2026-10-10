// Copies the freshly built .deb installer to an easy-to-find place:
//
//     installer/boring-music-player_1.2.0_amd64.deb     (in the project root)
//
// Tauri writes the installer deep inside src-tauri/target/ under a name with
// spaces in it ("Boring Music Player_1.2.0_amd64.deb"). This script runs
// automatically at the end of `npm run app:build` (see "scripts" in
// package.json) and copies the newest one out, under the same name in lower
// case with hyphens for the spaces. The version in the name is therefore
// the version that was really built.
//
// It can also be run by itself:  node scripts/copy-installer.mjs

import { copyFileSync, mkdirSync, readdirSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

// The project root is the folder above this script's folder.
const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const bundleDir = join(root, "src-tauri", "target", "release", "bundle", "deb");
const outDir = join(root, "installer");

// Find every .deb Tauri has built, newest first. (Old versions pile up in
// that folder, so "the only file there" is not a safe assumption.)
let debs;
try {
  debs = readdirSync(bundleDir)
    .filter((name) => name.endsWith(".deb"))
    .map((name) => ({ name, modified: statSync(join(bundleDir, name)).mtimeMs }))
    .sort((a, b) => b.modified - a.modified);
} catch {
  debs = [];
}

if (debs.length === 0) {
  console.error(`No .deb found in ${bundleDir}. Run "npm run app:build" first.`);
  process.exit(1);
}

// "Boring Music Player_1.2.0_amd64.deb" -> "boring-music-player_1.2.0_amd64.deb"
const outName = debs[0].name.toLowerCase().replaceAll(" ", "-");
const outFile = join(outDir, outName);

mkdirSync(outDir, { recursive: true });
copyFileSync(join(bundleDir, debs[0].name), outFile);

console.log(`\nInstaller copied from "${debs[0].name}" to:\n  ${outFile}`);
console.log(`Install or upgrade with:\n  sudo dpkg -i installer/${outName}\n`);
