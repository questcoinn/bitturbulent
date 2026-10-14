//! Fixed-size digest types: SHA-1 and SHA-256 info-hashes and the torrent id.

use std::fmt;
use std::str::FromStr;

use crate::hex::{self, ParseHashError};

macro_rules! digest_type {
    ($(#[$meta:meta])* $name:ident, $len:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name([u8; $len]);

        impl $name {
            /// Length of the digest in bytes.
            pub const LEN: usize = $len;

            /// Wraps raw digest bytes.
            pub const fn from_bytes(bytes: [u8; $len]) -> Self {
                Self(bytes)
            }

            /// Returns the raw digest bytes.
            pub const fn as_bytes(&self) -> &[u8; $len] {
                &self.0
            }
        }

        impl From<[u8; $len]> for $name {
            fn from(bytes: [u8; $len]) -> Self {
                Self(bytes)
            }
        }

        impl From<$name> for [u8; $len] {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl AsRef<[u8]> for $name {
            fn as_ref(&self) -> &[u8] {
                &self.0
            }
        }

        /// Lowercase hex, as shown by qBittorrent.
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                hex::encode(&self.0, f)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "({})"), self)
            }
        }

        /// Parses hex in either case.
        impl FromStr for $name {
            type Err = ParseHashError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                hex::decode::<$len>(s).map(Self)
            }
        }
    };
}

digest_type!(
    /// BitTorrent v1 info-hash: SHA-1 of the bencoded `info` dictionary.
    Sha1Hash,
    20
);

digest_type!(
    /// BitTorrent v2 info-hash: SHA-256 of the bencoded `info` dictionary.
    Sha256Hash,
    32
);

digest_type!(
    /// Stable 20-byte identifier of a torrent inside bitturbulent.
    ///
    /// Derived from an [`InfoHash`](crate::InfoHash) by
    /// [`InfoHash::torrent_id`](crate::InfoHash::torrent_id): the first 20
    /// bytes of the v2 hash when present, otherwise the v1 hash. This matches
    /// qBittorrent's `TorrentID` (libtorrent `info_hash_t::get_best()`).
    ///
    /// It is a distinct type from [`Sha1Hash`] even though both are 20 bytes:
    /// a truncated v2 hash must not be compared against a v1 hash by accident.
    TorrentId,
    20
);

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn seq<const N: usize>() -> [u8; N] {
        std::array::from_fn(|i| i as u8)
    }

    #[test]
    fn sha1_displays_lowercase_hex() {
        let h = Sha1Hash::from_bytes(seq());
        assert_eq!(h.to_string(), "000102030405060708090a0b0c0d0e0f10111213");
    }

    #[test]
    fn sha256_displays_64_lowercase_hex_chars() {
        let s = Sha256Hash::from_bytes([0xab; 32]).to_string();
        assert_eq!(s.len(), 64);
        assert!(s.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')));
    }

    #[test]
    fn round_trips_through_string() {
        let h1 = Sha1Hash::from_bytes(seq());
        assert_eq!(h1.to_string().parse::<Sha1Hash>(), Ok(h1));
        let h2 = Sha256Hash::from_bytes(seq());
        assert_eq!(h2.to_string().parse::<Sha256Hash>(), Ok(h2));
        let id = TorrentId::from_bytes(seq());
        assert_eq!(id.to_string().parse::<TorrentId>(), Ok(id));
    }

    #[test]
    fn parses_uppercase_like_lowercase() {
        let lower = "000102030405060708090a0b0c0d0e0f10111213";
        let upper = lower.to_uppercase();
        assert_eq!(upper.parse::<Sha1Hash>(), lower.parse::<Sha1Hash>());
    }

    #[test]
    fn parse_reports_length_and_char_errors() {
        assert_eq!(
            "00".parse::<Sha256Hash>(),
            Err(ParseHashError::InvalidLength {
                expected: 64,
                actual: 2
            })
        );
        let bad = format!("g{}", "0".repeat(39));
        assert_eq!(
            bad.parse::<TorrentId>(),
            Err(ParseHashError::InvalidChar { index: 0 })
        );
    }

    #[test]
    fn debug_shows_type_and_hex() {
        let h = Sha1Hash::from_bytes([0; 20]);
        assert_eq!(format!("{h:?}"), format!("Sha1Hash({})", "0".repeat(40)));
    }

    #[test]
    fn usable_as_map_key() {
        let mut map = HashMap::new();
        map.insert(TorrentId::from_bytes(seq()), "a");
        assert_eq!(map.get(&TorrentId::from(seq::<20>())), Some(&"a"));
    }

    #[test]
    fn orders_by_bytes() {
        let a = Sha1Hash::from_bytes([0; 20]);
        let mut b = [0; 20];
        b[0] = 1;
        assert!(a < Sha1Hash::from_bytes(b));
    }
}
