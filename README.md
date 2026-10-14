# bitturbulent

BitTorrent 클라이언트 Rust 구현체(목표). 구조는 qBittorrent 참고.

## Workspace 구조

```
crates/
  bt-core/       공용 타입 (InfoHash, TorrentId, PieceIndex, FileIndex)
  bt-engine/     BitTorrent 엔진 (libtorrent 역할)
  bt-session/    상태 머신, 큐잉, 경로, resume (SessionImpl 역할)
  bitturbulent/  CLI 바이너리
xtask/           개발용 명령 (품질 게이트)
```

의존 방향은 `bitturbulent → bt-session → bt-engine → bt-core` 한 방향이다.
`bt-core`는 비동기 런타임에 의존하지 않는다. 이 규칙은 `cargo xtask ci`가 검사한다.

## 개발

Rust stable 툴체인(rustfmt, clippy 포함)만 있으면 된다. 모든 명령은 저장소 루트에서 실행한다.

### 빌드

```bash
cargo build            # 개발용 빌드 -> target/debug/bitturbulent(.exe)
cargo build --release  # 최적화 빌드 -> target/release/bitturbulent(.exe)
cargo check            # 실행 파일 없이 컴파일 오류만 빠르게 확인
```

기본 대상은 `crates/` 아래 크레이트다. `xtask`까지 포함하려면 `--workspace`를 붙인다.

### 실행

```bash
cargo run -p bitturbulent
```

로그 수준은 `RUST_LOG`로 바꾼다(기본 `info`).

```bash
RUST_LOG=debug cargo run -p bitturbulent        # bash
$env:RUST_LOG="debug"; cargo run -p bitturbulent  # PowerShell
```

### 테스트

```bash
cargo test --workspace             # 전체 (단위 테스트 + doctest)
cargo test -p bt-core              # 크레이트 하나
cargo test -p bt-core torrent_id   # 이름에 torrent_id가 들어간 테스트만
cargo test -p bt-core --doc        # doctest만
cargo test -p bt-core -- --nocapture  # 테스트 안의 출력(println!)도 표시
```

### 커밋 전 검사

```bash
cargo xtask ci
```

포맷 검사, clippy(경고를 오류로 취급), 전체 테스트, 크레이트 경계 검사를 차례로 실행한다.
하나라도 실패하면 멈춘다. GitHub Actions도 Windows와 Linux에서 같은 명령을 실행한다.

포맷 검사가 실패하면 `cargo fmt --all`로 자동 정리한다.
