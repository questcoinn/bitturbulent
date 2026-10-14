# torrent-identity Specification

## Purpose

토렌트를 식별하는 해시 값(v1 SHA-1, v2 SHA-256 info-hash와 그로부터 유도한 TorrentId)을 일관된 규칙으로 표현하고 변환한다. 이를 통해 엔진, 세션, 외부 클라이언트(qBittorrent)가 같은 토렌트를 같은 값으로 가리키게 한다.

## Requirements

### Requirement: InfoHash는 v1, v2 중 최소 하나를 가진다
InfoHash SHALL 20바이트 v1(SHA-1) 해시, 32바이트 v2(SHA-256) 해시, 또는 둘 다를 가진다. 둘 다 없는 InfoHash는 만들 수 없어야 MUST 한다.

#### Scenario: v1 전용 토렌트
- **WHEN** v1 해시만으로 InfoHash를 만든다
- **THEN** v1 해시는 그 값을 반환하고, v2 해시는 없음으로 보고된다

#### Scenario: v2 전용 토렌트
- **WHEN** v2 해시만으로 InfoHash를 만든다
- **THEN** v2 해시는 그 값을 반환하고, v1 해시는 없음으로 보고된다

#### Scenario: 하이브리드 토렌트
- **WHEN** v1과 v2 해시를 모두 주고 InfoHash를 만든다
- **THEN** 두 해시가 모두 원래 값 그대로 반환된다

#### Scenario: 해시가 하나도 없는 경우
- **WHEN** v1과 v2 해시를 모두 주지 않고 InfoHash를 만들려고 한다
- **THEN** 만들어지지 않는다(컴파일 단계에서 막히거나 오류가 반환된다)

### Requirement: TorrentId는 InfoHash에서 결정적으로 유도된다
TorrentId SHALL 20바이트 값이다. v2 해시가 있으면 v2 해시의 앞 20바이트를 쓰고, 없으면 v1 해시를 그대로 쓴다. 이 규칙은 qBittorrent(libtorrent `get_best()`)가 보여 주는 ID와 같아야 MUST 한다.

#### Scenario: v1 전용에서 유도
- **WHEN** v1 해시 `H1`만 가진 InfoHash에서 TorrentId를 얻는다
- **THEN** TorrentId의 바이트는 `H1`과 같다

#### Scenario: v2를 가진 경우 v2 우선
- **WHEN** v1 해시 `H1`과 v2 해시 `H2`를 모두 가진 InfoHash에서 TorrentId를 얻는다
- **THEN** TorrentId의 바이트는 `H2`의 앞 20바이트와 같고, `H1`과는 무관하다

#### Scenario: 같은 InfoHash에서는 항상 같은 TorrentId
- **WHEN** 같은 InfoHash에서 TorrentId를 두 번 얻는다
- **THEN** 두 TorrentId는 같다

### Requirement: 해시 값은 소문자 16진수로 표시된다
SHA-1 해시와 TorrentId SHALL 40자, SHA-256 해시 SHALL 64자의 소문자 16진수 문자열로 표시된다. qBittorrent 화면에 나오는 값과 바로 비교할 수 있어야 한다.

#### Scenario: SHA-1 해시 표시
- **WHEN** 바이트가 `0x00, 0x01, ..., 0x13`인 SHA-1 해시를 문자열로 표시한다
- **THEN** 결과는 `000102030405060708090a0b0c0d0e0f10111213`이다

#### Scenario: SHA-256 해시 표시 길이
- **WHEN** 임의의 SHA-256 해시를 문자열로 표시한다
- **THEN** 결과는 64자이고, `0-9a-f` 문자만 들어 있다

### Requirement: 16진수 문자열에서 해시 값을 파싱한다
SHA-1 해시, SHA-256 해시, TorrentId SHALL 16진수 문자열에서 파싱된다. 대소문자는 구분하지 않는다. 길이가 맞지 않거나 16진수가 아닌 문자가 있으면 파싱은 오류를 반환해야 MUST 하고, panic해서는 안 된다.

#### Scenario: 왕복 변환
- **WHEN** 임의의 해시를 문자열로 표시한 뒤 다시 파싱한다
- **THEN** 원래 값과 같은 해시가 나온다

#### Scenario: 대문자 입력
- **WHEN** 대문자 16진수 40자를 SHA-1 해시로 파싱한다
- **THEN** 같은 내용의 소문자 문자열을 파싱한 결과와 같은 값이 나온다

#### Scenario: 길이 오류
- **WHEN** 39자 또는 41자 문자열을 SHA-1 해시로 파싱한다
- **THEN** 길이 오류가 반환된다

#### Scenario: 잘못된 문자
- **WHEN** `g`가 들어간 40자 문자열을 SHA-1 해시로 파싱한다
- **THEN** 잘못된 문자 오류가 반환된다

### Requirement: 식별 값은 비교하고 키로 쓸 수 있다
해시 값, InfoHash, TorrentId SHALL 서로 같은지 비교할 수 있고, 해시 맵의 키로 쓸 수 있으며, 정렬 순서가 정해져 있다. 바이트 내용이 같으면 같은 값으로 취급한다.

#### Scenario: 맵 키로 조회
- **WHEN** TorrentId를 키로 맵에 값을 넣고, 같은 바이트로 다시 만든 TorrentId로 조회한다
- **THEN** 넣은 값이 조회된다
