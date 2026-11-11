use crate::message::Message;
use crate::palette::Palette;
use crate::support_words;
use crate::view::parts;
use front2::{SupportCheck, SupportReport};
use iced::widget::{column, container, row};
use iced::{Element, Length};

const SIDE_WIDTH: f32 = 44.0;

pub fn unmet<'a>(report: &SupportReport, palette: Palette) -> Element<'a, Message> {
    let rows = support_words::unmet(report)
        .into_iter()
        .map(|check| unmet_row(check, palette))
        .collect();
    parts::ruled(rows, palette)
}

fn unmet_row<'a>(check: &SupportCheck, palette: Palette) -> Element<'a, Message> {
    let sides = column![
        side("현재", support_words::observed(check), palette),
        side("필요", &check.expected, palette),
    ]
    .spacing(3);
    row![
        container(parts::body(support_words::name(check).to_string(), palette))
            .width(Length::Fixed(parts::KEY_WIDTH)),
        container(sides).width(Length::Fill),
    ]
    .spacing(12)
    .into()
}

fn side<'a>(caption: &str, content: &str, palette: Palette) -> Element<'a, Message> {
    row![
        container(parts::body(caption.to_string(), palette)).width(Length::Fixed(SIDE_WIDTH)),
        container(parts::value(content.to_string(), palette)).width(Length::Fill),
    ]
    .align_y(iced::Alignment::Center)
    .into()
}
