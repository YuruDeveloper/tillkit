use iced::Font;
use iced::font::Weight;

pub const MONO_REGULAR_BYTES: &[u8] = include_bytes!("../assets/fonts/IBMPlexMono-Regular.ttf");
pub const MONO_BOLD_BYTES: &[u8] = include_bytes!("../assets/fonts/IBMPlexMono-Bold.ttf");
pub const BODY_BYTES: &[u8] = include_bytes!("../assets/fonts/IBMPlexSansKR-Regular.ttf");
pub const DISPLAY_BYTES: &[u8] = include_bytes!("../assets/fonts/ArchivoNarrow-Bold.ttf");

pub const MONO: Font = Font::with_name("IBM Plex Mono");
pub const MONO_BOLD: Font = Font {
    weight: Weight::Bold,
    ..Font::with_name("IBM Plex Mono")
};
pub const BODY: Font = Font::with_name("IBM Plex Sans KR");
pub const DISPLAY: Font = Font {
    weight: Weight::Bold,
    ..Font::with_name("Archivo Narrow")
};
