//! Image work for cover art: preparing a picture the user chose so it can be
//! embedded, and making small thumbnails for the song list.
//!
//! `tags.rs` moves picture *bytes* in and out of audio files. This file
//! deals with what is inside those bytes (decoding, resizing, re-encoding),
//! using the `image` library.
//!
//! Learning links:
//! - image crate: <https://docs.rs/image>
//! - `PathBuf` vs `Path` (owned vs borrowed, like `String` vs `&str`):
//!   <https://doc.rust-lang.org/std/path/index.html>

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageFormat};

use crate::error::AppResult;
use crate::models::Track;
use crate::tags::{self, CoverArt};

/// Covers bigger than this (in pixels, on the longer side) are shrunk before
/// being embedded. A 4000px cover adds megabytes to *every* song on the
/// album and no player shows it that large.
const MAX_EMBEDDED_SIZE: u32 = 1200;

/// Loads an image file chosen by the user and gets it ready for embedding.
///
/// - A JPEG or PNG that is already a sensible size is embedded untouched, so
///   no quality is lost.
/// - Anything else (WebP, BMP, GIF, or an oversized picture) is shrunk if
///   needed and converted to JPEG, the format every music player can show.
pub fn prepare_for_embedding(image_path: &Path) -> AppResult<CoverArt> {
    let bytes = fs::read(image_path)?;
    let format = image::guess_format(&bytes)?;
    let picture = image::load_from_memory(&bytes)?;

    let fits = picture.width() <= MAX_EMBEDDED_SIZE && picture.height() <= MAX_EMBEDDED_SIZE;

    match format {
        ImageFormat::Jpeg if fits => Ok(CoverArt {
            data: bytes,
            mime: "image/jpeg".into(),
        }),
        ImageFormat::Png if fits => Ok(CoverArt {
            data: bytes,
            mime: "image/png".into(),
        }),
        _ => {
            // `thumbnail` scales to fit the given box, keeping the aspect
            // ratio. It would also scale a small picture *up*, so only call
            // it when the picture is actually too big.
            let resized = if fits {
                picture
            } else {
                picture.thumbnail(MAX_EMBEDDED_SIZE, MAX_EMBEDDED_SIZE)
            };
            Ok(CoverArt {
                data: encode_jpeg(&resized, 90)?,
                mime: "image/jpeg".into(),
            })
        }
    }
}

/// Encodes a picture as JPEG. `quality` runs from 1 (tiny, ugly) to 100.
fn encode_jpeg(picture: &DynamicImage, quality: u8) -> AppResult<Vec<u8>> {
    // JPEG has no transparency, so convert to plain RGB first.
    let rgb = picture.to_rgb8();
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(Cursor::new(&mut out), quality).encode_image(&rgb)?;
    Ok(out)
}

/// A folder of ready-made thumbnails, so scrolling the library does not mean
/// decoding a full-size cover for every row every time.
///
/// Files are named `<track id>_<file mtime>_<size>.jpg`. Because the file's
/// modification time is part of the name, a thumbnail made from an older
/// version of the song is simply never looked up again.
pub struct ThumbCache {
    dir: PathBuf,
}

impl ThumbCache {
    /// Uses (and creates if needed) `dir` as the cache folder.
    pub fn new(dir: PathBuf) -> AppResult<Self> {
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    /// Returns a JPEG thumbnail of the track's cover, at most `size` pixels
    /// on each side, or `None` if the track has no cover.
    pub fn thumbnail(&self, track: &Track, size: u32) -> AppResult<Option<Vec<u8>>> {
        let cached = self
            .dir
            .join(format!("{}_{}_{}.jpg", track.id, track.mtime, size));

        // Fast path: we made this one before.
        if let Ok(bytes) = fs::read(&cached) {
            return Ok(Some(bytes));
        }

        let Some(cover) = tags::read_cover(Path::new(&track.path))? else {
            return Ok(None);
        };
        let picture = image::load_from_memory(&cover.data)?;
        let jpeg = encode_jpeg(&picture.thumbnail(size, size), 85)?;

        // Failing to save the cache file only costs speed next time, so the
        // error is deliberately ignored (`let _ =`).
        let _ = fs::write(&cached, &jpeg);
        Ok(Some(jpeg))
    }

    /// Deletes every cached thumbnail of one track. Called after its cover
    /// is changed so the old files do not pile up.
    pub fn forget(&self, track_id: i64) {
        let prefix = format!("{track_id}_");
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return;
        };
        // `flatten()` skips directory entries that could not be read.
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(&prefix) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{tiny_png, write_wav};
    use image::{ImageBuffer, Rgb};

    /// Saves a plain grey picture of the given size and format.
    fn save_image(path: &Path, width: u32, height: u32, format: ImageFormat) {
        let pixels = ImageBuffer::from_pixel(width, height, Rgb([120u8, 120, 120]));
        DynamicImage::ImageRgb8(pixels)
            .save_with_format(path, format)
            .unwrap();
    }

    #[test]
    fn small_png_is_embedded_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cover.png");
        fs::write(&path, tiny_png([1, 2, 3])).unwrap();

        let cover = prepare_for_embedding(&path).unwrap();
        assert_eq!(cover.mime, "image/png");
        assert_eq!(cover.data, tiny_png([1, 2, 3]));
    }

    #[test]
    fn oversized_picture_is_shrunk_keeping_its_shape() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("huge.png");
        save_image(&path, 3000, 1500, ImageFormat::Png);

        let cover = prepare_for_embedding(&path).unwrap();
        assert_eq!(cover.mime, "image/jpeg");
        let result = image::load_from_memory(&cover.data).unwrap();
        assert_eq!((result.width(), result.height()), (1200, 600));
    }

    #[test]
    fn other_formats_are_converted_to_jpeg_without_resizing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cover.bmp");
        save_image(&path, 300, 300, ImageFormat::Bmp);

        let cover = prepare_for_embedding(&path).unwrap();
        assert_eq!(cover.mime, "image/jpeg");
        let result = image::load_from_memory(&cover.data).unwrap();
        assert_eq!((result.width(), result.height()), (300, 300));
    }

    #[test]
    fn a_file_that_is_not_an_image_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        fs::write(&path, "definitely not a picture").unwrap();
        assert!(prepare_for_embedding(&path).is_err());
    }

    #[test]
    fn thumbnails_are_made_cached_and_forgotten() {
        let dir = tempfile::tempdir().unwrap();
        let song = dir.path().join("song.wav");
        write_wav(&song, 1);
        let cache = ThumbCache::new(dir.path().join("thumbs")).unwrap();

        let mut track = Track {
            id: 7,
            path: song.to_string_lossy().into_owned(),
            folder_id: 1,
            title: "Song".into(),
            artist: String::new(),
            album: String::new(),
            album_artist: String::new(),
            track_no: None,
            disc_no: None,
            year: None,
            duration_ms: 1000,
            has_art: false,
            mtime: 100,
        };

        // No cover yet: no thumbnail, and nothing written to the cache.
        assert_eq!(cache.thumbnail(&track, 64).unwrap(), None);

        let big = dir.path().join("big.png");
        save_image(&big, 400, 400, ImageFormat::Png);
        tags::write_cover(&song, &prepare_for_embedding(&big).unwrap()).unwrap();
        track.mtime = 200; // the file changed, as the scanner would record

        let thumb = cache.thumbnail(&track, 64).unwrap().unwrap();
        let decoded = image::load_from_memory(&thumb).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (64, 64));
        assert!(dir.path().join("thumbs/7_200_64.jpg").exists());

        cache.forget(7);
        assert!(!dir.path().join("thumbs/7_200_64.jpg").exists());
    }
}
