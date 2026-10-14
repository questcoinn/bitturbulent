//! Info-hash of a torrent covering v1, v2 and hybrid torrents.

use crate::hash::{Sha1Hash, Sha256Hash, TorrentId};

/// Info-hash of a torrent: a v1 (SHA-1) hash, a v2 (SHA-256) hash, or both.
///
/// Mirrors qBittorrent's `BitTorrent::InfoHash`. At least one of the two
/// hashes is always present; the fields are private so the only way to build
/// a value is through [`from_v1`](Self::from_v1), [`from_v2`](Self::from_v2)
/// or [`hybrid`](Self::hybrid).
///
/// ```compile_fail,E0451
/// use bt_core::InfoHash;
/// let _ = InfoHash { v1: None, v2: None };
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct InfoHash {
    v1: Option<Sha1Hash>,
    v2: Option<Sha256Hash>,
}

impl InfoHash {
    /// Info-hash of a v1-only torrent.
    pub const fn from_v1(v1: Sha1Hash) -> Self {
        Self {
            v1: Some(v1),
            v2: None,
        }
    }

    /// Info-hash of a v2-only torrent.
    pub const fn from_v2(v2: Sha256Hash) -> Self {
        Self {
            v1: None,
            v2: Some(v2),
        }
    }

    /// Info-hash of a hybrid torrent carrying both hashes.
    pub const fn hybrid(v1: Sha1Hash, v2: Sha256Hash) -> Self {
        Self {
            v1: Some(v1),
            v2: Some(v2),
        }
    }

    /// The v1 (SHA-1) hash, if the torrent has one.
    pub const fn v1(&self) -> Option<Sha1Hash> {
        self.v1
    }

    /// The v2 (SHA-256) hash, if the torrent has one.
    pub const fn v2(&self) -> Option<Sha256Hash> {
        self.v2
    }

    /// The torrent id: the first 20 bytes of the v2 hash when present,
    /// otherwise the v1 hash.
    pub fn torrent_id(&self) -> TorrentId {
        match (self.v1, self.v2) {
            (_, Some(v2)) => {
                let mut id = [0u8; TorrentId::LEN];
                id.copy_from_slice(&v2.as_bytes()[..TorrentId::LEN]);
                TorrentId::from_bytes(id)
            }
            (Some(v1), None) => TorrentId::from_bytes(*v1.as_bytes()),
            (None, None) => unreachable!("InfoHash always holds at least one hash"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h1() -> Sha1Hash {
        Sha1Hash::from_bytes([0x11; 20])
    }

    fn h2() -> Sha256Hash {
        Sha256Hash::from_bytes(std::array::from_fn(|i| i as u8))
    }

    #[test]
    fn v1_only() {
        let ih = InfoHash::from_v1(h1());
        assert_eq!(ih.v1(), Some(h1()));
        assert_eq!(ih.v2(), None);
    }

    #[test]
    fn v2_only() {
        let ih = InfoHash::from_v2(h2());
        assert_eq!(ih.v1(), None);
        assert_eq!(ih.v2(), Some(h2()));
    }

    #[test]
    fn hybrid_keeps_both() {
        let ih = InfoHash::hybrid(h1(), h2());
        assert_eq!(ih.v1(), Some(h1()));
        assert_eq!(ih.v2(), Some(h2()));
    }

    #[test]
    fn torrent_id_from_v1_only() {
        let id = InfoHash::from_v1(h1()).torrent_id();
        assert_eq!(id.as_bytes(), h1().as_bytes());
    }

    #[test]
    fn torrent_id_prefers_truncated_v2() {
        let id = InfoHash::hybrid(h1(), h2()).torrent_id();
        assert_eq!(&id.as_bytes()[..], &h2().as_bytes()[..20]);
        assert_eq!(id, InfoHash::from_v2(h2()).torrent_id());
    }

    #[test]
    fn torrent_id_is_deterministic() {
        let ih = InfoHash::hybrid(h1(), h2());
        assert_eq!(ih.torrent_id(), ih.torrent_id());
    }
}
