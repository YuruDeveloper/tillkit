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
        text("TILLKIT")
            .font(fonts::MONO_BOLD)
            .size(20)
            .color(palette.ink),
        parts::label(
            format!("FRONT 2 TOOL · V{}", env!("CARGO_PKG_VERSION")),
            palette
        ),
        Space::new().height(18),
        navigation,
        Space::new().height(12),
        forced_mark(app),
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

fn forced_mark<'a>(app: &App) -> Element<'a, Message> {
    if !app.forced {
        return Space::new().into();
    }
    container(parts::warned("FORCED", app.palette))
        .padding([0, 12])
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

    fn drawn_line_count(current: Screen, palette: Palette) -> usize {
        Screen::ALL
            .map(|screen| selection_line(screen, current, palette))
            .iter()
            .filter(|line| **line != Color::TRANSPARENT)
            .count()
    }

    fn selected_line_colors<const COUNT: usize>(
        screens: [Screen; COUNT],
        palette: Palette,
    ) -> [Color; COUNT] {
        screens.map(|screen| selection_line(screen, screen, palette))
    }

    #[test]
    fn letter_1161_only_the_selected_item_has_a_line_and_one_selection_line_is_idol() {
        for palette in [PAPER, DUSK] {
            let counts = Screen::ALL.map(|current| drawn_line_count(current, palette));
            assert!(counts.iter().all(|count| *count == 1));
            assert_eq!(
                selected_line_colors([Screen::Device, Screen::Backup, Screen::Info], palette),
                [palette.idol; 3]
            );
            assert_eq!(
                selected_line_colors([Screen::Write], palette),
                [palette.ink; 1]
            );
        }
    }
}
