use crate::geometry::Rect;
use crate::graphics::{BLACK, Canvas, Color, WHITE};

pub(super) const CHECKBOX: i32 = 13;
const SURFACE: Color = [0xC0, 0xC0, 0xC0, 0xFF];
const BUTTON_FACE: Color = [0xDF, 0xDF, 0xDF, 0xFF];
const BUTTON_SHADOW: Color = [0x80, 0x80, 0x80, 0xFF];
const WINDOW_FRAME: Color = [0x0A, 0x0A, 0x0A, 0xFF];
const CHECKMARK_INSET: i32 = 3;
const CHECKMARK: [&str; 7] = [
    "......#", ".....##", "#...###", "##.###.", "#####..", ".###...", "..#....",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Checkbox {
    pub(crate) checked: bool,
    pub(crate) pressed: bool,
}

pub(super) fn sunken(canvas: &mut Canvas, area: Rect, interior: Color) {
    canvas.fill(area, interior);
    for (width, top_left, bottom_right) in
        [(2, WINDOW_FRAME, BUTTON_FACE), (1, BUTTON_SHADOW, WHITE)]
    {
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
    let interior = if state.pressed { SURFACE } else { WHITE };
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
                        SURFACE => 's',
                        BUTTON_FACE => 'f',
                        BUTTON_SHADOW => 'g',
                        WINDOW_FRAME => 'k',
                        BLACK => '#',
                        TRANSPARENT => ' ',
                        _ => '?',
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn checkbox_matches_windows_98() {
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
    fn fields_wear_the_98_css_border() {
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
    fn pressing_greys_the_box() {
        let pressed = pixels(Checkbox {
            checked: false,
            pressed: true,
        });
        assert_eq!(pressed[6], "gksssssssssfw");
    }
}
