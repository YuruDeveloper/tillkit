use crate::palette::Palette;
use iced::widget::image::Handle;

pub const GRAIN_WIDTH: u32 = 1600;
pub const GRAIN_HEIGHT: u32 = 1000;
const MAXIMUM_ALPHA: u32 = 14;
const MAXIMUM_DUSK_ALPHA: u32 = 8;
const SEED: u32 = 0x2f6f_4f01;

fn maximum_alpha(palette: Palette) -> u32 {
    if palette.is_dusk {
        MAXIMUM_DUSK_ALPHA
    } else {
        MAXIMUM_ALPHA
    }
}

pub fn pixels(palette: Palette) -> Vec<u8> {
    let [red, green, blue, _] = palette.ink.into_rgba8();
    let limit = maximum_alpha(palette);
    let mut state = SEED;
    let mut bytes = Vec::with_capacity((GRAIN_WIDTH * GRAIN_HEIGHT * 4) as usize);
    for _ in 0..GRAIN_WIDTH * GRAIN_HEIGHT {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let alpha = (state >> 8) % (limit + 1);
        bytes.extend_from_slice(&[red, green, blue, alpha as u8]);
    }
    bytes
}

pub fn handle(palette: Palette) -> Handle {
    Handle::from_rgba(GRAIN_WIDTH, GRAIN_HEIGHT, pixels(palette))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::{DUSK, PAPER};

    fn is_faint_ink(pixel: &[u8], ink: [u8; 3], limit: u32) -> bool {
        pixel[..3] == ink && u32::from(pixel[3]) <= limit
    }

    #[test]
    fn grain_stays_faint_and_takes_the_ink_of_the_theme() {
        for palette in [PAPER, DUSK] {
            let bytes = pixels(palette);
            let [red, green, blue, _] = palette.ink.into_rgba8();
            let mut grains = bytes.chunks_exact(4);
            assert_eq!(bytes.len(), (GRAIN_WIDTH * GRAIN_HEIGHT * 4) as usize);
            let limit = maximum_alpha(palette);
            assert!(limit <= MAXIMUM_ALPHA);
            assert!(grains.all(|pixel| is_faint_ink(pixel, [red, green, blue], limit)));
            assert!(bytes.chunks_exact(4).any(|pixel| pixel[3] > 0));
        }
    }
}
