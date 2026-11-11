pub mod device;
pub mod pairs;
pub mod parts;
pub mod rabbit;
pub mod rail;
pub mod scope;
pub mod write;

use crate::app::App;
use crate::message::Message;
use crate::screen::Screen;
use iced::widget::image::FilterMethod;
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::{Space, container, image, row, scrollable, stack};
use iced::{ContentFit, Element, Length};

const CONTENT_WIDTH: f32 = 860.0;
const MARGIN_WIDTH: f32 = 120.0;

pub fn root(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let content = match app.screen {
        Screen::Device => device::view(app),
        Screen::Pairs => pairs::view(app),
        Screen::Write => write::view(app),
        Screen::Scope => scope::view(app),
    };
    let center = scrollable(
        container(content)
            .padding([28, 32])
            .max_width(CONTENT_WIDTH),
    )
    .direction(Direction::Vertical(
        Scrollbar::new().width(4).scroller_width(4),
    ))
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |theme, status| parts::scroll_style(theme, status, palette));
    let layout = row![
        rail::view(app),
        parts::bar(palette.ink, Length::Fixed(1.5), Length::Fill),
        center,
        Space::new().width(Length::Fixed(MARGIN_WIDTH)),
    ];
    let grain = image(app.grain.clone())
        .width(Length::Fill)
        .height(Length::Fill)
        .content_fit(ContentFit::Cover)
        .filter_method(FilterMethod::Nearest);
    container(stack![grain, layout])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| parts::fill(palette.paper))
        .into()
}
