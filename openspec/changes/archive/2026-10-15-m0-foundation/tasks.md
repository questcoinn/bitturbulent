# Tasks

## 1. Workspace 골격

- [x] 1.1 루트 `Cargo.toml`을 virtual manifest로 바꾼다. `resolver = "3"`, `[workspace.package]`(edition 2024), `[workspace.dependencies]`, `[workspace.lints]`(clippy 기본 그룹)를 넣는다. 확인: `cargo metadata --no-deps`가 성공한다.
- [x] 1.2 `crates/bt-core`, `crates/bt-engine`, `crates/bt-session`(lib)과 `crates/bitturbulent`(bin)을 만든다. 기존 `src/main.rs`는 `crates/bitturbulent/src/main.rs`로 옮기고 루트 `src/`를 지운다. 각 크레이트에 `[lints] workspace = true`를 둔다. 의존은 design D1대로 연결한다. 확인: `cargo build --workspace`가 성공하고, `cargo tree -p bt-engine`에 `bt-session`이 없다.

## 2. 토렌트 식별 타입 (`bt-core`, spec: torrent-identity)

- [x] 2.1 16진수 인코딩/디코딩 헬퍼와 `ParseHashError`(`InvalidLength`, `InvalidChar`)를 구현한다. 확인: 길이 오류와 잘못된 문자 단위 테스트가 통과한다.
- [x] 2.2 `Sha1Hash`, `Sha256Hash`를 구현한다(`from_bytes`/`as_bytes`, `From<[u8; N]>`, 소문자 `Display`, 대소문자 무시 `FromStr`, 16진수 `Debug`, 비교/해시/정렬 derive). 확인: spec의 표시 예시(`0001..13`), 왕복 변환, 대문자 입력 테스트가 통과한다.
- [x] 2.3 `InfoHash`를 구현한다. 필드는 비공개로 두고 생성자 `from_v1`/`from_v2`/`hybrid`와 접근자 `v1()`/`v2()`를 둔다. 확인: v1 전용, v2 전용, 하이브리드 시나리오 테스트가 통과한다. 두 값이 다 없는 InfoHash를 만드는 코드는 `compile_fail` doctest로 막힘을 확인한다.
- [x] 2.4 `TorrentId`와 `InfoHash::torrent_id()`를 구현한다(v2가 있으면 앞 20바이트, 없으면 v1). `Display`/`FromStr`/derive는 해시 타입과 같게 맞춘다. 확인: v1 전용 유도, v2 우선, 결정성, `HashMap` 키 조회 테스트가 통과한다.
- [x] 2.5 `bt-core` 공개 타입에 rustdoc을 단다(TorrentId 유도 규칙과 qBittorrent 대응 관계 포함). 확인: `cargo doc -p bt-core --no-deps`가 경고 없이 성공한다.

## 3. 인덱스 타입 (`bt-core`, spec: piece-file-indices)

- [x] 3.1 `PieceIndex`, `FileIndex` newtype을 구현한다. 제공 항목은 `From<u32>`, `TryFrom<usize>`, `get()`, `as_usize()`, 10진수 `Display`, 비교/해시/정렬 derive이고, `usize::BITS >= 32` const assert를 둔다. 확인: 최댓값, 범위를 넘는 변환 오류, 표시(`42`), 정렬 테스트가 통과한다.
- [x] 3.2 두 타입을 섞어 쓰는 코드가 거부되는지 `compile_fail` doctest로 검증한다. 확인: `cargo test -p bt-core --doc`가 통과한다.

## 4. CLI 로깅과 품질 게이트 (spec: build-pipeline)

- [x] 4.1 `bitturbulent` main을 `#[tokio::main]` + `anyhow::Result`로 바꾼다. `tracing-subscriber`(EnvFilter, 기본 `info`)를 초기화하고 시작 로그를 info 하나, debug 하나 남긴다. 확인: `cargo run -p bitturbulent`에는 info만 나오고, `RUST_LOG=debug`일 때는 debug도 나온다.
- [x] 4.2 `xtask` 크레이트와 `.cargo/config.toml` alias를 추가하고, `cargo xtask ci`가 fmt check, clippy `-D warnings`, test를 차례로 실행하게 한다. 실패하면 거기서 멈추고 0이 아닌 코드로 끝나야 한다. 확인: 깨끗한 상태에서는 종료 코드가 0이다. 일부러 lint 경고를 넣거나 포맷을 어기면 0이 아닌 코드로 끝나는지 직접 확인한 뒤 되돌린다.
- [x] 4.3 `cargo xtask ci`에 경계 검사를 추가한다(`cargo_metadata`로 `bt-core`에 workspace 내부 의존과 `tokio`가 없는지 확인). 확인: 현재 상태에서는 통과한다. `bt-core`에 `tokio` 의존을 잠시 추가하면 실패하는지 확인한 뒤 되돌린다.
- [x] 4.4 `.github/workflows/ci.yml`를 추가한다. 트리거는 push(main)와 pull_request, matrix는 ubuntu/windows(`fail-fast: false`)이고, stable 툴체인, rust-cache, `cargo xtask ci`를 실행한다. 확인: 워크플로 YAML이 design D6의 단계와 일치하는지 검토한다.
- [x] 4.5 README에 개발 명령(`cargo xtask ci`)과 workspace 구조를 짧게 적는다. 확인: README에 적힌 명령을 그대로 실행하면 통과한다.

## 5. 통합 확인

- [x] 5.1 Windows 로컬에서 `cargo xtask ci`를 처음부터 끝까지 실행한다. 확인: 종료 코드 0.
- [x] 5.2 사용자가 GitHub 저장소를 만들고 push한 뒤, Actions의 ubuntu와 windows job이 모두 성공하는지 확인한다(M0 완료 기준).

## Workflow follow-up

- M0 완료 기준(CI에서 build, test, clippy 통과)을 확인한 뒤 change를 archive한다.
- archive하면 `torrent-identity`, `piece-file-indices`, `build-pipeline` spec이 `openspec/specs/`에 생긴다.
