#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod adb_shell;
mod app;
mod clock;
mod data_paths;
mod diagnosis;
mod fonts;
mod grain;
mod launch;
mod message;
mod palette;
mod plan_review;
mod screen;
mod step_status;
mod view;
mod worker;
mod write_flow;
mod write_step;

use app::App;
use iced::Size;

const WINDOW_SIZE: Size = Size::new(1180.0, 780.0);
const MINIMUM_WINDOW_SIZE: Size = Size::new(920.0, 600.0);

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .title("FrontBox")
        .theme(App::theme)
        .settings(iced::Settings {
            default_font: fonts::BODY,
            ..iced::Settings::default()
        })
        .window(iced::window::Settings {
            size: WINDOW_SIZE,
            min_size: Some(MINIMUM_WINDOW_SIZE),
            ..iced::window::Settings::default()
        })
        .font(fonts::MONO_REGULAR_BYTES)
        .font(fonts::MONO_BOLD_BYTES)
        .run()
}
