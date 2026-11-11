use crate::app::App;
use crate::confirmation::Confirmation;
use crate::message::Message;
use crate::palette::Palette;
use crate::view::{parts, rail, support_rows};
use iced::widget::{Column, Space, column, container, opaque, row};
use iced::{Color, Element, Length, Padding};

const DIALOG_WIDTH: f32 = 620.0;
const SCRIM_ALPHA: f32 = 0.92;
const FORCE_WARNING: &str = "이 기기는 지원하는 조건과 맞지 않습니다. 그래도 진행하면 맞지 않는 기기에 기록하게 되어, 기기가 부팅하지 못할 수 있습니다.";
const FORCE_KEPT: &str =
    "파일 대조와 기록 뒤의 확인은 그대로 합니다. 기기를 다시 확인하면 강제 진행이 풀립니다.";

pub fn layer(app: &App, confirmation: Confirmation) -> Element<'_, Message> {
    let palette = app.palette;
    let dialog = match confirmation {
        Confirmation::ForceSupport => force_support(app),
    };
    let scrim = Color {
        a: SCRIM_ALPHA,
        ..palette.paper
    };
    let placed = container(container(dialog).max_width(DIALOG_WIDTH))
        .padding(Padding {
            top: 96.0,
            left: rail::RAIL_WIDTH + 33.5,
            right: 32.0,
            bottom: 32.0,
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
    content = content
        .push(parts::body(FORCE_KEPT, palette))
        .push(answers("CONTINUE ANYWAY", palette));
    parts::block(content, palette.tint_warn, palette)
}

fn answers<'a>(confirm: &str, palette: Palette) -> Element<'a, Message> {
    column![
        Space::new().height(4),
        row![
            parts::go(confirm, Some(Message::Confirm), palette),
            parts::bracket("CANCEL", Some(Message::Cancel), palette),
        ]
        .spacing(18)
        .align_y(iced::Alignment::Center),
    ]
    .into()
}
