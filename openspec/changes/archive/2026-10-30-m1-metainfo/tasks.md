# Tasks

## 1. 크레이트 골격과 경계 (spec: build-pipeline)

- [x] 1.1 `crates/bt-metainfo`(lib)를 만든다. 의존은 `bt-core`, `thiserror`이고 `[lints] workspace = true`를 둔다. 루트 `[workspace.dependencies]`에 `bt-metainfo`와 `sha1`, `sha2`(최신 안정판)를 추가하고, `bt-engine`이 `bt-metainfo`에 의존하게 한다. 확인: `cargo build --workspace`가 성공하고, `cargo tree -p bt-metainfo`에 `bt-engine`, `bt-session`, `tokio`가 없다.
- [x] 1.2 `xtask/src/main.rs`의 `ALLOWED_INTERNAL_DEPS`를 design D10대로 바꾸고, 비동기 런타임 검사 대상을 `bt-core`, `bt-metainfo` 목록으로 넓힌다. 확인: `cargo xtask ci`가 통과한다. `bt-metainfo`에 `bt-engine` 의존을 잠시 추가하면 경계 검사가 실패하는지 확인한 뒤 되돌린다. Cargo 순환 의존 오류로 먼저 실패해도 된다. `tokio` 의존을 잠시 추가했을 때도 실패하는지 확인한 뒤 되돌린다.
- [x] 1.3 README의 workspace 구조 설명에 `bt-metainfo` 계층을 추가한다. 확인: README의 크레이트 목록과 의존 방향이 `cargo metadata` 결과와 일치한다.

## 2. bencode 디코더 (spec: bencode)

- [x] 2.1 `Value`/`Kind`(원본 구간 포함), `Limits`(기본 100 MiB, 100, 10,000,000), `DecodeError { offset, kind }`를 정의하고, 정수, 문자열, 리스트, 사전을 읽는 재귀 하강 디코더를 구현한다(design D2, D3). 최상위 값 하나만 읽고, 읽은 길이를 내부 API로 돌려준다. 확인: spec "네 가지 값", "뒤에 남은 바이트" 시나리오 단위 테스트가 통과한다.
- [x] 2.2 비정규 입력 허용(정렬되지 않은 키, 앞자리 0, `-0`)과 중복 키 처음 값 조회(`Value::get`)를 구현한다. 확인: spec "비정규 입력", "중복 키" 시나리오 테스트가 통과한다.
- [x] 2.3 원본 구간 반환을 테스트한다. 확인: 비정규 `info` 사전의 `raw`가 입력 부분 슬라이스와 바이트 단위로 같은 테스트가 통과한다.
- [x] 2.4 형식 오류(입력 끝, 잘못된 바이트, 정수 범위 초과, 정수 키)와 제한(깊이, 토큰, 크기, 큰 길이 주장)을 구현한다. 확인: spec의 오류와 제한 시나리오 테스트가 모두 통과한다. 101단계 중첩 테스트가 스택 오버플로 없이 깊이 오류를 낸다. `max_depth` rustdoc에 재귀 깊이 경고가 들어 있다.

## 3. bencode 인코더 (spec: bencode)

- [x] 3.1 `OwnedValue`(사전은 `BTreeMap`), `encode()`, `Value::to_owned_value()`를 구현한다. 확인: spec "키 정렬", "음수와 0" 시나리오 테스트가 통과한다.
- [x] 3.2 손으로 만든 정규형 입력 여러 개로 왕복 테스트(decode → to_owned_value → encode == 입력)를 추가한다. fixture 왕복은 5.3에서 한다. 확인: 왕복 테스트가 통과하고, `cargo doc -p bt-metainfo --no-deps`가 경고 없이 성공한다.

## 4. .torrent 파싱 (spec: torrent-metainfo)

- [x] 4.1 `TorrentInfo`, `FileEntry`, `RawPath`(디스크 경로 변환 없음, lossy `display()`만 있음), `TorrentError`를 정의한다(design D5, D9). `RawPath`에 `AsRef<Path>`/`Into<PathBuf>`가 없다는 것을 `compile_fail` doctest로 고정한다. 확인: `cargo test -p bt-metainfo --doc`가 통과한다.
- [x] 4.2 v1 `info` 파싱을 구현한다. 대상은 단일/다중 파일, `name.utf-8`/`path.utf-8` 우선, `attr`의 `p` 패딩 표시, 피스 길이와 해시와 개수 검증, `u32` 개수 상한, `piece_size()`, 필수 필드 오류다. 확인: spec "단일/다중 파일", "패딩", "경로 원본 보존"(`..` 보존, 빈 path 오류), "피스 정보"(16384/40000 → 7232), "필수 필드" 시나리오 테스트가 통과한다.
- [x] 4.3 info-hash 계산을 구현한다. SHA-1은 원본 `info` 구간에 적용한다. `meta version`이 2이고 v1 필드가 있으면 SHA-256도 계산해 `InfoHash::hybrid`를 만든다. v2 전용과 알 수 없는 버전은 거부한다. 확인: 비정규 info 사전 테스트(원본 SHA-1 ≠ 재인코딩 SHA-1), 하이브리드/v2 전용/meta version 3 테스트가 통과한다. 이 단계의 테스트 입력은 바이트로 직접 만든다.
- [x] 4.4 최상위 필드를 구현한다. 대상은 `announce-list`(tier 보존, 빈 tier 제거) 우선과 `announce` 대체, 트래커 스킴 비필터링, `url-list`(문자열 또는 리스트), `private`, `creation date`/`comment`(`.utf-8` 우선)/`created by`(타입이 틀리면 무시)다. 확인: 해당 spec 시나리오 테스트가 모두 통과한다. `wss://` 보존 테스트도 포함한다.
- [x] 4.5 `TorrentDescriptor`와 `load`, `load_with_limits`, `load_from_file`을 구현한다(design D6). `load_from_file`은 파일 크기를 먼저 확인한다. 확인: 메타데이터 포함 시나리오 테스트가 통과한다. 상한을 작게 준 `load_from_file` 테스트가 크기 제한 오류를 낸다.

## 5. fixture 대조 (spec: torrent-metainfo "qBittorrent와 같은 식별자", bencode "왕복")

- [x] 5.1 **(사용자 수동 단계)** 로컬 qBittorrent 5.2.4 토렌트 생성기로 작은 더미 파일을 써서 `v1-single.torrent`, `v1-multi.torrent`(정렬 최적화 끔), `v1-multi-padded.torrent`(정렬 최적화 켬)를 만든다. 공식 배포 .torrent 1~2개도 받는다. 각 파일을 qBittorrent에 추가(일시정지 상태로)해서 속성 창의 "정보 해시 v1"을 기록한다. 확인: 파일들이 `crates/bt-metainfo/tests/fixtures/`에 있고 해시 목록이 전달되었다.
- [x] 5.2 libtorrent 저장소(`test/test_torrents/`)에서 하이브리드 .torrent 1개와 v2 전용 .torrent 1개를 가져온다. 표준 라이브러리만 쓰는 `info_hashes.py`(독립 bencode 디코더 + hashlib)를 작성한다. 출처, 라이선스(BSD), 기대값 확인 방법은 `fixtures/README.md`에 적는다. 확인: `python -I info_hashes.py <file>`가 하이브리드 파일의 v1/v2 값을 출력한다. 사용자가 하이브리드 파일을 qBittorrent(lt 1.2)에 넣었을 때 보이는 v1 값이 스크립트의 v1 값과 같다.
- [x] 5.3 `tests/fixtures.rs`에 기대값 상수 표를 두고 대조 테스트를 작성한다. 내용은 다음과 같다.
  - v1 fixture는 qBittorrent 값과 같아야 한다.
  - 하이브리드는 스크립트의 v1/v2 값과 같고, 토렌트 ID가 v2의 앞 20바이트여야 한다.
  - v2 전용은 거부되어야 한다.
  - 정렬 최적화 fixture에는 패딩 파일이 표시되어야 한다.
  - qBittorrent가 만든 fixture는 bencode 왕복 결과가 원본과 같아야 한다.

  확인: `cargo test -p bt-metainfo --test fixtures`가 통과한다.

## 6. magnet 해석 (spec: magnet-link)

- [x] 6.1 퍼센트 디코딩과 base32 디코딩(RFC 4648, 대소문자 무시)을 직접 구현한다(design D7). 확인: RFC 4648 테스트 벡터와 `%20`, `%2F`, 잘못된 `%` 시퀀스 단위 테스트가 통과한다.
- [x] 6.2 libtorrent `parse_magnet_uri`의 `tr` tier 할당 방식을 소스에서 확인한다. design D7의 "tr마다 tier 하나" 가정이 맞는지 보고, 다르면 design과 spec을 고친다. 확인 결과를 `docs/qBittorrent/file-download-flow.md`에 한 줄로 남긴다. 확인: 문서에 libtorrent 소스 위치와 동작이 적혀 있다.
- [x] 6.3 `MagnetError`와 `TorrentDescriptor::parse`를 구현한다. 대상은 btih(hex/base32), btmh(`1220`만 허용), 하이브리드, `dn`/`tr`/`ws`, 알 수 없는 매개변수 무시, 40/64자 hex 문자열 처리, 메타데이터 없는 descriptor다. 확인: magnet-link spec의 모든 시나리오 테스트가 통과한다. fixture 하이브리드 토렌트의 해시로 만든 magnet에서 얻은 info-hash가 .torrent에서 얻은 값과 같은 테스트도 포함한다.

## 7. 문서와 통합 확인

- [x] 7.1 `docs/qBittorrent/file-download-flow.md`에 M1 구현에서 확인한 qBittorrent 동작을 추가한다. 내용은 `TorrentDescriptor::parse`의 hex 변환, `removeUnsupportedTrackers`가 parse 경로에만 있다는 점, `loadTorrentLimits` 기본값(`preferences.cpp:323-351`), lt 1.2 생성기에 v2/하이브리드 선택지가 없다는 점(`torrentcreatordialog.cpp:149-153`)이다. 확인: 각 항목에 커밋 `5998334` 기준 파일과 줄 번호가 붙어 있다.
- [x] 7.2 `docs/download-implementation-plan.md`를 갱신한다. 모듈 구조를 workspace 크레이트 구조로 바꾸고, M2 항목에 "지원하지 않는 트래커 스킴과 잘못된 URL은 건너뛴다"를 추가한다. M4 항목에는 "검증 전 경로 정리, 패딩 파일은 디스크에 만들지 않음"을 추가한다. 확인: 문서의 크레이트 이름이 실제 workspace와 같다.
- [x] 7.3 Windows 로컬에서 `cargo xtask ci`를 끝까지 실행한다. 확인: 종료 코드 0.

## Workflow follow-up

- M1 완료 기준(fixture info-hash가 qBittorrent 값과 일치, 로컬 `cargo xtask ci` 통과)을 확인한 뒤 change를 archive한다.
- archive하면 `bencode`, `torrent-metainfo`, `magnet-link` spec이 `openspec/specs/`에 생기고, `build-pipeline` spec이 다섯 계층 기준으로 갱신된다.
