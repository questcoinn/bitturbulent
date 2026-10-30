//! Magnet link scenarios from the magnet-link spec.

use bt_core::{Sha1Hash, Sha256Hash};
use bt_metainfo::{MagnetError, TorrentDescriptor};

const V1_HEX: &str = "000102030405060708090a0b0c0d0e0f10111213";
/// `V1_HEX` in RFC 4648 base32 (computed with Python's `base64.b32encode`).
const V1_BASE32: &str = "AAAQEAYEAUDAOCAJBIFQYDIOB4IBCEQT";
const V2_HEX: &str = "202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f";

fn v1() -> Sha1Hash {
    V1_HEX.parse().unwrap()
}

fn v2() -> Sha256Hash {
    V2_HEX.parse().unwrap()
}

fn parse(s: &str) -> TorrentDescriptor {
    TorrentDescriptor::parse(s).unwrap_or_else(|e| panic!("{s}: {e}"))
}

fn parse_err(s: &str) -> MagnetError {
    TorrentDescriptor::parse(s).unwrap_err()
}

#[test]
fn btih_hex_is_case_insensitive() {
    let d = parse(&format!("magnet:?xt=urn:btih:{}", V1_HEX.to_uppercase()));
    assert_eq!(d.info_hash().v1(), Some(v1()));
    assert_eq!(d.info_hash().v2(), None);
}

#[test]
fn btih_base32_equals_hex() {
    let hex = parse(&format!("magnet:?xt=urn:btih:{V1_HEX}"));
    let b32 = parse(&format!("magnet:?xt=urn:btih:{V1_BASE32}"));
    assert_eq!(b32.info_hash(), hex.info_hash());
    let lower = parse(&format!("magnet:?xt=urn:btih:{}", V1_BASE32.to_lowercase()));
    assert_eq!(lower.info_hash(), hex.info_hash());
}

#[test]
fn btmh_alone_is_v2_only() {
    let d = parse(&format!("magnet:?xt=urn:btmh:1220{V2_HEX}"));
    assert_eq!(d.info_hash().v1(), None);
    assert_eq!(d.info_hash().v2(), Some(v2()));
}

#[test]
fn non_sha256_multihash_is_rejected() {
    let e = parse_err(&format!("magnet:?xt=urn:btmh:1114{}", &V2_HEX[..40]));
    assert_eq!(e, MagnetError::UnsupportedMultihash);
}

#[test]
fn btih_and_btmh_make_a_hybrid() {
    let d = parse(&format!(
        "magnet:?xt=urn:btih:{V1_HEX}&xt=urn:btmh:1220{V2_HEX}"
    ));
    let ih = d.info_hash();
    assert_eq!((ih.v1(), ih.v2()), (Some(v1()), Some(v2())));
    assert_eq!(ih.torrent_id().as_bytes()[..], v2().as_bytes()[..20]);
}

#[test]
fn not_a_magnet_is_rejected() {
    assert_eq!(parse_err("hello"), MagnetError::NotMagnet);
    assert_eq!(
        parse_err("http://example.com/a.torrent"),
        MagnetError::NotMagnet
    );
    assert_eq!(parse_err(""), MagnetError::NotMagnet);
}

#[test]
fn missing_xt_is_rejected() {
    assert_eq!(parse_err("magnet:?dn=hello"), MagnetError::MissingInfoHash);
    // An unknown xt scheme is ignored, so this has no info-hash either.
    assert_eq!(
        parse_err("magnet:?xt=urn:sha1:ABCDEF"),
        MagnetError::MissingInfoHash
    );
}

#[test]
fn malformed_hashes_are_rejected() {
    let bad = MagnetError::InvalidHash { scheme: "btih" };
    assert_eq!(
        parse_err(&format!("magnet:?xt=urn:btih:{}", &V1_HEX[..39])),
        bad
    );
    assert_eq!(
        parse_err(&format!("magnet:?xt=urn:btih:{}g", &V1_HEX[..39])),
        bad
    );
    assert_eq!(
        parse_err(&format!("magnet:?xt=urn:btih:{}1", &V1_BASE32[..31])),
        bad
    );
    assert_eq!(
        parse_err(&format!("magnet:?xt=urn:btmh:1220{}", &V2_HEX[..63])),
        MagnetError::InvalidHash { scheme: "btmh" }
    );
}

#[test]
fn display_name_is_percent_decoded() {
    let d = parse(&format!("magnet:?xt=urn:btih:{V1_HEX}&dn=hello%20world"));
    assert_eq!(d.name(), Some(&b"hello world"[..]));
}

#[test]
fn each_tracker_is_its_own_tier_in_order() {
    let d = parse(&format!(
        "magnet:?xt=urn:btih:{V1_HEX}&tr=http%3A%2F%2Fa&tr=udp%3A%2F%2Fb"
    ));
    assert_eq!(d.trackers(), [vec!["http://a"], vec!["udp://b"]]);
}

#[test]
fn unsupported_trackers_are_kept() {
    let d = parse(&format!("magnet:?xt=urn:btih:{V1_HEX}&tr=wss%3A%2F%2Fc"));
    assert_eq!(d.trackers(), [vec!["wss://c"]]);
}

#[test]
fn web_seeds_are_read() {
    let d = parse(&format!(
        "magnet:?xt=urn:btih:{V1_HEX}&ws=http%3A%2F%2Fseed%2Fa"
    ));
    assert_eq!(d.url_seeds(), ["http://seed/a"]);
}

#[test]
fn unknown_parameters_are_ignored() {
    let plain = parse(&format!("magnet:?xt=urn:btih:{V1_HEX}"));
    let extra = parse(&format!(
        "magnet:?xt=urn:btih:{V1_HEX}&x.pe=1.2.3.4:6881&so=0,2&kt=a+b&novalue"
    ));
    assert_eq!(extra, plain);
}

#[test]
fn parameter_names_ignore_case_and_number_suffix() {
    let d = parse(&format!(
        "magnet:?XT.1=urn:btih:{V1_HEX}&Tr.1=http%3A%2F%2Fa&tr.2=http%3A%2F%2Fb&DN=x"
    ));
    assert_eq!(d.info_hash().v1(), Some(v1()));
    assert_eq!(d.trackers(), [vec!["http://a"], vec!["http://b"]]);
    assert_eq!(d.name(), Some(&b"x"[..]));
}

#[test]
fn bare_40_hex_is_btih() {
    let bare = parse(V1_HEX);
    let link = parse(&format!("magnet:?xt=urn:btih:{V1_HEX}"));
    assert_eq!(bare, link);
}

#[test]
fn bare_64_hex_is_btmh() {
    let d = parse(V2_HEX);
    assert_eq!(d.info_hash().v1(), None);
    assert_eq!(d.info_hash().v2(), Some(v2()));
}

#[test]
fn hex_with_whitespace_is_not_a_hash() {
    assert_eq!(parse_err(&format!(" {V1_HEX}")), MagnetError::NotMagnet);
}

#[test]
fn magnet_descriptor_has_no_metadata() {
    let d = parse(&format!(
        "magnet:?xt=urn:btih:{V1_HEX}&dn=n&tr=http%3A%2F%2Fa"
    ));
    assert!(d.info().is_none());
    assert_eq!(d.info_hash().v1(), Some(v1()));
    assert_eq!(d.trackers(), [vec!["http://a"]]);
    assert_eq!(d.creation_date(), None);
    assert_eq!(d.comment(), None);
}
