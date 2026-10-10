// On Windows, a release build would otherwise open a console window behind
// the app. This line has no effect on Linux or macOS.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// The program's entry point. Everything real lives in `lib.rs`.
fn main() {
    music_player_lib::run()
}
