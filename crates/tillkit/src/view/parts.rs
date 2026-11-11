use crate::fonts;
use crate::message::Message;
use crate::palette::Palette;
use iced::widget::{Column, Space, button, column, container, row, scrollable, text, text_input};
use iced::{Border, Color, Element, Length, Theme};

pub const KEY_WIDTH: f32 = 210.0;
pub const VALUE_COLUMN_CHARACTERS: usize = 36;

pub fn fill(color: Color) -> container::Style {
    container::Style {
        background: Some(color.into()),
        ..container::Style::default()
    }
}

pub fn bar<'a>(color: Color, width: Length, height: Length) -> Element<'a, Message> {
    container(Space::new())
        .width(width)
        .height(height)
        .style(move |_| fill(color))
        .into()
}

pub fn display<'a>(content: &'a str, palette: Palette) -> Element<'a, Message> {
    text(content)
        .font(fonts::DISPLAY)
        .size(40)
        .color(palette.ink)
        .into()
}

pub fn label<'a>(content: impl Into<String>, palette: Palette) -> Element<'a, Message> {
    text(content.into())
        .font(fonts::MONO)
        .size(11)
        .color(palette.ink_2)
        .into()
}

pub fn value<'a>(content: impl Into<String>, palette: Palette) -> Element<'a, Message> {
    text(content.into())
        .font(fonts::MONO)
        .size(12)
        .color(palette.ink)
        .into()
}

pub fn strong<'a>(content: impl Into<String>, palette: Palette) -> Element<'a, Message> {
    text(content.into())
        .font(fonts::MONO_BOLD)
        .size(13)
        .color(palette.ink)
        .into()
}

pub fn body<'a>(content: impl Into<String>, palette: Palette) -> Element<'a, Message> {
    text(content.into())
        .font(fonts::BODY)
        .size(14)
        .color(palette.ink)
        .into()
}

pub fn outline(color: Color, width: f32) -> Border {
    Border {
        color,
        width,
        radius: 0.0.into(),
    }
}

pub fn square<'a>(color: Color, edge: Option<Color>) -> Element<'a, Message> {
    let border = match edge {
        Some(edge) => outline(edge, 1.0),
        None => Border::default(),
    };
    container(Space::new())
        .width(8)
        .height(8)
        .style(move |_| fill(color).border(border))
        .into()
}

pub fn flat(status: button::Status, palette: Palette) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    let edge = if hovered {
        palette.ink
    } else {
        Color::TRANSPARENT
    };
    button::Style {
        background: None,
        text_color: palette.ink,
        border: outline(edge, 1.0),
        ..button::Style::default()
    }
}

pub fn go<'a>(caption: &str, message: Option<Message>, palette: Palette) -> Element<'a, Message> {
    let (word, line) = if message.is_some() {
        (palette.ink, palette.idol)
    } else {
        (palette.ink_2, palette.rule)
    };
    let face = column![
        text(format!("{caption} \u{2192}"))
            .font(fonts::MONO_BOLD)
            .size(13)
            .color(word),
        bar(line, Length::Fill, Length::Fixed(2.0)),
    ]
    .spacing(3)
    .width(Length::Shrink);
    button(face)
        .padding([5, 6])
        .on_press_maybe(message)
        .style(move |_, status| flat(status, palette))
        .into()
}

pub fn bracket<'a>(
    caption: &str,
    message: Option<Message>,
    palette: Palette,
) -> Element<'a, Message> {
    let word = if message.is_some() {
        palette.ink
    } else {
        palette.ink_2
    };
    button(
        text(format!("[ {caption} ]"))
            .font(fonts::MONO)
            .size(12)
            .color(word),
    )
    .padding([5, 6])
    .on_press_maybe(message)
    .style(move |_, status| flat(status, palette))
    .into()
}

pub fn block<'a>(
    content: impl Into<Element<'a, Message>>,
    ground: Color,
    palette: Palette,
) -> Element<'a, Message> {
    column![
        bar(palette.ink, Length::Fill, Length::Fixed(2.0)),
        container(content)
            .padding(16)
            .width(Length::Fill)
            .style(move |_| fill(ground)),
    ]
    .into()
}

pub fn panel<'a>(
    content: impl Into<Element<'a, Message>>,
    palette: Palette,
) -> Element<'a, Message> {
    block(content, palette.paper_2, palette)
}

pub fn running<'a>(caption: &str, palette: Palette) -> Element<'a, Message> {
    row![
        text("\u{25B6}")
            .font(fonts::MONO)
            .size(11)
            .color(palette.idol),
        strong(caption.to_string(), palette),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .into()
}

pub fn fault<'a>(
    caption: &str,
    sentence: &str,
    detail: &str,
    palette: Palette,
) -> Element<'a, Message> {
    let heading = row![
        square(palette.idol, None),
        strong(caption.to_string(), palette)
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);
    let mut content = column![heading, body(sentence.to_string(), palette)].spacing(8);
    if !detail.is_empty() {
        content = content.push(value(detail.to_string(), palette));
    }
    block(content, palette.tint_idol, palette)
}

pub fn caution<'a>(caption: &str, sentence: &str, palette: Palette) -> Element<'a, Message> {
    let heading = row![
        square(palette.warn, Some(palette.ink)),
        strong(format!("{caption} · warn"), palette)
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);
    let content = column![heading, body(sentence.to_string(), palette)].spacing(8);
    block(content, palette.tint_warn, palette)
}

pub fn stacked<'a>(
    key: &str,
    content: impl Into<String>,
    palette: Palette,
) -> Element<'a, Message> {
    column![label(key.to_string(), palette), value(content, palette)]
        .spacing(3)
        .into()
}

pub fn fits_value_column(content: &str) -> bool {
    content
        .split_whitespace()
        .all(|word| word.chars().count() <= VALUE_COLUMN_CHARACTERS)
}

pub fn key_value<'a>(
    key: &str,
    content: impl Into<String>,
    palette: Palette,
) -> Element<'a, Message> {
    let content = content.into();
    if !fits_value_column(&content) {
        return stacked(key, content, palette);
    }
    row![
        container(label(key.to_string(), palette)).width(Length::Fixed(KEY_WIDTH)),
        container(value(content, palette)).width(Length::Fill),
    ]
    .spacing(12)
    .into()
}

pub fn ruled<'a>(rows: Vec<Element<'a, Message>>, palette: Palette) -> Element<'a, Message> {
    let mut list = Column::new();
    for item in rows {
        list = list
            .push(bar(palette.rule, Length::Fill, Length::Fixed(1.0)))
            .push(container(item).padding([7, 0]));
    }
    list.push(bar(palette.rule, Length::Fill, Length::Fixed(1.0)))
        .into()
}

pub fn input<'a>(
    placeholder: &str,
    current: &str,
    on_input: fn(String) -> Message,
    palette: Palette,
) -> Element<'a, Message> {
    text_input(placeholder, current)
        .on_input(on_input)
        .font(fonts::MONO)
        .size(13)
        .padding([7, 9])
        .style(move |_, status| input_style(status, palette))
        .into()
}

fn input_style(status: text_input::Status, palette: Palette) -> text_input::Style {
    let focused = matches!(status, text_input::Status::Focused { .. });
    let width = if focused { 2.0 } else { 1.0 };
    text_input::Style {
        background: palette.paper.into(),
        border: outline(palette.ink, width),
        icon: palette.ink_2,
        placeholder: palette.ink_2,
        value: palette.ink,
        selection: palette.tint_warn,
    }
}

pub fn scroll_style(
    theme: &Theme,
    status: scrollable::Status,
    palette: Palette,
) -> scrollable::Style {
    let mut style = scrollable::default(theme, status);
    for rail in [&mut style.vertical_rail, &mut style.horizontal_rail] {
        rail.background = None;
        rail.border = Border::default();
        rail.scroller.background = palette.rule.into();
        rail.scroller.border = Border::default();
    }
    style
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_without_a_break_goes_below_its_key_when_it_is_long() {
        assert!(fits_value_column("5.15-android13-8"));
        assert!(fits_value_column(&"a".repeat(VALUE_COLUMN_CHARACTERS)));
        assert!(fits_value_column(
            "boot_b · /dev/block/sde21 · 100663296 bytes"
        ));
        assert!(!fits_value_column(&"a".repeat(VALUE_COLUMN_CHARACTERS + 1)));
        assert!(!fits_value_column(&"0123abcd".repeat(8)));
        assert!(!fits_value_column(
            "2597c218aae470a130f61162feaae70afd97f011"
        ));
    }
}
