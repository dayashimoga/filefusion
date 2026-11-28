//! `ID3v2` and audio metadata tag inspection, editing, and privacy stripping (`FR-MET-001..002`).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use file_core::{ErrorCode, FileFusionError, FileFusionResult, FormatId};
use id3::{Tag, TagLike, Version};
use serde::{Deserialize, Serialize};

/// Normalized descriptive audio tag metadata (`FR-MET-001`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioTagMetadata {
    /// Track title.
    pub title: Option<String>,
    /// Primary artist / performer.
    pub artist: Option<String>,
    /// Album or release name.
    pub album: Option<String>,
    /// Release year if recorded.
    pub year: Option<i32>,
    /// Track number within release.
    pub track: Option<u32>,
    /// Embedded front cover / APIC artwork frame present.
    pub has_embedded_artwork: bool,
}

/// Reads `ID3v1` / `ID3v2` tags from an MP3 file (`FR-MET-001`).
///
/// # Errors
/// Returns [`FileFusionError`] if the file cannot be read or contains corrupt ID3 frames.
pub fn read_id3_metadata(path: &Path) -> FileFusionResult<AudioTagMetadata> {
    match Tag::read_from_path(path) {
        Ok(tag) => Ok(AudioTagMetadata {
            title: tag.title().map(str::to_owned),
            artist: tag.artist().map(str::to_owned),
            album: tag.album().map(str::to_owned),
            year: tag.year(),
            track: tag.track(),
            has_embedded_artwork: tag.pictures().next().is_some(),
        }),
        Err(err) if matches!(err.kind, id3::ErrorKind::NoTag) => Ok(AudioTagMetadata::default()),
        Err(err) => Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            format!("failed reading ID3 tags: {err}"),
        )),
    }
}

/// Writes normalized [`AudioTagMetadata`] as ID3v2.4 onto an MP3 file (`FR-MET-001`).
///
/// # Errors
/// Returns [`FileFusionError`] on I/O or tag serialization failure.
pub fn write_id3_metadata(path: &Path, meta: &AudioTagMetadata) -> FileFusionResult<()> {
    let mut tag = match Tag::read_from_path(path) {
        Ok(existing) => existing,
        Err(err) if matches!(err.kind, id3::ErrorKind::NoTag) => Tag::new(),
        Err(err) => {
            return Err(FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!("failed opening existing ID3 tag: {err}"),
            ));
        }
    };

    if let Some(title) = &meta.title {
        tag.set_title(title);
    }
    if let Some(artist) = &meta.artist {
        tag.set_artist(artist);
    }
    if let Some(album) = &meta.album {
        tag.set_album(album);
    }
    if let Some(year) = meta.year {
        tag.set_year(year);
    }
    if let Some(track) = meta.track {
        tag.set_track(track);
    }

    tag.write_to_path(path, Version::Id3v24).map_err(|err| {
        FileFusionError::new(
            ErrorCode::IntInvariant,
            format!("failed writing ID3v2.4 tag: {err}"),
        )
    })
}

/// Removes all `ID3v1` and `ID3v2` tags (including personal comments and APIC artwork) from `path`
/// (`FR-MET-002`). Returns `true` if a tag was stripped.
///
/// # Errors
/// Returns [`FileFusionError`] on I/O error while rewriting the MP3 container.
pub fn strip_id3_metadata(path: &Path) -> FileFusionResult<bool> {
    match Tag::remove_from_path(path) {
        Ok(removed) => Ok(removed),
        Err(err) => Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            format!("failed stripping ID3 metadata: {err}"),
        )),
    }
}

/// Lightweight check indicating whether non-essential metadata chunks exist (`ID3`, `LIST/INFO`,
/// or FLAC `VORBIS_COMMENT` / `PICTURE` metadata blocks).
#[must_use]
pub fn has_non_essential_metadata(path: &Path, format: FormatId) -> bool {
    match format {
        FormatId::Mp3 => Tag::read_from_path(path).is_ok(),
        FormatId::Wav => {
            let Ok(mut f) = File::open(path) else {
                return false;
            };
            let mut head = [0u8; 4096];
            let Ok(n) = f.read(&mut head) else {
                return false;
            };
            head[..n].windows(4).any(|w| w == b"INFO")
        }
        FormatId::Flac => has_flac_metadata_block(path),
        _ => false,
    }
}

fn has_flac_metadata_block(path: &Path) -> bool {
    let Ok(mut f) = File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 4];
    if f.read_exact(&mut magic).is_err() || &magic != b"fLaC" {
        return false;
    }

    for _ in 0..1024 {
        let mut hdr = [0u8; 4];
        if f.read_exact(&mut hdr).is_err() {
            return false;
        }
        let is_last = (hdr[0] & 0x80) != 0;
        let block_type = hdr[0] & 0x7F;
        let block_len = u32::from_be_bytes([0, hdr[1], hdr[2], hdr[3]]);

        // FLAC metadata block types: 4 = VORBIS_COMMENT, 6 = PICTURE.
        if block_type == 4 || block_type == 6 {
            return true;
        }
        if is_last {
            return false;
        }
        if f.seek(SeekFrom::Current(i64::from(block_len))).is_err() {
            return false;
        }
    }
    false
}
