use crate::app::App;
use crate::fonts;
use crate::message::Message;
use crate::palette::Palette;
use crate::screen::Screen;
use crate::view::parts;
use iced::widget::{Column, Space, button, column, container, row, text};
use iced::{Color, Element, Length};

pub const RAIL_WIDTH: f32 = 196.0;

pub fn view(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let mut navigation = Column::new().spacing(2);
    for screen in Screen::ALL {
        navigation = navigation.push(item(screen, app.screen, palette));
    }
    let theme_caption = if palette.is_dusk { "PAPER" } else { "DUSK" };
    let content = column![
        text("FRONTBOX")
            .font(fonts::MONO_BOLD)
            .size(20)
            .color(palette.ink),
        parts::label(
            format!("FRONT 2 TOOL · V{}", env!("CARGO_PKG_VERSION")),
            palette
        ),
        Space::new().height(18),
        navigation,
        Space::new().height(Length::Fill),
        parts::bracket(theme_caption, Some(Message::ToggleTheme), palette),
        parts::label("MODIFIED LTBOX · GPL-3.0-OR-LATER", palette),
    ]
    .spacing(6);
    container(content)
        .padding([24, 18])
        .width(Length::Fixed(RAIL_WIDTH))
        .height(Length::Fill)
        .into()
}

fn item<'a>(screen: Screen, current: Screen, palette: Palette) -> Element<'a, Message> {
    let font = if screen == current {
        fonts::MONO_BOLD
    } else {
        fonts::MONO
    };
    let face = button(
        text(screen.caption())
            .font(font)
            .size(12)
            .color(palette.ink),
    )
    .width(Length::Fill)
    .padding([6, 8])
    .on_press(Message::Navigate(screen))
    .style(move |_, status| parts::flat(status, palette));
    let line = selection_line(screen, current, palette);
    row![parts::bar(line, Length::Fixed(3.0), Length::Fill), face]
        .spacing(4)
        .height(Length::Shrink)
        .into()
}

fn selection_line(screen: Screen, current: Screen, palette: Palette) -> Color {
    if screen != current {
        return Color::TRANSPARENT;
    }
    if screen.has_selection_line() {
        palette.ink
    } else {
        palette.idol
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::{DUSK, PAPER};

    #[test]
    fn letter_1161_only_the_selected_item_has_a_line_and_one_selection_line_is_idol() {
        for palette in [PAPER, DUSK] {
            for current in Screen::ALL {
                let lines = Screen::ALL.map(|screen| selection_line(screen, current, palette));
                let drawn = lines
                    .iter()
                    .filter(|line| **line != Color::TRANSPARENT)
                    .count();
                assert_eq!(drawn, 1);
            }
            for screen in [Screen::Device, Screen::Gsi, Screen::Scope] {
                assert_eq!(selection_line(screen, screen, palette), palette.idol);
            }
            for screen in [Screen::Pairs, Screen::Kernel, Screen::Write] {
                assert_eq!(selection_line(screen, screen, palette), palette.ink);
            }
        }
    }
}
