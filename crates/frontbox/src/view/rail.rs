use crate::app::App;
use crate::fonts;
use crate::message::Message;
use crate::palette::Palette;
use crate::screen::Screen;
use crate::view::parts;
use iced::widget::{Column, Space, button, column, container, text};
use iced::{Border, Element, Length};

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
            format!("FRONT 2 PAIR TOOL · V{}", env!("CARGO_PKG_VERSION")),
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
    let selected = screen == current;
    let word = if selected { palette.paper } else { palette.ink };
    button(
        text(screen.caption())
            .font(fonts::MONO)
            .size(12)
            .color(word),
    )
    .width(Length::Fill)
    .padding([6, 8])
    .on_press(Message::Navigate(screen))
    .style(move |_, status| item_style(selected, status, palette))
    .into()
}

fn item_style(selected: bool, status: button::Status, palette: Palette) -> button::Style {
    if !selected {
        return parts::flat(status, palette);
    }
    button::Style {
        background: Some(palette.ink.into()),
        text_color: palette.paper,
        border: Border::default(),
        ..button::Style::default()
    }
}
