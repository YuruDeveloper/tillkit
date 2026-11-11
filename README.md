# FrontBox (working name)

FrontBox is a tool for one device: the Toss Front 2 terminal.
The device must have a GSI on slot B, installed from stock firmware 1.609.5.
The tool keeps, signs and writes the `init_boot_b` and `vbmeta_b` pair of that device.

FrontBox is a modified version of [LTBox](https://github.com/miner7222/LTBox) by miner7222.
The Lenovo functions of LTBox are not in this repository.
The name of the tool can change.

[한국어](READMEs/README_ko-KR.md)

## Before you start

> [!WARNING]
> A change to firmware can make your device unbootable or erase your data. This tool comes without warranty. You use it at your risk.

- The tool writes only when all checks give the correct result. It does not write to a device that it cannot identify.
- A write changes two partitions, one after the other. If the power stops between the two writes, the device can become unbootable.
- The only recovery from that condition is an EDL restore from a full backup. This tool cannot do that restore. No person did a test of that restore on a device.
- Keep a full backup of the device in a different location before you write.

## Supported device

The tool writes only when each of these values is correct.

| Check | Necessary value |
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
| Root access | `adb root` or `su` |

The top `vbmeta` of the device must have the AOSP test key `testkey_rsa4096` and the algorithm `SHA256_RSA4096`.
The tool does not work with other Toss terminals, other firmware versions or slot A.

## What the tool does

| Screen | Function |
| --- | --- |
| DEVICE | Reads the properties of the device and compares them with the necessary values. This screen does not write. |
| PAIRS | Reads the pair that is on the device into a local folder. Makes a signed candidate pair from a patched `init_boot` image of the Magisk app. |
| WRITE | Makes a write plan and puts the files in `/data/local/tmp`. Does a check, writes the pair, reads the pair again and compares the hashes. Starts the device again after one more check. |
| SCOPE | Shows the range of the tool, the items that have no test on a device, and the notices. |

The write has these safety steps:

1. The pair that is on the device must be in the local folder before a plan is possible.
2. The plan is a file. The approval is for the SHA-256 of that file.
3. The script on the device does all checks again before it writes.
4. If a write does not complete, the script writes the earlier pair again and reads it.
5. The restart of the device has a different approval. It is possible only after a write with correct hashes.

A candidate keeps the key, the rollback index, the flags and all other descriptors of the top `vbmeta`.
Only the hash descriptor of `init_boot` changes.

## No test on a device

The checks for these items used a simulated device and a local shell only.

- The start of the device from a candidate pair that this tool made. The signed content is the same as the candidate that started the device, but the file hashes are different.
- The write script and the pair read of this tool on a real device.
- A write through `su`. The earlier work on the device used `adb root`.
- The restore of a pair that the tool read earlier.
- The EDL recovery after the power stops during a write.

## Not in this tool

- GSI build and installation.
- Kernel image replacement.
- EDL full backup and restore.
- Automatic removal of `/data/local/tmp/front2-write-*` on the device.
- A time limit for device commands. If the device gives no answer during a write, the tool continues to wait and the write state is not known. The tool does not try again and does not start the device again.

The engine crates from LTBox stay in the repository for this work.
They have no screen in this tool.

## Files that must stay out of the repository

Do not add images, firmware, loaders, device logs, keys or read-back pairs of a real device.
`.gitignore` stops the usual file types, but you must examine each commit.
The tool keeps its data in the local application data folder, in `frontbox/pairs` and `frontbox/plans`.

## Build from source

Install [Rust through rustup](https://rustup.rs/) and the native build tools of your system.
The repository pins the Rust toolchain in [rust-toolchain.toml](rust-toolchain.toml).

```sh
cargo build --release --locked -p frontbox
```

The executable is `target/release/frontbox` (`frontbox.exe` on Windows).
The only builds until now are Windows builds.

| Crate | Responsibility |
| --- | --- |
| [front2](crates/front2) | Device profile, support checks, pair store, candidate signature, write plan, device script and apply flow. |
| [frontbox](crates/frontbox) | The iced desktop application. |
| [ltbox-core](crates/ltbox-core) | Shared errors, paths, logs and downloads from LTBox. |
| [ltbox-device](crates/ltbox-device) | ADB, Fastboot and EDL transport from LTBox. |
| [ltbox-patch](crates/ltbox-patch) | Boot image and AVB functions from LTBox. |

Run these checks before a change:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

## Design

The interface has one shape rule: a tone block with a 2 px ink line on its top edge.
A block has no side borders, no shadow and no round corners.
Red is a signal with five fixed shapes. Do not use it for decoration.

## Changes from LTBox

This repository started from LTBox 3.3.5, commit `8f6d141c`, in November 2026.

- Removed the LTBox application, its Lenovo screens and its translations.
- Removed the Lenovo model table, the Lenovo server queries, the region and country changes, and the fixed partition map.
- Removed the automatic update, the packages and the release workflow of LTBox.
- Added the `front2` and `frontbox` crates.

## Credits

LTBox is the work of [miner7222](https://github.com/miner7222/LTBox) and its contributors.
The LTBox README gives thanks to these people for their data and guides:

- **Anonymous [ㅇㅇ](https://gall.dcinside.com/board/lists?id=tabletpc)**
- **[갓파더](https://ppomppu.co.kr/zboard/view.php?id=androidtab&page=1&divpage=38&no=197457)**
- **[limzei89](https://note.com/limzei89/n/nd5217eb57827)**
- **[hitin911](https://xdaforums.com/m/hitin911.12861404/)**
- **[corrupt3904](https://gall.dcinside.com/mgallery/board/view/?id=andtabcus&no=20290)**

## License

FrontBox is licensed under [GPL-3.0-or-later](LICENSE), the same license as LTBox.
Third-party components keep their licenses:

| Component | License |
| --- | --- |
| qdl (`vendor/qdl`, Qualcomm) | BSD-3-Clause |
| magiskboot-rs | GPL-3.0-or-later |
| kptools-rs | GPL-2.0-or-later |
| avbtool-rs | Apache-2.0 |
| IBM Plex Mono (`crates/frontbox/assets/fonts`) | SIL Open Font License 1.1 |
| IBM Plex Sans KR (`crates/frontbox/assets/fonts`) | SIL Open Font License 1.1 |
| Archivo Narrow (`crates/frontbox/assets/fonts`) | SIL Open Font License 1.1 |

[![GPLv3](https://www.gnu.org/graphics/gplv3-127x51.png)](https://www.gnu.org/licenses/gpl-3.0)
