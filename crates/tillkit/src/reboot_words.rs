use crate::reboot::{DeviceConnection, RebootTarget};

const TO_SYSTEM: &str =
    "기기를 일반 부팅으로 다시 시작합니다. 기기의 저장 영역에는 기록하지 않습니다.";
const TO_RECOVERY: &str =
    "기기를 복구 모드로 다시 시작합니다. 기기의 저장 영역에는 기록하지 않습니다.";
const TO_BOOTLOADER: &str =
    "기기를 부트로더(fastboot)로 다시 시작합니다. 기기의 저장 영역에는 기록하지 않습니다.";
const TO_EDL: &str = "기기를 EDL 모드로 다시 시작합니다. EDL 모드에서는 화면이 꺼진 것처럼 보이고, 일반 부팅으로 돌아오려면 이 기기의 Firehose 로더 파일이 필요합니다.";

pub fn connection(connection: DeviceConnection) -> &'static str {
    match connection {
        DeviceConnection::Absent => "연결된 기기를 찾지 못했습니다",
        DeviceConnection::MultipleDevices => "기기가 여러 대 연결되어 있습니다",
        DeviceConnection::Android => "일반 부팅 (ADB)",
        DeviceConnection::Recovery => "복구 모드 (ADB)",
        DeviceConnection::Sideload => "복구 모드 (sideload)",
        DeviceConnection::Unauthorized => "USB 디버깅을 허용하지 않은 기기",
        DeviceConnection::AdbServerBlocking => "다른 ADB 서버가 쓰고 있는 기기",
        DeviceConnection::Bootloader => "부트로더 (fastboot)",
        DeviceConnection::FastbootUserspace => "fastbootd",
        DeviceConnection::Edl => "EDL",
    }
}

pub fn no_target(connection: DeviceConnection) -> &'static str {
    match connection {
        DeviceConnection::MultipleDevices => "기기를 한 대만 연결한 뒤에 다시 확인해 주세요.",
        DeviceConnection::Unauthorized => {
            "기기 화면에서 USB 디버깅을 허용한 뒤에 다시 확인해 주세요."
        }
        DeviceConnection::AdbServerBlocking => {
            "PC에서 돌고 있는 다른 ADB 프로그램을 끈 뒤에 다시 확인해 주세요."
        }
        _ => "USB 연결을 확인한 뒤에 다시 확인해 주세요.",
    }
}

pub fn code(target: RebootTarget) -> &'static str {
    match target {
        RebootTarget::System => "SYSTEM",
        RebootTarget::Recovery => "RECOVERY",
        RebootTarget::Bootloader => "BOOTLOADER",
        RebootTarget::Edl => "EDL",
    }
}

pub fn name(target: RebootTarget) -> &'static str {
    match target {
        RebootTarget::System => "일반 부팅",
        RebootTarget::Recovery => "복구 모드",
        RebootTarget::Bootloader => "부트로더 (fastboot)",
        RebootTarget::Edl => "EDL 모드",
    }
}

pub fn question(target: RebootTarget) -> &'static str {
    match target {
        RebootTarget::System => TO_SYSTEM,
        RebootTarget::Recovery => TO_RECOVERY,
        RebootTarget::Bootloader => TO_BOOTLOADER,
        RebootTarget::Edl => TO_EDL,
    }
}
