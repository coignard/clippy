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

use crate::geometry::Rect;
use crate::graphics::{Bitmap, Canvas, Image};

pub(crate) struct NineSlice {
    image: Image,
    border: i32,
}

impl NineSlice {
    pub(crate) fn new(image: Image, border: i32) -> Self {
        Self { image, border }
    }

    pub(crate) fn draw(&self, canvas: &mut Canvas, area: Rect) {
        let source = self.image.size();
        for y in 0..area.height {
            for x in 0..area.width {
                let column = self.source_coordinate(x, area.width, source.width);
                let row = self.source_coordinate(y, area.height, source.height);
                canvas.plot(area.x + x, area.y + y, self.image.pixel(column, row));
            }
        }
    }

    fn source_coordinate(&self, target: i32, target_length: i32, source_length: i32) -> i32 {
        let middle = source_length - 2 * self.border;
        if target < self.border {
            target
        } else if target >= target_length - self.border {
            source_length - (target_length - target)
        } else {
            self.border + (target - self.border) % middle
        }
    }
}
