//! `TorrentDescriptor`: what a torrent is, from a `.torrent` file or a
//! magnet link. Mirrors qBittorrent's `BitTorrent::TorrentDescriptor`.

use std::fs;
use std::path::Path;

use bt_core::InfoHash;

use crate::bencode::{DecodeError, DecodeErrorKind, Limits, Value, decode_with_limits};
use crate::magnet::{self, MagnetError};
use crate::torrent::{TorrentError, TorrentInfo, utf8_preferred};

/// A torrent to add: its identity, where to find peers and, when known,
/// its metadata.
///
/// Built from a `.torrent` file ([`load`](Self::load)), the metadata is
/// always present. Built from a magnet link ([`parse`](Self::parse)) it is
/// absent until fetched from peers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TorrentDescriptor {
    pub(crate) info_hash: InfoHash,
    pub(crate) name: Option<Vec<u8>>,
    pub(crate) trackers: Vec<Vec<String>>,
    pub(crate) url_seeds: Vec<String>,
    pub(crate) creation_date: Option<i64>,
    pub(crate) comment: Option<String>,
    pub(crate) created_by: Option<String>,
    pub(crate) info: Option<TorrentInfo>,
}

impl TorrentDescriptor {
    /// Loads a `.torrent` file's contents with default [`Limits`].
    pub fn load(data: &[u8]) -> Result<Self, TorrentError> {
        Self::load_with_limits(data, Limits::default())
    }

    /// Loads a `.torrent` file's contents.
    pub fn load_with_limits(data: &[u8], limits: Limits) -> Result<Self, TorrentError> {
        let root = decode_with_limits(data, limits)?;
        if root.as_dict().is_none() {
            return Err(TorrentError::NotADictionary);
        }
        let info = TorrentInfo::from_info(
            root.get(b"info")
                .ok_or(TorrentError::MissingField("info"))?,
        )?;

        Ok(Self {
            info_hash: info.info_hash(),
            name: Some(info.name().to_vec()),
            trackers: parse_trackers(&root),
            url_seeds: parse_url_seeds(&root),
            creation_date: root.get(b"creation date").and_then(Value::as_int),
            comment: utf8_preferred(&root, b"comment.utf-8", b"comment").and_then(lossy_string),
            created_by: root.get(b"created by").and_then(lossy_string),
            info: Some(info),
        })
    }

    /// Reads and loads a `.torrent` file with default [`Limits`].
    pub fn load_from_file(path: &Path) -> Result<Self, TorrentError> {
        Self::load_from_file_with_limits(path, Limits::default())
    }

    /// Reads and loads a `.torrent` file. A file larger than
    /// [`Limits::max_size`] is rejected before its contents are read.
    pub fn load_from_file_with_limits(path: &Path, limits: Limits) -> Result<Self, TorrentError> {
        let size = fs::metadata(path)?.len();
        if size > limits.max_size as u64 {
            return Err(DecodeError::new(0, DecodeErrorKind::SizeLimit).into());
        }
        Self::load_with_limits(&fs::read(path)?, limits)
    }

    /// Parses a magnet link, or a bare info-hash given as 40 (v1) or 64 (v2)
    /// hex characters. The result has no metadata.
    ///
    /// Tracker URLs are kept as written, without filtering by scheme.
    pub fn parse(s: &str) -> Result<Self, MagnetError> {
        magnet::parse(s)
    }

    /// The info-hash.
    pub fn info_hash(&self) -> InfoHash {
        self.info_hash
    }

    /// The torrent name (from `.torrent`) or display-name hint (`dn` of a
    /// magnet link), as raw bytes.
    pub fn name(&self) -> Option<&[u8]> {
        self.name.as_deref()
    }

    /// Tracker URLs grouped by tier, in order. Not filtered by scheme.
    pub fn trackers(&self) -> &[Vec<String>] {
        &self.trackers
    }

    /// Web seed URLs.
    pub fn url_seeds(&self) -> &[String] {
        &self.url_seeds
    }

    /// `creation date` as Unix seconds, if present and an integer.
    pub fn creation_date(&self) -> Option<i64> {
        self.creation_date
    }

    /// `comment` (`comment.utf-8` preferred), if present.
    pub fn comment(&self) -> Option<&str> {
        self.comment.as_deref()
    }

    /// `created by`, if present.
    pub fn created_by(&self) -> Option<&str> {
        self.created_by.as_deref()
    }

    /// The metadata, if known.
    pub fn info(&self) -> Option<&TorrentInfo> {
        self.info.as_ref()
    }
}

fn lossy_string(v: &Value<'_>) -> Option<String> {
    v.as_bytes()
        .map(|b| String::from_utf8_lossy(b).into_owned())
}

fn url_list(v: &Value<'_>) -> Vec<String> {
    v.as_list()
        .unwrap_or_default()
        .iter()
        .filter_map(lossy_string)
        .filter(|s| !s.is_empty())
        .collect()
}

/// `announce-list` tiers (empty tiers dropped), or `announce` as one tier.
fn parse_trackers(root: &Value<'_>) -> Vec<Vec<String>> {
    let tiers: Vec<_> = root
        .get(b"announce-list")
        .and_then(Value::as_list)
        .unwrap_or_default()
        .iter()
        .map(url_list)
        .filter(|tier| !tier.is_empty())
        .collect();
    if !tiers.is_empty() {
        return tiers;
    }
    root.get(b"announce")
        .and_then(lossy_string)
        .filter(|s| !s.is_empty())
        .map(|url| vec![vec![url]])
        .unwrap_or_default()
}

/// `url-list` as either one string or a list of strings.
fn parse_url_seeds(root: &Value<'_>) -> Vec<String> {
    match root.get(b"url-list") {
        Some(v) if v.as_bytes().is_some() => lossy_string(v)
            .filter(|s| !s.is_empty())
            .into_iter()
            .collect(),
        Some(v) => url_list(v),
        None => Vec::new(),
    }
}
