//! Helpers shared by the unit tests: tiny audio and image files made from
//! scratch, so the tests need no fixture files checked into the project.
//!
//! This module is only compiled for `cargo test` (see the `#[cfg(test)]`
//! next to its `mod` line in `lib.rs`).

use std::fs;
use std::io::Cursor;
use std::path::Path;

use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};

/// Sample rate of the test WAV files. Low, to keep them small.
const SAMPLE_RATE: u32 = 8000;

/// Writes a valid WAV file containing `seconds` of a quiet 440 Hz tone.
///
/// WAV is used because it is simple enough to write by hand: a 44-byte
/// header followed by raw samples. Format reference:
/// <http://soundfile.sapp.org/doc/WaveFormat/>
pub fn write_wav(path: &Path, seconds: u32) {
    let sample_count = SAMPLE_RATE * seconds;
    let data_bytes = sample_count * 2; // 16-bit mono = 2 bytes per sample

    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // size of the fmt chunk
    wav.extend_from_slice(&1u16.to_le_bytes()); // 1 = uncompressed PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // channels
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // bytes per second
    wav.extend_from_slice(&2u16.to_le_bytes()); // bytes per sample frame
    wav.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_bytes.to_le_bytes());
    for n in 0..sample_count {
        let t = n as f32 / SAMPLE_RATE as f32;
        let sample = (t * 440.0 * std::f32::consts::TAU).sin() * 3000.0;
        wav.extend_from_slice(&(sample as i16).to_le_bytes());
    }

    fs::write(path, wav).unwrap();
}

/// Returns the bytes of a 2x2 PNG filled with one colour.
pub fn tiny_png(rgb: [u8; 3]) -> Vec<u8> {
    let pixels = ImageBuffer::from_pixel(2, 2, Rgb(rgb));
    let mut out = Vec::new();
    DynamicImage::ImageRgb8(pixels)
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .unwrap();
    out
}

/// Builds one RIFF chunk: 4-letter name, length, data, and a padding byte
/// if the data length is odd.
pub fn riff_chunk(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = id.to_vec();
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
    out
}

/// A RIFF "LIST INFO" tag chunk holding just a title.
pub fn info_title_chunk(title: &str) -> Vec<u8> {
    let mut text = title.as_bytes().to_vec();
    text.push(0); // strings in this chunk are zero-terminated
    let mut body = b"INFO".to_vec();
    body.extend(riff_chunk(b"INAM", &text));
    riff_chunk(b"LIST", &body)
}

/// Appends raw bytes to a WAV file and corrects the size in its header,
/// the way a (possibly buggy) audio program would.
pub fn append_to_wav(path: &Path, extra: &[u8]) {
    let mut bytes = fs::read(path).unwrap();
    bytes.extend_from_slice(extra);
    let riff_size = (bytes.len() - 8) as u32;
    bytes[4..8].copy_from_slice(&riff_size.to_le_bytes());
    fs::write(path, bytes).unwrap();
}
