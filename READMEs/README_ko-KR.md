# TillKit

TillKit은 기기 하나만을 위한 데스크톱 도구입니다. 대상은 순정 1.609.5에서 슬롯 B의 GSI로 전환한 토스 프론트 2 단말입니다.
이 도구는 그 기기의 `init_boot_b`와 `vbmeta_b` 쌍을 보관하고, 서명하고, 기록합니다.

TillKit은 miner7222의 [LTBox](https://github.com/miner7222/LTBox)를 고친 판입니다.
LTBox의 Lenovo 전용 기능은 이 저장소에 없습니다. 도구의 이름은 아직 정해지지 않았습니다.

[English](../README.md)

## 시작하기 전에

> [!WARNING]
> 펌웨어를 바꾸면 기기가 부팅하지 못하게 되거나 데이터가 지워질 수 있습니다. 이 도구는 보증 없이 제공되며, 사용에 따른 책임은 사용자에게 있습니다.

- 모든 지원 검사를 통과한 기기에만 기록합니다. 식별하지 못한 기기에는 기록하지 않습니다.
- 기록은 두 파티션을 차례로 바꿉니다. 두 기록 사이에 전원이 끊기면 기기가 부팅하지 못할 수 있습니다.
- 그 상태에서 복구하는 방법은 전체 백업을 EDL로 되돌리는 것뿐입니다. 이 도구는 그 복구를 하지 못하며, 그 복구를 실제 기기에서 시험한 적도 없습니다.
- 기록하기 전에 기기의 전체 백업을 다른 곳에 보관해 주세요.

## 지원하는 기기

아래 값이 모두 맞을 때만 기록을 허용합니다.

| 검사 | 필요한 값 |
| --- | --- |
| `ro.product.vendor.manufacturer` | `TOSSPLACE` |
| `ro.product.vendor.device` | `toss_front2` |
| `ro.vendor.build.fingerprint` | `TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/475:user/dev-keys` |
| `ro.soc.model` | `QCM4325` |
| `ro.board.platform` | `bengal` |
| `ro.build.ab_update` | `true` |
| `ro.boot.slot_suffix` | `_b` |
| `ro.boot.vbmeta.device_state` | `locked` |
| `ro.boot.verifiedbootstate` | `green` |
| `sys.boot_completed` | `1` |
| 루트 접근 | `adb root` 또는 `su` |

기기의 top `vbmeta`는 AOSP 테스트 키 `testkey_rsa4096`과 알고리즘 `SHA256_RSA4096`으로 서명되어 있어야 합니다.
다른 토스 단말, 다른 펌웨어 판, 슬롯 A는 지원하지 않습니다.

## 하는 일

| 화면 | 기능 |
| --- | --- |
| DEVICE | 기기의 속성을 읽어 지원 값과 대조합니다. 이 화면은 아무것도 기록하지 않습니다. |
| PAIRS | 기기의 현재 쌍을 로컬 보관함으로 읽어 옵니다. Magisk 앱이 패치한 `init_boot` 이미지로 서명된 후보 쌍을 만듭니다. |
| WRITE | 기록 계획을 세우고, 파일을 `/data/local/tmp`에 올리고, 사전 점검을 한 뒤에 쌍을 기록하고 다시 읽어 해시를 대조합니다. 한 번 더 확인한 뒤에 재부팅합니다. |
| SCOPE | 지원 범위, 실제 기기에서 검증하지 않은 항목, 고지를 보여 줍니다. |

기록에는 다음 안전 단계가 있습니다.

1. 기기의 현재 쌍이 보관함에 있어야 계획을 세울 수 있습니다.
2. 계획은 파일로 저장되고, 승인은 그 파일의 SHA-256에 묶입니다.
3. 기기에서 도는 스크립트가 기록하기 전에 모든 조건을 다시 검사합니다.
4. 기록이 실패하면 스크립트가 이전 쌍을 다시 기록하고 다시 읽어 확인합니다.
5. 재부팅은 별도의 승인이며, 검증된 기록 뒤에만 할 수 있습니다.

후보 쌍은 top `vbmeta`의 키, rollback index, flags, 다른 descriptor를 모두 유지하고, `init_boot`의 hash descriptor만 바꿉니다.

## 실제 기기에서 검증하지 않은 것

아래 항목은 모의 장치와 로컬 셸에서만 시험했습니다.

- 이 도구가 만든 후보 쌍의 부팅. 실제 기기에서 부팅한 후보와 서명 내용은 같지만 파일 전체 해시는 다릅니다.
- 이 도구의 기록 스크립트와 쌍 읽기를 실제 기기에서 실행하는 것.
- `su`를 거친 기록. 검증된 경로는 `adb root`입니다.
- 읽어 둔 쌍을 되돌리는 기록.
- 전원이 끊긴 뒤의 EDL 복구.

## 이 도구에 없는 것

- GSI 구성과 설치
- 커널 이미지 교체
- EDL 전체 백업과 복구
- 기기의 `/data/local/tmp/front2-write-*` 자동 정리
- 기기 명령의 시간 제한. 기록 중에 기기가 응답하지 않으면 도구는 계속 기다리며, 그때의 기록 상태는 알 수 없습니다. 도구는 자동으로 다시 시도하거나 재부팅하지 않습니다.

LTBox에서 온 엔진 크레이트는 이 작업을 위해 저장소에 남아 있지만, 이 도구에는 그 기능을 쓰는 화면이 없습니다.

## 저장소에 넣으면 안 되는 파일

실제 기기의 이미지, 펌웨어, 로더, 기기 로그, 키, 읽어 온 쌍을 저장소에 넣지 않습니다.
`.gitignore`가 흔한 파일 형식을 막지만, 커밋마다 직접 확인해야 합니다.
도구의 데이터는 로컬 애플리케이션 데이터 폴더의 `tillkit/pairs`와 `tillkit/plans`에 저장됩니다.

## 빌드

```sh
cargo build --release --locked -p tillkit
```

실행 파일은 `target/release/tillkit`(Windows에서는 `tillkit.exe`)입니다. 지금까지는 Windows에서만 빌드하고 실행했습니다.

## 라이선스와 고지

TillKit은 LTBox와 같은 [GPL-3.0-or-later](../LICENSE)를 따릅니다. 원저작물과 제3자 구성 요소의 고지, LTBox에서 바뀐 내용은 [영문 README](../README.md)에 있습니다.
