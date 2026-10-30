# 다운로드 기능 구현 계획

> 작성일: 2026-10-14
> 참고 문서: [qBittorrent 파일 다운로드 동작 분석](qBittorrent/file-download-flow.md)
>
> 이 계획은 BitTorrent 엔진을 Rust로 직접 구현하는 것을 전제로 한다.
> librqbit 같은 기존 엔진 크레이트를 쓰기로 하면 M1~M5는 거의 없어지고, M6부터 시작하면 된다.

## 나누는 원칙
- **아래 계층부터 쌓는다.** 각 마일스톤이 끝날 때마다 직접 실행해서 확인할 수 있는 결과가 나오게 한다.
- **작업 단위는 PR 하나 크기(1~3일)로 잡는다.** 각 작업에는 테스트나 실행으로 확인할 완료 기준을 붙인다.
- **qBittorrent 구조를 그대로 따른다.** 엔진(qBittorrent의 libtorrent 역할)과 앱 로직(`SessionImpl`/`AddTorrentManager` 역할)을 나누고, 둘은 이벤트 채널로만 통신한다.
- **상호운용 테스트에는 로컬에 설치된 qBittorrent를 쓴다.** 시드나 피어 역할을 맡겨서 실제 클라이언트와 통신이 맞는지 확인한다.

## 크레이트 구조
처음에는 단일 크레이트 안의 모듈로 제안했지만, M0에서 workspace로 바꿨다. 경계는 `cargo xtask ci`가 검사한다.
```
crates/
  bt-core/       # 공용 타입: InfoHash, TorrentId, PieceIndex, FileIndex          (M0)
  bt-metainfo/   # bencode, .torrent, magnet, TorrentDescriptor                  (M1)
  bt-engine/     # 트래커, 피어, 저장소, 피스 선택, 다운로드 루프 (libtorrent 역할) (M2~M5, M9)
  bt-session/    # 상태 머신, 큐잉, 경로, resume, 추가 계층 (SessionImpl/AddTorrentManager 역할) (M6~M8, M10)
  bitturbulent/  # CLI
```
의존 방향은 `bitturbulent → bt-session → bt-engine → bt-metainfo → bt-core` 한 방향이다.
`bt-core`와 `bt-metainfo`는 비동기 런타임에 의존하지 않는다.

## 마일스톤

### M0. 기반
- 비동기 런타임(tokio), 에러 타입, 로깅(tracing)을 정한다.
- 공용 타입을 정의한다: `InfoHash`, `TorrentId`, `PieceIndex`, `FileIndex`.
- **완료 기준:** CI에서 빌드, `cargo test`, clippy가 통과한다.

### M1. 메타데이터
1. bencode 파서와 인코더를 만든다. 바이트 단위로 원본을 보존해 info-hash 계산에 쓴다.
2. .torrent(v1) 파싱: 단일/다중 파일, piece length, piece 해시, announce-list, private 플래그를 읽는다.
3. info-hash(SHA-1)를 계산한다.
4. magnet URI 파싱: `xt`, `dn`, `tr`, `ws`를 읽고 `TorrentDescriptor`를 만든다. 메타데이터는 있을 수도, 없을 수도 있다.
- **완료 기준:** 실제 .torrent 여러 개의 info-hash가 qBittorrent가 표시하는 값과 같다.

### M2. 트래커
1. HTTP 트래커 announce(compact 응답 지원)를 구현한다.
2. UDP 트래커(BEP 15)를 구현한다.
3. tier 순서대로 다시 시도하고, 주기적으로 재announce한다.
4. 지원하지 않는 스킴(http, https, udp 외)과 잘못된 URL은 건너뛴다. M1의 `TorrentDescriptor`는 트래커를 거르지 않고 그대로 넘긴다(qBittorrent, libtorrent와 의도적으로 다름).
- **완료 기준:** 공개 토렌트로 피어 목록을 받아 온다.

### M3. 피어 와이어 프로토콜
1. 핸드셰이크를 하고 메시지 코덱을 만든다(`tokio_util::codec`).
2. 연결 상태 관리: choke/unchoke, interested, bitfield, have.
3. 블록 요청과 수신: 16KiB 단위로 받고, 요청 파이프라이닝을 한다.
- **완료 기준:** 로컬 qBittorrent 시드 하나에서 피스 1개를 받아 SHA-1 검증까지 통과한다.

### M4. 저장소
1. 파일↔피스 매핑을 만든다. 피스가 파일 경계를 넘는 경우와 길이 0인 파일을 처리한다.
2. sparse 방식으로 디스크에 쓴다. 디스크 I/O는 별도 스레드 풀(`spawn_blocking`)에서 한다.
3. 피스 해시를 검증한다. 실패하면 해당 피스를 버리고 보낸 피어에 실패를 기록한다.
4. 기존 파일 검사(checking): 재시작했을 때 이미 받은 피스를 찾아낸다.
5. M1의 "검증 전 경로"(`RawPath`)를 OS별로 안전한 디스크 경로로 바꾸는 정리 함수를 만든다. `..`, 절대 경로, 금지 문자, Windows 예약 이름을 처리한다. 패딩 파일(`FileEntry::is_pad`)은 디스크에 만들지 않는다.
- **완료 기준:** 다중 파일 토렌트에서 피스를 쓰고, 다시 읽어 검증한 결과가 일치한다.

### M5. 첫 end-to-end 다운로드
1. 피스 선택: rarest-first, 엔드게임 모드, 순차 다운로드 옵션.
2. 파일 우선순위: Ignored로 지정한 파일은 받지 않는다.
3. 여러 피어를 동시에 관리한다: 연결 수 제한, 끊긴 피어 정리.
4. 최소한의 업로드: 다른 피어의 요청에 응답한다. 이게 없으면 상대가 choke를 풀어 주지 않는다.
5. CLI를 만든다: `bitturbulent add <file.torrent> --save <dir>`.
- **완료 기준:** 실제 공개 토렌트(리눅스 ISO 등)를 끝까지 받고, 해시가 일치한다.

### M6. 세션과 상태 머신 (분석 문서 4~5절)
1. `Session`/`Torrent` 상태를 정의한다. 분석 문서 5.2절의 상태 판정 우선순위를 그대로 옮긴다.
2. 엔진→세션 이벤트 채널을 만든다(alert 대응): added, state_update, file_completed, finished, error.
3. 엔진 상태를 주기적으로 스냅샷해 진행률과 속도를 계산한다.
4. start/stop과 Forced/AutoManaged 모드를 구현한다.
5. 큐잉: 동시에 다운로드할 토렌트 수를 제한한다.
6. Stop condition: MetadataReceived, FilesChecked 시점에 자동으로 멈춘다.
- **완료 기준:** 토렌트 여러 개를 동시에 넣었을 때 큐 제한대로 상태가 바뀐다.

### M7. 경로 관리 (분석 문서 7절)
1. `savePath`와 `downloadPath`(미완료 경로)를 나눈다.
2. 미완료 파일에 `.!qB` 같은 확장자를 붙이고, 파일 하나가 끝나면 바로 이름을 바꾼다.
3. 토렌트가 끝나면 저장 경로로 옮긴다. 이동은 한 번에 하나씩 순서대로 처리한다.
4. FileSearcher처럼 동작하는 탐색: 원래 이름과 `.!qB` 이름을 모두 찾고, savePath를 본 다음 downloadPath를 본다.
5. 콘텐츠 레이아웃(Original, Subfolder, NoSubfolder)을 지원한다.
- **완료 기준:** 다운로드 도중 프로그램을 종료하고 다시 시작해도 이어받고, 완료되면 파일이 저장 경로로 옮겨진다.

### M8. Resume data (분석 문서 9절)
1. 토렌트별 상태(받은 피스, 경로, 옵션)를 직렬화한다. 쓰는 방식은 임시 파일에 쓴 뒤 rename하는 원자적 쓰기다.
2. 상태가 바뀌면 저장을 미뤘다가(debounce) 모아서 쓴다. 종료할 때 한 번에 flush한다.
3. 시작할 때 복원하고, 저장된 상태를 검사(fast resume)한다. 맞지 않으면 전체를 다시 검사한다.
- **완료 기준:** 강제 종료한 뒤 다시 시작해도 진행률이 유지된다.

### M9. 매직 링크 (분석 문서 6절)
1. 확장 핸드셰이크(BEP 10)를 지원한다.
2. ut_metadata(BEP 9)로 메타데이터를 받고 info-hash로 검증한다.
3. 메타데이터를 받은 뒤 파일 경로와 우선순위를 확정하고, 토렌트를 다시 초기화한다(qBittorrent의 `reload()`에 해당).
4. DHT(BEP 5)를 구현한다. 트래커가 없는 매직 링크에는 꼭 필요하고 분량이 커서, 따로 2~3개 작업으로 나눈다.
5. 미리보기용 메타데이터만 받는 모드: 임시 경로를 쓰고 디스크에 데이터를 쓰지 않는다.
- **완료 기준:** 트래커가 없는 매직 링크로 다운로드를 끝까지 마친다.

### M10. 추가 진입 계층 (분석 문서 2~3절)
1. 소스 해석: 로컬 파일, `file://`, magnet, HTTP URL을 구분한다. URL은 reqwest로 받고, 크기 제한과 magnet 리다이렉트를 처리한다.
2. 중복 검사: 같은 info-hash가 이미 있으면 트래커와 웹시드를 병합한다. private 토렌트는 병합하지 않는다.
3. 전역 "추가 트래커" 설정과 카테고리/태그 기반 경로 결정(AutoTMM)을 넣는다.
- **완료 기준:** 세 종류의 소스를 같은 API로 추가할 수 있다.

### M11 이후 (확장)
- 프로토콜 확장: BitTorrent v2/하이브리드, uTP, PEX, 프로토콜 암호화(MSE), 웹시드(BEP 19).
- 운영 기능: 공유 비율과 시딩 시간 제한, 대역폭 제한, IP 필터.
- 사용자 인터페이스: UI 또는 Web API.

## 순서 의존성
```
M0 → M1 → M2 ─┐
          M3 ─┼→ M4 → M5 → M6 → M7 → M8 → M10
              └────────────────→ M9 (M3 + M6 이후)
```
M2(트래커)와 M3(와이어 프로토콜)은 서로 의존하지 않아 동시에 진행할 수 있다. M5가 끝나면 첫 다운로드 결과가 나온다. 이 지점에서 구조를 한 번 점검하고 넘어간다.
