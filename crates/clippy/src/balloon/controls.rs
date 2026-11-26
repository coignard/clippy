// This file is part of Clippy.
//
// Copyright (c) 2026  René Coignard <contact@renecoignard.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use crate::assets::{CLIPPIT, FONT};
use crate::balloon::layout::{BASELINE, LINE_HEIGHT};
use crate::geometry::Rect;
use crate::graphics::{BLACK, Canvas, Color, WHITE, shade};

pub(super) const CHECKBOX: i32 = 13;
pub(super) const BUTTON_WIDTH: i32 = 60;
pub(super) const BUTTON_HEIGHT: i32 = 22;
pub(super) const BUTTON_PADDING: i32 = 11;
const BUTTON_BEVEL: i32 = 2;
const BUTTON_EDGE_SHADE: u16 = 75;
const BUTTON_PUSH: i32 = 1;
const FACE: Color = [0xD4, 0xD0, 0xC8, 0xFF];
const SHADOW: Color = [0x80, 0x80, 0x80, 0xFF];
const DARK: Color = [0x40, 0x40, 0x40, 0xFF];
const CHECKMARK_INSET: i32 = 3;
const CHECKMARK: [&str; 7] = [
    "......#", ".....##", "#...###", "##.###.", "#####..", ".###...", "..#....",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Checkbox {
    pub(crate) checked: bool,
    pub(crate) pressed: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Press(Option<(usize, bool)>);

impl Press {
    pub(crate) fn begin(&mut self, row: usize, rows: usize) {
        if row < rows {
            self.0 = Some((row, true));
        }
    }

    pub(crate) fn drag(&mut self, inside: bool) {
        if let Some((_, pressed)) = &mut self.0 {
            *pressed = inside;
        }
    }

    pub(crate) fn release(&mut self, inside: bool) -> Option<usize> {
        let (row, _) = self.0.take().filter(|_| inside)?;
        Some(row)
    }

    pub(crate) fn holds(self, row: usize) -> bool {
        self.0 == Some((row, true))
    }
}

pub(super) fn sunken(canvas: &mut Canvas, area: Rect, interior: Color) {
    canvas.fill(area, interior);
    for (width, top_left, bottom_right) in [(2, DARK, FACE), (1, SHADOW, WHITE)] {
        canvas.fill(Rect::new(area.x, area.y, width, area.height), top_left);
        canvas.fill(Rect::new(area.x, area.y, area.width, width), top_left);
        canvas.fill(
            Rect::new(area.right() - width, area.y, width, area.height),
            bottom_right,
        );
        canvas.fill(
            Rect::new(area.x, area.bottom() - width, area.width, width),
            bottom_right,
        );
    }
}

pub(super) fn checkbox(canvas: &mut Canvas, x: i32, y: i32, state: Checkbox) {
    let interior = if state.pressed { FACE } else { WHITE };
    sunken(canvas, Rect::new(x, y, CHECKBOX, CHECKBOX), interior);
    if !state.checked {
        return;
    }
    for (row, line) in (0..).zip(CHECKMARK) {
        for (column, pixel) in (0..).zip(line.chars()) {
            if pixel == '#' {
                canvas.plot(
                    x + CHECKMARK_INSET + column,
                    y + CHECKMARK_INSET + row,
                    BLACK,
                );
            }
        }
    }
}

pub(super) fn button(canvas: &mut Canvas, area: Rect, label: &str, pressed: bool) {
    let edge = button_edge();
    let (left, top, right, bottom) = (area.x, area.y, area.right() - 1, area.bottom() - 1);
    let (straight_width, straight_height) = (
        area.width - 2 * BUTTON_BEVEL,
        area.height - 2 * BUTTON_BEVEL,
    );
    canvas.fill(Rect::new(left + BUTTON_BEVEL, top, straight_width, 1), edge);
    canvas.fill(
        Rect::new(left + BUTTON_BEVEL, bottom, straight_width, 1),
        edge,
    );
    canvas.fill(
        Rect::new(left, top + BUTTON_BEVEL, 1, straight_height),
        edge,
    );
    canvas.fill(
        Rect::new(right, top + BUTTON_BEVEL, 1, straight_height),
        edge,
    );
    for step in 1..BUTTON_BEVEL {
        for y in [top + step, bottom - step] {
            canvas.plot(left + BUTTON_BEVEL - step, y, edge);
            canvas.plot(right - BUTTON_BEVEL + step, y, edge);
        }
    }
    let push = if pressed { BUTTON_PUSH } else { 0 };
    let x = left + (area.width - FONT.width(label)) / 2 + push;
    let baseline = top + (area.height - LINE_HEIGHT) / 2 + BASELINE + push;
    FONT.draw(canvas, label, x, baseline, CLIPPIT.balloon.text);
}

fn button_edge() -> Color {
    shade(CLIPPIT.balloon.fill, BUTTON_EDGE_SHADE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Size;
    use crate::graphics::{Bitmap, TRANSPARENT};

    fn pixels(state: Checkbox) -> Vec<String> {
        let mut canvas = Canvas::new(Size::new(CHECKBOX, CHECKBOX));
        checkbox(&mut canvas, 0, 0, state);
        rows(&canvas)
    }

    fn rows(canvas: &Canvas) -> Vec<String> {
        let size = canvas.size();
        (0..size.height)
            .map(|y| {
                (0..size.width)
                    .map(|x| match canvas.pixel(x, y) {
                        WHITE => 'w',
                        FACE => 'f',
                        SHADOW => 'g',
                        DARK => 'k',
                        BLACK => '#',
                        TRANSPARENT => ' ',
                        color if color == button_edge() => 'e',
                        _ => '?',
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn checkbox_matches_classic_windows() {
        assert_eq!(
            pixels(Checkbox {
                checked: true,
                pressed: false,
            }),
            [
                "ggggggggggggw",
                "gkkkkkkkkkkfw",
                "gkwwwwwwwwwfw",
                "gkwwwwwww#wfw",
                "gkwwwwww##wfw",
                "gkw#www###wfw",
                "gkw##w###wwfw",
                "gkw#####wwwfw",
                "gkww###wwwwfw",
                "gkwww#wwwwwfw",
                "gkwwwwwwwwwfw",
                "gfffffffffffw",
                "wwwwwwwwwwwww",
            ]
        );
    }

    #[test]
    fn fields_wear_the_classic_sunken_border() {
        let mut canvas = Canvas::new(Size::new(8, 21));
        sunken(&mut canvas, Rect::new(0, 0, 8, 21), WHITE);
        let rows = rows(&canvas);
        assert_eq!(rows[0], "gggggggw");
        assert_eq!(rows[1], "gkkkkkfw");
        assert!(rows[2..19].iter().all(|row| row == "gkwwwwfw"));
        assert_eq!(rows[19], "gffffffw");
        assert_eq!(rows[20], "wwwwwwww");
    }

    #[test]
    fn buttons_wear_the_bevelled_edge_of_office_balloons() {
        let mut canvas = Canvas::new(Size::new(8, 6));
        button(&mut canvas, Rect::new(0, 0, 8, 6), "", false);
        assert_eq!(
            rows(&canvas),
            [
                "  eeee  ", " e    e ", "e      e", "e      e", " e    e ", "  eeee  ",
            ]
        );
        assert_eq!(button_edge(), [0xBF, 0xBF, 0xA8, 0xFF]);
    }

    fn ink(label: &str, pressed: bool) -> (i32, i32, i32, i32) {
        let area = Rect::new(0, 0, BUTTON_WIDTH, BUTTON_HEIGHT);
        let mut canvas = Canvas::new(Size::new(area.width, area.height));
        button(&mut canvas, area, label, pressed);
        let inked: Vec<(i32, i32)> = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| (x, y)))
            .filter(|&(x, y)| canvas.pixel(x, y) == CLIPPIT.balloon.text)
            .collect();
        let xs = inked.iter().map(|&(x, _)| x);
        let ys = inked.iter().map(|&(_, y)| y);
        (
            xs.clone().min().unwrap(),
            ys.clone().min().unwrap(),
            xs.max().unwrap(),
            ys.max().unwrap(),
        )
    }

    #[test]
    fn labels_sit_in_the_middle_like_in_windows_and_sink_a_pixel_when_held() {
        let (left, top, right, bottom) = ink("Cancel", false);
        assert_eq!((left, top, right, bottom), (14, 6, 45, 14));
        assert_eq!(
            ink("Cancel", true),
            (left + 1, top + 1, right + 1, bottom + 1)
        );
    }

    #[test]
    fn pressing_greys_the_box() {
        let pressed = pixels(Checkbox {
            checked: false,
            pressed: true,
        });
        assert_eq!(pressed[6], "gkffffffffffw");
    }
}
