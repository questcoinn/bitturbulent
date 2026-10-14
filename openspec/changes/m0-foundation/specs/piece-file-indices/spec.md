# Spec Delta

## Purpose

토렌트 안의 피스와 파일을 가리키는 인덱스 값을 정의한다. 와이어 프로토콜이 쓰는 값의 범위와 맞추고, 피스 번호와 파일 번호를 실수로 섞어 쓰지 못하게 한다.

## ADDED Requirements

### Requirement: 인덱스는 32비트 부호 없는 정수 범위를 가진다
PieceIndex와 FileIndex SHALL 0부터 2^32-1까지의 값을 표현한다. BitTorrent 와이어 프로토콜의 piece index(4바이트 big-endian 부호 없는 정수)와 범위가 같다.

#### Scenario: 최댓값 표현
- **WHEN** 값 4294967295로 PieceIndex를 만든다
- **THEN** 다시 꺼낸 정수 값은 4294967295다

#### Scenario: 범위를 넘는 변환
- **WHEN** 2^32 이상인 플랫폼 크기 정수를 PieceIndex로 변환하려고 한다
- **THEN** 변환은 오류를 반환하고, 값이 잘리지 않는다

### Requirement: 피스 인덱스와 파일 인덱스는 서로 섞어 쓸 수 없다
PieceIndex와 FileIndex SHALL 서로 다른 타입이다. 한쪽을 요구하는 자리에 다른 쪽이나 맨 정수를 넣으면 컴파일 오류가 나야 MUST 한다.

#### Scenario: 타입 혼용 거부
- **WHEN** PieceIndex를 받는 함수에 FileIndex를 넘기는 코드를 컴파일한다
- **THEN** 컴파일이 실패한다

### Requirement: 인덱스는 10진수로 표시되고 순서를 가진다
PieceIndex와 FileIndex SHALL 10진수 정수로 표시된다. 크기 비교와 정렬은 담고 있는 정수 값의 순서를 따른다. 맵의 키로 쓸 수 있다.

#### Scenario: 표시
- **WHEN** 값 42인 FileIndex를 문자열로 표시한다
- **THEN** 결과는 `42`다

#### Scenario: 정렬
- **WHEN** 값이 3, 1, 2인 PieceIndex 목록을 정렬한다
- **THEN** 순서는 1, 2, 3이 된다

### Requirement: 인덱스는 배열 접근용 크기로 변환할 수 있다
PieceIndex와 FileIndex SHALL 손실 없이 플랫폼 크기 정수로 변환된다. 피스 목록이나 파일 목록 같은 배열의 위치로 바로 쓸 수 있다.

#### Scenario: 배열 위치로 사용
- **WHEN** 값 5인 PieceIndex를 플랫폼 크기 정수로 바꾼다
- **THEN** 결과는 5다
