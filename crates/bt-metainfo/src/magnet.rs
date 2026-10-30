//! Magnet links and bare info-hash strings, as accepted by qBittorrent's
//! `TorrentDescriptor::parse` and libtorrent's `parse_magnet_uri`.

use std::str::FromStr;

use bt_core::{InfoHash, Sha1Hash, Sha256Hash};

use crate::descriptor::TorrentDescriptor;

/// Error returned when a string is neither a usable magnet link nor a bare
/// info-hash.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MagnetError {
    /// Not `magnet:?...` and not a 40- or 64-character hex string.
    #[error("not a magnet link or info-hash")]
    NotMagnet,
    /// No `xt` with a recognised info-hash.
    #[error("magnet link has no info-hash")]
    MissingInfoHash,
    /// A `urn:btih:` or `urn:btmh:` value with the wrong length or characters.
    #[error("invalid {scheme} info-hash")]
    InvalidHash {
        /// `"btih"` or `"btmh"`, or `"xt"` when the value has a broken
        /// percent escape.
        scheme: &'static str,
    },
    /// A `urn:btmh:` multihash other than SHA-256 (`1220`).
    #[error("unsupported multihash: only SHA-256 is supported")]
    UnsupportedMultihash,
}

/// Parses a magnet link, or a bare 40-character (v1) or 64-character (v2)
/// hex info-hash, into a descriptor without metadata.
pub(crate) fn parse(s: &str) -> Result<TorrentDescriptor, MagnetError> {
    if is_hex(s, 40) {
        return parse_uri(&format!("magnet:?xt=urn:btih:{s}"));
    }
    if is_hex(s, 64) {
        // 0x12 0x20 is the multihash tag for SHA-256.
        return parse_uri(&format!("magnet:?xt=urn:btmh:1220{s}"));
    }
    parse_uri(s)
}

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn parse_uri(uri: &str) -> Result<TorrentDescriptor, MagnetError> {
    let query = uri.strip_prefix("magnet:?").ok_or(MagnetError::NotMagnet)?;

    let mut v1 = None;
    let mut v2 = None;
    let mut name = None;
    let mut trackers = Vec::new();
    let mut url_seeds = Vec::new();

    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let key = strip_number_suffix(key).to_ascii_lowercase();
        let decoded = percent_decode(value);
        match key.as_str() {
            "xt" => {
                let xt = decoded
                    .and_then(|b| String::from_utf8(b).ok())
                    .ok_or(MagnetError::InvalidHash { scheme: "xt" })?;
                // A repeated kind overwrites the earlier one, like libtorrent.
                if let Some(h) = xt.strip_prefix("urn:btih:") {
                    v1 = Some(parse_btih(h)?);
                } else if let Some(h) = xt.strip_prefix("urn:btmh:") {
                    v2 = Some(parse_btmh(h)?);
                }
            }
            "dn" => name = decoded.filter(|b| !b.is_empty()).or(name),
            "tr" => {
                if let Some(url) = decoded.filter(|b| !b.is_empty()) {
                    // Each tracker gets its own tier, like libtorrent.
                    trackers.push(vec![lossy(url)]);
                }
            }
            "ws" => {
                if let Some(url) = decoded.filter(|b| !b.is_empty()) {
                    url_seeds.push(lossy(url));
                }
            }
            _ => {}
        }
    }

    let info_hash = match (v1, v2) {
        (Some(v1), Some(v2)) => InfoHash::hybrid(v1, v2),
        (Some(v1), None) => InfoHash::from_v1(v1),
        (None, Some(v2)) => InfoHash::from_v2(v2),
        (None, None) => return Err(MagnetError::MissingInfoHash),
    };

    Ok(TorrentDescriptor {
        info_hash,
        name,
        trackers,
        url_seeds,
        creation_date: None,
        comment: None,
        created_by: None,
        info: None,
    })
}

/// `tr.1` -> `tr`. Only an all-digit suffix is removed.
fn strip_number_suffix(key: &str) -> &str {
    match key.rsplit_once('.') {
        Some((base, n)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => base,
        _ => key,
    }
}

fn parse_btih(h: &str) -> Result<Sha1Hash, MagnetError> {
    let invalid = MagnetError::InvalidHash { scheme: "btih" };
    match h.len() {
        40 => Sha1Hash::from_str(h).map_err(|_| invalid),
        32 => base32_decode(h)
            .and_then(|b| <[u8; 20]>::try_from(b).ok())
            .map(Sha1Hash::from_bytes)
            .ok_or(invalid),
        _ => Err(invalid),
    }
}

fn parse_btmh(h: &str) -> Result<Sha256Hash, MagnetError> {
    let digest = h
        .strip_prefix("1220")
        .ok_or(MagnetError::UnsupportedMultihash)?;
    Sha256Hash::from_str(digest).map_err(|_| MagnetError::InvalidHash { scheme: "btmh" })
}

fn lossy(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Decodes `%XX` escapes. `None` if a `%` is not followed by two hex digits.
fn percent_decode(s: &str) -> Option<Vec<u8>> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes.get(i + 1..i + 3)?;
            let hi = char::from(hex[0]).to_digit(16)?;
            let lo = char::from(hex[1]).to_digit(16)?;
            out.push((hi * 16 + lo) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Some(out)
}

/// RFC 4648 base32, case-insensitive, trailing `=` padding optional.
/// `None` on a character outside the alphabet.
fn base32_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 5 / 8);
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for b in s.trim_end_matches('=').bytes() {
        let value = match b.to_ascii_uppercase() {
            c @ b'A'..=b'Z' => c - b'A',
            c @ b'2'..=b'7' => c - b'2' + 26,
            _ => return None,
        };
        buffer = (buffer << 5) | u32::from(value);
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base32_rfc4648_vectors() {
        let vectors: [(&str, &[u8]); 7] = [
            ("", b""),
            ("MY======", b"f"),
            ("MZXQ====", b"fo"),
            ("MZXW6===", b"foo"),
            ("MZXW6YQ=", b"foob"),
            ("MZXW6YTB", b"fooba"),
            ("MZXW6YTBOI======", b"foobar"),
        ];
        for (encoded, plain) in vectors {
            assert_eq!(base32_decode(encoded).as_deref(), Some(plain), "{encoded}");
        }
    }

    #[test]
    fn base32_is_case_insensitive_and_padding_optional() {
        assert_eq!(base32_decode("mzxw6ytboi").as_deref(), Some(&b"foobar"[..]));
    }

    #[test]
    fn base32_rejects_bad_characters() {
        assert_eq!(base32_decode("MZ1W"), None);
        assert_eq!(base32_decode("MZ8W"), None);
    }

    #[test]
    fn percent_decoding() {
        assert_eq!(
            percent_decode("hello%20world").as_deref(),
            Some(&b"hello world"[..])
        );
        assert_eq!(
            percent_decode("wss%3A%2F%2Fc").as_deref(),
            Some(&b"wss://c"[..])
        );
        assert_eq!(percent_decode("%2f%2F").as_deref(), Some(&b"//"[..]));
        assert_eq!(percent_decode("%FF").as_deref(), Some(&b"\xff"[..]));
        assert_eq!(percent_decode("a+b").as_deref(), Some(&b"a+b"[..]));
    }

    #[test]
    fn percent_decoding_rejects_bad_escapes() {
        assert_eq!(percent_decode("%"), None);
        assert_eq!(percent_decode("%2"), None);
        assert_eq!(percent_decode("%zz"), None);
        assert_eq!(percent_decode("abc%4"), None);
    }

    #[test]
    fn number_suffix() {
        assert_eq!(strip_number_suffix("tr.1"), "tr");
        assert_eq!(strip_number_suffix("tr.12"), "tr");
        assert_eq!(strip_number_suffix("x.pe"), "x.pe");
        assert_eq!(strip_number_suffix("tr."), "tr.");
        assert_eq!(strip_number_suffix("tr"), "tr");
    }
}
