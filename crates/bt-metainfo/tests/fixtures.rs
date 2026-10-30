//! Real `.torrent` files checked against values recorded outside this crate.
//! See `tests/fixtures/README.md` for where each file and value came from.

use std::path::PathBuf;

use bt_core::{PieceIndex, Sha1Hash, Sha256Hash};
use bt_metainfo::bencode::{decode, encode};
use bt_metainfo::{TorrentDescriptor, TorrentError};

struct Expected {
    file: &'static str,
    /// From qBittorrent's "info hash v1", or `info_hashes.py` for hybrids.
    v1: &'static str,
    /// From `info_hashes.py`.
    v2: Option<&'static str>,
}

const EXPECTED: &[Expected] = &[
    Expected {
        file: "v1-single.torrent",
        v1: "ca7e68a5de4666e6fcbc073f41d2cc76f187940e",
        v2: None,
    },
    Expected {
        file: "v1-multi.torrent",
        v1: "d336bb29938c21e9ffb333c8846f0280be61cb89",
        v2: None,
    },
    Expected {
        file: "v1-multi-padded.torrent",
        v1: "b202e43238eda79ac31b68b6a5e5ba6244e08c4b",
        v2: None,
    },
    Expected {
        file: "debian-13.7.0-amd64-netinst.iso.torrent",
        v1: "7acf8fb590b2060dd9c3146ef770169d593433b0",
        v2: None,
    },
    Expected {
        file: "ubuntu-26.04.1-live-server-amd64.iso.torrent",
        v1: "e4f19cf636409b72571766c2ae89c98fc21a109d",
        v2: None,
    },
    Expected {
        file: "v2_hybrid.torrent",
        v1: "514c76c1f27ec61ca8b37851bcd1cbf0b26cf120",
        v2: Some("518fbaf39b37020c896e8768a967da6d76bbd5ef7a02c761021b65a72c6cfa11"),
    },
];

fn path(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(file)
}

fn load(file: &str) -> TorrentDescriptor {
    TorrentDescriptor::load_from_file(&path(file)).unwrap_or_else(|e| panic!("{file}: {e}"))
}

#[test]
fn info_hashes_match_recorded_values() {
    for e in EXPECTED {
        let ih = load(e.file).info_hash();
        let v1: Sha1Hash = e.v1.parse().unwrap();
        let v2: Option<Sha256Hash> = e.v2.map(|h| h.parse().unwrap());
        assert_eq!(ih.v1(), Some(v1), "{} v1", e.file);
        assert_eq!(ih.v2(), v2, "{} v2", e.file);
    }
}

#[test]
fn hybrid_torrent_id_is_v2_prefix() {
    let ih = load("v2_hybrid.torrent").info_hash();
    let v2 = ih.v2().unwrap();
    assert_eq!(ih.torrent_id().as_bytes()[..], v2.as_bytes()[..20]);
}

#[test]
fn hybrid_magnet_matches_torrent() {
    let ih = load("v2_hybrid.torrent").info_hash();
    let (v1, v2) = (ih.v1().unwrap(), ih.v2().unwrap());
    let magnet = format!("magnet:?xt=urn:btih:{v1}&xt=urn:btmh:1220{v2}");
    let d = TorrentDescriptor::parse(&magnet).unwrap();
    assert_eq!(d.info_hash(), ih);
    assert_eq!(d.info_hash().torrent_id(), ih.torrent_id());
}

#[test]
fn v2_only_is_rejected() {
    let result = TorrentDescriptor::load_from_file(&path("v2_only.torrent"));
    assert!(matches!(result, Err(TorrentError::V2OnlyUnsupported)));
}

#[test]
fn single_file_layout() {
    let d = load("v1-single.torrent");
    let ti = d.info().unwrap();
    assert!(!ti.is_multi_file());
    assert_eq!(ti.name(), b"single.txt");
    assert_eq!(ti.total_length(), 40000);
    assert_eq!(ti.piece_length(), 16384);
    let sizes: Vec<_> = (0..ti.piece_count())
        .map(|i| ti.piece_size(PieceIndex::from(i)).unwrap())
        .collect();
    assert_eq!(sizes, [16384, 16384, 7232]);
    assert_eq!(d.created_by(), Some("qBittorrent v5.2.4"));
    assert!(d.trackers().is_empty());
}

fn layout(file: &str) -> Vec<(String, u64, bool)> {
    load(file)
        .info()
        .unwrap()
        .files()
        .iter()
        .map(|f| (f.path().display(), f.length(), f.is_pad()))
        .collect()
}

#[test]
fn multi_file_layout_keeps_order_and_utf8_names() {
    let d = load("v1-multi.torrent");
    assert_eq!(d.info().unwrap().name(), b"multi");
    assert_eq!(
        layout("v1-multi.torrent"),
        [
            ("a.txt".into(), 10000, false),
            ("한글이름.txt".into(), 25000, false),
            ("sub/b.txt".into(), 5000, false),
        ]
    );
    assert_eq!(d.info().unwrap().piece_count(), 3);
}

#[test]
fn padded_layout_marks_pad_files() {
    // libtorrent 1.2 sorts files by size and pads each one to a piece
    // boundary, except the last.
    assert_eq!(
        layout("v1-multi-padded.torrent"),
        [
            ("한글이름.txt".into(), 25000, false),
            (".pad/0".into(), 7768, true),
            ("a.txt".into(), 10000, false),
            (".pad/1".into(), 6384, true),
            ("sub/b.txt".into(), 5000, false),
        ]
    );
    let ti = load("v1-multi-padded.torrent").info().unwrap().clone();
    assert_eq!(ti.total_length(), 54152);
    assert_eq!(ti.piece_count(), 4);
}

#[test]
fn distribution_torrents_have_trackers() {
    for file in [
        "debian-13.7.0-amd64-netinst.iso.torrent",
        "ubuntu-26.04.1-live-server-amd64.iso.torrent",
    ] {
        let d = load(file);
        assert!(!d.trackers().is_empty(), "{file}");
        assert!(d.info().unwrap().piece_count() > 0, "{file}");
    }
}

#[test]
fn qbittorrent_fixtures_round_trip() {
    for file in [
        "v1-single.torrent",
        "v1-multi.torrent",
        "v1-multi-padded.torrent",
    ] {
        let data = std::fs::read(path(file)).unwrap();
        let reencoded = encode(&decode(&data).unwrap().to_owned_value());
        assert_eq!(reencoded, data, "{file}");
    }
}
