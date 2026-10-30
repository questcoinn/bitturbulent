//! `.torrent` parsing scenarios from the torrent-metainfo spec, on inputs
//! built in code.

use std::collections::BTreeMap;

use bt_core::{FileIndex, PieceIndex, Sha1Hash};
use bt_metainfo::bencode::{DecodeErrorKind, Limits, OwnedValue, encode};
use bt_metainfo::{TorrentDescriptor, TorrentError};
use sha1::{Digest, Sha1};
use sha2::Sha256;

fn int(n: i64) -> OwnedValue {
    OwnedValue::Int(n)
}

fn s(v: &str) -> OwnedValue {
    OwnedValue::Bytes(v.as_bytes().to_vec())
}

fn list(items: impl IntoIterator<Item = OwnedValue>) -> OwnedValue {
    OwnedValue::List(items.into_iter().collect())
}

fn dict<const N: usize>(entries: [(&str, OwnedValue); N]) -> OwnedValue {
    OwnedValue::Dict(
        entries
            .into_iter()
            .map(|(k, v)| (k.as_bytes().to_vec(), v))
            .collect::<BTreeMap<_, _>>(),
    )
}

fn insert(v: &mut OwnedValue, key: &str, value: OwnedValue) {
    let OwnedValue::Dict(map) = v else {
        panic!("not a dict")
    };
    map.insert(key.as_bytes().to_vec(), value);
}

fn remove(v: &mut OwnedValue, key: &str) {
    let OwnedValue::Dict(map) = v else {
        panic!("not a dict")
    };
    map.remove(key.as_bytes());
}

fn pieces(count: usize) -> OwnedValue {
    OwnedValue::Bytes((0..count * 20).map(|i| i as u8).collect())
}

/// A valid single-file v1 `info` dictionary: 1000 bytes, one piece.
fn single_info() -> OwnedValue {
    dict([
        ("name", s("a.txt")),
        ("length", int(1000)),
        ("piece length", int(16384)),
        ("pieces", pieces(1)),
    ])
}

fn file(length: i64, path: &[&str]) -> OwnedValue {
    dict([
        ("length", int(length)),
        ("path", list(path.iter().map(|p| s(p)))),
    ])
}

/// A valid multi-file v1 `info` dictionary with the given files.
fn multi_info(files: Vec<OwnedValue>, piece_count: usize) -> OwnedValue {
    dict([
        ("name", s("dir")),
        ("files", OwnedValue::List(files)),
        ("piece length", int(16384)),
        ("pieces", pieces(piece_count)),
    ])
}

fn torrent(info: OwnedValue) -> OwnedValue {
    dict([("announce", s("http://t/announce")), ("info", info)])
}

fn load(root: &OwnedValue) -> Result<TorrentDescriptor, TorrentError> {
    TorrentDescriptor::load(&encode(root))
}

fn load_info(info: OwnedValue) -> Result<TorrentDescriptor, TorrentError> {
    load(&torrent(info))
}

fn err(info: OwnedValue) -> TorrentError {
    load_info(info).unwrap_err()
}

// --- info-hash -----------------------------------------------------------

#[test]
fn v1_info_hash_is_sha1_of_info_bytes() {
    let info = single_info();
    let d = load_info(info.clone()).unwrap();
    let expected = Sha1Hash::from_bytes(Sha1::digest(encode(&info)).into());
    assert_eq!(d.info_hash().v1(), Some(expected));
    assert_eq!(d.info_hash().v2(), None);
}

#[test]
fn non_canonical_info_is_hashed_as_written() {
    // `name` before `length`: keys out of order.
    let info: &[u8] =
        b"d4:name5:a.txt6:lengthi1000e12:piece lengthi16384e6:pieces20:aaaaaaaaaaaaaaaaaaaae";
    let mut data = b"d4:info".to_vec();
    data.extend_from_slice(info);
    data.push(b'e');

    let d = TorrentDescriptor::load(&data).unwrap();
    let as_written = Sha1Hash::from_bytes(Sha1::digest(info).into());
    let reencoded = bt_metainfo::bencode::decode(info).unwrap().to_owned_value();
    let canonical = Sha1Hash::from_bytes(Sha1::digest(encode(&reencoded)).into());

    assert_eq!(d.info_hash().v1(), Some(as_written));
    assert_ne!(as_written, canonical);
    assert_eq!(d.info().unwrap().info_bytes(), info);
}

#[test]
fn hybrid_has_both_hashes_and_v2_torrent_id() {
    let mut info = single_info();
    insert(&mut info, "meta version", int(2));
    insert(&mut info, "file tree", dict([]));
    let bytes = encode(&info);

    let d = load_info(info).unwrap();
    let ih = d.info_hash();
    let v1: [u8; 20] = Sha1::digest(&bytes).into();
    let v2: [u8; 32] = Sha256::digest(&bytes).into();
    assert_eq!(ih.v1().map(|h| *h.as_bytes()), Some(v1));
    assert_eq!(ih.v2().map(|h| *h.as_bytes()), Some(v2));
    assert_eq!(ih.torrent_id().as_bytes()[..], v2[..20]);

    // Files and pieces come from the v1 fields.
    let ti = d.info().unwrap();
    assert_eq!(ti.files().len(), 1);
    assert_eq!(ti.total_length(), 1000);
    assert_eq!(ti.piece_count(), 1);
}

#[test]
fn v2_only_is_rejected() {
    let info = dict([
        ("name", s("a.txt")),
        ("meta version", int(2)),
        ("piece length", int(16384)),
        ("file tree", dict([])),
    ]);
    assert!(matches!(err(info), TorrentError::V2OnlyUnsupported));
}

#[test]
fn unknown_meta_version_is_rejected() {
    let mut info = single_info();
    insert(&mut info, "meta version", int(3));
    assert!(matches!(err(info), TorrentError::UnsupportedMetaVersion(3)));
}

// --- files ---------------------------------------------------------------

#[test]
fn single_file_layout() {
    let d = load_info(single_info()).unwrap();
    let ti = d.info().unwrap();
    assert!(!ti.is_multi_file());
    assert_eq!(ti.name(), b"a.txt");
    assert_eq!(ti.files().len(), 1);
    let f = ti.file(FileIndex::from(0)).unwrap();
    assert_eq!(f.length(), 1000);
    assert_eq!(f.path().components(), [b"a.txt".to_vec()]);
    assert!(!f.is_pad());
}

#[test]
fn multi_file_layout_keeps_order() {
    let files = vec![
        file(1, &["b.txt"]),
        file(2, &["sub", "a.txt"]),
        file(3, &["c.txt"]),
    ];
    let d = load_info(multi_info(files, 1)).unwrap();
    let ti = d.info().unwrap();
    assert!(ti.is_multi_file());
    let paths: Vec<_> = ti.files().iter().map(|f| f.path().display()).collect();
    assert_eq!(paths, ["b.txt", "sub/a.txt", "c.txt"]);
    assert_eq!(ti.total_length(), 6);
}

#[test]
fn padding_files_are_marked() {
    let mut pad = file(16383, &[".pad", "16383"]);
    insert(&mut pad, "attr", s("p"));
    let mut exec = file(1, &["run.sh"]);
    insert(&mut exec, "attr", s("x"));
    let files = vec![file(1, &["a"]), pad, exec];
    let d = load_info(multi_info(files, 2)).unwrap();
    let pads: Vec<_> = d
        .info()
        .unwrap()
        .files()
        .iter()
        .map(|f| f.is_pad())
        .collect();
    assert_eq!(pads, [false, true, false]);
    // Padding counts toward the total size and piece layout.
    assert_eq!(d.info().unwrap().total_length(), 16385);
}

#[test]
fn dangerous_paths_are_preserved_verbatim() {
    let files = vec![file(1, &["..", "evil.txt"]), file(1, &["C:", "x"])];
    let d = load_info(multi_info(files, 1)).unwrap();
    let ti = d.info().unwrap();
    assert_eq!(
        ti.files()[0].path().components(),
        [b"..".to_vec(), b"evil.txt".to_vec()]
    );
    assert_eq!(ti.files()[1].path().display(), "C:/x");
}

#[test]
fn utf8_name_and_path_are_preferred() {
    let mut f = file(1, &["legacy"]);
    insert(&mut f, "path.utf-8", list([s("유니코드.txt")]));
    let mut info = multi_info(vec![f], 1);
    insert(&mut info, "name.utf-8", s("이름"));
    let d = load_info(info).unwrap();
    let ti = d.info().unwrap();
    assert_eq!(ti.name(), "이름".as_bytes());
    assert_eq!(d.name(), Some("이름".as_bytes()));
    assert_eq!(ti.files()[0].path().display(), "유니코드.txt");
}

#[test]
fn non_utf8_name_is_kept_as_bytes() {
    let mut info = single_info();
    insert(
        &mut info,
        "name",
        OwnedValue::Bytes(b"\xb0\xa1.txt".to_vec()),
    );
    let d = load_info(info).unwrap();
    assert_eq!(d.info().unwrap().name(), b"\xb0\xa1.txt");
}

#[test]
fn empty_path_is_rejected() {
    let info = multi_info(vec![file(1, &[])], 1);
    assert!(matches!(err(info), TorrentError::InvalidField("path")));
}

#[test]
fn length_and_files_together_or_neither_are_rejected() {
    let mut both = multi_info(vec![file(1, &["a"])], 1);
    insert(&mut both, "length", int(1));
    assert!(matches!(err(both), TorrentError::InvalidField("files")));

    let mut neither = single_info();
    remove(&mut neither, "length");
    assert!(matches!(err(neither), TorrentError::MissingField("length")));
}

#[test]
fn empty_files_list_is_rejected() {
    assert!(matches!(
        err(multi_info(vec![], 0)),
        TorrentError::InvalidField("files")
    ));
}

// --- pieces --------------------------------------------------------------

#[test]
fn last_piece_is_shorter() {
    let mut info = single_info();
    insert(&mut info, "length", int(40000));
    insert(&mut info, "pieces", pieces(3));
    let d = load_info(info).unwrap();
    let ti = d.info().unwrap();
    assert_eq!(ti.piece_count(), 3);
    let sizes: Vec<_> = (0..3)
        .map(|i| ti.piece_size(PieceIndex::from(i)).unwrap())
        .collect();
    assert_eq!(sizes, [16384, 16384, 7232]);
    assert_eq!(ti.piece_size(PieceIndex::from(3)), None);
    assert!(ti.piece_hash(PieceIndex::from(2)).is_some());
    assert!(ti.piece_hash(PieceIndex::from(3)).is_none());
}

#[test]
fn exact_multiple_has_full_last_piece() {
    let mut info = single_info();
    insert(&mut info, "length", int(32768));
    insert(&mut info, "pieces", pieces(2));
    let d = load_info(info).unwrap();
    assert_eq!(
        d.info().unwrap().piece_size(PieceIndex::from(1)),
        Some(16384)
    );
}

#[test]
fn piece_count_mismatch_is_rejected() {
    let mut info = single_info();
    insert(&mut info, "length", int(40000));
    insert(&mut info, "pieces", pieces(2));
    assert!(matches!(
        err(info),
        TorrentError::PieceCountMismatch {
            expected: 3,
            actual: 2
        }
    ));
}

#[test]
fn pieces_not_multiple_of_20_is_rejected() {
    let mut info = single_info();
    insert(&mut info, "pieces", OwnedValue::Bytes(vec![0; 19]));
    assert!(matches!(err(info), TorrentError::InvalidField("pieces")));
}

#[test]
fn non_positive_piece_length_is_rejected() {
    for bad in [0, -1, i64::from(u32::MAX) + 1] {
        let mut info = single_info();
        insert(&mut info, "piece length", int(bad));
        assert!(matches!(
            err(info),
            TorrentError::InvalidField("piece length")
        ));
    }
}

#[test]
fn piece_count_beyond_u32_is_rejected() {
    let mut info = single_info();
    insert(&mut info, "piece length", int(1));
    insert(&mut info, "length", int(1 << 33));
    assert!(matches!(err(info), TorrentError::TooManyPieces));
}

// --- required fields -------------------------------------------------------

#[test]
fn missing_info_is_reported() {
    let root = dict([("announce", s("http://t"))]);
    assert!(matches!(
        load(&root),
        Err(TorrentError::MissingField("info"))
    ));
}

#[test]
fn missing_or_mistyped_required_fields_are_named() {
    for key in ["name", "piece length", "pieces"] {
        let mut info = single_info();
        remove(&mut info, key);
        match err(info) {
            TorrentError::MissingField(k) => assert_eq!(k, key),
            other => panic!("{key}: {other:?}"),
        }
    }
    let mut info = single_info();
    insert(&mut info, "name", int(1));
    assert!(matches!(err(info), TorrentError::InvalidField("name")));
}

#[test]
fn negative_file_length_is_rejected() {
    let mut info = single_info();
    insert(&mut info, "length", int(-1));
    assert!(matches!(err(info), TorrentError::InvalidField("length")));
    let info = multi_info(vec![file(-1, &["a"])], 1);
    assert!(matches!(err(info), TorrentError::InvalidField("length")));
}

#[test]
fn non_dictionary_root_and_info_are_rejected() {
    assert!(matches!(
        TorrentDescriptor::load(b"li1ee"),
        Err(TorrentError::NotADictionary)
    ));
    assert!(matches!(
        load(&dict([("info", int(1))])),
        Err(TorrentError::InvalidField("info"))
    ));
}

#[test]
fn bencode_errors_are_passed_through() {
    match TorrentDescriptor::load(b"d4:info") {
        Err(TorrentError::Decode(e)) => assert_eq!(e.kind(), DecodeErrorKind::UnexpectedEof),
        other => panic!("{other:?}"),
    }
}

// --- trackers, web seeds, flags, extras -----------------------------------

#[test]
fn announce_list_takes_precedence_and_drops_empty_tiers() {
    let mut root = torrent(single_info());
    insert(
        &mut root,
        "announce-list",
        list([
            list([s("http://a"), s("udp://b")]),
            list([]),
            list([s("http://c")]),
        ]),
    );
    let d = load(&root).unwrap();
    assert_eq!(
        d.trackers(),
        [vec!["http://a", "udp://b"], vec!["http://c"]]
    );
}

#[test]
fn announce_alone_is_one_tier() {
    let d = load_info(single_info()).unwrap();
    assert_eq!(d.trackers(), [vec!["http://t/announce"]]);
}

#[test]
fn empty_announce_list_falls_back_to_announce() {
    let mut root = torrent(single_info());
    insert(&mut root, "announce-list", list([list([])]));
    let d = load(&root).unwrap();
    assert_eq!(d.trackers(), [vec!["http://t/announce"]]);
}

#[test]
fn unsupported_tracker_schemes_are_kept() {
    let mut root = torrent(single_info());
    insert(
        &mut root,
        "announce-list",
        list([list([s("wss://x"), s("junk")])]),
    );
    let d = load(&root).unwrap();
    assert_eq!(d.trackers(), [vec!["wss://x", "junk"]]);
}

#[test]
fn no_trackers_is_fine() {
    let d = load(&dict([("info", single_info())])).unwrap();
    assert!(d.trackers().is_empty());
}

#[test]
fn url_list_as_string_or_list() {
    let mut root = torrent(single_info());
    insert(&mut root, "url-list", s("http://seed/a"));
    assert_eq!(load(&root).unwrap().url_seeds(), ["http://seed/a"]);

    insert(
        &mut root,
        "url-list",
        list([s("http://s1"), s("http://s2")]),
    );
    assert_eq!(load(&root).unwrap().url_seeds(), ["http://s1", "http://s2"]);
}

#[test]
fn private_flag() {
    let mut info = single_info();
    assert!(
        !load_info(info.clone())
            .unwrap()
            .info()
            .unwrap()
            .is_private()
    );
    insert(&mut info, "private", int(1));
    assert!(
        load_info(info.clone())
            .unwrap()
            .info()
            .unwrap()
            .is_private()
    );
    insert(&mut info, "private", int(0));
    assert!(!load_info(info).unwrap().info().unwrap().is_private());
}

#[test]
fn optional_extras_are_read_or_ignored() {
    let mut root = torrent(single_info());
    insert(&mut root, "creation date", int(1_700_000_000));
    insert(&mut root, "comment", s("old"));
    insert(&mut root, "comment.utf-8", s("새 주석"));
    insert(&mut root, "created by", s("qBittorrent v5.2.4"));
    let d = load(&root).unwrap();
    assert_eq!(d.creation_date(), Some(1_700_000_000));
    assert_eq!(d.comment(), Some("새 주석"));
    assert_eq!(d.created_by(), Some("qBittorrent v5.2.4"));

    insert(&mut root, "creation date", s("yesterday"));
    insert(&mut root, "created by", int(5));
    let d = load(&root).unwrap();
    assert_eq!(d.creation_date(), None);
    assert_eq!(d.created_by(), None);
}

// --- descriptor and file loading ------------------------------------------

#[test]
fn descriptor_from_torrent_has_metadata() {
    let d = load_info(single_info()).unwrap();
    let info = d.info().expect("metadata present");
    assert_eq!(info.info_hash(), d.info_hash());
    assert_eq!(d.name(), Some(&b"a.txt"[..]));
}

fn temp_file(name: &str, data: &[u8]) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("bt-metainfo-test-{}-{name}", std::process::id()));
    std::fs::write(&path, data).unwrap();
    path
}

#[test]
fn load_from_file_reads_torrent() {
    let data = encode(&torrent(single_info()));
    let path = temp_file("ok.torrent", &data);
    let d = TorrentDescriptor::load_from_file(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(d, TorrentDescriptor::load(&data).unwrap());
}

#[test]
fn load_from_file_checks_size_first() {
    let path = temp_file("big.torrent", &[b'x'; 2048]);
    let limits = Limits {
        max_size: 1024,
        ..Limits::default()
    };
    let result = TorrentDescriptor::load_from_file_with_limits(&path, limits);
    std::fs::remove_file(&path).unwrap();
    match result {
        Err(TorrentError::Decode(e)) => assert_eq!(e.kind(), DecodeErrorKind::SizeLimit),
        other => panic!("{other:?}"),
    }
}

#[test]
fn load_from_missing_file_is_io_error() {
    let path = std::env::temp_dir().join("bt-metainfo-test-does-not-exist.torrent");
    assert!(matches!(
        TorrentDescriptor::load_from_file(&path),
        Err(TorrentError::Io(_))
    ));
}
