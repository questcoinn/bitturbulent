//! Metadata from a `.torrent` file's `info` dictionary (v1 and hybrid).

use bt_core::{FileIndex, InfoHash, PieceIndex, Sha1Hash, Sha256Hash};
use sha1::{Digest, Sha1};
use sha2::Sha256;

use crate::bencode::{DecodeError, Value};

/// Error returned when a `.torrent` file cannot be loaded.
#[derive(Debug, thiserror::Error)]
pub enum TorrentError {
    /// The input is not valid bencode or exceeds the decode limits.
    #[error("invalid bencode: {0}")]
    Decode(#[from] DecodeError),
    /// The file could not be read.
    #[error("cannot read torrent file: {0}")]
    Io(#[from] std::io::Error),
    /// The top-level value is not a dictionary.
    #[error("torrent is not a bencoded dictionary")]
    NotADictionary,
    /// A required field is absent.
    #[error("missing field `{0}`")]
    MissingField(&'static str),
    /// A field has the wrong type or an out-of-range value.
    #[error("invalid field `{0}`")]
    InvalidField(&'static str),
    /// `meta version` is 2 but the v1 fields are absent.
    #[error("v2-only torrents are not supported")]
    V2OnlyUnsupported,
    /// `meta version` is present and is not 2.
    #[error("unsupported meta version {0}")]
    UnsupportedMetaVersion(i64),
    /// `pieces` holds a different number of hashes than the total size needs.
    #[error("expected {expected} piece hashes, found {actual}")]
    PieceCountMismatch { expected: u64, actual: u64 },
    /// More pieces than a [`PieceIndex`] can address.
    #[error("too many pieces")]
    TooManyPieces,
    /// More files than a [`FileIndex`] can address.
    #[error("too many files")]
    TooManyFiles,
}

/// A file path exactly as written in the `.torrent`, not yet checked.
///
/// The components may contain `..`, look like absolute paths or contain
/// characters the OS rejects. There is deliberately no way to turn this into
/// a filesystem path directly; the storage layer must sanitize it first.
///
/// ```
/// fn show(p: &bt_metainfo::RawPath) -> String {
///     p.display()
/// }
/// ```
///
/// ```compile_fail
/// fn to_path(p: &bt_metainfo::RawPath) -> &std::path::Path {
///     p.as_ref()
/// }
/// ```
///
/// ```compile_fail
/// fn to_path_buf(p: bt_metainfo::RawPath) -> std::path::PathBuf {
///     p.into()
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawPath(Vec<Vec<u8>>);

impl RawPath {
    /// The path components as raw bytes, in order.
    pub fn components(&self) -> &[Vec<u8>] {
        &self.0
    }

    /// The components joined with `/`, with invalid UTF-8 replaced.
    /// For display only.
    pub fn display(&self) -> String {
        self.0
            .iter()
            .map(|c| String::from_utf8_lossy(c))
            .collect::<Vec<_>>()
            .join("/")
    }
}

/// One file of a torrent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    path: RawPath,
    length: u64,
    pad: bool,
}

impl FileEntry {
    /// Path relative to the torrent's root. For a single-file torrent this
    /// is just the torrent name.
    pub fn path(&self) -> &RawPath {
        &self.path
    }

    /// Size in bytes.
    pub fn length(&self) -> u64 {
        self.length
    }

    /// Whether this is a BEP 47 padding file (`attr` contains `p`). Padding
    /// files count toward piece layout but must not be created on disk.
    pub fn is_pad(&self) -> bool {
        self.pad
    }
}

/// The parsed `info` dictionary: what the torrent contains and how it is
/// split into pieces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TorrentInfo {
    info_hash: InfoHash,
    name: Vec<u8>,
    piece_length: u32,
    pieces: Vec<Sha1Hash>,
    files: Vec<FileEntry>,
    total_length: u64,
    multi_file: bool,
    private: bool,
    info_bytes: Box<[u8]>,
}

impl TorrentInfo {
    /// Parses a decoded `info` dictionary. Hashes are computed over
    /// `info.raw()`, the bytes as they appeared in the file.
    pub(crate) fn from_info(info: &Value<'_>) -> Result<Self, TorrentError> {
        if info.as_dict().is_none() {
            return Err(TorrentError::InvalidField("info"));
        }

        let hybrid = match info.get(b"meta version") {
            None => false,
            Some(v) => match v.as_int() {
                Some(2) if info.get(b"pieces").is_some() => true,
                Some(2) => return Err(TorrentError::V2OnlyUnsupported),
                Some(n) => return Err(TorrentError::UnsupportedMetaVersion(n)),
                None => return Err(TorrentError::InvalidField("meta version")),
            },
        };

        let name = utf8_preferred(info, b"name.utf-8", b"name")
            .ok_or(TorrentError::MissingField("name"))?
            .as_bytes()
            .ok_or(TorrentError::InvalidField("name"))?
            .to_vec();

        let piece_length = info
            .get(b"piece length")
            .ok_or(TorrentError::MissingField("piece length"))?
            .as_int()
            .and_then(|n| u32::try_from(n).ok())
            .filter(|&n| n > 0)
            .ok_or(TorrentError::InvalidField("piece length"))?;

        let pieces = info
            .get(b"pieces")
            .ok_or(TorrentError::MissingField("pieces"))?
            .as_bytes()
            .filter(|b| b.len() % Sha1Hash::LEN == 0)
            .ok_or(TorrentError::InvalidField("pieces"))?;

        let (files, multi_file) = match (info.get(b"length"), info.get(b"files")) {
            (Some(length), None) => {
                let length = parse_length(length)?;
                let path = RawPath(vec![name.clone()]);
                let file = FileEntry {
                    path,
                    length,
                    pad: false,
                };
                (vec![file], false)
            }
            (None, Some(files)) => (parse_files(files)?, true),
            (Some(_), Some(_)) => return Err(TorrentError::InvalidField("files")),
            (None, None) => return Err(TorrentError::MissingField("length")),
        };
        if u32::try_from(files.len()).is_err() {
            return Err(TorrentError::TooManyFiles);
        }

        let total_length = files
            .iter()
            .try_fold(0u64, |sum, f| sum.checked_add(f.length))
            .ok_or(TorrentError::InvalidField("length"))?;

        let expected = total_length.div_ceil(u64::from(piece_length));
        if u32::try_from(expected).is_err() {
            return Err(TorrentError::TooManyPieces);
        }
        let actual = (pieces.len() / Sha1Hash::LEN) as u64;
        if actual != expected {
            return Err(TorrentError::PieceCountMismatch { expected, actual });
        }
        let pieces = pieces
            .as_chunks::<{ Sha1Hash::LEN }>()
            .0
            .iter()
            .map(|&c| Sha1Hash::from_bytes(c))
            .collect();

        let private = info.get(b"private").and_then(Value::as_int) == Some(1);

        let raw = info.raw();
        let v1 = Sha1Hash::from_bytes(Sha1::digest(raw).into());
        let info_hash = if hybrid {
            InfoHash::hybrid(v1, Sha256Hash::from_bytes(Sha256::digest(raw).into()))
        } else {
            InfoHash::from_v1(v1)
        };

        Ok(Self {
            info_hash,
            name,
            piece_length,
            pieces,
            files,
            total_length,
            multi_file,
            private,
            info_bytes: raw.into(),
        })
    }

    /// The info-hash: v1 only, or v1 and v2 for a hybrid torrent.
    pub fn info_hash(&self) -> InfoHash {
        self.info_hash
    }

    /// The torrent name (`name.utf-8` if present, else `name`) as raw bytes.
    pub fn name(&self) -> &[u8] {
        &self.name
    }

    /// Nominal piece size in bytes. Only the last piece may be shorter.
    pub fn piece_length(&self) -> u32 {
        self.piece_length
    }

    /// Number of pieces.
    pub fn piece_count(&self) -> u32 {
        self.pieces.len() as u32
    }

    /// The SHA-1 hash of a piece, or `None` if `index` is out of range.
    pub fn piece_hash(&self, index: PieceIndex) -> Option<Sha1Hash> {
        self.pieces.get(index.as_usize()).copied()
    }

    /// The size of a piece in bytes, or `None` if `index` is out of range.
    pub fn piece_size(&self, index: PieceIndex) -> Option<u32> {
        let last = self.piece_count().checked_sub(1)?;
        match index.get() {
            i if i < last => Some(self.piece_length),
            i if i == last => {
                let before = u64::from(last) * u64::from(self.piece_length);
                Some((self.total_length - before) as u32)
            }
            _ => None,
        }
    }

    /// All files in torrent order, padding files included.
    pub fn files(&self) -> &[FileEntry] {
        &self.files
    }

    /// One file, or `None` if `index` is out of range.
    pub fn file(&self, index: FileIndex) -> Option<&FileEntry> {
        self.files.get(index.as_usize())
    }

    /// Sum of all file lengths, padding files included.
    pub fn total_length(&self) -> u64 {
        self.total_length
    }

    /// Whether the torrent uses the multi-file (`files`) layout.
    pub fn is_multi_file(&self) -> bool {
        self.multi_file
    }

    /// Whether `private` is 1.
    pub fn is_private(&self) -> bool {
        self.private
    }

    /// The `info` dictionary bytes exactly as they appeared in the file.
    pub fn info_bytes(&self) -> &[u8] {
        &self.info_bytes
    }
}

/// `dict[utf8_key]` if present, else `dict[key]`.
pub(crate) fn utf8_preferred<'v, 'a>(
    dict: &'v Value<'a>,
    utf8_key: &[u8],
    key: &[u8],
) -> Option<&'v Value<'a>> {
    dict.get(utf8_key).or_else(|| dict.get(key))
}

fn parse_length(v: &Value<'_>) -> Result<u64, TorrentError> {
    v.as_int()
        .and_then(|n| u64::try_from(n).ok())
        .ok_or(TorrentError::InvalidField("length"))
}

fn parse_files(files: &Value<'_>) -> Result<Vec<FileEntry>, TorrentError> {
    let list = files
        .as_list()
        .filter(|l| !l.is_empty())
        .ok_or(TorrentError::InvalidField("files"))?;
    list.iter()
        .map(|entry| {
            if entry.as_dict().is_none() {
                return Err(TorrentError::InvalidField("files"));
            }
            let length = parse_length(
                entry
                    .get(b"length")
                    .ok_or(TorrentError::MissingField("length"))?,
            )?;
            let components = utf8_preferred(entry, b"path.utf-8", b"path")
                .ok_or(TorrentError::MissingField("path"))?
                .as_list()
                .filter(|l| !l.is_empty())
                .ok_or(TorrentError::InvalidField("path"))?
                .iter()
                .map(|c| c.as_bytes().map(<[u8]>::to_vec))
                .collect::<Option<Vec<_>>>()
                .ok_or(TorrentError::InvalidField("path"))?;
            let pad = entry
                .get(b"attr")
                .and_then(Value::as_bytes)
                .is_some_and(|attr| attr.contains(&b'p'));
            Ok(FileEntry {
                path: RawPath(components),
                length,
                pad,
            })
        })
        .collect()
}
