# CLAUDE.md

## qBittorrent 동작 참조

이 프로젝트는 qBittorrent 구조를 따른다. qBittorrent가 어떻게 동작하는지 확인해야 할 때는 아래 순서로 찾는다.

1. **먼저 `docs/qBittorrent/`의 분석 문서를 본다.** 다운로드 흐름, 상태 판정, 경로 관리, resume data 등은 여기에 정리되어 있다.
2. **문서에 없거나 더 자세한 내용이 필요하면 origin 소스를 본다.**
   - origin: https://github.com/qbittorrent/qBittorrent
   - 분석 기준: `v5_3_x` 브랜치, 커밋 `599833466091488746fa7947b208d3dd48fd5ceb`
   - 분석 문서의 파일 경로와 줄 번호는 이 커밋 기준이다. 다른 커밋을 보면 줄 번호가 맞지 않을 수 있다.
   - 소스에서 새로 알아낸 내용이 계속 쓰일 것 같으면 `docs/qBittorrent/` 문서에 추가한다.

## 검증

- 코드를 바꾼 뒤에는 `cargo xtask ci`가 통과하는지 확인한다(fmt, clippy -D warnings, test, 크레이트 경계 검사).
  `cargo test`만으로는 clippy와 경계 검사가 빠진다.
- 검사 항목을 바꿀 때는 `.github/workflows/ci.yml`이 아니라 `xtask/src/main.rs`를 고친다.
  로컬과 CI는 같은 `cargo xtask ci`를 실행한다.
- Windows에서 `LNK1104: cannot open file ...exe` 링커 오류는 간헐적인 파일 잠금 문제다.
  코드를 고치지 말고 같은 명령을 다시 실행한다.
