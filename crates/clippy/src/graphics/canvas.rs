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

use std::fmt;

use crate::geometry::{Rect, Size};
use crate::graphics::{Bitmap, Color, TRANSPARENT, index};

#[derive(Clone, PartialEq, Eq)]
pub struct Canvas {
    size: Size,
    pixels: Vec<Color>,
}

impl Canvas {
    pub(crate) fn new(size: Size) -> Self {
        let area = usize::try_from(size.width * size.height).unwrap_or_default();
        Self {
            size,
            pixels: vec![TRANSPARENT; area],
        }
    }

    pub fn size(&self) -> Size {
        self.size
    }

    pub fn rgba(&self) -> &[u8] {
        self.pixels.as_flattened()
    }

    pub fn is_solid(&self, x: i32, y: i32) -> bool {
        self.pixel(x, y) != TRANSPARENT
    }

    pub(crate) fn plot(&mut self, x: i32, y: i32, color: Color) {
        if let Some(index) = index(self.size, x, y) {
            self.pixels[index] = color;
        }
    }

    pub(crate) fn fill(&mut self, area: Rect, color: Color) {
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                self.plot(x, y, color);
            }
        }
    }

    pub(crate) fn draw(&mut self, bitmap: &impl Bitmap, source: Rect, x: i32, y: i32) {
        for row in 0..source.height {
            for column in 0..source.width {
                let color = bitmap.pixel(source.x + column, source.y + row);
                if color != TRANSPARENT {
                    self.plot(x + column, y + row, color);
                }
            }
        }
    }

    pub(crate) fn scaled(self, factor: i32) -> Self {
        if factor == 1 {
            return self;
        }
        let mut scaled = Self::new(self.size.scaled(factor));
        for y in 0..scaled.size.height {
            for x in 0..scaled.size.width {
                scaled.plot(x, y, self.pixel(x / factor, y / factor));
            }
        }
        scaled
    }
}

impl Bitmap for Canvas {
    fn pixel(&self, x: i32, y: i32) -> Color {
        index(self.size, x, y).map_or(TRANSPARENT, |index| self.pixels[index])
    }
}

impl fmt::Debug for Canvas {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Canvas")
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphics::BLACK;

    #[test]
    fn plotting_outside_is_ignored() {
        let mut canvas = Canvas::new(Size::new(2, 2));
        canvas.plot(-1, 0, BLACK);
        canvas.plot(2, 1, BLACK);
        assert!(canvas.rgba().iter().all(|&byte| byte == 0));
    }

    #[test]
    fn scaling_repeats_pixels_without_blending() {
        let mut canvas = Canvas::new(Size::new(2, 1));
        canvas.plot(1, 0, BLACK);
        let scaled = canvas.scaled(3);
        assert_eq!(scaled.size(), Size::new(6, 3));
        assert!((0..3).all(|x| scaled.pixel(x, 2) == TRANSPARENT));
        assert!((3..6).all(|x| scaled.pixel(x, 2) == BLACK));
    }

    #[test]
    fn drawing_skips_transparent_pixels() {
        let mut source = Canvas::new(Size::new(2, 1));
        source.plot(1, 0, BLACK);
        let mut target = Canvas::new(Size::new(2, 1));
        target.fill(Rect::new(0, 0, 2, 1), [1, 2, 3, 255]);
        target.draw(&source, Rect::new(0, 0, 2, 1), 0, 0);
        assert_eq!(target.pixel(0, 0), [1, 2, 3, 255]);
        assert_eq!(target.pixel(1, 0), BLACK);
    }
}
