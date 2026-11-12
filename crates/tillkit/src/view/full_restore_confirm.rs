use crate::app::App;
use crate::full_restore_review::FullRestoreReview;
use crate::message::Message;
use crate::palette::Palette;
use crate::view::confirm::{answers, stated};
use crate::view::install::gibibytes;
use crate::view::parts;
use iced::Element;
use iced::widget::{Column, column};

const WRITE_WARNING: &str = "아래 영역을 고른 전체 백업의 내용으로 기기에 기록합니다. 기록하는 동안에는 USB 케이블을 뽑거나 기기를 끄지 말아 주세요.";
const NOT_THIS_DEVICE: &str = "이 백업이 이 기기의 것인지는 TillKit이 확인하지 못합니다. 기기마다 다른 보정 자료도 백업의 것으로 바뀝니다.";
const USER_DATA_KEPT: &str =
    "사용자 데이터는 그대로 둡니다. 순정으로 부팅하면 Android가 초기화를 요구할 수 있습니다.";
const WRITE_KEPT: &str = "기록하기 전에 백업 파일을 SHA-256으로 다시 확인하고, 기록한 영역은 모두 다시 읽어서 확인합니다. 도중에 멈추면 기기를 EDL 모드로 연결해 둔 채로 다시 점검해 주세요. 다시 비교하면 아직 백업과 다른 영역만 기록합니다.";
const NOT_TESTED: &str =
    "TillKit으로 복원한 기기가 부팅하는지는 아직 실제 기기에서 확인하지 못했습니다.";
const RESTART_QUESTION: &str = "기기를 지금 다시 시작합니다. 다시 시작하면 이 연결은 끊어집니다. 설치를 이어서 할 때에는 다시 시작하지 않고, 같은 연결에서 설치의 CHECK DEVICE를 새로 하면 됩니다. 나중에 하려면 LATER를 눌러 주세요.";

pub fn write(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let mut content = Column::new()
        .spacing(12)
        .push(parts::warned("RESTORE THE FULL BACKUP", palette))
        .push(parts::body(WRITE_WARNING, palette));
    if let Some(review) = &app.full_restore.review {
        content = content
            .push(facts(review, palette))
            .push(parts::body(boot_lun_sentence(review.boot_lun), palette));
        if !review.with_user_data {
            content = content.push(parts::body(USER_DATA_KEPT, palette));
        }
    }
    content = content
        .push(parts::body(NOT_THIS_DEVICE, palette))
        .push(parts::body(WRITE_KEPT, palette))
        .push(parts::body(NOT_TESTED, palette))
        .push(answers("RESTORE", "CANCEL", palette));
    parts::block(content, palette.tint_warn, palette)
}

pub fn boot_lun_sentence(lun: u8) -> String {
    format!(
        "부팅 위치를 LUN {lun}로 맞춥니다. 이 값은 백업의 파티션 표에 있는 활성 표시에서 정한 것입니다."
    )
}

fn facts<'a>(review: &FullRestoreReview, palette: Palette) -> Element<'a, Message> {
    let counted = format!("{} / {}", review.different.len(), review.areas);
    let user_data = if review.with_user_data {
        "백업의 내용으로 되돌림"
    } else {
        "지금 그대로 둠"
    };
    let rows = vec![
        stated(
            "전체 백업",
            parts::value(review.backup.display().to_string(), palette),
            palette,
        ),
        stated("다른 영역", parts::value(counted, palette), palette),
        stated(
            "기록할 영역",
            parts::value(review.different.join(" · "), palette),
            palette,
        ),
        stated(
            "기록할 양",
            parts::value(gibibytes(review.different_bytes), palette),
            palette,
        ),
        stated(
            "부팅 위치",
            parts::value(format!("LUN {}", review.boot_lun), palette),
            palette,
        ),
        stated("사용자 데이터", parts::body(user_data, palette), palette),
    ];
    parts::ruled(rows, palette)
}

pub fn restart<'a>(palette: Palette) -> Element<'a, Message> {
    let content = column![
        parts::strong("RESTART THE DEVICE", palette),
        parts::body(RESTART_QUESTION, palette),
        answers("RESTART", "LATER", palette),
    ]
    .spacing(12);
    parts::panel(content, palette)
}
