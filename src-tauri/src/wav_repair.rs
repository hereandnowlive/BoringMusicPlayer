//! Works around a specific kind of damaged WAV file.
//!
//! # The problem
//!
//! A WAV file is a list of "chunks". Each chunk is a 4-letter name, a
//! 4-byte length, then that many bytes of data, and the next chunk starts
//! straight after (with one padding byte if the length was odd):
//!
//! ```text
//! RIFF <size> WAVE | fmt  <16> ... | data <n> ...audio... | LIST <n> ...tags... | id3  <n> ...
//! ```
//!
//! Some files have junk between two chunks. Two real examples from this
//! music collection: a WAV from FL Studio 11 with one stray zero byte after
//! its `acid` chunk, and another with 56 bytes of debris after its `cue `
//! chunk. A program reading such a file strictly looks for the next chunk
//! in the wrong place, sees nonsense, and gives up. The audio still plays,
//! because the `data` chunk comes before the damage, but every tag after
//! it is invisible. Worse, a tag editor that cannot see the existing tags
//! appends a new tag chunk to the end of the file, where it is just as
//! unreachable, so edits "succeed" and then seem to vanish.
//!
//! # The fix
//!
//! [`repaired_bytes`] walks the chunks, steps over the junk to the next
//! recognisable chunk, and rebuilds the file with the chunks packed
//! correctly. It only does so if the *whole* rest of the file then makes
//! sense; anything stranger is left strictly alone. `tags.rs` uses it in
//! two ways:
//!
//! - **Reading** (scanning, showing art): the repaired copy is parsed in
//!   memory. The file on disk is not touched.
//! - **Editing**: [`repair_file`] first saves the repaired version, and the
//!   edit is then made to a healthy file.
//!
//! The junk between chunks is dropped, along with tag chunks that are
//! stale duplicates (see [`chunks_to_keep`]). Every chunk that is kept,
//! including the audio, is copied exactly.
//!
//! Learning links:
//! - The WAV / RIFF layout: <http://soundfile.sapp.org/doc/WaveFormat/>
//! - `Read` and `Seek`: <https://doc.rust-lang.org/std/io/index.html>

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use crate::error::AppResult;

/// The most junk, in bytes, we are willing to step over between two chunks.
const MAX_GAP: usize = 4096;

/// Chunk names we accept as "the file makes sense again from here" when
/// searching through junk. Being picky here stops a random run of letters
/// inside the junk from being mistaken for a chunk.
const KNOWN_CHUNKS: &[&[u8; 4]] = &[
    b"LIST", b"id3 ", b"ID3 ", b"bext", b"smpl", b"acid", b"cue ", b"inst", b"fact", b"iXML",
    b"_PMX", b"cart", b"JUNK", b"PAD ", b"PEAK", b"DISP", b"plst",
];

/// Where one chunk sits in the original file.
#[derive(Debug, Clone, Copy)]
struct Chunk {
    id: [u8; 4],
    /// Position of the chunk's data (just after its 8-byte header).
    data_start: u64,
    size: u32,
}

/// If `path` is a WAV file with misplaced chunks that can be fixed, returns
/// the corrected file contents. Returns `None` for every other file:
/// healthy WAVs, other formats, and files damaged in ways we do not
/// understand (those are left strictly alone).
pub fn repaired_bytes(path: &Path) -> AppResult<Option<Vec<u8>>> {
    let mut file = File::open(path)?;
    let Some(chunks) = find_misplaced_chunks(&mut file)? else {
        return Ok(None);
    };
    drop(file);

    let original = fs::read(path)?;
    let mut fixed = Vec::with_capacity(original.len());
    fixed.extend_from_slice(b"RIFF");
    fixed.extend_from_slice(&[0; 4]); // total size, filled in at the end
    fixed.extend_from_slice(b"WAVE");

    for chunk in chunks_to_keep(&chunks, &original) {
        let start = chunk.data_start as usize;
        let end = start + chunk.size as usize;
        fixed.extend_from_slice(&chunk.id);
        fixed.extend_from_slice(&chunk.size.to_le_bytes());
        fixed.extend_from_slice(&original[start..end]);
        if chunk.size % 2 == 1 {
            fixed.push(0); // chunks are padded to an even length
        }
    }

    // The size in the header counts everything after itself.
    let riff_size = (fixed.len() - 8) as u32;
    fixed[4..8].copy_from_slice(&riff_size.to_le_bytes());

    Ok(Some(fixed))
}

/// Decides which chunks go into the repaired file.
///
/// Everything is kept except surplus *tag* chunks. A damaged file that has
/// been through tag editors tends to collect many of them, because each
/// editor, unable to see the tags already there, appended another set. One
/// file here had 14 `LIST` and 18 ID3 chunks. Left in, they would make the
/// file's tags ambiguous, so one of each kind is kept:
///
/// - **`LIST INFO`** (the WAV format's own simple tags): the last one.
/// - **ID3**: the last one written by *another* program, if there is one.
///   The ones this app's tag library appended while the file was damaged
///   (it names them `ID3 ` in capitals; other programs use `id3 `) hold
///   only the single field that was being edited at the time, since it
///   could not see the rest. Choosing one of those would throw away the
///   title, artist and cover that the file really has.
fn chunks_to_keep(chunks: &[Chunk], original: &[u8]) -> Vec<Chunk> {
    let is_info = |chunk: &Chunk| {
        let start = chunk.data_start as usize;
        &chunk.id == b"LIST" && original.get(start..start + 4) == Some(&b"INFO"[..])
    };
    let is_id3 = |chunk: &Chunk| chunk.id.eq_ignore_ascii_case(b"id3 ");

    // `rposition` searches from the end, giving the index of the last match.
    let info_to_keep = chunks.iter().rposition(is_info);
    let id3_to_keep = chunks
        .iter()
        .rposition(|chunk| &chunk.id == b"id3 ")
        .or_else(|| chunks.iter().rposition(is_id3));

    chunks
        .iter()
        .enumerate()
        .filter(|(index, chunk)| {
            if is_info(chunk) {
                Some(*index) == info_to_keep
            } else if is_id3(chunk) {
                Some(*index) == id3_to_keep
            } else {
                true
            }
        })
        .map(|(_, chunk)| *chunk)
        .collect()
}

/// Rewrites `path` in its corrected form if it needs it. Returns whether
/// anything was changed.
///
/// The new contents go to a temporary file which then replaces the
/// original in one step (`rename`), so a crash or power cut part-way
/// through cannot leave a half-written song behind.
pub fn repair_file(path: &Path) -> AppResult<bool> {
    let Some(fixed) = repaired_bytes(path)? else {
        return Ok(false);
    };

    let mut temp_name = path.as_os_str().to_owned();
    temp_name.push(".repair-tmp");
    let temp_path = Path::new(&temp_name);

    let permissions = fs::metadata(path)?.permissions();
    fs::write(temp_path, fixed)?;
    fs::set_permissions(temp_path, permissions)?;
    fs::rename(temp_path, path)?;
    Ok(true)
}

/// Walks the chunk list of a WAV file.
///
/// Returns `Some(chunks)` only when the file is a WAV, junk had to be
/// stepped over at least once, and everything else made sense.
/// Returns `None` when there is nothing to fix or the file cannot be
/// understood.
fn find_misplaced_chunks(file: &mut File) -> io::Result<Option<Vec<Chunk>>> {
    let file_len = file.metadata()?.len();

    let mut header = [0u8; 12];
    if file.read_exact(&mut header).is_err() {
        return Ok(None); // shorter than a WAV header
    }
    if &header[0..4] != b"RIFF" || &header[8..12] != b"WAVE" {
        return Ok(None); // not a WAV file
    }

    let mut chunks = Vec::new();
    let mut junk_found = false;
    let mut pos: u64 = 12;

    while pos + 8 <= file_len {
        if let Some(chunk) = read_chunk_header(file, pos, file_len)? {
            chunks.push(chunk);
            // Move past the header, the data, and the padding byte if any.
            pos = chunk.data_start + u64::from(chunk.size) + u64::from(chunk.size % 2);
            continue;
        }

        // No chunk here. Is there a recognisable one a little further on?
        match find_next_chunk(file, pos, file_len)? {
            Some(next) => {
                junk_found = true;
                pos = next;
            }
            None => {
                // Unrecognisable. That is acceptable only if the rest of
                // the file is empty space; otherwise leave the file alone.
                if !rest_is_zeros(file, pos)? {
                    return Ok(None);
                }
                break;
            }
        }
    }

    Ok(junk_found.then_some(chunks))
}

/// Reads the 8-byte chunk header at `pos`. Returns `None` if what is there
/// cannot be a chunk: the name is not printable text, or the chunk would
/// run past the end of the file.
fn read_chunk_header(file: &mut File, pos: u64, file_len: u64) -> io::Result<Option<Chunk>> {
    let mut header = [0u8; 8];
    file.seek(SeekFrom::Start(pos))?;
    file.read_exact(&mut header)?;

    let id = [header[0], header[1], header[2], header[3]];
    let size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);

    let name_is_text = id.iter().all(|byte| (0x20..0x7f).contains(byte));
    let fits_in_file = pos + 8 + u64::from(size) <= file_len;

    Ok((name_is_text && fits_in_file).then_some(Chunk {
        id,
        data_start: pos + 8,
        size,
    }))
}

/// Searches the next [`MAX_GAP`] bytes after `pos` for the start of a chunk
/// with one of the [`KNOWN_CHUNKS`] names. Returns its position, or `None`
/// if there is not one.
fn find_next_chunk(file: &mut File, pos: u64, file_len: u64) -> io::Result<Option<u64>> {
    // Read the stretch to be searched in one go.
    let mut window = vec![0u8; MAX_GAP.min((file_len - pos) as usize)];
    file.seek(SeekFrom::Start(pos))?;
    file.read_exact(&mut window)?;

    // Slide along it one byte at a time, starting one byte in (position 0
    // was already tried by the caller).
    for offset in 1..window.len().saturating_sub(3) {
        let name = &window[offset..offset + 4];
        if !KNOWN_CHUNKS.iter().any(|known| known.as_slice() == name) {
            continue;
        }
        let candidate = pos + offset as u64;
        if candidate + 8 <= file_len && read_chunk_header(file, candidate, file_len)?.is_some() {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

/// Whether everything from `pos` to the end of the file is zero bytes.
fn rest_is_zeros(file: &mut File, pos: u64) -> io::Result<bool> {
    let mut rest = Vec::new();
    file.seek(SeekFrom::Start(pos))?;
    file.read_to_end(&mut rest)?;
    Ok(rest.iter().all(|&byte| byte == 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{append_to_wav, info_title_chunk, riff_chunk, write_wav};

    #[test]
    fn healthy_and_non_wav_files_are_left_alone() {
        let dir = tempfile::tempdir().unwrap();

        let wav = dir.path().join("fine.wav");
        write_wav(&wav, 1);
        append_to_wav(&wav, &info_title_chunk("Fine"));
        assert_eq!(repaired_bytes(&wav).unwrap(), None);

        let text = dir.path().join("notes.txt");
        fs::write(&text, "not audio").unwrap();
        assert_eq!(repaired_bytes(&text).unwrap(), None);
        assert!(!repair_file(&text).unwrap());
    }

    #[test]
    fn a_stray_byte_between_chunks_is_removed_and_nothing_else_changes() {
        let dir = tempfile::tempdir().unwrap();
        let healthy = dir.path().join("healthy.wav");
        let damaged = dir.path().join("damaged.wav");
        write_wav(&healthy, 1);
        write_wav(&damaged, 1);

        let acid = riff_chunk(b"acid", &[7; 24]);
        let info = info_title_chunk("Concrete Jungle");

        // The same chunks, with and without a stray zero between them.
        append_to_wav(&healthy, &[acid.clone(), info.clone()].concat());
        append_to_wav(&damaged, &[acid, vec![0], info].concat());

        let fixed = repaired_bytes(&damaged).unwrap().expect("should need repair");
        assert_eq!(fixed, fs::read(&healthy).unwrap());

        assert!(repair_file(&damaged).unwrap());
        assert_eq!(fs::read(&damaged).unwrap(), fs::read(&healthy).unwrap());
        // Once fixed there is nothing more to do.
        assert!(!repair_file(&damaged).unwrap());
    }

    #[test]
    fn non_zero_junk_between_chunks_is_stepped_over_too() {
        let dir = tempfile::tempdir().unwrap();
        let healthy = dir.path().join("healthy.wav");
        let damaged = dir.path().join("damaged.wav");
        write_wav(&healthy, 1);
        write_wav(&damaged, 1);

        let cue = riff_chunk(b"cue ", &[1; 52]);
        let info = info_title_chunk("Diabolico");
        let mut junk = vec![0u8; 55];
        junk.push(0xff);

        append_to_wav(&healthy, &[cue.clone(), info.clone()].concat());
        append_to_wav(&damaged, &[cue, junk, info].concat());

        let fixed = repaired_bytes(&damaged).unwrap().expect("should need repair");
        assert_eq!(fixed, fs::read(&healthy).unwrap());
    }

    #[test]
    fn of_many_tag_chunks_one_of_each_kind_survives() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("song.wav");
        write_wav(&path, 1);
        append_to_wav(
            &path,
            &[
                vec![0], // the damage
                info_title_chunk("oldest title"),
                riff_chunk(b"id3 ", b"oldest tag"),
                info_title_chunk("newest title"),
                riff_chunk(b"id3 ", b"newest real tag"),
                // Appended by this app while the file was unreadable:
                riff_chunk(b"ID3 ", b"partial edit 1"),
                riff_chunk(b"ID3 ", b"partial edit 2"),
            ]
            .concat(),
        );

        let fixed = repaired_bytes(&path).unwrap().unwrap();
        let as_text = String::from_utf8_lossy(&fixed);
        assert!(as_text.contains("newest title"));
        assert!(as_text.contains("newest real tag"));
        assert!(!as_text.contains("oldest"));
        assert!(!as_text.contains("partial edit"));
    }

    #[test]
    fn with_only_this_apps_tag_chunks_the_last_one_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("song.wav");
        write_wav(&path, 1);
        append_to_wav(
            &path,
            &[vec![0], riff_chunk(b"ID3 ", b"first try"), riff_chunk(b"ID3 ", b"second try")]
                .concat(),
        );

        let as_text = String::from_utf8_lossy(&repaired_bytes(&path).unwrap().unwrap()).into_owned();
        assert!(as_text.contains("second try"));
        assert!(!as_text.contains("first try"));
    }

    #[test]
    fn files_damaged_in_other_ways_are_not_touched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mystery.wav");
        write_wav(&path, 1);
        // Junk that is never followed by a recognisable chunk.
        append_to_wav(&path, &[0, 0xff, 0xfe, 0xfd, 0xfc, 1, 2, 3, 4, 5, 6, 7]);

        assert_eq!(repaired_bytes(&path).unwrap(), None);

        // Junk, a real chunk, then more junk the file never recovers from.
        write_wav(&path, 1);
        let debris = [0xffu8; 40];
        append_to_wav(&path, &[&[0u8][..], &info_title_chunk("x"), &debris[..]].concat());

        assert_eq!(repaired_bytes(&path).unwrap(), None);
    }
}
