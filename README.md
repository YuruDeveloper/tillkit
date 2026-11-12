# TillKit

TillKit is a tool for the Toss Front 2 terminal.
It makes a backup of the boot partitions, writes images and adds the Magisk root patch.
The device must have a GSI on slot B, installed from stock firmware 1.609.5.
The device must also have root access.

TillKit is a changed version of [LTBox](https://github.com/miner7222/LTBox) by miner7222.
The Lenovo functions of LTBox are not in this repository.

[한국어](READMEs/README_ko-KR.md)

## Before you start

> [!WARNING]
> A change to firmware can make your device unbootable or erase your data. This tool comes without warranty. You use it at your risk.

- No person did a test of the write functions of TillKit on a device.
- A write changes two partitions, one after the other. If the device stops between the two writes, it can become unbootable.
- The only recovery from that condition is an EDL restore from a full backup. TillKit cannot make a full backup and cannot do that restore.
- Make a full backup of the device with a different tool before you write.

## What you need

### USB debugging

USB debugging must be on.

1. Touch the build number seven times in the settings of the device. This shows the `Developer options`.
2. Set `USB debugging` to on in the `Developer options`.
3. Connect the device to the computer. The device shows a message. Accept the connection.

The names of the settings can be different on your system.

### Root access

The device must have root access before you use TillKit.
USB debugging is not sufficient.
`adb root` must be available, or the device must have `su`.
TillKit reads and writes the boot partitions with root access.
Without root access, TillKit cannot make a backup and cannot write.

The `ROOT` screen does not get root access on a device that has none.
It adds Magisk to a device on which only `adb root` is available.

### One ADB program

If a different ADB program on the computer uses the device, stop that program first.

## Supported device

The `DEVICE` screen examines each of these values.

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

The tool does not write to a device that has an incorrect value.
But you can read the incorrect items on the `DEVICE` screen, accept them and continue.
The tool continues to compare the files and to do the check after the write.
The tool does not make sure that such a device starts after a write.
You cannot continue with these devices:

- A device that has no root access.
- A device that did not complete its start.
- A device with a slot that is not `_a` or `_b`.

## Screens

| Screen | Function |
| --- | --- |
| DEVICE | Reads the properties of the device and tells you if you can use the tool with it. This screen does not write. |
| BACKUP | Reads `init_boot`, `boot` and `vbmeta` of the device into a folder on the computer. You select that folder on this screen. `RESTORE` in the list writes the images of that backup to the device again. |
| WRITE | You select a folder with an image and push `WRITE`. The tool does the steps in sequence, from the plan to the check. |
| ROOT | Downloads Magisk, adds the Magisk patch to the `init_boot` of the device and writes it. |
| ADVANCED | Starts the device again in system, recovery, bootloader (fastboot) or EDL mode. |
| INFO | Shows the items that the tool does not do, and the notices. |

### Folder for the WRITE screen

Put only one of these files in the folder.

| File | Function |
| --- | --- |
| `init_boot.img` | A root patch from a different tool. TillKit signs it and writes `init_boot` and `vbmeta`. |
| `boot.img` | A kernel image. TillKit signs it and writes `boot` and `vbmeta`. TillKit cannot use a compressed kernel image. |
| `system.img` | A GSI. TillKit makes an installation plan only and does not write to the device. For this file, the screen shows a line for the folder of the full backup. Select that folder before you push `WRITE`. |

### Safety steps of a write

1. The images that are on the device must be in a backup of the `BACKUP` screen before a plan is possible. Thus, make a backup first.
2. The plan is a file. The approval is for the SHA-256 of that file.
3. Before the write, the tool shows the partitions and the recovery images. You must give your approval again.
4. The script on the device examines the properties and the images of the device again immediately before it writes.
5. If a write does not complete, the script writes the images from before the write again and reads them.
6. The restart of the device has a different approval. It is possible only after a write with correct hashes.

If the device does not start with a kernel that you wrote, TillKit cannot write the kernel from before the write again.

### ROOT

- `ROOT` downloads the latest Magisk release from `github.com/topjohnwu/Magisk`. TillKit does not include Magisk.
- TillKit does not add the patch to an `init_boot` that has Magisk.
- TillKit does not install the Magisk app on the device. Install the app after the restart.
- TillKit does not make other types of root patch. You can write an `init_boot.img` from a different tool on the `WRITE` screen.

### ADVANCED

- The modes that you can select change with the connection of the device.
- To go from EDL mode to a usual start, the Firehose loader file of this device is necessary. TillKit does not include this file.
- If a write has a result that is not known, TillKit does not send a restart.

## No test on a device

No function of the TillKit screens operated on a device.
The checks for these items used a simulated device and a local shell only.

Write functions:

- Backup, write and `RESTORE` on a device.
- A write through `su`.
- A write to a device after you accept incorrect items.
- The EDL recovery after the device stops during a write.

Other functions:

- The start of the device from an image with the Magisk patch that TillKit made.
- The start of the device from a kernel image that TillKit signed.
- The restart from the `ADVANCED` screen.
- The download of Magisk from GitHub.

## Not in this tool

- A write to a device that has no root access, and root access for such a device.
- The GSI installation. TillKit makes only the plan.
- Full backup and full restore of the device.
- Recovery of a device that does not start after a write.
- Automatic removal of `/data/local/tmp/front2-write-*` on the device.
- A time limit for device commands. If the device gives no result during a write, the tool continues to wait and the write state is not known. The tool does not try again and does not start the device again.

## Files that must stay out of the repository

Do not add images, firmware, loaders, device logs, keys or backups of a device.
`.gitignore` stops the usual file types, but you must examine each commit.
The tool keeps its data in the folder that you select on the `BACKUP` screen.
If you select no folder, the tool uses the `tillkit-data` folder that is adjacent to the program file.
The tool writes the path of the selected folder into `tillkit-folder.json`, which is also adjacent to the program file.
Backups are in `pairs` and `kernel-sets`, write plans are in `plans`, and the Magisk download is in `magisk`.

## Build from source

Install [Rust through rustup](https://rustup.rs/) and the native build tools of your system.
The repository pins the Rust toolchain in [rust-toolchain.toml](rust-toolchain.toml).

```sh
cargo build --release --locked -p tillkit
```

The executable is `target/release/tillkit` (`tillkit.exe` on Windows).
The only builds until now are Windows builds.

| Crate | Responsibility |
| --- | --- |
| [front2](crates/front2) | Device profile, support checks, pair store, candidate signature, write plan, device script and apply flow. |
| [tillkit](crates/tillkit) | The iced desktop application. |
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
- Added the `front2` and `tillkit` crates.

## Credits

LTBox is the work of [miner7222](https://github.com/miner7222/LTBox) and its contributors.
The LTBox README gives thanks to these people for their data and guides:

- **Anonymous [ㅇㅇ](https://gall.dcinside.com/board/lists?id=tabletpc)**
- **[갓파더](https://ppomppu.co.kr/zboard/view.php?id=androidtab&page=1&divpage=38&no=197457)**
- **[limzei89](https://note.com/limzei89/n/nd5217eb57827)**
- **[hitin911](https://xdaforums.com/m/hitin911.12861404/)**
- **[corrupt3904](https://gall.dcinside.com/mgallery/board/view/?id=andtabcus&no=20290)**

## License

TillKit is licensed under [GPL-3.0-or-later](LICENSE), the same license as LTBox.
Third-party components keep their licenses:

| Component | License |
| --- | --- |
| qdl (`vendor/qdl`, Qualcomm) | BSD-3-Clause |
| magiskboot-rs | GPL-3.0-or-later |
| kptools-rs | GPL-2.0-or-later |
| avbtool-rs | Apache-2.0 |
| IBM Plex Mono (`crates/tillkit/assets/fonts`) | SIL Open Font License 1.1 |
| IBM Plex Sans KR (`crates/tillkit/assets/fonts`) | SIL Open Font License 1.1 |
| Archivo Narrow (`crates/tillkit/assets/fonts`) | SIL Open Font License 1.1 |

The `ROOT` screen downloads Magisk (topjohnwu, GPL-3.0-or-later). TillKit does not include Magisk.

[![GPLv3](https://www.gnu.org/graphics/gplv3-127x51.png)](https://www.gnu.org/licenses/gpl-3.0)
