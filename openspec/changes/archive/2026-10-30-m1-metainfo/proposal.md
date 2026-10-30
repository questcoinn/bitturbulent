# Proposal

## Why

[다운로드 기능 구현 계획](../../../docs/download-implementation-plan.md)의 M2 이후 단계(트래커, 피어, 저장소, 세션)는 모두 "이 토렌트가 무엇인가"를 입력으로 받는다. 여기에는 info-hash, 파일 목록, 피스 해시, 트래커가 들어간다. M0에서 식별 타입(`InfoHash`, `TorrentId`)은 만들었다. 하지만 .torrent 파일이나 magnet 링크에서 그 값을 만들어 내는 코드가 아직 없어서 다음 단계를 시작할 수 없다.

## What Changes

- 새 라이브러리 크레이트 `bt-metainfo`를 `bt-core`와 `bt-engine` 사이에 추가한다. 의존 방향은 `bt-engine → bt-metainfo → bt-core`다. 세션 계층(qBittorrent의 AddTorrentManager 역할)도 엔진을 거치지 않고 descriptor를 쓸 수 있게 하기 위해서다.
- bencode 디코더와 인코더를 만든다.
  - 디코더는 비정규 입력(정렬되지 않은 키, 앞자리 0)도 읽는다. 이건 qBittorrent(libtorrent)와 같은 동작이다.
  - 디코더는 원본 바이트 구간을 보존한다. info-hash를 이 구간으로 계산하기 위해서다.
  - 크기, 중첩 깊이, 토큰 수 제한을 둔다. 기본값은 qBittorrent와 같은 100 MiB, 100, 10,000,000이다.
  - 인코더는 정규형으로 출력한다. 디코더와 인코더의 왕복 테스트에 쓴다.
- .torrent 파싱을 구현한다.
  - v1 단일 파일과 다중 파일, piece length, 피스 해시, announce/announce-list(tier), url-list, private 플래그, 부가 정보(이름, 생성일, 주석, 생성자)를 읽는다.
  - 패딩 파일(`attr`에 `p`)은 따로 표시한다.
  - info-hash는 원본 info 바이트로 계산한다. v1은 SHA-1이다. `meta version`이 2인 하이브리드는 SHA-256도 함께 계산한다. 이러면 하이브리드의 `TorrentId`가 qBittorrent와 같아진다.
  - v2 전용 .torrent는 "v2 미지원" 오류로 거부한다. v2 file tree와 piece layers 파싱은 이번 범위가 아니다.
- magnet 링크 파싱을 구현한다.
  - `xt`(btih, btmh), `dn`, `tr`, `ws`를 읽는다.
  - 40자나 64자 16진수 문자열만 들어와도 magnet으로 받는다. 이건 qBittorrent `TorrentDescriptor::parse`와 같은 동작이다.
  - 트래커 주소는 걸러내지 않는다. **qBittorrent와 의도적으로 다른 부분이다.** 지원하지 않는 스킴은 M2 트래커 계층에서 건너뛴다.
- `TorrentDescriptor`를 만든다. .torrent에서 만들면 메타데이터가 있고, magnet에서 만들면 메타데이터가 없을 수 있다.
- 파일 경로는 .torrent에 적힌 조각 그대로 "검증 전 경로"로 보존한다. 이 타입에서 바로 디스크 경로로 바꾸는 API는 두지 않는다. 정리(sanitize)는 디스크 경로를 만드는 M4에서 한다.
- 테스트 fixture를 추가한다.
  - qBittorrent로 직접 만든 v1 파일(단일 파일, 다중 파일, 패딩 있음과 없음)과 공식 배포 .torrent는 qBittorrent 화면값과 대조한다.
  - 하이브리드와 v2 전용 파일은 libtorrent 테스트 토렌트를 쓰고, 독립 스크립트로 계산한 값과 대조한다.

## Capabilities

### New Capabilities
- `bencode`: bencode 값의 디코딩과 인코딩, 비정규 입력 허용 범위, 원본 바이트 구간 보존, 입력 제한을 다룬다.
- `torrent-metainfo`: .torrent 파일에서 info-hash, 파일 목록, 피스 정보, 트래커, 부가 정보를 읽는 규칙과 지원하는 형식(v1, 하이브리드)의 범위를 다룬다.
- `magnet-link`: magnet URI와 16진수 해시 문자열을 해석해 메타데이터 없는 토렌트 설명을 만드는 규칙을 다룬다.

### Modified Capabilities
- `build-pipeline`: workspace 계층이 넷(공용 타입, 엔진, 세션, CLI)에서 다섯(공용 타입, 메타데이터, 엔진, 세션, CLI)으로 바뀐다. 메타데이터 크레이트의 허용 의존과 "비동기 런타임 금지" 규칙을 추가한다.

## Impact

- **코드**: `crates/bt-metainfo/`를 새로 만든다. `bt-engine`의 의존에 `bt-metainfo`를 추가한다. `xtask`의 경계 검사 허용 목록을 갱신한다. `bt-core`는 바꾸지 않는다.
- **의존성 추가**: SHA-1과 SHA-256 계산용 크레이트(RustCrypto `sha1`, `sha2` 예정), magnet의 퍼센트 디코딩용 크레이트(필요하면).
- **테스트 자원**: `crates/bt-metainfo/tests/fixtures/`에 .torrent 파일을 둔다. qBittorrent 화면에서 info-hash를 확인하는 수동 단계가 있다. 로컬 qBittorrent 5.2.4는 libtorrent 1.2 빌드라서 v2와 하이브리드 확인은 여기서 할 수 없다.
- **다음 단계로 넘기는 것**: M2는 지원하지 않는 트래커 스킴을 건너뛰어야 한다. M4는 "검증 전 경로"를 OS별로 안전한 디스크 경로로 바꾸고, 패딩 파일은 디스크에 만들지 않아야 한다. M11에서는 v2 전체 지원과 lt20 빌드 qBittorrent로 하이브리드 값을 대조한다.
