use std::ops::Range;

use crate::assets::{BALLOON_FRAME, BALLOON_TIP, CLIPPIT, FONT};
use crate::balloon::controls;
use crate::balloon::controls::CHECKBOX;
use crate::balloon::layout::{
    BASELINE, BORDER, FIELD_INSET, FIELD_TEXT_WIDTH, LABEL_GAP, LINE_HEIGHT, Layout, TIP, TIP_INSET,
};
use crate::balloon::{Content, Placement, Prompt};
use crate::geometry::{Rect, Size};
use crate::graphics::{BLACK, Canvas, NAVY, WHITE};

const TIP_LEANING_RIGHT: i32 = 0;
const TIP_LEANING_LEFT: i32 = TIP.width;
const TIP_POINTING_DOWN: i32 = 0;
const TIP_POINTING_UP: i32 = TIP.height;

impl Layout<'_> {
    pub(crate) fn render(&self, placement: Placement) -> Canvas {
        let mut canvas = Canvas::new(self.size());
        let body = self.body(placement);
        BALLOON_FRAME.draw(&mut canvas, body);
        draw_tip(&mut canvas, body, placement);
        let (left, top) = self.text_origin(placement);
        draw_lines(&mut canvas, self.content.text(), &self.lines, left, top);
        if let Content::Prompt { prompt, focused } = self.content
            && let Some(area) = self.field(placement)
        {
            draw_field(&mut canvas, area, prompt, focused);
        }
        if let Content::Notice {
            checkbox: Some((caption, state)),
            ..
        } = self.content
            && let Some(row) = self.checkbox(placement)
        {
            controls::checkbox(&mut canvas, row.x, row.y, state);
            let left = row.x + CHECKBOX + LABEL_GAP;
            draw_lines(&mut canvas, caption, &self.label, left, row.y);
        }
        canvas
    }
}

fn draw_lines(canvas: &mut Canvas, text: &str, lines: &[Range<usize>], left: i32, top: i32) {
    for (row, line) in (0..).zip(lines) {
        FONT.draw(
            canvas,
            &text[line.clone()],
            left,
            top + row * LINE_HEIGHT + BASELINE,
            CLIPPIT.balloon.text,
        );
    }
}

fn draw_tip(canvas: &mut Canvas, body: Rect, placement: Placement) {
    let (column, x) = if placement.extends_left() {
        (TIP_LEANING_LEFT, body.right() - BORDER - TIP_INSET)
    } else {
        (TIP_LEANING_RIGHT, body.x + BORDER + TIP_INSET)
    };
    let (row, y) = if placement.is_above() {
        (TIP_POINTING_DOWN, body.bottom() - BORDER)
    } else {
        (TIP_POINTING_UP, 0)
    };
    canvas.draw(
        &*BALLOON_TIP,
        Rect::new(column, row, TIP.width, TIP.height),
        x,
        y,
    );
}

fn draw_field(canvas: &mut Canvas, area: Rect, prompt: &Prompt, focused: bool) {
    controls::sunken(canvas, area, WHITE);
    let field = prompt.field();
    let text = field.text();
    let offset = |index: usize| FONT.width(&text[..index]) - prompt.scroll();
    let visible = text_area(area);
    let mut line = Canvas::new(Size::new(visible.width, visible.height));
    FONT.draw(&mut line, text, offset(0), BASELINE, BLACK);
    let selection = field.selection();
    if focused && !selection.is_empty() {
        let (start, end) = (offset(selection.start), offset(selection.end));
        line.fill(Rect::new(start, 0, end - start, LINE_HEIGHT), NAVY);
        FONT.draw(&mut line, &text[selection], start, BASELINE, WHITE);
    } else if focused && prompt.is_caret_shown() {
        line.fill(Rect::new(offset(field.caret()), 0, 1, LINE_HEIGHT), BLACK);
    }
    canvas.draw(
        &line,
        Rect::new(0, 0, visible.width, visible.height),
        visible.x,
        visible.y,
    );
}

fn text_area(field: Rect) -> Rect {
    Rect::new(
        field.x + FIELD_INSET,
        field.y + (field.height - LINE_HEIGHT) / 2,
        FIELD_TEXT_WIDTH,
        LINE_HEIGHT,
    )
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::balloon::layout::TIP_REACH;
    use crate::balloon::{Edit, Motion};
    use crate::graphics::{Bitmap, TRANSPARENT};

    fn prompt(text: &str) -> Prompt {
        let mut prompt = Prompt::new(
            "What are you focusing on?".into(),
            text.into(),
            Instant::now(),
        );
        prompt.edit(Edit::Move(Motion::End), Instant::now());
        prompt
    }

    fn opaque_columns(canvas: &Canvas, y: i32) -> Vec<i32> {
        (0..canvas.size().width)
            .filter(|&x| canvas.pixel(x, y) != TRANSPARENT)
            .collect()
    }

    #[test]
    fn tail_points_at_clippy() {
        let screen = Rect::new(0, 0, 1000, 800);
        let clippy = Size::new(124, 93);
        let corners = [
            (
                Rect::new(860, 690, clippy.width, clippy.height),
                Placement::TopLeft,
            ),
            (
                Rect::new(16, 690, clippy.width, clippy.height),
                Placement::TopRight,
            ),
            (
                Rect::new(860, 16, clippy.width, clippy.height),
                Placement::BottomLeft,
            ),
            (
                Rect::new(16, 16, clippy.width, clippy.height),
                Placement::BottomRight,
            ),
        ];
        for (clippy, expected) in corners {
            let layout = Layout::new(Content::Message("Ship the plan"));
            let (placement, frame) = Placement::choose(clippy, layout.size(), screen);
            assert_eq!(placement, expected);
            let canvas = layout.render(placement);
            let height = canvas.size().height;
            let (tip, base) = if placement.is_above() {
                (height - 1, height - TIP_REACH)
            } else {
                (0, TIP_REACH - 1)
            };
            let middle = clippy.x + clippy.width / 2 - frame.x;
            let base = opaque_columns(&canvas, base);
            let nearest = base.iter().min_by_key(|&&x| (x - middle).abs()).copied();
            assert_eq!(
                opaque_columns(&canvas, tip),
                Vec::from_iter(nearest),
                "{placement:?}"
            );
        }
    }

    #[test]
    fn tail_opens_the_border() {
        for placement in [Placement::TopLeft, Placement::BottomRight] {
            let canvas = Layout::new(Content::Message("Hi")).render(placement);
            let height = canvas.size().height;
            let (border, base) = if placement.is_above() {
                (height - TIP_REACH - 1, height - TIP_REACH)
            } else {
                (TIP_REACH, TIP_REACH - 1)
            };
            let base = opaque_columns(&canvas, base);
            let opening = base[base.len() / 2];
            assert_eq!(
                canvas.pixel(opening, border),
                CLIPPIT.balloon.fill,
                "{placement:?}"
            );
        }
    }

    #[test]
    fn corners_are_pixelated_and_transparent() {
        let canvas = Layout::new(Content::Message("Hi")).render(Placement::TopLeft);
        assert_eq!(canvas.pixel(0, 0), TRANSPARENT);
        assert_eq!(canvas.pixel(4, 0), BLACK);
        assert_eq!(canvas.pixel(0, 4), BLACK);
        assert_eq!(canvas.pixel(3, 3), CLIPPIT.balloon.fill);
    }

    #[test]
    fn every_pixel_is_opaque_or_clear() {
        let mut prompt = prompt("Ship the plan");
        prompt.edit(Edit::Select(Motion::WordBackward), Instant::now());
        for placement in [Placement::TopLeft, Placement::BottomRight] {
            let content = Content::Prompt {
                prompt: &prompt,
                focused: true,
            };
            let canvas = Layout::new(content).render(placement);
            let alphas = canvas.rgba().iter().skip(3).step_by(4);
            assert!(alphas.copied().all(|alpha| alpha == 0 || alpha == 255));
        }
    }

    #[test]
    fn fields_have_the_98_css_metrics() {
        let prompt = prompt("Ship");
        let layout = Layout::new(Content::Prompt {
            prompt: &prompt,
            focused: true,
        });
        let field = layout.field(Placement::TopLeft).unwrap();
        assert_eq!(field.height, 21);
        assert_eq!(text_area(field).x - field.x, 4);
    }

    #[test]
    fn field_shows_caret_and_selection_only_while_focused() {
        let caret = prompt("Ship");
        let mut selection = prompt("Ship");
        selection.edit(Edit::SelectAll, Instant::now());
        let render = |prompt: &Prompt, focused: bool| {
            let layout = Layout::new(Content::Prompt { prompt, focused });
            let field = layout
                .field(Placement::TopLeft)
                .expect("prompts have a field");
            (layout.render(Placement::TopLeft), text_area(field))
        };
        let caret_drawn = |focused| {
            let (canvas, line) = render(&caret, focused);
            let x = line.x + FONT.width("Ship");
            (line.y..line.bottom()).all(|y| canvas.pixel(x, y) == BLACK)
        };
        let highlighted = |focused| {
            let (canvas, line) = render(&selection, focused);
            (line.x..line.right()).any(|x| canvas.pixel(x, line.y) == NAVY)
        };
        assert!(caret_drawn(true));
        assert!(!caret_drawn(false));
        assert!(highlighted(true));
        assert!(!highlighted(false));
    }
}
