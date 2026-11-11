#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod adb_shell;
mod app;
mod app_reboot;
mod app_root;
mod app_write;
mod clock;
mod confirmation;
mod data_paths;
mod diagnosis;
mod folder_refusal;
mod fonts;
mod grain;
mod gsi_review;
mod gsi_worker;
mod install_review;
mod kernel_worker;
mod launch;
mod message;
mod pair_session;
mod palette;
mod plan_review;
mod plan_source;
mod reboot;
mod reboot_flow;
mod reboot_message;
mod reboot_words;
mod root_message;
mod root_review;
mod root_worker;
mod saved_backup;
mod screen;
mod step_status;
mod support_words;
mod view;
mod worker;
mod write_flow;
mod write_kind;
mod write_message;
mod write_origin;
mod write_step;

use app::App;
use front2::magisk::{dispatch_magiskboot_helper, register_magiskboot_host};
use iced::Size;

const WINDOW_SIZE: Size = Size::new(1180.0, 780.0);
const MINIMUM_WINDOW_SIZE: Size = Size::new(920.0, 600.0);

fn main() -> iced::Result {
    if let Some(code) = dispatch_magiskboot_helper() {
        std::process::exit(code);
    }
    if let Ok(executable) = std::env::current_exe() {
        let _ = register_magiskboot_host(executable);
    }
    iced::application(App::new, App::update, App::view)
        .title("TillKit")
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
