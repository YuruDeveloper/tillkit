use crate::message::Message;
use crate::palette::Palette;
use crate::view::parts;
use iced::widget::{Column, Row, Space};
use iced::{Element, Length};

const PIXEL: f32 = 3.0;
const BITMAP: [&str; 12] = [
    "..#...#.....",
    "..#...#.....",
    "..#...#.....",
    "..##.##.....",
    ".#######....",
    ".#o#####....",
    ".########...",
    "..#########.",
    "..##########",
    "..##########",
    "..###..####.",
    "..##....##..",
];

pub fn view<'a>(palette: Palette) -> Element<'a, Message> {
    let mut picture = Column::new();
    for line in BITMAP {
        picture = picture.push(pixel_row(line, palette));
    }
    picture.into()
}

fn pixel_row<'a>(line: &str, palette: Palette) -> Element<'a, Message> {
    let mut cells = Row::new();
    for cell in line.chars() {
        cells = cells.push(pixel(cell, palette));
    }
    cells.into()
}

fn pixel<'a>(cell: char, palette: Palette) -> Element<'a, Message> {
    let size = Length::Fixed(PIXEL);
    match cell {
        '#' => parts::bar(palette.ink, size, size),
        'o' => parts::bar(palette.hidden, size, size),
        _ => Space::new().width(size).height(size).into(),
    }
}
