# Proposal

## Why

[다운로드 기능 구현 계획](../../../docs/download-implementation-plan.md)의 M1~M10은 모두 같은 크레이트 경계, 공용 타입, 품질 기준 위에 쌓인다. 저장소는 지금 `cargo new` 직후 상태다. 그래서 첫 기능(M1 bencode/metainfo)을 넣기 전에 이 기반을 먼저 고정해야 한다. 나중에 바꾸면 모든 계층의 시그니처와 의존 관계를 다시 고쳐야 한다.

## What Changes

- 단일 크레이트를 Cargo workspace로 바꾼다. 크레이트는 `bt-core`(공용 타입), `bt-engine`(엔진, M1~M5), `bt-session`(앱 로직, M6~), `bitturbulent`(CLI 바이너리)로 나눈다. 의존 방향은 `bitturbulent → bt-session → bt-engine → bt-core` 한 방향으로 고정한다. 그러면 "엔진과 세션은 이벤트 채널로만 통신한다"는 원칙을 컴파일러가 강제한다.
- `bt-core`에 토렌트 식별 타입을 정의한다. `Sha1Hash`, `Sha256Hash`, v1/v2/하이브리드를 모두 담는 `InfoHash`, 그리고 20바이트 `TorrentId`다. 16진수 문자열 표시와 파싱을 포함한다. 구조는 qBittorrent의 `InfoHash`/`TorrentID`를 따른다.
- `bt-core`에 `PieceIndex`, `FileIndex` 인덱스 newtype을 정의한다.
- 비동기 런타임(tokio), 에러 타입(라이브러리는 thiserror, CLI는 anyhow), 로깅(tracing)의 사용 방식을 정한다.
- 로컬과 CI가 같은 명령을 쓰는 품질 게이트(fmt, clippy, test)를 만든다. GitHub Actions에서 Windows와 Linux 두 OS로 돌린다.
- `bt-engine`, `bt-session`은 이번 change에서 빈 크레이트로 만든다. 기능 구현은 M1 이후의 몫이다.

## Capabilities

### New Capabilities
- `torrent-identity`: 토렌트를 식별하는 해시 값(v1/v2 info-hash, TorrentId)의 구성 규칙, 상호 변환, 16진수 표기를 다룬다.
- `piece-file-indices`: 피스와 파일을 가리키는 인덱스 값의 범위, 표기, 서로 섞어 쓸 수 없다는 규칙을 다룬다.
- `build-pipeline`: workspace 크레이트 의존 경계, 품질 게이트 명령, CI에서 이를 실행하는 방식을 다룬다.

### Modified Capabilities
- (없음. 기존 spec이 없다.)

## Impact

- **코드**: 루트 `Cargo.toml`이 virtual manifest로 바뀐다. `src/main.rs`는 `crates/bitturbulent/`로 옮긴다. `crates/`와 `xtask/` 디렉터리를 새로 만든다.
- **의존성 추가**: `thiserror`, `tracing`, `tracing-subscriber`, `tokio`, `anyhow`.
- **인프라**: `.github/workflows/ci.yml`를 추가한다. GitHub 저장소 생성과 push는 사용자가 직접 하며, 이번 change에는 워크플로 파일만 들어간다.
- **결정을 미룬 것**: MSRV는 고정하지 않고 최신 stable만 사용한다. edition 2024이므로 실질적인 하한은 1.85다. 크레이트 이름 접두사는 임시로 `bt-`를 쓰고, 공개 배포 시점에 다시 정한다.
