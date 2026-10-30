# Design

## Context

- M0 결과로 `bt-core`(식별 타입), 빈 `bt-engine`/`bt-session`, CLI, `cargo xtask ci` 품질 게이트가 있다. `xtask/src/main.rs`의 `ALLOWED_INTERNAL_DEPS`와 `bt-core` 전용 비동기 런타임 검사가 크레이트 경계를 지킨다.
- `bt-core`의 `InfoHash`는 이미 v1, v2, 하이브리드를 표현할 수 있다. 이번 change는 `bt-core`를 바꾸지 않고, 그 생성자에 넣을 해시 값을 만든다.
- qBittorrent에서 이 역할을 하는 것은 `TorrentDescriptor`(`src/base/bittorrent/torrentdescriptor.cpp`)와 그 아래의 libtorrent `load_torrent_buffer`/`parse_magnet_uri`다. 입력 제한 기본값은 `preferences.cpp:323-351`에 있다.
- 로컬 qBittorrent 5.2.4는 libtorrent 1.2 빌드다. 그래서 v1 info-hash만 화면에서 확인할 수 있다.
- 동기는 proposal.md의 Why를, 요구사항은 `specs/bencode`, `specs/torrent-metainfo`, `specs/magnet-link`, `specs/build-pipeline`을 본다.

## Goals / Non-Goals

**Goals:**
- M2(트래커), M3(피어), M4(저장소)가 별도 변환 없이 쓸 수 있는 메타데이터 모델을 고정한다.
- M8(resume)과 M9(ut_metadata)에서 다시 쓸 bencode 계층을 이번에 완성한다.

**Non-Goals:**
- v2 file tree, piece layers, 머클 트리 파싱(M11).
- 경로 정리(sanitize)와 디스크 경로 생성(M4, M7).
- .torrent 생성과 저장(qBittorrent `saveToFile`에 해당). 필요해지면 인코더 위에 추가한다.
- HTTP URL이나 `file://` 같은 소스 해석(M10).
- 성능 최적화(zero-copy 이상의 튜닝, 병렬 해시).

## Decisions

### D1. 크레이트와 모듈 배치

```
crates/bt-metainfo/
  Cargo.toml          # deps: bt-core, thiserror, sha1, sha2
  src/
    lib.rs            # 공개 API 재노출
    bencode/
      mod.rs          # Value, Limits, decode(), encode()
      decode.rs
      encode.rs
    torrent.rs        # TorrentInfo, FileEntry, RawPath, v1/하이브리드 파싱
    magnet.rs         # magnet / 16진수 문자열 해석
    descriptor.rs     # TorrentDescriptor (load, load_from_file, parse)
  tests/
    fixtures/         # .torrent 파일, 독립 계산 스크립트
    fixtures.rs       # qBittorrent/독립 값 대조 테스트
```

- 의존 방향은 `bt-engine → bt-metainfo → bt-core`다. `bt-session`도 `bt-metainfo`를 직접 쓸 수 있다(spec build-pipeline).
- bencode를 별도 크레이트로 빼지 않는다. 지금은 쓰는 곳이 이 크레이트뿐이다. 나중에 엔진의 와이어 프로토콜(BEP 10 확장 메시지)이 bencode를 쓰게 되어도 `bt-metainfo::bencode`를 공개 모듈로 열어 두면 된다.
- 대안으로 `bt-bencode`와 `bt-metainfo` 두 크레이트를 검토했다. 하지만 경계 검사 항목만 늘고 얻는 게 없어서 택하지 않았다.

### D2. bencode 값 모델: 빌린 디코딩 값과 소유 인코딩 값을 나눈다

```rust
pub struct Value<'a> { kind: Kind<'a>, raw: &'a [u8] }   // raw = 원본 구간
pub enum Kind<'a> {
    Int(i64),
    Bytes(&'a [u8]),
    List(Vec<Value<'a>>),
    Dict(Vec<(&'a [u8], Value<'a>)>),   // 입력 순서 그대로, 중복 포함
}
impl Value<'_> { fn get(&self, key: &[u8]) -> Option<&Value>  /* 처음 일치 */ }

pub enum OwnedValue {                    // 인코더 입력
    Int(i64), Bytes(Vec<u8>), List(Vec<OwnedValue>),
    Dict(BTreeMap<Vec<u8>, OwnedValue>), // 정렬이 타입으로 보장됨
}
```

- 디코딩 값은 입력 버퍼를 빌린다. 그래서 바이트 문자열과 원본 구간을 복사 없이 줄 수 있다. info-hash는 `info` 값의 `raw`에 해시를 적용하면 된다.
- 사전을 `Vec<(key, value)>`로 두는 이유: 정렬되지 않은 입력과 중복 키를 그대로 담아야 한다. "처음 값 우선" 조회(libtorrent `dict_find`와 같은 선형 탐색)도 단순해진다. 토렌트 사전은 키가 수십 개 수준이라 선형 탐색 비용은 문제가 되지 않는다.
- 인코더는 `OwnedValue`만 받는다. `BTreeMap<Vec<u8>, _>`가 바이트 사전순 정렬을 보장하므로 정규형 출력이 타입에서 나온다. 왕복 테스트는 `Value::to_owned_value()`를 거친다.
- 대안으로 하나의 값 타입에 원본 구간을 선택 필드로 두는 방법을 검토했다. 하지만 "인코딩한 값의 원본 구간"처럼 의미 없는 상태가 생겨서 택하지 않았다.

### D3. 디코더 동작과 제한

- 재귀 하강 방식으로 구현한다. 중첩 깊이는 `Limits.max_depth`(기본 100)로 막는다. 깊이 100 정도의 재귀는 기본 스레드 스택에서 충분히 안전하다.
- 토큰은 값 하나를 읽을 때마다 1씩 센다(정수, 문자열, 리스트, 사전 각각). 사전 키도 문자열이므로 토큰으로 센다.
- 문자열 길이는 숫자를 읽은 직후 남은 입력 길이와 비교한다. 입력이 빌린 슬라이스라서 할당 자체가 없다. 그래서 `999999999:` 같은 입력도 메모리를 쓰지 않고 "입력 끝" 오류가 된다.
- 허용하는 비정규 형태는 정렬되지 않은 키, 중복 키, 정수와 길이의 앞자리 0이다. `-0`도 0으로 읽는다. 거부하는 형태는 spec의 형식 오류 목록이다.
- 최상위 값 하나를 읽고 멈춘다. 남은 바이트는 무시한다(spec). 내부 API는 읽은 길이를 돌려준다. M3 와이어 프로토콜에서 "메시지 안의 bencode 뒤에 원시 데이터"를 읽을 때(ut_metadata) 이 길이가 필요하다.
- 크기 제한은 디코딩을 시작할 때 입력 길이로 확인한다. 파일에서 읽을 때는 `std::fs::metadata`로 먼저 확인한다.

### D4. 해시 계산은 RustCrypto `sha1`/`sha2`

- 둘 다 순수 Rust이고 `unsafe`를 쓰는 의존이 없다. workspace는 `unsafe_code = "forbid"`이지만, 이건 우리 코드에만 적용된다. Windows와 Linux에서 별도 시스템 라이브러리가 필요 없다.
- 버전은 `[workspace.dependencies]`에 추가할 때의 최신 안정판으로 정한다.
- 대안인 `ring`/`openssl`은 빌드 도구 의존이 생기고, SHA-1을 쓰는 API가 불편해서 택하지 않았다.

### D5. 메타데이터 모델

```rust
pub struct TorrentInfo {
    info_hash: InfoHash,         // bt_core
    name: Vec<u8>,               // name.utf-8 우선, 원본 바이트
    piece_length: u32,
    pieces: Vec<Sha1Hash>,
    files: Vec<FileEntry>,       // 인덱스 = FileIndex
    total_length: u64,
    multi_file: bool,
    private: bool,
    info_bytes: Box<[u8]>,       // 원본 info 구간 사본 (M9 ut_metadata 응답용)
}
pub struct FileEntry { path: RawPath, length: u64, pad: bool }
pub struct RawPath(Vec<Vec<u8>>);  // 검증 전 경로 조각
```

- `piece_length`는 `u32`다. 0 이하이거나 `u32`를 넘으면 오류다. 2의 거듭제곱 조건은 v1 규격에 없으므로 검사하지 않는다.
- 피스와 파일 접근은 `PieceIndex`/`FileIndex`로 한다. 개수가 `u32`를 넘는 토렌트는 파싱 단계에서 거부한다(spec). 그래서 이후 계층에서는 인덱스 변환이 실패하지 않는다.
- `piece_size(PieceIndex) -> u32`를 제공한다. 마지막 피스만 짧다.
- 단일 파일 토렌트의 `files`는 원소가 하나이고, 그 경로는 `[name]`이다. 다중 파일의 경로는 `name`을 포함하지 않는 상대 경로다. 최종 디렉터리 구성(콘텐츠 레이아웃)은 M7에서 정한다.
- **`RawPath`에는 `AsRef<Path>`, `Into<PathBuf>`, `to_path()`를 구현하지 않는다.** 화면 표시용으로 손실을 허용하는 `display()`(UTF-8 lossy, `/`로 연결)만 둔다. 디스크 경로는 M4에서 만들 정리 함수만 만들 수 있게 한다(spec torrent-metainfo "원본 보존").
- 이름과 경로를 `String`이 아니라 바이트로 두는 이유: UTF-8이 아닌 이름을 가진 오래된 토렌트가 실제로 있다. 어떤 인코딩으로 볼지는 표시나 디스크 단계에서 정해야 원본을 잃지 않는다.
- `info_bytes`를 복사해 보관하는 이유: 디코딩 값은 입력 버퍼를 빌리므로 `TorrentInfo`가 버퍼보다 오래 살려면 소유 사본이 필요하다. M9에서 다른 피어에게 메타데이터를 보낼 때도 이 바이트를 그대로 쓴다.

### D6. TorrentDescriptor와 진입 API

```rust
pub struct TorrentDescriptor {
    info_hash: InfoHash,
    name: Option<Vec<u8>>,        // torrent: info.name, magnet: dn
    trackers: Vec<Vec<String>>,   // tier 목록
    url_seeds: Vec<String>,
    creation_date: Option<i64>,   // 이하 .torrent에서만
    comment: Option<String>,
    created_by: Option<String>,
    info: Option<TorrentInfo>,    // magnet이면 None
}
impl TorrentDescriptor {
    pub fn load(data: &[u8]) -> Result<Self, TorrentError>;
    pub fn load_with_limits(data: &[u8], limits: Limits) -> Result<Self, TorrentError>;
    pub fn load_from_file(path: &Path) -> Result<Self, TorrentError>;
    pub fn load_from_file_with_limits(path: &Path, limits: Limits) -> Result<Self, TorrentError>;
    pub fn parse(s: &str) -> Result<Self, MagnetError>;   // magnet 또는 40/64자 hex
}
```

- 이름과 시그니처를 qBittorrent의 `load`/`loadFromFile`/`parse`에 맞춘다. 분석 문서와 코드를 오가며 읽기 쉽게 하기 위해서다.
- 트래커와 웹시드 URL은 `String`으로 둔다. 바이트가 UTF-8이 아니면 lossy 변환한다. URL은 원래 ASCII여야 하므로 정보 손실의 실질적인 위험이 낮다. M2가 문자열로 다루기도 쉽다.
- `comment`와 `created_by`도 lossy `String`이다. 표시 전용이기 때문이다.
- descriptor는 "메타데이터가 나중에 생길 수 있음"을 `Option<TorrentInfo>`로 표현한다. qBittorrent의 `setTorrentInfo`에 해당하는 갱신은 M9에서 추가한다.

### D7. magnet 해석은 직접 구현한다

- `magnet:?` 접두어를 확인한 뒤 `&`로 나누고, 각 항목을 첫 `=`에서 키와 값으로 나눈다. 값은 퍼센트 디코딩한다. 퍼센트 디코딩 결과가 UTF-8이 아니면 lossy 변환한다.
- 매개변수 이름은 대소문자를 구분하지 않고, 끝의 `.<숫자>` 접미사(`tr.1` 등)를 뗀 뒤 비교한다. 접두어 `magnet:?`는 대소문자를 구분한다. 둘 다 libtorrent와 같은 동작이다.
- base32 디코딩(RFC 4648, 대소문자 무시, 패딩 없음)과 퍼센트 디코딩은 합쳐서 수십 줄 정도다. 그래서 `url`, `percent-encoding`, `data-encoding` 크레이트를 쓰지 않는다. M0의 16진수 직접 구현(D3)과 같은 판단이다.
- `xt`가 여러 개면 btih와 btmh를 각각 모은다. 같은 종류가 두 번 나오면 마지막 값을 쓴다(libtorrent와 같음). 알 수 없는 `xt` 스킴은 무시한다.
- 트래커 tier: 각 `tr`을 등장 순서대로 tier 하나씩으로 만든다. libtorrent `parse_magnet_uri`가 `tr`마다 `tier++`를 하는 것을 소스로 확인했다(`docs/qBittorrent/file-download-flow.md` 6.3).
- 16진수 문자열 판정은 qBittorrent의 `isV1Hash`/`isV2Hash`와 같다. 길이가 정확히 40이나 64이고 모두 16진수 문자여야 한다. 앞뒤 공백은 자르지 않는다.

### D8. 트래커 필터링을 하지 않는다 (qBittorrent와의 의도적 차이)

- qBittorrent는 `parse()` 경로에서만 `removeUnsupportedTrackers`로 http, https, udp 이외 트래커를 버린다(`torrentdescriptor.cpp:103-124`). `load()` 경로에서는 버리지 않는다.
- 구현 중에 확인한 사실: libtorrent `parse_magnet_uri`도 `is_valid_tracker_url`로 http, https, udp와 올바른 호스트만 남긴다. 따라서 magnet 경로에서는 qBittorrent와 libtorrent 둘 다 거르고, 이 프로젝트만 거르지 않는다.
- 이 프로젝트는 두 경로 모두 걸러내지 않는다. 사용자가 이렇게 결정했다. 그래서 입력 경로에 따라 결과가 달라지지 않고, 원본 정보도 보존된다.
- 그 대신 M2 트래커 계층이 지원하지 않는 스킴과 잘못된 URL을 "건너뛰기"로 처리해야 한다. 이 내용을 계획 문서의 M2 항목에 추가한다(tasks).

### D9. 오류 타입

- `bencode::DecodeError { offset: usize, kind: DecodeErrorKind }`. `kind`는 `UnexpectedEof`, `InvalidByte`, `IntOverflow`, `NonStringKey`, `DepthLimit`, `TokenLimit`, `SizeLimit` 중 하나다.
- `TorrentError`의 variant는 다음과 같다.
  - `Decode(DecodeError)`
  - `Io(std::io::Error)`
  - `NotADictionary` (최상위 값이 사전이 아님)
  - `MissingField(&'static str)`, `InvalidField(&'static str)`
  - `V2OnlyUnsupported`, `UnsupportedMetaVersion(i64)`
  - `PieceCountMismatch { expected, actual }`
  - `TooManyPieces`, `TooManyFiles`
- `MagnetError`의 variant는 `NotMagnet`, `MissingInfoHash`, `InvalidHash { scheme }`, `UnsupportedMultihash`다.
- 필드 이름은 `&'static str`로 담는다(`"info"`, `"piece length"`, `"files[3].length"` 같은 형태는 쓰지 않고 키 이름만 담는다). 어느 파일에서 오류가 났는지 알려 주는 정보는 필요해지면 나중에 추가한다.
- 크레이트 하나에 오류 하나로 합치지 않는 이유: magnet 해석은 디코딩이나 I/O 오류가 날 수 없다. 타입만 보고 어떤 오류가 가능한지 알 수 있게 하기 위해서다.

### D10. 경계 검사 갱신

- `ALLOWED_INTERNAL_DEPS`를 다음과 같이 바꾼다.
  - `bt-core: []`
  - `bt-metainfo: [bt-core]`
  - `bt-engine: [bt-core, bt-metainfo]`
  - `bt-session: [bt-core, bt-metainfo, bt-engine]`
- 비동기 런타임 검사 대상을 `bt-core` 하나에서 `["bt-core", "bt-metainfo"]` 목록으로 바꾼다.
- 경계 검사를 바꾸면서 `.github/workflows/ci.yml`은 건드리지 않는다(CLAUDE.md).

### D11. fixture와 기대값

```
crates/bt-metainfo/tests/fixtures/
  v1-single.torrent         # qBittorrent로 생성
  v1-multi.torrent          # qBittorrent로 생성, 정렬 최적화 끔
  v1-multi-padded.torrent   # qBittorrent로 생성, 정렬 최적화 켬
  <공식 배포>.torrent        # Debian 등 1~2개
  hybrid.torrent            # libtorrent test/test_torrents 에서
  v2-only.torrent           # 위와 같음 (거부 확인용)
  info_hashes.py            # 독립 계산 스크립트
  README.md                 # 출처, 라이선스, 기대값 확인 방법
```

- 기대값은 `tests/fixtures.rs` 안에 상수 표로 둔다. 별도 데이터 파일 형식(toml 등)을 읽으면 dev 의존만 늘어난다.
- `info_hashes.py`는 표준 라이브러리만 쓴다. bencode 최소 디코더 하나와 `hashlib.sha1`/`sha256`이 전부다. Rust 구현을 참고하지 않고 따로 작성해서 독립성을 유지한다. 실행은 `python -I`로 한다.
- 하이브리드 fixture의 v1 값은 lt 1.2 qBittorrent로도 확인할 수 있다(lt 1.2는 하이브리드를 v1로 읽는다). 그래서 v1 쪽은 qBittorrent 대조, v2 쪽은 스크립트 대조로 이중 확인한다.
- 정규형 fixture로 왕복 테스트(bencode spec)도 한다. qBittorrent가 만든 파일은 정규형이다.
- 비정규 입력(정렬되지 않은 키, 앞자리 0)과 공격성 입력(깊은 중첩, 큰 길이)은 테스트 코드 안에서 바이트로 만든다.

## Risks / Trade-offs

- [관대한 디코딩이 libtorrent와 세부적으로 다를 수 있음(예: `-0`, `i-e`, 공백 처리)] → spec에 허용과 거부 목록을 명시하고, 목록 밖의 차이는 실제 파일에서 발견될 때 고친다. 차이가 나도 info-hash는 원본 바이트로 계산하므로 식별자가 틀릴 위험은 없다.
- [호출자가 `max_depth`를 아주 크게 주면 재귀 때문에 스택이 넘칠 수 있음] → rustdoc에 경고를 적는다. 기본값과 qBittorrent 설정 범위(최대 수백)에서는 안전하다. 문제가 되면 명시적 스택 방식으로 바꾼다. 공개 API는 바뀌지 않는다.
- [독립 스크립트를 같은 사람(AI)이 작성해서 같은 오해를 공유할 수 있음] → v1 해시는 qBittorrent 화면과 대조하고, 하이브리드의 v1도 qBittorrent로 확인한다. v2는 "같은 바이트의 SHA-256"이라 오해할 여지가 작다.
- [트래커를 거르지 않아서 M2 이전에는 잘못된 URL이 그대로 전달됨] → M2 계획에 건너뛰기 처리를 명시한다(D8).
- [`RawPath`에 디스크 경로 변환이 없어서 M4 전까지 실제 파일을 쓸 수 없음] → 의도한 것이다. M1에는 디스크 쓰기가 없다.
- [lossy UTF-8 변환으로 트래커 URL이 바뀔 수 있음] → 실제 URL은 ASCII라 영향이 없다. 문제가 되는 파일이 나오면 바이트 보존으로 바꾼다.

## Migration Plan

새 크레이트를 추가하는 change라서 이전(migration)할 데이터가 없다. 기존 코드 중에는 `bt-engine/Cargo.toml`(의존 추가), `xtask/src/main.rs`(경계 표), 루트 `Cargo.toml`(workspace 의존)만 바뀐다. 문제가 생기면 커밋을 되돌리면 된다.

## Open Questions

- 공식 배포 fixture로 무엇을 쓸지(Debian netinst, Ubuntu 등)는 파일 크기와 라이선스만 맞으면 된다. 구현할 때 정한다.
