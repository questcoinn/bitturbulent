# .torrent fixtures

`tests/fixtures.rs`가 이 파일들의 info-hash를 아래 기대값과 대조한다.

| 파일 | 형식 | 출처 | v1 기대값 확인 | v2 기대값 확인 |
|---|---|---|---|---|
| `v1-single.torrent` | v1, 단일 파일 | qBittorrent 5.2.4(lt 1.2) 토렌트 생성기 | qBittorrent 화면 | - |
| `v1-multi.torrent` | v1, 다중 파일, 정렬 최적화 끔 | 위와 같음 | qBittorrent 화면 | - |
| `v1-multi-padded.torrent` | v1, 다중 파일, 정렬 최적화 켬(패딩 파일 2개) | 위와 같음 | qBittorrent 화면 | - |
| `debian-13.7.0-amd64-netinst.iso.torrent` | v1 | https://cdimage.debian.org/debian-cd/ (2026-09-12) | qBittorrent 화면 | - |
| `ubuntu-26.04.1-live-server-amd64.iso.torrent` | v1 | https://releases.ubuntu.com/26.04.1/ (2026-08-27) | qBittorrent 화면 | - |
| `v2_hybrid.torrent` | 하이브리드 | libtorrent `RC_2_0` `test/test_torrents/` | `info_hashes.py` | `info_hashes.py` |
| `v2_only.torrent` | v2 전용(거부 확인용) | libtorrent `RC_2_0` `test/test_torrents/` | - | - |

## 만든 방법

qBittorrent로 만든 세 파일은 조각 크기 16 KiB, 트래커 없음으로 만들었다. 원본 더미 파일은 다음과 같다.

```
single.txt              40,000 B
multi/a.txt             10,000 B
multi/한글이름.txt       25,000 B
multi/sub/b.txt          5,000 B
```

정렬 최적화(libtorrent 1.2의 `optimize_alignment`)를 켜면 파일이 큰 순서로 다시 정렬되고, 각 파일 뒤에 `.pad/<번호>` 패딩 파일이 들어간다. 마지막 파일 뒤에는 패딩이 없다.

## 기대값 확인 방법

- **qBittorrent 화면:** .torrent를 qBittorrent에 추가하는 대화상자의 "정보 해시 v1" 값을 기록했다. 로컬 qBittorrent 5.2.4는 libtorrent 1.2 빌드라서 v2 값은 보여 주지 않는다.
- **`info_hashes.py`:** 표준 라이브러리만 쓰는 독립 스크립트다. 원본 `info` 바이트의 SHA-1(v1)을 계산하고, `meta version`이 2이면 SHA-256(v2)도 계산한다. Rust 구현을 참고하지 않고 따로 작성했다.

  ```bash
  python -I crates/bt-metainfo/tests/fixtures/info_hashes.py crates/bt-metainfo/tests/fixtures/*.torrent
  ```

## 라이선스

- libtorrent 테스트 파일(`v2_*.torrent`)은 libtorrent 저장소의 BSD 3-Clause 라이선스를 따른다. Copyright (c) Arvid Norberg.
- Debian과 Ubuntu .torrent는 각 배포판이 공개 배포하는 메타데이터 파일이다.
