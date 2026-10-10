//! Reading and writing the metadata stored *inside* audio files: title,
//! artist, album... and the embedded cover picture.
//!
//! All of this is done by the `lofty` library, which understands the
//! different tag formats (ID3 in MP3, Vorbis comments in FLAC/Ogg, atoms in
//! M4A...) and gives us one interface over them. This is the only file that
//! talks to lofty.
//!
//! Learning links:
//! - lofty: <https://docs.rs/lofty>
//! - What ID3 tags are: <https://en.wikipedia.org/wiki/ID3>
//! - Traits (why we import `Accessor`, `AudioFile`... before calling their
//!   methods): <https://doc.rust-lang.org/book/ch10-02-traits.html>

use std::io::Cursor;
use std::path::Path;

use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFile, TaggedFileExt};
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey, Tag};

use crate::error::{AppError, AppResult};
use crate::wav_repair;

/// File extensions the scanner treats as music. Everything here can be
/// decoded by the player (see `player.rs`).
pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "m4a", "aac", "ogg", "oga", "wav", "aiff", "aif",
];

/// The tag fields the library keeps for each song.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackTags {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub track_no: Option<u32>,
    pub disc_no: Option<u32>,
    pub year: Option<i32>,
    pub duration_ms: i64,
    pub has_art: bool,
}

/// Changes to make to a file's text tags. Each field is `None` to leave
/// that tag alone, or `Some(text)` to set it (`Some("")` removes the tag).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextEdit {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
}

/// A picture taken out of, or about to be put into, an audio file.
#[derive(Debug, Clone, PartialEq)]
pub struct CoverArt {
    /// The encoded image file contents (JPEG or PNG bytes, not raw pixels).
    pub data: Vec<u8>,
    /// MIME type, e.g. `image/jpeg`.
    pub mime: String,
}

impl CoverArt {
    /// The file extension that suits this picture ("jpg", "png"...), for
    /// when it is saved as a file of its own.
    ///
    /// The return type `&'static str` is a string that lives for the whole
    /// run of the program, which is true of text written directly in the
    /// source code like the names below.
    /// <https://doc.rust-lang.org/book/ch10-03-lifetime-syntax.html#the-static-lifetime>
    pub fn file_extension(&self) -> &'static str {
        match self.mime.as_str() {
            "image/png" => "png",
            "image/gif" => "gif",
            "image/bmp" => "bmp",
            "image/webp" => "webp",
            // Nearly every embedded cover is a JPEG, so that is the
            // fallback for a type we do not recognise.
            _ => "jpg",
        }
    }
}

/// Whether `path` has one of the [`AUDIO_EXTENSIONS`] (case-insensitive).
pub fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| AUDIO_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Opens an audio file and parses its tags and audio properties.
///
/// The format is worked out from the file's *contents*, falling back to its
/// extension. That matters for extensions lofty does not know (`.oga` is
/// Ogg audio, but lofty only recognises `.ogg`) and for misnamed files.
///
/// A WAV file with misplaced chunks (see `wav_repair.rs`) is parsed from a
/// corrected copy held in memory, so its tags can be read even though the
/// file on disk is left as it is.
fn open(path: &Path) -> AppResult<TaggedFile> {
    if let Some(fixed) = wav_repair::repaired_bytes(path)? {
        return Probe::new(Cursor::new(fixed))
            .guess_file_type()
            .map_err(tag_error)?
            .read()
            .map_err(tag_error);
    }

    Probe::open(path)
        .map_err(tag_error)?
        .guess_file_type()
        .map_err(tag_error)?
        .read()
        .map_err(tag_error)
}

/// Reads the tags and length of one audio file.
///
/// A file with no title tag gets its file name as the title, so every song
/// has something to show in the list.
pub fn read_tags(path: &Path) -> AppResult<TrackTags> {
    let file = open(path)?;

    // A file can hold several tags (an MP3 may have both ID3v1 and ID3v2).
    // The "primary" one is the format's native tag; fall back to whatever is
    // there if it is missing.
    let tag = file.primary_tag().or_else(|| file.first_tag());

    let mut tags = TrackTags {
        duration_ms: file.properties().duration().as_millis() as i64,
        has_art: file.tags().iter().any(|t| !t.pictures().is_empty()),
        ..TrackTags::default()
    };

    if let Some(tag) = tag {
        // The accessors return `Option<Cow<str>>`: "maybe a string, maybe
        // borrowed". `text()` below flattens that to a trimmed `String`.
        tags.title = text(tag.title());
        tags.artist = text(tag.artist());
        tags.album = text(tag.album());
        tags.album_artist = text(tag.get_string(ItemKey::AlbumArtist));
        tags.track_no = tag.track();
        tags.disc_no = tag.disk();
        tags.year = tag.date().map(|date| i32::from(date.year));
    }

    if tags.title.is_empty() {
        tags.title = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
    }

    Ok(tags)
}

/// Returns the cover picture embedded in an audio file, if there is one.
///
/// Files can contain several pictures (front, back, artist photo...). We
/// prefer the one marked "front cover" and otherwise take the first.
pub fn read_cover(path: &Path) -> AppResult<Option<CoverArt>> {
    let file = open(path)?;

    // `flat_map` turns "a list of tags, each with a list of pictures" into
    // one flat list of pictures.
    let pictures: Vec<&Picture> = file.tags().iter().flat_map(|t| t.pictures()).collect();

    let chosen = pictures
        .iter()
        .find(|p| p.pic_type() == PictureType::CoverFront)
        .or_else(|| pictures.first());

    Ok(chosen.map(|picture| CoverArt {
        data: picture.data().to_vec(),
        mime: picture
            .mime_type()
            .map(|mime| mime.as_str().to_string())
            // Some files do not record the type; sniff it from the bytes.
            .unwrap_or_else(|| sniff_mime(picture.data()).to_string()),
    }))
}

/// Embeds `cover` into the audio file as its front cover, replacing the
/// cover it had. **This rewrites the file on disk.** Other tags, and other
/// kinds of pictures (back cover, artist...), are kept.
pub fn write_cover(path: &Path, cover: &CoverArt) -> AppResult<()> {
    // Editing needs a file the tag library can follow; see wav_repair.rs.
    wav_repair::repair_file(path)?;
    let mut file = open(path)?;

    let tag = primary_tag_mut(&mut file)?;

    // Remove the existing cover. "Other" is removed as well because some
    // formats (M4A) do not record what a picture is, so their cover is read
    // back as "Other"; leaving it would make the old cover win.
    remove_pictures(tag, |kind| {
        matches!(kind, PictureType::CoverFront | PictureType::Other)
    });

    tag.push_picture(
        Picture::unchecked(cover.data.clone())
            .pic_type(PictureType::CoverFront)
            .mime_type(MimeType::from_str(&cover.mime))
            .build(),
    );

    file.save_to_path(path, WriteOptions::default())
        .map_err(tag_error)?;

    // Read the file back and make sure the new cover is really there.
    if read_cover(path)?.as_ref() != Some(cover) {
        return Err(not_saved());
    }
    Ok(())
}

/// Changes the title, artist and/or album stored in the audio file.
/// **This rewrites the file on disk.** Everything else in the file (other
/// tags, pictures, the audio itself) is kept.
pub fn write_text(path: &Path, edit: &TextEdit) -> AppResult<()> {
    // Editing needs a file the tag library can follow; see wav_repair.rs.
    wav_repair::repair_file(path)?;
    let mut file = open(path)?;
    let tag = primary_tag_mut(&mut file)?;

    // Each `if let` runs only for the fields the caller wants changed.
    if let Some(title) = &edit.title {
        match title.trim() {
            "" => tag.remove_title(),
            text => tag.set_title(text.to_string()),
        }
    }
    if let Some(artist) = &edit.artist {
        match artist.trim() {
            "" => tag.remove_artist(),
            text => tag.set_artist(text.to_string()),
        }
    }
    if let Some(album) = &edit.album {
        match album.trim() {
            "" => tag.remove_album(),
            text => tag.set_album(text.to_string()),
        }
    }

    file.save_to_path(path, WriteOptions::default())
        .map_err(tag_error)?;

    // Read the file back and make sure the new values are really there.
    let saved = read_tags(path)?;
    let stuck = |wanted: &Option<String>, actual: &str| match wanted.as_deref().map(str::trim) {
        // A cleared title shows the file name instead, so only check
        // fields that were given a value.
        Some(text) if !text.is_empty() => text == actual,
        _ => true,
    };
    if !(stuck(&edit.title, &saved.title)
        && stuck(&edit.artist, &saved.artist)
        && stuck(&edit.album, &saved.album))
    {
        return Err(not_saved());
    }
    Ok(())
}

/// Returns the file's native ("primary") tag for editing, creating it if
/// the file does not have one yet.
///
/// `read_tags` prefers the primary tag but falls back to any other tag in
/// the file (an old MP3 may only have an ID3v1 tag). If we created an empty
/// primary tag next to such a tag, the file's existing title, artist and so
/// on would seem to vanish, so the new tag starts as a copy of them.
fn primary_tag_mut(file: &mut TaggedFile) -> AppResult<&mut Tag> {
    let tag_type = file.primary_tag_type();

    if file.tag(tag_type).is_none() {
        let mut tag = Tag::new(tag_type);
        if let Some(old) = file.first_tag() {
            if let Some(title) = old.title() {
                tag.set_title(title.into_owned());
            }
            if let Some(artist) = old.artist() {
                tag.set_artist(artist.into_owned());
            }
            if let Some(album) = old.album() {
                tag.set_album(album.into_owned());
            }
            if let Some(track) = old.track() {
                tag.set_track(track);
            }
        }
        file.insert_tag(tag);
    }

    file.tag_mut(tag_type)
        .ok_or_else(|| AppError::Tag("could not create a tag for this file".into()))
}

/// Removes every embedded picture from the audio file. **This rewrites the
/// file on disk.** Text tags are kept.
pub fn remove_cover(path: &Path) -> AppResult<()> {
    // Editing needs a file the tag library can follow; see wav_repair.rs.
    wav_repair::repair_file(path)?;
    let mut file = open(path)?;

    let tag_types: Vec<_> = file.tags().iter().map(|t| t.tag_type()).collect();
    for tag_type in tag_types {
        if let Some(tag) = file.tag_mut(tag_type) {
            remove_pictures(tag, |_| true);
        }
    }

    file.save_to_path(path, WriteOptions::default())
        .map_err(tag_error)?;
    Ok(())
}

/// Removes the pictures in `tag` whose type makes `should_remove` return true.
///
/// `impl Fn(PictureType) -> bool` means "any function or closure taking a
/// picture type and returning a bool". Closures:
/// <https://doc.rust-lang.org/book/ch13-01-closures.html>
fn remove_pictures(tag: &mut Tag, should_remove: impl Fn(PictureType) -> bool) {
    // Removing by index shifts the later pictures down, so search again from
    // the start after each removal rather than looping over indexes.
    while let Some(index) = tag
        .pictures()
        .iter()
        .position(|picture| should_remove(picture.pic_type()))
    {
        tag.remove_picture(index);
    }
}

/// Guesses an image's MIME type from its first bytes.
fn sniff_mime(data: &[u8]) -> &'static str {
    match image::guess_format(data) {
        Ok(image::ImageFormat::Png) => "image/png",
        Ok(image::ImageFormat::Gif) => "image/gif",
        Ok(image::ImageFormat::Bmp) => "image/bmp",
        Ok(image::ImageFormat::WebP) => "image/webp",
        _ => "image/jpeg",
    }
}

/// Flattens lofty's optional strings into a trimmed `String` ("" if absent).
fn text<S: AsRef<str>>(value: Option<S>) -> String {
    value
        .map(|s| s.as_ref().trim().to_string())
        .unwrap_or_default()
}

/// The error for a write that reported success but cannot be read back.
/// It means the file is laid out in a way the tag library cannot follow,
/// and saying so beats letting an edit silently disappear.
fn not_saved() -> AppError {
    AppError::Tag(
        "the change was written but cannot be read back, so this file's structure is probably damaged"
            .into(),
    )
}

/// Converts any of lofty's error types into our [`AppError`].
fn tag_error(error: impl std::fmt::Display) -> AppError {
    AppError::Tag(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{append_to_wav, info_title_chunk, riff_chunk, tiny_png, write_wav};

    #[test]
    fn a_cover_gets_the_extension_of_its_image_type() {
        let cover = |mime: &str| CoverArt {
            data: Vec::new(),
            mime: mime.to_string(),
        };
        assert_eq!(cover("image/png").file_extension(), "png");
        assert_eq!(cover("image/jpeg").file_extension(), "jpg");
        assert_eq!(cover("image/something-new").file_extension(), "jpg");
    }

    #[test]
    fn recognises_audio_files_by_extension() {
        assert!(is_audio_file(Path::new("/music/song.mp3")));
        assert!(is_audio_file(Path::new("/music/SONG.FLAC")));
        assert!(!is_audio_file(Path::new("/music/cover.jpg")));
        assert!(!is_audio_file(Path::new("/music/no_extension")));
    }

    #[test]
    fn untagged_file_uses_its_file_name_as_title() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("My Song.wav");
        write_wav(&path, 2);

        let tags = read_tags(&path).unwrap();
        assert_eq!(tags.title, "My Song");
        assert_eq!(tags.artist, "");
        assert!(!tags.has_art);
        // Two seconds of audio, give or take rounding.
        assert!((1990..=2010).contains(&tags.duration_ms), "{}", tags.duration_ms);
    }

    #[test]
    fn format_is_detected_from_contents_when_the_extension_is_unknown() {
        let dir = tempfile::tempdir().unwrap();
        // WAV data under an extension the tag library has never heard of.
        let path = dir.path().join("song.oga");
        write_wav(&path, 1);

        let tags = read_tags(&path).unwrap();
        assert!((990..=1010).contains(&tags.duration_ms));
    }

    #[test]
    fn cover_can_be_embedded_replaced_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("song.wav");
        write_wav(&path, 1);
        assert_eq!(read_cover(&path).unwrap(), None);

        // Embed a cover and read it back.
        let red = CoverArt { data: tiny_png([255, 0, 0]), mime: "image/png".into() };
        write_cover(&path, &red).unwrap();
        assert_eq!(read_cover(&path).unwrap(), Some(red));
        assert!(read_tags(&path).unwrap().has_art);

        // Embedding again replaces it rather than adding a second picture.
        let blue = CoverArt { data: tiny_png([0, 0, 255]), mime: "image/png".into() };
        write_cover(&path, &blue).unwrap();
        assert_eq!(read_cover(&path).unwrap(), Some(blue));
        let file = lofty::read_from_path(&path).unwrap();
        let picture_count: usize = file.tags().iter().map(|t| t.pictures().len()).sum();
        assert_eq!(picture_count, 1);

        // The audio is still intact after two rewrites.
        assert!((990..=1010).contains(&read_tags(&path).unwrap().duration_ms));

        remove_cover(&path).unwrap();
        assert_eq!(read_cover(&path).unwrap(), None);
        assert!(!read_tags(&path).unwrap().has_art);
    }

    /// The real-world case behind `wav_repair.rs`: a WAV with a stray byte
    /// in front of its tags.
    #[test]
    fn a_wav_with_misplaced_chunks_can_be_read_and_edited() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Artist - Song.wav");
        write_wav(&path, 1);
        append_to_wav(
            &path,
            &[riff_chunk(b"acid", &[0; 24]), vec![0], info_title_chunk("Real Title")].concat(),
        );
        let damaged = std::fs::read(&path).unwrap();

        // Reading finds the tag behind the damage, without touching the file.
        assert_eq!(read_tags(&path).unwrap().title, "Real Title");
        assert_eq!(std::fs::read(&path).unwrap(), damaged);

        // Editing repairs the file, and both kinds of edit then stick.
        let edit = TextEdit { artist: Some("New Artist".into()), ..TextEdit::default() };
        write_text(&path, &edit).unwrap();
        let cover = CoverArt { data: tiny_png([5, 6, 7]), mime: "image/png".into() };
        write_cover(&path, &cover).unwrap();

        let tags = read_tags(&path).unwrap();
        assert_eq!((tags.title.as_str(), tags.artist.as_str()), ("Real Title", "New Artist"));
        assert_eq!(read_cover(&path).unwrap(), Some(cover));
        assert!((990..=1010).contains(&tags.duration_ms));
        assert_eq!(crate::wav_repair::repaired_bytes(&path).unwrap(), None);
    }

    #[test]
    fn text_tags_can_be_set_changed_and_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("song.wav");
        write_wav(&path, 1);
        let cover = CoverArt { data: tiny_png([1, 2, 3]), mime: "image/png".into() };
        write_cover(&path, &cover).unwrap();

        // Set all three on a file that had none.
        let edit = TextEdit {
            title: Some("  My Title ".into()), // surrounding spaces are trimmed
            artist: Some("My Artist".into()),
            album: Some("My Album".into()),
        };
        write_text(&path, &edit).unwrap();
        let tags = read_tags(&path).unwrap();
        assert_eq!((tags.title.as_str(), tags.artist.as_str(), tags.album.as_str()),
                   ("My Title", "My Artist", "My Album"));

        // Change one; `None` fields are left alone, and so is the cover.
        let edit = TextEdit { artist: Some("Someone Else".into()), ..TextEdit::default() };
        write_text(&path, &edit).unwrap();
        let tags = read_tags(&path).unwrap();
        assert_eq!((tags.title.as_str(), tags.artist.as_str(), tags.album.as_str()),
                   ("My Title", "Someone Else", "My Album"));
        assert_eq!(read_cover(&path).unwrap(), Some(cover));
        assert!((990..=1010).contains(&tags.duration_ms));

        // An empty string removes the tag; the title falls back to the file name.
        let edit = TextEdit { title: Some(String::new()), album: Some(String::new()), ..TextEdit::default() };
        write_text(&path, &edit).unwrap();
        let tags = read_tags(&path).unwrap();
        assert_eq!((tags.title.as_str(), tags.artist.as_str(), tags.album.as_str()),
                   ("song", "Someone Else", ""));
    }

    /// Tries the whole cover art round trip on *your own* files, to check a
    /// format or an awkward file without risking it. Every audio file in
    /// the folder you name is copied to a temporary folder, and the copy
    /// gets a cover embedded, read back and removed again. Your originals
    /// are only read.
    ///
    /// It is skipped by a normal `cargo test`. Run it with:
    ///
    /// ```text
    /// SAMPLE_MUSIC=/path/to/some/songs cargo test real_files -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "needs SAMPLE_MUSIC to point at a folder of audio files"]
    fn cover_round_trip_on_real_files() {
        let folder = std::env::var("SAMPLE_MUSIC").expect("set SAMPLE_MUSIC to a folder");
        let dir = tempfile::tempdir().unwrap();
        let cover = CoverArt { data: tiny_png([200, 30, 30]), mime: "image/png".into() };
        let mut tested = 0;

        for entry in std::fs::read_dir(&folder).unwrap().flatten() {
            let original = entry.path();
            if !original.is_file() || !is_audio_file(&original) {
                continue;
            }
            let copy = dir.path().join(entry.file_name());
            std::fs::copy(&original, &copy).unwrap();
            let name = entry.file_name().to_string_lossy().into_owned();

            let before = read_tags(&copy).unwrap_or_else(|e| panic!("{name}: {e}"));

            write_cover(&copy, &cover).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(read_cover(&copy).unwrap().as_ref(), Some(&cover), "{name}");

            // Nothing but the picture changed.
            let after = read_tags(&copy).unwrap();
            assert_eq!(
                TrackTags { has_art: true, ..before.clone() },
                after,
                "{name}: tags changed when the cover was written"
            );

            remove_cover(&copy).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(read_cover(&copy).unwrap(), None, "{name}");

            // Text edits stick, and change nothing else.
            let edit = TextEdit {
                title: Some("New Title".into()),
                artist: Some("New Artist".into()),
                album: Some("New Album".into()),
            };
            write_text(&copy, &edit).unwrap_or_else(|e| panic!("{name}: {e}"));
            let edited = read_tags(&copy).unwrap();
            assert_eq!(
                TrackTags {
                    title: "New Title".into(),
                    artist: "New Artist".into(),
                    album: "New Album".into(),
                    has_art: false, // the cover was removed just above
                    ..before.clone()
                },
                edited,
                "{name}: text edit went wrong"
            );

            println!("ok  {name}  ({} - {})", after.artist, after.title);
            tested += 1;
        }

        assert!(tested > 0, "no audio files found in {folder}");
    }
}
