#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod adb_shell;
mod app;
mod app_gsi;
mod app_kernel;
mod clock;
mod data_paths;
mod diagnosis;
mod fonts;
mod grain;
mod gsi_action;
mod gsi_flow;
mod gsi_message;
mod gsi_review;
mod gsi_worker;
mod install_review;
mod kernel_message;
mod kernel_worker;
mod launch;
mod message;
mod palette;
mod plan_review;
mod plan_source;
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
        .font(fonts::BODY_BYTES)
        .font(fonts::DISPLAY_BYTES)
        .run()
}
