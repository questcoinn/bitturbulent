# build-pipeline Specification

## Purpose

프로젝트의 크레이트 의존 경계와 품질 게이트를 정의한다. 로컬과 CI가 같은 기준으로 변경을 검증하고, 엔진과 앱 로직의 분리가 구조적으로 유지되게 한다.

## Requirements

### Requirement: 크레이트 의존 방향은 한 방향이다
workspace SHALL 공용 타입, 메타데이터, 엔진, 세션, CLI 다섯 계층으로 나뉜다. 의존 방향은 CLI → 세션 → 엔진 → 메타데이터 → 공용 타입 한 방향만 허용하고, 위 계층은 자기보다 아래에 있는 어느 계층에든 직접 의존할 수 있다. 엔진은 세션에 의존해서는 안 MUST NOT 되고, 메타데이터 크레이트는 공용 타입 외의 workspace 크레이트에, 공용 타입 크레이트는 다른 workspace 크레이트에 의존해서는 안 MUST NOT 된다.

#### Scenario: 엔진이 세션을 참조
- **WHEN** 엔진 크레이트에 세션 크레이트 의존성을 추가한다
- **THEN** 빌드나 품질 게이트가 실패한다

#### Scenario: 메타데이터가 엔진을 참조
- **WHEN** 메타데이터 크레이트에 엔진 크레이트 의존성을 추가한다
- **THEN** 빌드나 품질 게이트가 실패한다

#### Scenario: 정상 구조
- **WHEN** workspace의 의존 그래프를 확인한다
- **THEN** 공용 타입 크레이트에는 workspace 내부 의존이 없고, 메타데이터는 공용 타입에만, 엔진은 메타데이터와 공용 타입에만, 세션은 엔진, 메타데이터, 공용 타입에만 의존한다

### Requirement: 공용 타입 크레이트는 비동기 런타임에 의존하지 않는다
공용 타입 크레이트 SHALL 비동기 런타임 없이 빌드된다. 그래서 어느 계층에서든 런타임 부담 없이 가져다 쓸 수 있다.

#### Scenario: 런타임 의존 검사
- **WHEN** 품질 게이트가 공용 타입 크레이트의 의존 트리를 검사한다
- **THEN** 비동기 런타임 크레이트가 없으면 통과하고, 있으면 실패한다

### Requirement: 단일 명령 품질 게이트
프로젝트 SHALL 한 명령으로 다음을 차례로 실행하는 품질 게이트를 제공한다: 포맷 검사, 모든 대상에 대한 lint(경고를 오류로 취급), workspace 전체 테스트, 의존 경계 검사. 어느 하나라도 실패하면 명령은 0이 아닌 종료 코드로 끝나야 MUST 한다.

#### Scenario: 깨끗한 상태
- **WHEN** 포맷, lint, 테스트, 경계 검사를 모두 만족하는 상태에서 품질 게이트를 실행한다
- **THEN** 종료 코드 0으로 끝난다

#### Scenario: lint 경고 존재
- **WHEN** lint 경고가 하나라도 있는 상태에서 품질 게이트를 실행한다
- **THEN** 0이 아닌 종료 코드로 끝나고, 경고 위치가 출력된다

#### Scenario: 포맷 위반
- **WHEN** 포맷이 맞지 않는 파일이 있는 상태에서 품질 게이트를 실행한다
- **THEN** 0이 아닌 종료 코드로 끝난다

### Requirement: 품질 게이트는 Windows와 Linux에서 동일하게 동작한다
품질 게이트 SHALL Windows와 Linux에서 추가 도구 설치 없이(Rust 툴체인만으로) 같은 명령으로 실행된다.

#### Scenario: 양쪽 OS 실행
- **WHEN** Windows와 Linux에서 각각 같은 품질 게이트 명령을 실행한다
- **THEN** 두 OS에서 같은 검사 단계가 실행된다

### Requirement: CI는 품질 게이트를 실행한다
CI SHALL main 브랜치 push와 pull request마다 Windows와 Linux에서 로컬과 같은 품질 게이트 명령을 실행한다. 품질 게이트가 실패하면 CI도 실패로 보고해야 MUST 한다.

#### Scenario: pull request 검증
- **WHEN** pull request를 연다
- **THEN** Windows와 Linux 두 job에서 품질 게이트가 실행되고, 결과가 PR에 표시된다

#### Scenario: 한 OS에서만 실패
- **WHEN** 품질 게이트가 Windows에서는 통과하고 Linux에서는 실패한다
- **THEN** CI 전체 결과는 실패다

### Requirement: CLI는 로그 수준을 환경 변수로 조절한다
CLI 바이너리 SHALL 시작할 때 로깅을 초기화한다. 로그 수준은 `RUST_LOG` 환경 변수로 정하고, 지정하지 않으면 `info` 수준을 쓴다.

#### Scenario: 기본 로그 수준
- **WHEN** `RUST_LOG` 없이 CLI를 실행한다
- **THEN** info 이상 로그가 출력되고, debug 로그는 출력되지 않는다

#### Scenario: 환경 변수로 변경
- **WHEN** `RUST_LOG=debug`로 CLI를 실행한다
- **THEN** debug 로그도 출력된다

### Requirement: 메타데이터 크레이트는 비동기 런타임에 의존하지 않는다
메타데이터 크레이트 SHALL 비동기 런타임 없이 빌드된다. 그래서 엔진, 세션, CLI, 도구 어디서든 런타임 부담 없이 .torrent와 magnet을 해석할 수 있다.

#### Scenario: 런타임 의존 검사
- **WHEN** 품질 게이트가 메타데이터 크레이트의 의존 트리를 검사한다
- **THEN** 비동기 런타임 크레이트가 없으면 통과하고, 있으면 실패한다
