# Design

## Context

- 저장소에는 `src/main.rs`(Hello world) 하나와 의존성 없는 `Cargo.toml`(edition 2024)만 있다. git remote는 아직 없다.
- 개발 환경은 Windows이고, 툴체인은 rustc 1.99 stable이다.
- 동기는 proposal.md의 Why를 보면 된다. 요구사항은 `specs/torrent-identity`, `specs/piece-file-indices`, `specs/build-pipeline`에 있다.

## Goals / Non-Goals

**Goals:**
- M1 이후 작업이 새 크레이트나 새 의존 관계를 고민하지 않고 정해진 자리에 코드를 넣을 수 있게 한다.
- `bt-core` 공개 타입의 모양을 v2/하이브리드까지 고려해 고정한다.

**Non-Goals:**
- SHA-1/SHA-256 해시 계산. 이건 M1(metainfo)에서 한다. `bt-core`는 바이트 배열만 다룬다.
- serde 직렬화. resume data(M8)에서 필요할 때 feature로 추가한다.
- 엔진/세션의 실제 API와 이벤트 채널 타입. 이건 M5~M6에서 정한다.
- MSRV 고정, crates.io 배포 준비.

## Decisions

### D1. 디렉터리 구조와 virtual manifest

```
Cargo.toml              # [workspace] only (virtual manifest)
crates/
  bt-core/              # lib: 식별자, 인덱스
  bt-engine/            # lib: 빈 크레이트 (deps: bt-core)
  bt-session/           # lib: 빈 크레이트 (deps: bt-engine, bt-core)
  bitturbulent/         # bin: CLI (deps: bt-session, bt-engine, bt-core)
xtask/                  # bin: 품질 게이트 실행기 (publish = false)
.cargo/config.toml      # alias: xtask = "run -p xtask --"
.github/workflows/ci.yml
```

- 루트를 virtual manifest로 둔다. 그러면 `cargo build`/`test`가 기본으로 workspace 전체를 대상으로 하고, 루트에 섞인 패키지가 없다.
- `[workspace.package]`로 edition 2024와 license 등을 공유하고, `[workspace.dependencies]`로 외부 크레이트 버전을 한곳에서 관리한다.
- `resolver = "3"`을 쓴다(edition 2024 기본값).
- **의존 방향 강제.** `bt-session → bt-engine` 의존이 이미 있으므로, `bt-engine`이 `bt-session`에 의존하면 Cargo가 순환 의존으로 빌드를 거부한다. 별도 검사 없이 spec의 "엔진→세션 금지"가 지켜진다. 그 밖의 경계(`bt-core`에 workspace 내부 의존 없음, tokio 없음)는 D5의 경계 검사가 맡는다.
- 대안으로 단일 크레이트와 모듈 경계 조합을 검토했다. 하지만 경계를 컴파일러가 강제하지 못해서 탐색 단계에서 기각했다.

### D2. 식별 타입의 모양 (`bt-core`)

```rust
pub struct Sha1Hash([u8; 20]);
pub struct Sha256Hash([u8; 32]);

pub struct InfoHash { v1: Option<Sha1Hash>, v2: Option<Sha256Hash> }
// 생성자: InfoHash::from_v1(h), InfoHash::from_v2(h), InfoHash::hybrid(h1, h2)
// 접근자: v1() -> Option<Sha1Hash>, v2() -> Option<Sha256Hash>
// 필드는 비공개 -> "둘 다 None" 상태를 만들 수 없음

pub struct TorrentId([u8; 20]);
// InfoHash::torrent_id(): v2가 있으면 v2[..20], 없으면 v1
```

- 필드를 비공개로 두고 생성자를 세 개만 열어서, "최소 하나는 있음"을 타입 수준에서 보장한다. 런타임 오류 경로가 생기지 않는다.
  - 대안 1: `enum InfoHash { V1(..), V2(..), Hybrid(..) }`. 이것도 같은 보장을 준다. 하지만 `v1()`/`v2()` 접근자가 매번 match를 해야 하고, qBittorrent 코드와 1:1로 대응시키기도 어려워서 택하지 않았다.
  - 대안 2: v1 전용 `[u8; 20]`. 탐색 단계에서 기각했다(v2가 들어올 때 전 계층의 시그니처가 바뀜).
- `TorrentId`를 `Sha1Hash`의 별칭으로 두지 않고 별도 타입으로 만든다. 같은 20바이트라도 "v1 해시"와 "v2를 줄인 ID"는 의미가 다르다. M10 중복 검사에서 둘을 섞어 비교하는 실수를 막기 위해서다.
- derive 목록은 `Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord`이고, 바이트 사전순으로 비교한다. `Debug`는 16진수로 직접 구현한다. 로그에 바이트 배열이 그대로 찍히지 않게 하려는 것이다.
- `from_bytes`/`as_bytes`, `From<[u8; N]>`를 둔다. M1에서 계산한 digest를 바로 감쌀 수 있게 하기 위해서다.

### D3. 16진수 표기는 직접 구현한다

- 소문자 출력(`Display`)과 대소문자 무시 파싱(`FromStr`)이면 충분하다. 구현은 수십 줄 정도라서 `hex` 크레이트를 쓰지 않는다. 그래서 `bt-core`의 외부 의존은 `thiserror` 하나뿐이다.
- 파싱 오류는 `ParseHashError` 하나로 묶는다. 경우는 `InvalidLength { expected, actual }`와 `InvalidChar { index }` 두 가지다.
- 세 해시 타입의 구현이 겹치므로 내부 매크로나 const-generic 헬퍼(`fn decode<const N: usize>`)로 한 번만 작성한다.

### D4. 인덱스 newtype (`bt-core`)

```rust
pub struct PieceIndex(u32);
pub struct FileIndex(u32);
```

- `From<u32>`, `TryFrom<usize>`(범위를 넘으면 오류), `get() -> u32`, `as_usize() -> usize`를 제공한다.
- usize 변환이 손실 없다는 전제를 위해 `usize`가 32비트 이상이라고 가정한다. 이 가정은 `const _: () = assert!(usize::BITS >= 32);`로 컴파일 시점에 확인한다.
- 혼용 거부(spec)는 `compile_fail` doctest로 검증한다. trybuild 같은 별도 도구가 필요 없다.
- `FileIndex`를 `usize`가 아닌 `u32`로 둔 이유: libtorrent의 `file_index_t`가 32비트이고, 피스와 표현을 맞추면 resume data 직렬화가 단순해진다.

### D5. 품질 게이트는 `cargo xtask ci`로 한다

`cargo xtask ci`는 다음 단계를 차례로 실행하고, 하나라도 실패하면 거기서 중단한다.
1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`
4. 경계 검사: `cargo metadata`로 의존 그래프를 읽어 두 가지를 확인한다. (a) 라이브러리 크레이트의 workspace 내부 직접 의존이 허용 목록(`bt-core`: 없음, `bt-engine`: core, `bt-session`: core·engine) 안에 있는지. (b) `bt-core`의 비-dev 의존 트리 전체에 비동기 런타임(`tokio`, `async-std`, `smol`)이 없는지. 의존 그래프를 다루므로 `cargo_metadata` 크레이트를 xtask에서만 사용한다.

- xtask를 고른 이유: spec이 "Rust 툴체인만으로 Windows와 Linux에서 같은 명령"을 요구한다. `just`는 별도 설치가 필요하다. 셸 스크립트를 쓰면 PowerShell과 bash 두 벌을 관리해야 한다. Cargo alias(`.cargo/config.toml`)는 여러 명령을 이어서 실행하지 못한다.
- lint 수준은 `[workspace.lints]`에 모아 두고, 각 크레이트는 `[lints] workspace = true`로 이어받는다. 처음에는 clippy 기본 그룹에 `-D warnings`만 적용한다. `pedantic`은 노이즈가 많아서 넣지 않는다. 필요해지면 개별 lint를 추가한다.

### D6. CI (GitHub Actions)

- 트리거는 `push`(main)와 `pull_request`다.
- matrix는 `ubuntu-latest`, `windows-latest`이고, `fail-fast: false`로 두어 두 OS의 결과를 모두 본다.
- 단계는 checkout → `dtolnay/rust-toolchain@stable`(components: rustfmt, clippy) → `Swatinem/rust-cache` → `cargo xtask ci`다.
- 로컬과 CI가 같은 진입점(`cargo xtask ci`)을 쓰므로, 검사 항목을 바꿀 때는 xtask만 고치면 된다.

### D7. 런타임, 에러, 로깅 방침

| 영역 | 라이브러리 크레이트 | CLI 바이너리 |
|---|---|---|
| 비동기 | `tokio`: 필요한 feature만 켬(M0에서는 `bt-core`가 사용하지 않음) | `tokio` `full`, `#[tokio::main]` |
| 에러 | `thiserror`로 크레이트/모듈별 `enum Error` | `anyhow::Result`를 main에서 반환 |
| 로깅 | `tracing` 매크로만 사용, subscriber 초기화 금지 | `tracing-subscriber`(fmt + EnvFilter), 기본 `info` |

- M0에서 `bt-engine`과 `bt-session`은 의존성 선언만 하고 코드는 없다. tokio feature는 실제로 쓰는 시점(M2/M3)에 켠다.
- 라이브러리에서 subscriber를 초기화하지 않는 이유: 테스트나 다른 바이너리가 각자 subscriber를 정할 수 있어야 하기 때문이다.

## Risks / Trade-offs

- [workspace와 xtask로 초기 파일이 늘어남] → 크레이트 4개와 xtask 1개 수준이라 관리 비용은 작다. 나중에 쪼개는 비용보다 싸다고 판단했다.
- [빈 크레이트(`bt-engine`, `bt-session`)를 미리 만들면 모양이 굳을 수 있음] → M0에서는 의존 방향만 고정하고, 공개 API는 만들지 않는다.
- [`TorrentId` 유도 규칙이 qBittorrent와 어긋날 위험] → v2 앞 20바이트 규칙을 단위 테스트로 고정한다. M1에서 실제 하이브리드 .torrent로 qBittorrent 화면 값과 대조한다.
- [GitHub 저장소가 아직 없어서 CI 동작을 바로 확인할 수 없음] → 로컬 `cargo xtask ci`로 같은 검사를 먼저 확인한다. 저장소를 만들고 push한 뒤 Actions 결과로 최종 확인한다(tasks 5.x).
- [MSRV를 정하지 않음] → CI는 stable만 사용한다. 실질적인 하한은 edition 2024의 1.85다. 공개 배포 시점에 다시 정한다.

## Open Questions

- 크레이트 이름 접두사(`bt-` 또는 `bitturbulent-`): 디렉터리와 패키지 이름만 바뀌고 구조와 spec은 같아서, 공개 배포 전까지 미뤄도 된다.
