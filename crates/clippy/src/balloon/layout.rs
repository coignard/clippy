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

use std::ops::Range;

use textwrap::core::Fragment;
use textwrap::wrap_algorithms::wrap_first_fit;
use unicode_linebreak::{BreakOpportunity, linebreaks};
use unicode_segmentation::UnicodeSegmentation;

use crate::assets::FONT;
use crate::balloon::controls::CHECKBOX;
use crate::balloon::{Content, Placement};
use crate::geometry::{Rect, Size};

pub(super) const LINE_HEIGHT: i32 = 13;
pub(super) const BASELINE: i32 = 11;
pub(super) const BORDER: i32 = 1;
pub(super) const TIP: Size = Size::new(10, 16);
pub(super) const TIP_REACH: i32 = TIP.height - BORDER;
pub(super) const FIELD_INSET: i32 = 4;
pub(super) const FIELD_TEXT_WIDTH: i32 = MAX_TEXT_WIDTH - 2 * FIELD_INSET;
const PADDING: i32 = 8;
const MIN_TEXT_WIDTH: i32 = 120;
const MAX_TEXT_WIDTH: i32 = 200;
pub(super) const LABEL_GAP: i32 = 6;
const CONTROL_GAP: i32 = 6;
const FIELD_HEIGHT: i32 = LINE_HEIGHT + 8;

pub(crate) struct Layout<'a> {
    pub(super) content: Content<'a>,
    pub(super) lines: Vec<Range<usize>>,
    pub(super) label: Vec<Range<usize>>,
    text_width: i32,
    body: Size,
}

impl<'a> Layout<'a> {
    pub(crate) fn new(content: Content<'a>) -> Self {
        let text = content.text();
        let lines = wrap(text, MAX_TEXT_WIDTH);
        let longest = widest(text, &lines);
        let mut label = Vec::new();
        let (text_width, controls) = match content {
            Content::Message(_) | Content::Notice { checkbox: None, .. } => {
                (longest.clamp(MIN_TEXT_WIDTH, MAX_TEXT_WIDTH), 0)
            }
            Content::Prompt { .. } => (MAX_TEXT_WIDTH, CONTROL_GAP + FIELD_HEIGHT),
            Content::Notice {
                checkbox: Some((caption, _)),
                ..
            } => {
                label = wrap(caption, MAX_TEXT_WIDTH - CHECKBOX - LABEL_GAP);
                let row = CHECKBOX + LABEL_GAP + widest(caption, &label);
                let width = longest.max(row).clamp(MIN_TEXT_WIDTH, MAX_TEXT_WIDTH);
                (width, CONTROL_GAP + height(&label).max(CHECKBOX))
            }
        };
        let chrome = 2 * (BORDER + PADDING);
        Self {
            content,
            body: Size::new(text_width + chrome, height(&lines) + controls + chrome),
            lines,
            label,
            text_width,
        }
    }

    pub(crate) fn size(&self) -> Size {
        Size::new(self.body.width, self.body.height + TIP_REACH)
    }

    pub(crate) fn field(&self, placement: Placement) -> Option<Rect> {
        let Content::Prompt { .. } = self.content else {
            return None;
        };
        Some(self.controls(placement, FIELD_HEIGHT))
    }

    pub(crate) fn checkbox(&self, placement: Placement) -> Option<Rect> {
        let Content::Notice {
            checkbox: Some(_), ..
        } = self.content
        else {
            return None;
        };
        Some(self.controls(placement, height(&self.label).max(CHECKBOX)))
    }

    pub(super) fn body(&self, placement: Placement) -> Rect {
        let top = if placement.is_above() { 0 } else { TIP_REACH };
        Rect::new(0, top, self.body.width, self.body.height)
    }

    pub(super) fn text_origin(&self, placement: Placement) -> (i32, i32) {
        let body = self.body(placement);
        (body.x + BORDER + PADDING, body.y + BORDER + PADDING)
    }

    fn controls(&self, placement: Placement, height_of_controls: i32) -> Rect {
        let (left, top) = self.text_origin(placement);
        Rect::new(
            left,
            top + height(&self.lines) + CONTROL_GAP,
            self.text_width,
            height_of_controls,
        )
    }
}

fn widest(text: &str, lines: &[Range<usize>]) -> i32 {
    lines
        .iter()
        .map(|line| FONT.width(&text[line.clone()]))
        .max()
        .unwrap_or(0)
}

fn height(lines: &[Range<usize>]) -> i32 {
    i32::try_from(lines.len()).map_or(i32::MAX, |count| count.saturating_mul(LINE_HEIGHT))
}

fn wrap(text: &str, max_width: i32) -> Vec<Range<usize>> {
    let mut lines = Vec::new();
    let mut paragraph = Vec::new();
    let mut start = 0;
    for (end, opportunity) in linebreaks(text) {
        let segment = &text[start..end];
        let word = segment.trim_end_matches(char::is_whitespace);
        let space = FONT.width(segment[word.len()..].trim_end_matches(['\r', '\n']));
        if !word.is_empty() || space > 0 {
            split(word, start, max_width, space, &mut paragraph);
        }
        if opportunity == BreakOpportunity::Mandatory {
            if paragraph.is_empty() {
                lines.push(start..start);
            }
            for line in wrap_first_fit(&paragraph, &[f64::from(max_width)]) {
                if let (Some(first), Some(last)) = (line.first(), line.last()) {
                    lines.push(first.text.start..last.text.end);
                }
            }
            paragraph.clear();
        }
        start = end;
    }
    if text.is_empty() || text.ends_with('\n') {
        lines.push(text.len()..text.len());
    }
    lines
}

fn split(word: &str, offset: usize, max_width: i32, space: i32, pieces: &mut Vec<Piece>) {
    let mut start = offset;
    let mut end = offset;
    let mut width = 0;
    for grapheme in word.graphemes(true) {
        let advance = FONT.width(grapheme);
        if width > 0 && width + advance > max_width {
            pieces.push(Piece::new(start..end, width, 0));
            start = end;
            width = 0;
        }
        width += advance;
        end += grapheme.len();
    }
    pieces.push(Piece::new(start..end, width, space));
}

#[derive(Debug)]
struct Piece {
    text: Range<usize>,
    width: i32,
    space: i32,
}

impl Piece {
    fn new(text: Range<usize>, width: i32, space: i32) -> Self {
        Self { text, width, space }
    }
}

impl Fragment for Piece {
    fn width(&self) -> f64 {
        f64::from(self.width)
    }

    fn whitespace_width(&self) -> f64 {
        f64::from(self.space)
    }

    fn penalty_width(&self) -> f64 {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str, max_width: i32) -> Vec<&str> {
        wrap(text, max_width)
            .into_iter()
            .map(|range| &text[range])
            .collect()
    }

    #[test]
    fn keeps_short_text_on_one_line() {
        assert_eq!(lines("Ship it", 200), ["Ship it"]);
    }

    #[test]
    fn breaks_between_words() {
        let text = "дописать план приложения Clippy на Rust";
        let wrapped = lines(text, 100);
        assert!(wrapped.len() > 1);
        assert!(wrapped.iter().all(|line| FONT.width(line) <= 100));
        assert_eq!(wrapped.join(" "), text);
    }

    #[test]
    fn splits_words_longer_than_a_line() {
        let wrapped = lines("https://example.com/a/very/long/path/that/does/not/fit", 60);
        assert!(wrapped.len() > 1);
        assert!(wrapped.iter().all(|line| FONT.width(line) <= 60));
        assert_eq!(
            wrapped.concat(),
            "https://example.com/a/very/long/path/that/does/not/fit"
        );
    }

    #[test]
    fn honours_explicit_line_breaks() {
        assert_eq!(lines("one\ntwo", 200), ["one", "two"]);
        assert_eq!(lines("one\n\ntwo", 200), ["one", "", "two"]);
        assert_eq!(lines("one\n", 200), ["one", ""]);
        assert_eq!(lines("", 200), [""]);
    }

    #[test]
    fn breaks_after_hyphens_and_slashes_but_not_at_no_break_spaces() {
        let narrower = |text: &str| FONT.width(text) - 1;
        assert_eq!(
            lines("well-known", narrower("well-known")),
            ["well-", "known"]
        );
        assert_eq!(lines("and/or", narrower("and/or")), ["and/", "or"]);
        let glued = "in 10\u{a0}minutes";
        assert_eq!(lines(glued, narrower(glued)), ["in", "10\u{a0}minutes"]);
    }
}
