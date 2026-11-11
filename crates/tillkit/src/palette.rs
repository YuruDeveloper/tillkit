use iced::{Color, color};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub paper: Color,
    pub paper_2: Color,
    pub ink: Color,
    pub ink_2: Color,
    pub rule: Color,
    pub idol: Color,
    pub warn: Color,
    pub tint_warn: Color,
    pub tint_idol: Color,
    pub hidden: Color,
    pub is_dusk: bool,
}

pub const PAPER: Palette = Palette {
    paper: color!(0xf1ede4),
    paper_2: color!(0xe7e2d6),
    ink: color!(0x161513),
    ink_2: color!(0x55524b),
    rule: color!(0x7e7c76),
    idol: color!(0xd7372b),
    warn: color!(0xc9a43a),
    tint_warn: color!(0xe9dfc5),
    tint_idol: color!(0xf1ccbf),
    hidden: color!(0x2f6f4f),
    is_dusk: false,
};

pub const DUSK: Palette = Palette {
    paper: color!(0x3a3732),
    paper_2: color!(0x45413b),
    ink: color!(0xece6da),
    ink_2: color!(0xb5ae9f),
    rule: color!(0x918c82),
    idol: color!(0xd7372b),
    warn: color!(0xc9b45a),
    tint_warn: color!(0x5f583f),
    tint_idol: color!(0x663d33),
    hidden: color!(0x2f6f4f),
    is_dusk: true,
};

impl Palette {
    pub fn for_system() -> Self {
        match dark_light::detect() {
            Ok(dark_light::Mode::Dark) => DUSK,
            _ => PAPER,
        }
    }

    pub fn toggled(self) -> Self {
        if self.is_dusk { PAPER } else { DUSK }
    }

    pub fn theme(self) -> iced::Theme {
        let name = if self.is_dusk { "dusk" } else { "paper" };
        iced::Theme::custom(
            name.to_string(),
            iced::theme::Palette {
                background: self.paper,
                text: self.ink,
                primary: self.ink,
                success: self.ink,
                warning: self.warn,
                danger: self.idol,
            },
        )
    }
}
