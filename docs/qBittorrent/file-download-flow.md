# qBittorrent 파일 다운로드 동작 분석

> 분석 대상: [qbittorrent/qBittorrent](https://github.com/qbittorrent/qBittorrent) `v5_3_x` 브랜치, 커밋 [`599833466`](https://github.com/qbittorrent/qBittorrent/commit/599833466091488746fa7947b208d3dd48fd5ceb) (2026-10-12)
> 작성일: 2026-10-14
>
> 줄 번호는 위 커밋 기준이다.

---

## 1. 전체 구조 요약

qBittorrent는 실제 BitTorrent 프로토콜 처리(피어 연결, 피스 요청, 해시 검증, 디스크 I/O)를 전부 **libtorrent-rasterbar**에 맡긴다.
qBittorrent 자체 코드는 다음 일만 한다.

1. **입력 정규화**: 파일 경로, 매직 링크, HTTP URL을 `TorrentDescriptor`로 만든다.
2. **파라미터 결정**: 저장 경로, 카테고리, 파일 우선순위, 플래그를 정해 `lt::add_torrent_params`를 만든다.
3. **비동기 추가**: `lt::session::async_add_torrent()`를 호출한다.
4. **alert 루프**: libtorrent가 보낸 alert를 메인(Qt) 스레드에서 받아 `TorrentImpl` 상태를 갱신한다.
5. **후처리**: `.!qB` 확장자를 정리하고, 임시 다운로드 경로에서 저장 경로로 이동하고, 완료 시그널을 보낸다.

```
[입력: GUI/WebUI/RSS/감시폴더/CLI]
        │ source(QString) + AddTorrentParams
        ▼
AddTorrentManager / GUIAddTorrentManager   (src/base/addtorrentmanager.cpp, src/gui/guiaddtorrentmanager.cpp)
        │  ├─ http(s)/ftp URL → Net::DownloadManager로 .torrent 다운로드
        │  ├─ magnet / 해시 문자열 → TorrentDescriptor::parse
        │  └─ 로컬 파일 → TorrentDescriptor::loadFromFile (+ TorrentFileGuard)
        ▼
SessionImpl::addTorrent → addTorrent_impl   (src/base/bittorrent/sessionimpl.cpp)
        │  LoadTorrentParams + lt::add_torrent_params 구성
        │  FileSearcher(별도 스레드)로 기존/미완료 파일 탐색
        ▼
lt::session::async_add_torrent(p)  ──► libtorrent 내부 스레드에서 다운로드 수행
        │
        ▼ (alert_notify → QueuedConnection)
SessionImpl::readAlerts → handleAlert(switch)
        │  add_torrent_alert      → createTorrent → TorrentImpl 생성
        │  state_update_alert     → TorrentImpl::updateStatus (진행률/상태)
        │  metadata_received_alert→ 매직 링크 메타데이터 처리
        │  file_completed_alert   → .!qB 제거, Mark-of-the-Web
        │  torrent_finished_alert → 저장 경로 이동, 완료 처리
        ▼
emit torrentFinished / allTorrentsFinished
```

---

## 2. 진입점 (입력 소스)

모든 UI 경로는 결국 `AddTorrentManager::addTorrent(source, params)` 또는 `Session::addTorrent(descriptor, params)` 중 하나로 모인다.

| 진입점 | 위치 | 경로 |
|---|---|---|
| 명령줄 / 파일 연결 / 다른 인스턴스에서 전달 | `app/application.cpp:885,888` | `AddTorrentManager::addTorrent` |
| GUI: 파일 열기, 링크 추가, 드래그앤드롭 | `gui/mainwindow.cpp:1200,1362,1540`, `gui/transferlistwidget.cpp:1470` | `GUIAddTorrentManager::addTorrent` |
| RSS 자동 다운로더 | `base/rss/rss_autodownloader.cpp:511` | `AddTorrentManager::addTorrent` |
| 검색 플러그인 | `gui/search/searchjobwidget.cpp:444`, `webui/api/searchcontroller.cpp:222` | `AddTorrentManager::addTorrent` |
| 감시 폴더 | `base/torrentfileswatcher.cpp:312` | `Session::addTorrent` (직접) |
| WebUI `torrents/add` | `webui/api/torrentscontroller.cpp:1237-1330` | URL은 `AddTorrentManager`, 업로드된 .torrent와 메타데이터 캐시 적중은 `Session::addTorrent` |

---

## 3. 1단계: 소스 해석 (`AddTorrentManager`)

`src/base/addtorrentmanager.cpp:54` `AddTorrentManager::addTorrent()`

1. **원격 URL** (`Net::DownloadManager::hasSupportedScheme`, 즉 `QNetworkAccessManager`가 지원하는 스킴)
   - `m_downloadedTorrents[source] = params`에 등록해서 같은 URL을 중복 요청하지 않게 한다.
   - `DownloadRequest(source).limit(getTorrentFileSizeLimit())`로 다운로드한다. 크기 제한을 넘으면 실패한다(`net/downloadhandlerimpl.cpp:198`). gzip 응답은 자동으로 해제한다(`:141`).
   - HTTP 리다이렉트가 `magnet:`으로 가면 `DownloadStatus::RedirectedToMagnet`으로 처리한다(`downloadhandlerimpl.cpp:223-228`).
   - 완료되면 `onDownloadFinished()`가 `TorrentDescriptor::load(data)` 또는 `parse(magnetURI)`를 호출한 뒤 `processTorrent()`로 넘긴다.
2. **매직 링크 / 해시 문자열**: `TorrentDescriptor::parse(source)`가 성공하면 바로 `processTorrent()`로 간다.
3. **로컬 .torrent 파일** (`file://` 지원)
   - `TorrentFileGuard`를 만든다. 설정에 따라 추가에 성공하면 원본 .torrent를 삭제하는 데 쓴다.
   - `TorrentDescriptor::loadFromFile()`로 읽는다.

`processTorrent()` (`:220`)는 같은 info-hash를 가진 토렌트가 세션에 이미 있으면 **중복 처리**(`handleDuplicateTorrent`)를 한다.
- 기존 토렌트에 메타데이터가 없으면 메타데이터를 넣어 준다.
- 공개 토렌트이고 "트래커 병합"이 켜져 있으면 트래커와 웹시드만 병합한다.

중복이 아니면 `Session::addTorrent()`를 호출하고, `m_sourcesByInfoHash`에 info-hash와 source를 매핑해 둔다.
나중에 `torrentAdded`, `addTorrentFailed` 시그널이 오면 이 매핑으로 원래 source를 찾아 시그널을 다시 보낸다.

### `TorrentDescriptor` 생성 세부 (`src/base/bittorrent/torrentdescriptor.cpp`)
- `load(data)` (`:131`), `loadFromFile(path)` (`:146`): libtorrent `load_torrent_buffer`/`load_torrent_file`을 `loadTorrentLimits()` (`:81`)로 호출한다. 트래커는 **거르지 않는다**.
- 제한값은 설정에서 온다. 기본값은 파일 크기 100 MiB(`preferences.cpp:323-326`), bdecode 깊이 100(`:336-339`), 토큰 10,000,000개(`:349-352`)다.
- `parse(str)` (`:163`):
  - 입력이 정확히 64자 16진수면(`isV2Hash`, `:71`) `magnet:?xt=urn:btmh:1220` + 입력으로, 정확히 40자 16진수면(`isV1Hash`, `:56`) `magnet:?xt=urn:btih:` + 입력으로 바꾼다. 앞뒤 공백은 자르지 않는다.
  - libtorrent `parse_magnet_uri`로 해석한 뒤 `removeUnsupportedTrackers` (`:103`, 호출은 `:173`)로 http, https, udp가 아니거나 호스트가 없는 트래커를 버린다(`isSupportedTrackerURL`, `:93`). 이 필터는 **parse 경로에만** 있다.
  - libtorrent 자체도 magnet의 트래커를 거른다(6.3절).
- GUI 토렌트 생성기의 형식 선택(V1/V2/하이브리드)은 libtorrent 2.x 빌드에서만 보인다. 1.2 빌드에서는 대신 "정렬 최적화"(패딩 파일) 옵션이 보인다(`src/gui/torrentcreatordialog.cpp:149-153`). 1.2 빌드 qBittorrent로는 v2와 하이브리드 토렌트를 만들 수도, v2 info-hash를 확인할 수도 없다.

### GUI 변형 (`GUIAddTorrentManager`)

`src/gui/guiaddtorrentmanager.cpp:96`
- `SkipDialog` 옵션이 있거나 "새 토렌트 추가 대화상자"를 끈 경우: 기본 `AddTorrentManager` 경로를 탄다.
- 그 외에는 `processTorrent()` (`:192`)에서 `AddNewTorrentDialog`를 띄운다.
  - **매직 링크라 메타데이터가 없으면** `Session::downloadMetadata()`로 메타데이터만 미리 받는다(`:251`). 받으면 `metadataDownloaded` 시그널로 대화상자의 파일 목록을 채운다(`:179`).
  - 사용자가 확인하면 `torrentAccepted`가 발생하고 `addTorrentToSession()`이 호출된다.
  - info-hash 하나당 대화상자를 하나만 연다(`m_dialogs`).

---

## 4. 2단계: 세션에 추가 (`SessionImpl::addTorrent_impl`)

`src/base/bittorrent/sessionimpl.cpp:2887`

### 4.1 사전 처리
- 세션 복원(resume data 로딩)이 끝나기 전에는 추가를 거부한다(`addTorrent`, `:2819`).
- 중복 검사를 다시 한다(`:2898`). WebUI와 감시 폴더는 `AddTorrentManager`를 거치지 않기 때문이다.
- 이전에 `downloadMetadata()`로 띄워 둔 메타데이터 전용 핸들이 있으면 **취소**하고 새로 추가한다(`cancelDownloadMetadata`, `:2942`). 사용자가 지정한 파라미터와 섞이지 않게 하려는 의도다(주석 참고).

### 4.2 `initLoadTorrentParams` (`:2825`): qBittorrent 측 파라미터
| 항목 | 결정 규칙 |
|---|---|
| `stopped` | `addStopped` 값, 없으면 전역 "정지 상태로 추가" 설정 |
| `operatingMode` | `addForced`이면 `Forced`, 아니면 `AutoManaged`(큐잉 적용) |
| `stopCondition` | None / MetadataReceived / FilesChecked |
| `category` | 없는 카테고리면 생성을 시도하고, 실패하면 빈 값 |
| `useAutoTMM` | 명시값이 없으면 `savePath`, `downloadPath`가 모두 비었고 AutoTMM 기본 비활성화가 아닐 때 true |
| `savePath` | 절대 경로면 그대로 쓰고, 상대 경로면 카테고리 기본 경로에 결합 |
| `downloadPath` | "미완료 파일 경로". `useDownloadPath`가 켜졌을 때만 설정 |
| `tags` | 없는 태그는 생성 |

### 4.3 `lt::add_torrent_params` 구성
- **메타데이터가 있는 경우** (.torrent)
  - `stopCondition == MetadataReceived`이면 바로 `stopped = true`로 바꾼다(`:2960`).
  - **콘텐츠 레이아웃**(Original / Subfolder / NoSubfolder)에 맞춰 파일 경로의 루트 폴더를 붙이거나 뗀다(`:2974-2986`).
  - 이름을 따로 지정하지 않았으면 루트 폴더 이름이나 단일 파일 이름을 토렌트 이름으로 쓴다.
  - **파일 우선순위**: 사용자가 지정하지 않았고 "제외 파일 이름" 기능이 켜져 있으면 `applyFilenameFilter`로 블랙리스트 파일을 `Ignored`로 설정한다. 기본값은 libtorrent 기본값(4)이 아니라 qBittorrent의 `Normal`이다(`:3022-3033`). `.pad` 파일까지 포함한 내부 인덱스(`nativeIndexes`)로 매핑한다.
  - seed mode가 아니면 `needFindIncompleteFiles = true`로 둔다.
- **메타데이터가 없는 경우** (매직 링크): 이름만 `p.name`(dn 파라미터)에서 가져온다.
- **추가 트래커**: 전역 "추가 트래커"와 "URL에서 가져온 트래커"를 기존 tier 최댓값 + 1 tier부터 덧붙인다. **private 토렌트는 제외**한다(`:3043-3071`).
- **플래그** (`:3073-3106`)
  - `storage_mode`: 사전 할당 설정이면 `allocate`, 아니면 `sparse`
  - `sequential_download`, `seed_mode` (seed mode는 해시 검사를 건너뛰고 바로 시딩)
  - `paused`: stopped이거나 AutoManaged이면 설정한다. AutoManaged는 libtorrent 큐 관리자가 시작 시점을 정한다.
  - `auto_managed`: stopped나 Forced가 아니면 설정
  - `duplicate_is_error`, `added_time`, 토렌트별 최대 연결 수와 업로드 슬롯 수
  - `userdata = new ExtensionData` (`NativeTorrentExtension`과 상태를 공유하는 데 사용)

### 4.4 기존 파일 탐색 → `async_add_torrent`
`findIncompleteFiles()` (`:3179`)는 `FileSearcher`를 **별도 스레드**에서 실행해 `QFuture`를 돌려준다.

`FileSearcher::search` (`filesearcher.cpp`) 동작:
1. `savePath`에서 각 파일이 `<이름>` 또는 `<이름>.!qB` 형태로 있는지 찾는다.
2. 하나도 없고 `downloadPath`가 설정되어 있으면 `downloadPath`에서 다시 찾는다.
3. 파일이 없고 "미완료 파일에 .!qB 붙이기"가 켜져 있으면, 파일 이름에 `.!qB`를 미리 붙여 둔다.
4. 실제로 사용할 디렉터리(`usedPath`)와 보정된 파일 이름 목록을 반환한다.

Future가 끝나면 메인 스레드에서 다음을 실행한다(`:3123-3174`).
- `p.save_path = usedPath`, `p.renamed_files[nativeIndex] = 보정된 파일 이름`
- `m_nativeSession->async_add_torrent(p)`
- `m_addTorrentAlertHandlers`(FIFO 큐)에 콜백을 등록한다. libtorrent는 `add_torrent_alert`를 추가 요청 순서대로 보내기 때문에, 핸들러를 순서대로 하나씩 꺼내 짝을 맞춘다(`handleAddTorrentAlert`, `:6117`).

> 다운로드 중인 파일은 `savePath`가 아니라 `downloadPath`(설정된 경우)에 쓰인다. `FileSearcher`의 2단계 탐색은 재시작이나 중복 추가 때 이미 받아 둔 데이터를 재사용하기 위한 것이다.

### 4.5 `add_torrent_alert` 처리
- **오류**: `duplicate_torrent`이면 `duplicateTorrentDetected`, 그 밖의 오류는 `addTorrentFailed`를 보낸다.
- **성공**
  1. `addToQueueTop`이면 `queue_position_top()`
  2. `createTorrent()` (`:6258`): `new TorrentImpl(...)`을 만들어 `m_torrents`에 등록한다. 하이브리드(v1+v2)면 v1 ID로도 인덱싱하고, 공유 제한이 있으면 seeding limit 타이머를 시작한다.
  3. `requestResumeData(save_info_dict)`: 즉시 fastresume를 저장한다.
  4. `emit torrentAdded`. 설정이 켜져 있으면 .torrent 백업 파일도 만든다.

---

## 5. 3단계: alert 루프와 상태 갱신

### 5.1 alert 펌프
- `initializeNativeSession()`에서 `set_alert_notify`를 등록한다(`sessionimpl.cpp:1870`). libtorrent 스레드가 alert를 만들면 `QueuedConnection`으로 메인 스레드의 `readAlerts()`를 예약한다. **모든 상태 변경은 Qt 메인 스레드에서 직렬로 처리된다.**
- `readAlerts()` (`:6087`)는 `pop_alerts`로 alert를 꺼내 `handleAlert()`의 switch에 넘기고(`:6127`), 마지막에 `processPendingFinishedTorrents()`를 호출한다.
- **주기적 상태 폴링**: `enqueueRefresh()` (`:6009`)가 `refreshInterval()`마다 `post_torrent_updates()`와 `post_session_stats()`를 요청한다. 응답은 `state_update_alert`로 오고, `handleStateUpdateAlert` (`:6712`)가 이를 받아 각 `TorrentImpl::updateStatus()`를 호출한 다음 `torrentsUpdated` 시그널을 보낸다. 마지막에 다시 `enqueueRefresh()`를 호출해 폴링이 계속 이어진다.

### 5.2 `TorrentImpl::updateStatus` / `updateState`
`torrentimpl.cpp:2679`, `:1289`

`lt::torrent_status`를 캐시하고, 진행률(피스 비트맵 차이로 파일별 진행률 계산), 완료 시각, 상태를 갱신한다.
그런 다음 `m_statusUpdatedTriggers` 큐에 쌓인 지연 작업을 실행한다. 아래의 checked/finished 처리가 이 큐를 쓰기 때문에, **최신 status를 받은 뒤에** 실행된다.

상태 판정 우선순위:

```
checking_resume_data → CheckingResumeData
저장소 이동 중        → Moving
파일 누락            → MissingFiles
errc 또는 upload_mode → Error
메타데이터 없음        → Stopped / Queued / (Forced)DownloadingMetadata
checking_files       → CheckingUploading / CheckingDownloading
finished|seeding     → StoppedUploading / QueuedUploading / ForcedUploading / Uploading / StalledUploading
그 외                → StoppedDownloading / QueuedDownloading / ForcedDownloading / Downloading / StalledDownloading
```

"Downloading"과 "Stalled"는 `download_payload_rate > 0`인지로만 구분한다.

### 5.3 `NativeTorrentExtension`: 체크/메타데이터 단계 후 자동 정지
`nativetorrentextension.cpp`의 `on_state`는 이전 상태가 `downloading_metadata` 또는 `checking_files`였으면 토렌트를 `auto_managed` 해제 후 `pause()`한다.
이렇게 해서 qBittorrent가 stop condition을 검사하고 파일 경로를 정리한 **다음에** 다운로드가 다시 시작되도록 한다. 재개는 `handleTorrentChecked`가 맡는다.

---

## 6. 매직 링크: 메타데이터 수신 흐름

### 6.1 세션에 추가된 매직 링크
1. `metadata_received_alert`가 오면 `SessionImpl::handleMetadataReceivedAlert` (`:6337`)를 거쳐 `TorrentImpl::handleMetadataReceived` (`:2523`)가 호출된다.
   - v2 또는 하이브리드면 info-hash가 바뀌었을 수 있으므로 세션 인덱스를 고친다(`handleTorrentInfoHashChanged`).
   - `m_maintenanceJob = HandleMetadata`로 설정하고 resume data 저장을 요청한다.
2. `save_resume_data_alert`가 오면 `handleSaveResumeData` (`:2222`)가 실행된다. 메타데이터(`params.ti`)를 받았으므로:
   - 콘텐츠 레이아웃을 적용해 파일 경로를 계산하고 `m_indexMap`(native 인덱스 → 논리 인덱스)을 만든다.
   - `findIncompleteFiles()`로 기존 파일을 다시 탐색한다.
3. `endReceivedMetadataHandling` (`:1891`)
   - 파일 우선순위 배열 크기를 맞추고 파일 이름 필터를 적용한다.
   - `renamed_files`(`.!qB` 포함)와 `save_path`를 확정한다.
   - stop condition이 `MetadataReceived`이면 여기서 정지한다.
   - **`reload()`**: 기존 libtorrent 핸들을 `remove_torrent`하고, 확정된 `add_torrent_params`로 **다시 추가**한다(`SessionImpl::reloadTorrent`, `:5792`). 큐 위치는 유지한다.
   - first/last piece 우선순위를 적용하고, `metadataReceived` 시그널을 보낸다.

### 6.2 메타데이터만 받는 경우 (GUI 대화상자 미리보기)
`SessionImpl::downloadMetadata` (`:3267`)
- `save_path = <임시폴더>/<torrentID>`
- `paused`와 `auto_managed`를 끄고 강제로 시작한다.
- **`upload_mode` 플래그**를 설정해 실수로 파일을 쓰지 않게 한다.
- `m_downloadedMetadata`에 핸들을 등록한다.
- 메타데이터가 오면 `remove_torrent(delete_files)`로 지우고 `metadataDownloaded(TorrentInfo)` 시그널을 보낸다.

---

### 6.3 magnet URI 파싱 (libtorrent `parse_magnet_uri`)
libtorrent `RC_2_0` 브랜치의 `src/magnet_uri.cpp`와 `src/parse_url.cpp`를 확인한 결과다(줄 번호는 따로 기록하지 않았다).
- 입력은 `magnet:?`으로 시작해야 하고, 접두어 비교는 대소문자를 구분한다. 매개변수 이름은 대소문자를 구분하지 않고, `tr.1`, `xt.2`처럼 `.<숫자>` 접미사를 떼고 비교한다.
- `xt=urn:btih:`는 40자 16진수나 32자 base32, `xt=urn:btmh:`는 `1220`으로 시작하는 SHA-256 multihash만 받는다. 형식이 틀리면 `invalid_info_hash`로 **전체 파싱이 실패**한다. 알 수 없는 `xt` 스킴은 무시한다. 같은 종류의 `xt`가 반복되면 **마지막 값**이 남는다. 유효한 `xt`가 하나도 없으면 `missing_info_hash_in_uri` 오류다.
- `tr`은 등장 순서대로 `tracker_tiers`에 `tier++`(0부터)를 받는다. 즉 **트래커마다 tier가 하나씩** 생긴다.
- `tr` 값은 `is_valid_tracker_url`을 통과해야 추가된다. 이 함수는 http, https, udp 스킴(대소문자 무시)과 ASCII 호스트명(또는 대괄호 IPv6), 숫자 포트만 허용하고, 공백과 제어 문자를 거부한다. 따라서 qBittorrent의 `removeUnsupportedTrackers`(`torrentdescriptor.cpp:103-124`)는 libtorrent가 이미 거른 결과를 한 번 더 거르는 셈이다.
- `dn`은 퍼센트 디코딩해 이름으로 쓰고, `ws`는 디코딩에 성공하면 웹시드로 추가한다.
- bitturbulent는 트래커를 거르지 않는다(M1 design D8). 지원하지 않는 트래커는 M2 트래커 계층에서 건너뛴다.

## 7. 파일 완료 / 토렌트 완료

### 7.1 파일 단위 (`file_completed_alert`)
`TorrentImpl::handleFileCompleted` (`torrentimpl.cpp:2486`)
- `m_completedFiles` 비트를 설정한다.
- **Windows/macOS**: "Mark-of-the-Web" 설정이 켜져 있고 상태가 `downloading`이면 완료된 파일에 MotW(Zone.Identifier)를 붙인다.
- **`.!qB` 확장자 제거**: `isAppendExtensionEnabled()`이면 `doRenameFile` 후 `lt::torrent_handle::rename_file`을 호출한다.

실제 디스크 경로 규칙(`makeActualPath`, `:628`):
- 크기가 0보다 크고 아직 완료되지 않은 파일에는 `.!qB`를 붙인다(`common.h:35`의 `QB_EXT`).
- "건너뛴 파일 전용 폴더" 설정이 켜져 있으면 `Ignored` 파일은 `<부모>/.unwanted/<파일>`에 둔다.

### 7.2 해시 체크 완료 (`torrent_checked_alert`)
`TorrentImpl::handleTorrentChecked` (`:2149`)
- stop condition이 `FilesChecked`이면 `stop()`한다.
- 다음 status 갱신 때: 진행률로 `m_hasFinishedStatus`를 판단하고, `adjustStorageLocation()`과 `manageActualFilePaths()`로 위치와 파일 이름을 정리한다. 정지 상태가 아니면 `NativeTorrentExtension`이 걸어 둔 pause를 풀어 재개한다.

### 7.3 토렌트 완료 (`torrent_finished_alert`)
`TorrentImpl::handleTorrentFinished` (`:2192`). 다음 status 갱신 때 아래를 실행한다.
1. **`adjustStorageLocation()`** (`:2574`): 완료되었으므로 목표 경로가 `downloadPath`에서 `savePath`로 바뀐다. 현재 위치와 다르면 `moveStorage(..., AdjustCurrentLocation)`을 호출한다.
   - `SessionImpl::addMoveTorrentStorageJob`이 **전역 이동 큐**(`m_moveStorageQueue`)에 넣는다. 이동은 한 번에 하나씩 `handle.move_storage()`로 실행되고, 끝나면(`storage_moved_alert`) 다음 작업을 시작한다.
2. `manageActualFilePaths()`: 남은 `.!qB`나 unwanted 경로를 정리한다.
3. "완료 시 재검사" 설정이 켜져 있고 이번 세션에서 실제로 다운로드가 있었으면(`m_unchecked`) `forceRecheck()`한다.
4. 그 밖의 경우 `m_hasFinishedStatus = true`로 설정한다. 이동이나 이름 변경이 진행 중이면 `m_moveFinishedTriggers`에 미뤄 두었다가 끝난 뒤 `SessionImpl::handleTorrentFinished`를 호출한다.

`SessionImpl::handleTorrentFinished` (`sessionimpl.cpp:5648`)
- `m_pendingFinishedTorrents`에 넣는다.
- .torrent 백업을 "완료된 토렌트 백업 폴더"로 옮긴다. 매직 링크였다면 이 시점에 .torrent를 생성한다.

`processPendingFinishedTorrents` (`:5846`, `readAlerts` 끝에서 호출)
- 로그 "Torrent download finished"를 남기고 **`emit torrentFinished`**를 보낸다. 이 시그널이 완료 후 외부 프로그램 실행, 알림, 메일 같은 앱 레벨 처리를 일으킨다.
- 공유 비율과 시딩 시간 제한을 검사한다(`processTorrentShareLimits`).
- 미완료 토렌트가 하나도 없으면 `emit allTorrentsFinished`를 보낸다(완료 후 종료/절전 기능용).

---

## 8. 오류 처리

| 상황 | 처리 |
|---|---|
| .torrent URL 다운로드 실패나 크기 초과 | `AddTorrentManager::handleAddTorrentFailed`가 경고 로그를 남기고 `addTorrentFailed`를 보낸다 |
| `add_torrent_alert` 오류 | `addTorrentFailed` 또는 `duplicateTorrentDetected`. `TorrentFileGuard` 자동 삭제를 취소한다 |
| `file_error_alert` | `TorrentImpl::handleFileError`가 마지막 오류를 저장한다. 같은 토렌트는 일정 시간 안에 한 번만 로그를 남기고 `torrentIOError`를 보낸다(`:6381`). libtorrent는 해당 토렌트를 `upload_mode`로 전환하고, 상태는 `Error`가 된다 |
| 파일 누락 (fastresume 거부) | `MissingFiles` 상태가 된다. 이동이나 `start()` 때 `reload()`로 복구를 시도한다 |
| 재개(`start`) | 오류 상태면 `clear_error()`와 `unset_flags(upload_mode)`를 호출한다 |

---

## 9. 지속성 (resume data)

- 추가 직후, 상태 변경 시(`deferredRequestResumeData`), 종료 시(`saveResumeData`, 최대 30초 대기) `save_resume_data`를 요청한다.
- `handleSaveResumeData`에서 `prepareResumeData`를 거쳐 `SessionImpl::handleTorrentResumeDataReady`로 가고, `m_resumeDataStorage->store()`에 저장한다. 저장소 구현은 `BencodeResumeDataStorage`(fastresume 파일) 또는 `DBResumeDataStorage`(SQLite) 중 하나다.
- 다음 실행 때 resume data로 복원하면 `checking_resume_data` 상태를 거쳐 이어받기를 한다.

---

## 10. bitturbulent 구현 시 참고할 설계 포인트

1. **엔진과 앱 로직 분리**: 프로토콜과 디스크 I/O는 엔진(libtorrent)이 맡고, 앱은 파라미터 결정, 이벤트 처리, 경로 관리만 한다. Rust에서는 엔진 스레드와 이벤트 채널(mpsc)을 두고, 앱 상태는 단일 태스크에서 직렬로 갱신하는 구조가 이에 해당한다.
2. **요청-응답 짝 맞추기**: `async_add_torrent`와 FIFO 핸들러 큐 패턴. Rust라면 oneshot 채널을 쓰는 편이 더 안전하다.
3. **두 가지 경로**: `downloadPath`(미완료)와 `savePath`(완료). 완료되면 이동 작업을 직렬 큐로 처리한다.
4. **미완료 표시**: `.!qB` 확장자로 표시하고, 파일 단위로 완료되는 즉시 이름을 바꾼다. 재시작 때는 두 형태(원래 이름, `.!qB` 이름)를 모두 탐색한다.
5. **매직 링크 2단계 처리**: 메타데이터를 받은 뒤 파라미터를 확정하고 핸들을 다시 추가(reload)한다. 미리보기용 메타데이터 다운로드는 `upload_mode`와 임시 경로로 디스크 쓰기를 막는다.
6. **Stop condition**: MetadataReceived와 FilesChecked 단계에서 엔진을 자동으로 pause해 두고, 앱이 정책을 판단한 뒤 재개한다.
7. **중복 처리**: info-hash(v1/v2/하이브리드) 기준으로 판단하고, 공개 토렌트면 트래커와 웹시드만 병합한다.
8. **폴링 + 이벤트 혼합**: 진행률은 주기적 `post_torrent_updates`로, 상태 전이(완료, 오류, 메타데이터 수신)는 개별 alert로 처리한다.
