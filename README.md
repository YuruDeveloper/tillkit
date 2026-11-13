# TillKit

TillKit is a tool for the Toss Front 2 terminal.
It installs a GSI, makes a backup of the boot partitions, writes images and adds the Magisk root patch.
The GSI installation is for a device with stock firmware 1.609.5 that you connect in EDL mode.
The other functions are for a device that has root access and a GSI on slot B.
That GSI must be installed from stock firmware 1.609.5.

TillKit is a changed version of [LTBox](https://github.com/miner7222/LTBox) by miner7222.
The Lenovo functions of LTBox are not in this repository.

[한국어](READMEs/README_ko-KR.md)

![The WRITE screen during the write step of a GSI installation](READMEs/Images/GsiInstallationWrite.png)

This file shows examples of the screens. The paths and the numbers on these screens are not from a device. The screens show Korean text.

## Before you start

> [!WARNING]
> A change to firmware can make your device unbootable or erase your data. This tool comes without warranty. You use it at your risk.

- No person did a test of the write functions of TillKit on a device.
- A write changes some partitions, one after the other. If the device or the connection stops during the writes, the device can become unbootable.
- The usual recovery from that condition is an EDL restore from a full backup. TillKit makes a full backup as a step of the GSI installation. The `WRITE` screen can write that full backup to the device again. Read "Full restore" below.
- The GSI installation erases the user data of the device.
- Keep a copy of the full backup in a different location. If the full backup on the computer is damaged, a restore is not possible.

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

### EDL mode and the loader file

The GSI installation does not use root access. These two items are necessary:

1. The device must be in EDL mode and connected to the computer. If the device has an ADB connection, the `ADVANCED` screen can start it again in EDL mode.
2. The Firehose loader file of this device is necessary. TillKit does not include this file.

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
| WRITE | You select one image file and push `WRITE`. The tool does the steps in sequence, from the plan to the check. |
| ROOT | Downloads Magisk, adds the Magisk patch to the `init_boot` of the device and writes it. |
| ADVANCED | Starts the device again in system, recovery, bootloader (fastboot) or EDL mode. |
| INFO | Shows the items that the tool does not do, and the notices. |

### Image for the WRITE screen

Select one image file. The name of the file is not important.
The tool reads the header of the file and finds the type of the image.

| Image | Function |
| --- | --- |
| A boot image without a kernel | A root patch from a different tool, for example an `init_boot` image from the Magisk app. TillKit signs it and writes `init_boot` and `vbmeta`. |
| A boot image with a kernel | A kernel image. TillKit signs it and writes `boot` and `vbmeta`. TillKit cannot use a compressed kernel image. |
| A system image | A GSI. TillKit installs it on a device that you connect in EDL mode. Read "GSI installation" below. |

The tool finds the backup for a write in the folder of the `BACKUP` screen. You do not select the backup on the `WRITE` screen.

### GSI installation

When you select a system image on the `WRITE` screen, these steps open in sequence.

1. Connection: Select the Firehose loader file. The tool connects to the device in EDL mode.
2. Full backup: The tool reads all of the device into `full-backups` in the data folder. If the data folder has a full backup, the tool uses the latest one. You can also select a full backup in a different folder.
3. Plan: The tool makes a plan file for the write. If the data folder has plans for the same image, the tool uses the latest of those plans and the full backup of that plan. Thus an installation that stopped continues with its first plan. If the tool cannot read that full backup, it stops and tells you. If you selected the folder of the full backup, the tool uses only a plan for that full backup. The tool makes a new plan only from a full backup in which the partition tables show slot A as active and slot B as not active. A full backup of a device with the stock firmware is in that condition. If a full backup is not in that condition, the step stops before it makes files and tells you. An example is a full backup of a device that has a GSI.
4. Check: The tool reads the device and compares it with the plan. The check finds a first installation or an installation that stopped. The tool does not write to the device before step 5.
5. Write: After your approval, the tool writes the images and the partition tables, and it changes the boot LUN. Then it reads all that it wrote and compares the hashes.
6. User data: The tool erases the user data immediately after the write. The approval of step 5 is also the approval for this step.
7. Restart: The tool gets a different approval for the restart.

![The WRITE screen after all the steps of a GSI installation](READMEs/Images/GsiInstallationDone.png)

- If the write does not complete, the device can become unbootable. Keep the device in EDL mode and do the check again. If the check finds the step that stopped, the tool continues from that step. To write the full backup to the device again, use the full restore.
- If the check finds a partition table that the plan does not write and that is different from the full backup, the check stops. The tool tells you to write the full backup to the device first.
- TillKit cannot compare the rollback values that the device keeps with the values of the images.
- TillKit cannot tell if the device started from the GSI after the restart.

### Full restore

The full restore writes a full backup to the device again. When the GSI installation has a full backup in step 2, `RESTORE TO DEVICE` opens the full restore. The full restore uses the connection of step 1 and the full backup of step 2.

1. Check: The tool reads the device and compares each area with the hash in the full backup. It shows the number and the names of the different areas, the quantity of data to write and the boot LUN. The user data is part of the restore by default. If you set it to off, the tool does not compare or write `userdata`.
2. Restore: After your approval, the tool writes only the different areas. It reads each area again and compares the hashes. Then it selects the boot LUN.
3. Restart: The restart is not necessary. You can do the check of the GSI installation again on the same connection.

![The full restore after the restore step](READMEs/Images/FullRestoreDone.png)

- If the device is the same as the full backup, the tool tells you and does not write.
- If a partition on LUN 1 or LUN 2 is different, the tool does not write. These partitions hold the first boot images.
- The tool finds the boot LUN from the active flag of `xbl` in the partition tables of the full backup. The tool cannot make sure that the device started from that LUN when you made the full backup.
- TillKit cannot make sure that the full backup is from this device. The restore also writes the calibration data of the full backup. This data is different on each device.
- If you set the user data to off, Android can tell you to erase the user data at the next start.
- If the restore stops, keep the device in EDL mode and do the check again. The tool writes only the areas that are different.

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
- TillKit does not make other types of root patch. You can write an `init_boot` image from a different tool on the `WRITE` screen.

### ADVANCED

- The modes that you can select change with the connection of the device.
- To go from EDL mode to a usual start, the Firehose loader file of this device is necessary. TillKit does not include this file.
- If a write has a result that is not known, TillKit does not send a restart.

## No test on a device

No function of the TillKit screens operated on a device.
The checks for these items used a simulated device and a local shell only.

Write functions:

- All steps of the GSI installation: the EDL connection, the full backup, the check, the write, the removal of the user data and the restart.
- A GSI installation that stopped and that the tool continues from the step that stopped.
- The full restore: the check, the write, the boot LUN and the restart.
- The start of the device from a GSI that TillKit wrote.
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
The tool keeps one ADB key for a USB connection without an ADB server.
That key is not in the data folder.
On Windows, the key is in `%LOCALAPPDATA%\ltbox\adb\adbkey`.

## Build from source

Install [Rust through rustup](https://rustup.rs/) and the native build tools of your system.
The repository pins the Rust toolchain in [rust-toolchain.toml](rust-toolchain.toml).

```sh
cargo build --release --locked -p tillkit
```

The executable is `target/release/tillkit` (`tillkit.exe` on Windows).
The only builds until now are Windows builds.

If you start the executable from `target/release`, the default data folder is `target/release/tillkit-data`.
`cargo clean` removes that folder together with the build, and your backups go with it.
Before you make a backup, copy the executable to a different folder or select a data folder on the `BACKUP` screen.

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
