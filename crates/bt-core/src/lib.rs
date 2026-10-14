//! Shared identifier and index types used by every bitturbulent crate.
//!
//! This crate is pure data: it has no async runtime and no dependency on
//! other workspace crates, so any layer can use it.

mod hash;
mod hex;
mod index;
mod info_hash;

pub use hash::{Sha1Hash, Sha256Hash, TorrentId};
pub use hex::ParseHashError;
pub use index::{FileIndex, IndexOutOfRange, PieceIndex};
pub use info_hash::InfoHash;
