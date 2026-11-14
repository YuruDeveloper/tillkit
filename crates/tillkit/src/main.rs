#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod adb_shell;
mod app;
mod app_full_restore;
mod app_install;
mod app_reboot;
mod app_restore;
mod app_root;
mod app_write;
mod clock;
mod confirmation;
mod counted_storage;
mod data_paths;
mod device_reading;
mod diagnosis;
mod edl_link;
mod edl_storage;
mod folder_choice;
mod fonts;
mod full_backup_store;
mod full_restore_flow;
mod full_restore_message;
#[cfg(debug_assertions)]
mod full_restore_preview;
mod full_restore_review;
mod full_restore_stage;
mod full_restore_worker;
mod grain;
mod gsi_review;
mod gsi_worker;
mod image_refusal;
mod install_device;
mod install_flow;
mod install_message;
#[cfg(debug_assertions)]
mod install_preview;
mod install_review;
mod install_stage;
mod install_worker;
mod kernel_worker;
mod launch;
mod message;
mod pair_session;
mod palette;
mod plan_build;
mod plan_review;
mod plan_source;
mod prepared_install;
mod reboot;
mod reboot_flow;
mod reboot_message;
mod reboot_words;
mod restore_session;
mod root_message;
mod root_review;
mod root_worker;
mod saved_backup;
mod screen;
mod step_status;
mod stored_image;
mod support_words;
mod view;
mod worker;
mod write_flow;
mod write_kind;
mod write_message;
mod write_origin;
mod write_step;

#[cfg(test)]
#[expect(
    dead_code,
    reason = "the tests of TillKit use a part of this fixture of front2"
)]
#[path = "../../front2/tests/full_backup_support/mod.rs"]
mod full_backup_support;
#[cfg(test)]
#[path = "../../front2/tests/gpt_support/mod.rs"]
mod gpt_support;
#[cfg(test)]
#[path = "../../front2/tests/install_support/mod.rs"]
mod install_support;

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
        .subscription(App::subscription)
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
