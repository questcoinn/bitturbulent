//! Torrent metadata: bencode, `.torrent` files and magnet links.
//!
//! This crate turns raw inputs into a [`TorrentDescriptor`], the value the
//! engine and session layers use to identify and describe a torrent. It has
//! no async runtime, so any layer can use it.

pub mod bencode;
mod descriptor;
mod magnet;
mod torrent;

pub use descriptor::TorrentDescriptor;
pub use magnet::MagnetError;
pub use torrent::{FileEntry, RawPath, TorrentError, TorrentInfo};
