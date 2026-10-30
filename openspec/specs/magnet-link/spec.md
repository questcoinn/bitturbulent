# magnet-link Specification

## Purpose

magnet URI와 16진수 해시 문자열을 해석해, 메타데이터 없이 info-hash와 이름 힌트, 트래커, 웹시드만 가진 토렌트 설명을 만드는 규칙을 정한다. 입력 형태는 qBittorrent의 `TorrentDescriptor::parse`가 받는 범위를 따른다.

## Requirements

### Requirement: btih로 v1 info-hash를 읽는다
시스템 SHALL magnet의 `xt=urn:btih:<값>`에서 v1 info-hash를 읽는다. 값은 40자 16진수(대소문자 무시)나 32자 base32다.

#### Scenario: 16진수 btih
- **WHEN** `magnet:?xt=urn:btih:` 뒤에 대문자 40자 16진수가 붙은 링크를 해석한다
- **THEN** info-hash는 그 값의 v1만 가진다

#### Scenario: base32 btih
- **WHEN** 같은 해시를 32자 base32로 적은 btih 링크를 해석한다
- **THEN** 16진수로 적은 링크와 같은 info-hash를 얻는다

### Requirement: btmh로 v2 info-hash를 읽는다
시스템 SHALL `xt=urn:btmh:1220<64자 16진수>`에서 v2(SHA-256) info-hash를 읽는다. `1220`은 SHA-256 multihash 접두어다. 다른 multihash 접두어는 지원하지 않는 형식으로 거부한다.

#### Scenario: v2 전용 magnet
- **WHEN** btmh 하나만 있는 링크를 해석한다
- **THEN** info-hash는 v2만 가진다

#### Scenario: 지원하지 않는 multihash
- **WHEN** `urn:btmh:1114...`처럼 SHA-256이 아닌 multihash를 가진 링크를 해석한다
- **THEN** 지원하지 않는 해시 형식 오류를 얻는다

### Requirement: btih와 btmh가 함께 있으면 하이브리드다
시스템 SHALL 한 링크에 btih와 btmh가 모두 있으면 둘을 함께 가진 하이브리드 info-hash를 만든다.

#### Scenario: 하이브리드 magnet
- **WHEN** btih와 btmh를 모두 가진 링크를 해석한다
- **THEN** info-hash는 v1과 v2를 모두 가지고, 토렌트 ID는 v2의 앞 20바이트다

### Requirement: info-hash가 없거나 잘못되면 거부한다
시스템 SHALL `magnet:?`으로 시작하지 않는 입력, `xt`가 없는 링크, 해시 값의 길이나 문자가 잘못된 링크를 오류로 거부한다.

#### Scenario: xt 없음
- **WHEN** `magnet:?dn=hello`를 해석한다
- **THEN** info-hash가 없다는 오류를 얻는다

#### Scenario: 잘못된 길이
- **WHEN** btih 값이 39자인 링크를 해석한다
- **THEN** 해시 형식 오류를 얻는다

### Requirement: 이름, 트래커, 웹시드를 읽는다
시스템 SHALL `dn`을 이름 힌트로, 각 `tr`을 트래커로, 각 `ws`를 웹시드로 읽고, 값은 퍼센트 디코딩한다. 트래커는 링크에 나온 순서대로 하나씩 별도 tier가 된다. 트래커 주소의 스킴이나 형식은 검사하거나 걸러내지 않는다.

#### Scenario: 퍼센트 디코딩된 이름
- **WHEN** `dn=hello%20world`가 있는 링크를 해석한다
- **THEN** 이름 힌트는 `hello world`다

#### Scenario: 트래커 순서와 tier
- **WHEN** `tr`이 `http://a`, `udp://b` 순서로 있는 링크를 해석한다
- **THEN** 트래커 목록은 tier 두 개이고, 첫 tier에 `http://a`, 둘째 tier에 `udp://b`가 있다

#### Scenario: 지원하지 않는 트래커 보존
- **WHEN** `tr=wss%3A%2F%2Fc`가 있는 링크를 해석한다
- **THEN** `wss://c`가 트래커 목록에 그대로 남는다

### Requirement: 알 수 없는 매개변수는 무시한다
시스템 SHALL 해석하지 않는 매개변수(`x.pe`, `so` 등)가 있어도 오류로 처리하지 않고 무시한다.

#### Scenario: 알 수 없는 매개변수
- **WHEN** `xt`와 함께 `x.pe=1.2.3.4:6881`이 있는 링크를 해석한다
- **THEN** 해석은 성공하고 결과는 `x.pe`가 없을 때와 같다

### Requirement: 16진수 해시 문자열만으로도 토렌트를 지정할 수 있다
시스템 SHALL magnet 링크 대신 들어온 문자열이 정확히 40자 16진수면 그 값을 btih로, 정확히 64자 16진수면 btmh(SHA-256)로 보고 magnet 링크와 같은 방식으로 해석한다. 이건 qBittorrent `TorrentDescriptor::parse`와 같은 동작이다.

#### Scenario: 40자 해시
- **WHEN** 40자 16진수 문자열을 해석한다
- **THEN** `magnet:?xt=urn:btih:<그 문자열>`을 해석한 것과 같은 결과를 얻는다

#### Scenario: 64자 해시
- **WHEN** 64자 16진수 문자열을 해석한다
- **THEN** v2만 가진 info-hash를 얻는다

#### Scenario: 해시도 magnet도 아님
- **WHEN** `hello`를 해석한다
- **THEN** 오류를 얻는다

### Requirement: magnet으로 만든 토렌트 설명에는 메타데이터가 없다
시스템 SHALL magnet 링크나 해시 문자열로 만든 토렌트 설명에 info-hash, 이름 힌트, 트래커, 웹시드만 담는다. 파일 목록과 피스 정보 같은 메타데이터는 없는 상태로 둔다.

#### Scenario: 메타데이터 없음
- **WHEN** magnet 링크로 토렌트 설명을 만든다
- **THEN** 메타데이터가 없고, info-hash와 트래커는 링크의 값과 같다
