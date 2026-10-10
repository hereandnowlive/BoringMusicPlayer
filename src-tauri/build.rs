// Cargo runs this small program before compiling the app. Tauri uses it to
// read tauri.conf.json and the capabilities/ folder and generate the glue
// code that `tauri::generate_context!()` picks up in src/lib.rs.
fn main() {
    // Anything a build script prints that starts with `cargo:` is an
    // instruction to Cargo, not a message. This one says "run me again,
    // and then recompile the app, whenever something in icons/ changes".
    //
    // It is needed because the window icon is baked into the program by
    // `generate_context!()` at compile time, and Cargo only watches the
    // files it has been told about. Without this line a new icon is
    // ignored ("Finished" with nothing compiled) and `npm run app` keeps
    // showing the old one.
    // https://doc.rust-lang.org/cargo/reference/build-scripts.html#rerun-if-changed
    println!("cargo:rerun-if-changed=icons");

    tauri_build::build()
}
