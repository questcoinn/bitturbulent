# Spec Delta

## Purpose

.torrent 파일에서 토렌트의 식별자(info-hash), 파일 목록, 피스 정보, 트래커, 웹시드, 부가 정보를 읽는 규칙을 정한다. 지원 범위는 v1과 하이브리드 토렌트이고, 결과는 qBittorrent가 같은 파일에서 보여 주는 식별자와 일치해야 한다.

## ADDED Requirements

### Requirement: v1 info-hash는 원본 info 바이트의 SHA-1이다
시스템 SHALL .torrent의 `info` 사전이 파일에서 차지하는 원본 바이트 구간에 SHA-1을 적용해 v1 info-hash를 계산한다. `info` 사전이 비정규형이어도 다시 인코딩한 바이트가 아니라 원본 바이트를 써야 MUST 한다.

#### Scenario: 정규형 토렌트
- **WHEN** v1 .torrent를 읽는다
- **THEN** info-hash는 v1 값만 가지고, 그 값은 `info` 구간 바이트의 SHA-1과 같다

#### Scenario: 비정규 info 사전
- **WHEN** `info` 사전의 키가 정렬되지 않은 .torrent를 읽는다
- **THEN** v1 info-hash는 파일 속 원본 `info` 구간의 SHA-1과 같고, 정렬해 다시 인코딩한 바이트의 SHA-1과는 다르다

### Requirement: 하이브리드 토렌트는 v1과 v2 해시를 모두 가진다
`info` 사전에 `meta version`이 2이고 v1 필드(`pieces`와 `length` 또는 `files`)도 있으면, 시스템 SHALL 하이브리드 토렌트로 읽는다. info-hash는 같은 원본 `info` 구간의 SHA-1(v1)과 SHA-256(v2)을 모두 가진다. 파일 목록과 피스 정보는 v1 필드에서 읽는다.

#### Scenario: 하이브리드 식별자
- **WHEN** 하이브리드 .torrent를 읽는다
- **THEN** info-hash는 v1과 v2를 모두 가지고, 토렌트 ID는 v2 해시의 앞 20바이트다

#### Scenario: 하이브리드의 파일 목록
- **WHEN** 하이브리드 .torrent를 읽는다
- **THEN** 파일 목록, 피스 길이, 피스 해시는 v1 필드의 값과 같다

### Requirement: v2 전용 토렌트와 알 수 없는 메타 버전은 거부한다
시스템 SHALL `meta version`이 2인데 v1 필드가 없는 .torrent를 "v2 전용 토렌트는 지원하지 않음" 오류로 거부한다. `meta version`이 있는데 2가 아닌 경우도 지원하지 않는 버전으로 거부한다.

#### Scenario: v2 전용
- **WHEN** v2 전용 .torrent를 읽는다
- **THEN** v2 미지원 오류를 얻는다

#### Scenario: 알 수 없는 버전
- **WHEN** `meta version`이 3인 .torrent를 읽는다
- **THEN** 지원하지 않는 메타 버전 오류를 얻는다

### Requirement: 단일 파일과 다중 파일 구조를 읽는다
시스템 SHALL `info`에 `length`가 있으면 단일 파일 토렌트로, `files`가 있으면 다중 파일 토렌트로 읽는다. 각 파일에 대해 길이와 경로를 순서대로 제공한다. 토렌트 이름(`name`)과 다중 파일 여부도 제공한다. 둘 다 있거나 둘 다 없으면 오류다.

#### Scenario: 단일 파일
- **WHEN** `length`가 1000인 단일 파일 .torrent를 읽는다
- **THEN** 파일은 하나이고, 길이는 1000, 경로는 토렌트 이름 하나로 이루어진다

#### Scenario: 다중 파일
- **WHEN** `files`에 파일 3개가 있는 .torrent를 읽는다
- **THEN** 파일 3개가 원래 순서대로 나오고, 전체 크기는 세 길이의 합이다

### Requirement: 패딩 파일을 구분한다
시스템 SHALL 다중 파일 항목의 `attr` 문자열에 `p`가 있으면 그 파일을 패딩 파일로 표시한다. 패딩 파일도 파일 목록과 전체 크기, 피스 계산에는 포함된다.

#### Scenario: 패딩 파일 표시
- **WHEN** qBittorrent에서 "정렬 최적화"를 켜고 만든 다중 파일 .torrent를 읽는다
- **THEN** `attr`에 `p`가 있는 항목은 패딩 파일로 표시되고, 나머지는 일반 파일로 표시된다

### Requirement: 파일 경로와 이름은 원본 그대로 보존한다
시스템 SHALL 토렌트 이름과 파일 경로 조각을 .torrent에 적힌 바이트 그대로 보존한다. `..`, 절대 경로처럼 보이는 조각, OS에서 쓸 수 없는 문자가 있어도 파싱 단계에서 고치거나 거부하지 않는다. `name.utf-8`이나 `path.utf-8`이 있으면 그 값을 우선한다. 파싱 결과에서 디스크 경로를 바로 얻는 방법은 제공하지 않는다.

#### Scenario: 위험한 경로 조각
- **WHEN** 경로가 [`..`, `evil.txt`]인 파일이 있는 .torrent를 읽는다
- **THEN** 파싱은 성공하고, 그 파일의 경로 조각은 정확히 `..`와 `evil.txt`다

#### Scenario: UTF-8 별도 필드 우선
- **WHEN** `name`과 `name.utf-8` 값이 서로 다른 .torrent를 읽는다
- **THEN** 토렌트 이름은 `name.utf-8`의 값이다

#### Scenario: 경로 조각이 비어 있음
- **WHEN** `path`가 빈 리스트인 파일 항목이 있는 .torrent를 읽는다
- **THEN** 해당 파일 항목 오류를 얻는다

### Requirement: 피스 정보를 검증해서 제공한다
시스템 SHALL 피스 길이, 피스 개수, 피스별 SHA-1 해시를 제공한다. 피스 길이가 0 이하이거나, `pieces` 길이가 20의 배수가 아니거나, 피스 개수가 ⌈전체 크기 / 피스 길이⌉와 다르면 오류다. 마지막 피스 크기는 전체 크기에서 앞 피스들의 크기를 뺀 값이다.

#### Scenario: 마지막 피스 크기
- **WHEN** 피스 길이 16384, 전체 크기 40000인 .torrent를 읽는다
- **THEN** 피스는 3개이고, 크기는 16384, 16384, 7232다

#### Scenario: 피스 개수 불일치
- **WHEN** 전체 크기로는 피스 3개가 필요한데 `pieces`에 해시가 2개만 있는 .torrent를 읽는다
- **THEN** 피스 정보 오류를 얻는다

### Requirement: 인덱스로 표현할 수 없는 크기는 거부한다
시스템 SHALL 피스 개수나 파일 개수가 32비트 부호 없는 정수 범위를 넘으면 오류로 거부한다. 그래서 모든 피스와 파일을 기존 인덱스 타입으로 가리킬 수 있다.

#### Scenario: 인덱스 범위
- **WHEN** 파싱에 성공한 토렌트의 피스와 파일을 순회한다
- **THEN** 모든 피스와 파일이 인덱스 값 하나에 대응한다

### Requirement: 트래커를 tier 구조로 읽는다
시스템 SHALL `announce-list`가 있으면 그 tier 구조와 순서를 그대로 트래커 목록으로 쓰고, 빈 tier는 버린다. `announce-list`가 없거나 비어 있으면 `announce` 하나를 tier 하나로 쓴다. 트래커 주소의 스킴이나 형식은 검사하거나 걸러내지 않는다.

#### Scenario: announce-list 우선
- **WHEN** `announce`와 2개 tier짜리 `announce-list`가 모두 있는 .torrent를 읽는다
- **THEN** 트래커 목록은 `announce-list`의 2개 tier와 같다

#### Scenario: announce만 있음
- **WHEN** `announce`만 있는 .torrent를 읽는다
- **THEN** 트래커 목록은 그 주소 하나가 담긴 tier 하나다

#### Scenario: 지원하지 않는 스킴 보존
- **WHEN** `wss://` 트래커가 들어 있는 .torrent를 읽는다
- **THEN** 그 주소가 트래커 목록에 그대로 남는다

### Requirement: 웹시드 주소를 읽는다
시스템 SHALL `url-list`를 웹시드 주소 목록으로 읽는다. 값은 문자열 하나일 수도 있고 문자열 리스트일 수도 있다.

#### Scenario: 문자열 하나
- **WHEN** `url-list`가 문자열 하나인 .torrent를 읽는다
- **THEN** 웹시드 목록에 그 주소 하나가 있다

### Requirement: private 플래그를 읽는다
시스템 SHALL `info` 사전의 `private` 값이 1이면 private 토렌트로 표시하고, 없거나 다른 값이면 public으로 표시한다.

#### Scenario: private 토렌트
- **WHEN** `info`에 `private`=1이 있는 .torrent를 읽는다
- **THEN** private 토렌트로 표시된다

### Requirement: 부가 정보는 있으면 읽고 문제가 있으면 무시한다
시스템 SHALL `creation date`, `comment`, `created by`를 선택 정보로 읽는다. `comment.utf-8`이 있으면 그 값을 우선한다. 이 필드가 없거나 타입이 틀려도 오류로 처리하지 않고 "없음"으로 둔다.

#### Scenario: 타입이 틀린 생성일
- **WHEN** `creation date`가 문자열로 들어 있는 .torrent를 읽는다
- **THEN** 파싱은 성공하고 생성일은 없음이다

### Requirement: 필수 필드가 없거나 틀리면 어느 필드인지 알려 준다
시스템 SHALL 최상위 값이 사전이 아니거나, `info`, `name`, `piece length`, `pieces` 중 하나가 없거나 타입이 틀리면 오류를 돌려준다. 오류에는 문제가 된 필드 이름이 들어가야 MUST 한다.

#### Scenario: info 없음
- **WHEN** `info` 사전이 없는 .torrent를 읽는다
- **THEN** `info` 필드가 없다는 오류를 얻는다

#### Scenario: 음수 파일 길이
- **WHEN** 파일 `length`가 -1인 .torrent를 읽는다
- **THEN** `length` 필드 오류를 얻는다

### Requirement: 파일에서 읽을 때 크기 제한을 먼저 확인한다
시스템 SHALL 경로로 .torrent를 읽을 때 파일 크기가 입력 크기 상한을 넘으면 내용을 메모리로 읽지 않고 크기 제한 오류를 돌려준다.

#### Scenario: 너무 큰 파일
- **WHEN** 크기 상한보다 큰 파일 경로로 .torrent를 읽는다
- **THEN** 크기 제한 오류를 얻고, 파일 내용 전체를 읽지 않는다

### Requirement: .torrent에서 만든 토렌트 설명은 메타데이터를 가진다
시스템 SHALL .torrent에서 토렌트 설명(descriptor)을 만들면 info-hash, 이름, 트래커, 웹시드와 함께 메타데이터(파일 목록과 피스 정보)를 항상 포함한다.

#### Scenario: 메타데이터 포함
- **WHEN** 유효한 .torrent로 토렌트 설명을 만든다
- **THEN** 메타데이터가 있고, 그 info-hash는 설명의 info-hash와 같다

### Requirement: 실제 토렌트에서 qBittorrent와 같은 식별자를 얻는다
시스템 SHALL 저장소의 v1 fixture(단일 파일, 다중 파일, 패딩 파일이 있는 다중 파일, 공식 배포 .torrent)에서 qBittorrent가 표시하는 v1 info-hash와 같은 값을 계산한다. 하이브리드 fixture에서는 독립 구현으로 계산한 v1, v2 값과 같은 값을 계산한다.

#### Scenario: v1 fixture 대조
- **WHEN** 각 v1 fixture를 읽는다
- **THEN** v1 info-hash가 qBittorrent 속성 창에서 확인해 기록한 값과 같다

#### Scenario: 하이브리드 fixture 대조
- **WHEN** 하이브리드 fixture를 읽는다
- **THEN** v1, v2 info-hash가 독립 스크립트로 계산해 기록한 값과 같다
