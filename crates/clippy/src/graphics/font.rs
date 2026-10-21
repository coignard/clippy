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

use std::borrow::Cow;

use skrifa::bitmap::{BitmapData, BitmapGlyph, BitmapStrike, BitmapStrikes};
use skrifa::charmap::Charmap;
use skrifa::instance::{LocationRef, Size};
use skrifa::metrics::GlyphMetrics;
use skrifa::{FontRef, GlyphId, MetadataProvider};

use crate::graphics::{Canvas, Color};

pub(crate) struct Font {
    charmap: Charmap<'static>,
    metrics: GlyphMetrics<'static>,
    strike: BitmapStrike<'static>,
    units_per_em: i32,
    pixels_per_em: i32,
}

impl Font {
    pub(crate) fn parse(data: &'static [u8], pixels_per_em: u16) -> Option<Self> {
        let font = FontRef::new(data).ok()?;
        let size = f32::from(pixels_per_em);
        let strike = BitmapStrikes::new(&font)
            .iter()
            .find(|strike| strike.ppem().total_cmp(&size).is_eq())?;
        Some(Self {
            charmap: font.charmap(),
            metrics: font.glyph_metrics(Size::unscaled(), LocationRef::default()),
            strike,
            units_per_em: i32::from(
                font.metrics(Size::unscaled(), LocationRef::default())
                    .units_per_em,
            ),
            pixels_per_em: i32::from(pixels_per_em),
        })
    }

    pub(crate) fn printable<'a>(&self, text: &'a str) -> Cow<'a, str> {
        if text
            .chars()
            .all(|character| character == '\n' || self.has(character))
        {
            return Cow::Borrowed(text);
        }
        let lines: Vec<String> = text
            .split('\n')
            .filter_map(|line| {
                if line.chars().all(|character| self.has(character)) {
                    return Some(line.to_owned());
                }
                let kept: String = line
                    .chars()
                    .filter(|&character| self.has(character))
                    .collect();
                let words: Vec<&str> = kept.split_whitespace().collect();
                (!words.is_empty()).then(|| words.join(" "))
            })
            .collect();
        Cow::Owned(lines.join("\n"))
    }

    pub(crate) fn width(&self, text: &str) -> i32 {
        text.chars().map(|character| self.advance(character)).sum()
    }

    pub(crate) fn advance(&self, character: char) -> i32 {
        self.glyph_advance(self.glyph(character))
    }

    pub(crate) fn draw(
        &self,
        canvas: &mut Canvas,
        text: &str,
        x: i32,
        baseline: i32,
        color: Color,
    ) {
        let mut pen = x;
        for character in text.chars() {
            let glyph = self.glyph(character);
            if let Some(bitmap) = self.strike.get(glyph) {
                draw_bitmap(canvas, &bitmap, pen, baseline, color);
            }
            pen += self.glyph_advance(glyph);
        }
    }

    fn has(&self, character: char) -> bool {
        self.charmap.map(character).is_some()
    }

    fn glyph(&self, character: char) -> GlyphId {
        self.charmap.map(character).unwrap_or(GlyphId::NOTDEF)
    }

    fn glyph_advance(&self, glyph: GlyphId) -> i32 {
        let units = pixels(self.metrics.advance_width(glyph).unwrap_or_default());
        (units * self.pixels_per_em + self.units_per_em / 2) / self.units_per_em
    }
}

fn draw_bitmap(canvas: &mut Canvas, bitmap: &BitmapGlyph<'_>, x: i32, baseline: i32, color: Color) {
    let BitmapData::Mask(mask) = &bitmap.data else {
        return;
    };
    let (Ok(mask), Ok(width)) = (
        mask.decode(bitmap.width, bitmap.height),
        usize::try_from(bitmap.width),
    ) else {
        return;
    };
    let left = x + pixels(bitmap.inner_bearing_x);
    let top = baseline - pixels(bitmap.inner_bearing_y);
    for (row, line) in (0..).zip(mask.chunks(width.max(1))) {
        for (column, &coverage) in (0..).zip(line) {
            if coverage != 0 {
                canvas.plot(left + column, top + row, color);
            }
        }
    }
}

#[expect(clippy::cast_possible_truncation)]
fn pixels(length: f32) -> i32 {
    length.round() as i32
}

#[cfg(test)]
mod tests {
    use crate::assets::FONT;
    use crate::geometry::Size;
    use crate::graphics::{BLACK, Bitmap, Canvas};

    #[test]
    fn leaves_out_what_it_cannot_draw() {
        assert_eq!(FONT.printable("Обед  at 13:00\nok"), "Обед  at 13:00\nok");
        assert_eq!(
            FONT.printable("🎉 Party 🥳 time\n\nStarts at 10:00"),
            "Party time\n\nStarts at 10:00"
        );
        assert_eq!(FONT.printable("❤️\nStarts at 10:00"), "Starts at 10:00");
    }

    #[test]
    fn draws_crisp_glyphs_around_the_baseline() {
        let text = "жg";
        let mut canvas = Canvas::new(Size::new(FONT.width(text), 13));
        FONT.draw(&mut canvas, text, 0, 11, BLACK);
        let rows: Vec<String> = (0..13)
            .map(|y| {
                (0..canvas.size().width)
                    .map(|x| {
                        if canvas.pixel(x, y) == BLACK {
                            '#'
                        } else {
                            '.'
                        }
                    })
                    .collect()
            })
            .collect();
        assert_eq!(
            rows,
            [
                "..............",
                "..............",
                "..............",
                "..............",
                "..............",
                ".#..#..#..####",
                "..#.#.#..#...#",
                "...###...#...#",
                "..#.#.#..#...#",
                "..#.#.#..#...#",
                ".#..#..#..####",
                ".............#",
                ".........####.",
            ]
        );
    }

    #[test]
    fn covers_english_and_russian_text() {
        let characters = (' '..='~').chain('А'..='я').chain("Ёё«»„“”‘’—–…№".chars());
        for character in characters {
            assert!(FONT.charmap.map(character).is_some(), "{character:?}");
        }
    }
}
