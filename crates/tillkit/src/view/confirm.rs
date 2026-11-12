use crate::app::App;
use crate::clock::format_utc;
use crate::confirmation::Confirmation;
use crate::fonts;
use crate::message::Message;
use crate::palette::Palette;
use crate::plan_review::PlanReview;
use crate::reboot_words;
use crate::support_words;
use crate::view::{full_restore_confirm, install_confirm, parts, rail, support_rows};
use crate::write_origin::WriteOrigin;
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::{Column, Space, column, container, opaque, row, scrollable, text};
use iced::{Color, Element, Length, Padding};

const DIALOG_WIDTH: f32 = 620.0;
const SCRIM_ALPHA: f32 = 0.92;
const NAME_WIDTH: f32 = 130.0;
const FORCE_WARNING: &str = "이 기기는 지원하는 조건과 맞지 않습니다. 그래도 진행하면 확인되지 않은 기기에 기록하게 됩니다. 이 도구는 이런 기기가 기록한 뒤에 부팅하는 것을 보장하지 않습니다.";
const FORCE_KEPT: &str = "파일 대조와 기록 뒤의 확인은 그대로 합니다. 기록을 준비할 때 기기를 다시 읽어서, 위 항목과 하나라도 다르면 기록하지 않습니다. 기기를 다시 확인하면 강제 진행이 풀립니다.";
const WRITE_WARNING: &str =
    "아래 내용을 기기에 기록합니다. 기록하는 동안에는 USB 케이블을 뽑거나 기기를 끄지 말아 주세요.";
const WRITE_KEPT: &str = "기록이 중간에 실패하면 백업해 둔 이미지로 되돌립니다. 사용자 데이터는 지우지 않습니다. 기록하는 도중에 전원이 끊기면 기기가 부팅하지 못할 수 있습니다.";
const FORCED_WARNING: &str = "지원하는 조건과 맞지 않는 기기에 강제로 진행하는 기록입니다. 기록한 뒤에 기기가 부팅하는 것을 보장하지 않습니다.";
const KERNEL_WARNING: &str =
    "기록한 커널로 부팅하지 못하면, 이 프로그램으로는 이전 커널로 되돌릴 수 없습니다.";
const ROOT_WORK: &str = "Magisk 루트 패치 기록";
const RESTORE_WORK: &str = "백업한 이미지를 기기에 다시 기록";
const ROOT_WARNING: &str = "TillKit이 Magisk로 직접 패치한 이미지로 이 기기가 부팅하는지는 아직 실제 기기에서 확인하지 못했습니다.";
const REBOOT_QUESTION: &str = "기록한 내용을 다시 읽어서 확인했습니다. 기기를 지금 다시 시작하면 새 이미지로 부팅합니다. 나중에 하려면 LATER를 눌러 주세요.";

pub fn layer(app: &App, confirmation: Confirmation) -> Element<'_, Message> {
    let palette = app.palette;
    let dialog = match confirmation {
        Confirmation::ForceSupport => force_support(app),
        Confirmation::Write => write(app),
        Confirmation::Reboot => reboot(palette),
        Confirmation::RebootTo => reboot_to(app),
        Confirmation::InstallWrite => install_confirm::write(app),
        Confirmation::InstallRestart => install_confirm::restart(palette),
        Confirmation::FullRestoreWrite => full_restore_confirm::write(app),
        Confirmation::FullRestoreRestart => full_restore_confirm::restart(palette),
    };
    let scrim = Color {
        a: SCRIM_ALPHA,
        ..palette.paper
    };
    let dialog = scrollable(dialog)
        .direction(Direction::Vertical(
            Scrollbar::new().width(4).scroller_width(4),
        ))
        .style(move |theme, status| parts::scroll_style(theme, status, palette));
    let placed = container(container(dialog).max_width(DIALOG_WIDTH))
        .padding(Padding {
            top: 96.0,
            left: rail::RAIL_WIDTH + 33.5,
            right: 32.0,
            bottom: 16.0,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| parts::fill(scrim));
    opaque(placed)
}

fn force_support(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let mut content = Column::new().spacing(12);
    content = content.push(parts::warned("CONTINUE ANYWAY", palette));
    content = content.push(parts::body(FORCE_WARNING, palette));
    if let Some(diagnosis) = &app.diagnosis {
        content = content.push(support_rows::unmet(&diagnosis.report, palette));
    }
    content = content.push(parts::body(FORCE_KEPT, palette)).push(answers(
        "CONTINUE ANYWAY",
        "CANCEL",
        palette,
    ));
    parts::block(content, palette.tint_warn, palette)
}

fn write(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let mut content = Column::new().spacing(12);
    content = content
        .push(parts::warned("WRITE TO DEVICE", palette))
        .push(parts::body(WRITE_WARNING, palette));
    if let (Some(source), Some(review)) = (source(app), &app.write.review) {
        content = content.push(write_facts(source, review, palette));
    }
    content = content.push(parts::body(WRITE_KEPT, palette));
    if app.write.origin == WriteOrigin::Magisk {
        content = content.push(parts::body(ROOT_WARNING, palette));
    }
    let forced = app
        .write
        .review
        .as_ref()
        .is_some_and(|review| !review.plan.support_override.is_empty());
    if forced {
        content = content.push(parts::body(FORCED_WARNING, palette));
    }
    let writes_kernel = app
        .write
        .review
        .as_ref()
        .is_some_and(|review| review.source.kernel_facts().is_some());
    if writes_kernel {
        content = content.push(parts::body(KERNEL_WARNING, palette));
    }
    content = content.push(answers("WRITE", "CANCEL", palette));
    parts::block(content, palette.tint_warn, palette)
}

fn source(app: &App) -> Option<[(&'static str, String); 2]> {
    if let Some((kind, image)) = app.write.chosen_image() {
        let image = image.display().to_string();
        return Some([("하는 일", kind.work().into()), ("고른 이미지", image)]);
    }
    if let Some(created_unix) = app.write.restored_backup {
        let backup = format_utc(created_unix);
        return Some([("하는 일", RESTORE_WORK.into()), ("고른 백업", backup)]);
    }
    let package = app.write.package.as_ref()?;
    let magisk = format!("{} · SHA-256 {}", package.version, package.short_sha256());
    Some([("하는 일", ROOT_WORK.into()), ("Magisk", magisk)])
}

fn write_facts<'a>(
    source: [(&'static str, String); 2],
    review: &PlanReview,
    palette: Palette,
) -> Element<'a, Message> {
    let [(work_name, work), (origin_name, origin)] = source;
    let targets: Vec<&str> = review
        .plan
        .writes
        .iter()
        .map(|write| write.target.label.as_str())
        .collect();
    let accepted: Vec<&str> = review
        .plan
        .support_override
        .accepted
        .iter()
        .map(support_words::name)
        .collect();
    let mut rows = vec![
        stated(work_name, parts::body(work, palette), palette),
        stated(origin_name, parts::value(origin, palette), palette),
        stated(
            "기록할 영역",
            parts::value(targets.join(" · "), palette),
            palette,
        ),
        stated(
            "실패 시 복구본",
            parts::value(review.plan.old_pair.clone(), palette),
            palette,
        ),
    ];
    if !accepted.is_empty() {
        rows.push(stated(
            "강제 진행 항목",
            parts::body(accepted.join(" · "), palette),
            palette,
        ));
    }
    parts::ruled(rows, palette)
}

pub fn stated<'a>(
    name: &str,
    content: Element<'a, Message>,
    palette: Palette,
) -> Element<'a, Message> {
    let name = text(name.to_string())
        .font(fonts::BODY)
        .size(14)
        .color(palette.ink);
    row![
        container(name).width(Length::Fixed(NAME_WIDTH)),
        container(content).width(Length::Fill),
    ]
    .spacing(12)
    .into()
}

fn reboot<'a>(palette: Palette) -> Element<'a, Message> {
    let content = column![
        parts::strong("REBOOT THE DEVICE", palette),
        parts::body(REBOOT_QUESTION, palette),
        answers("REBOOT", "LATER", palette),
    ]
    .spacing(12);
    parts::panel(content, palette)
}

fn reboot_to(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let mut content = Column::new()
        .spacing(12)
        .push(parts::strong("REBOOT THE DEVICE", palette));
    if let Some(target) = app.reboot.asked {
        content = content.push(parts::body(reboot_words::question(target), palette));
    }
    parts::panel(content.push(answers("REBOOT", "CANCEL", palette)), palette)
}

pub fn answers<'a>(confirm: &str, cancel: &str, palette: Palette) -> Element<'a, Message> {
    column![
        Space::new().height(4),
        row![
            parts::go(confirm, Some(Message::Confirm), palette),
            parts::bracket(cancel, Some(Message::Cancel), palette),
        ]
        .spacing(18)
        .align_y(iced::Alignment::Center),
    ]
    .into()
}
