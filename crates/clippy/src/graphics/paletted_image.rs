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

use std::sync::Arc;

use crate::geometry::Size;
use crate::graphics::{Bitmap, Color, TRANSPARENT, index};

pub(crate) struct PalettedImage {
    size: Size,
    indices: Vec<u8>,
    palette: Arc<[Color]>,
}

impl PalettedImage {
    pub(crate) fn new(size: Size, indices: Vec<u8>, palette: Arc<[Color]>) -> Self {
        Self {
            size,
            indices,
            palette,
        }
    }

    pub(crate) fn size(&self) -> Size {
        self.size
    }
}

impl Bitmap for PalettedImage {
    fn pixel(&self, x: i32, y: i32) -> Color {
        index(self.size, x, y)
            .and_then(|index| self.indices.get(index))
            .and_then(|&entry| self.palette.get(usize::from(entry)))
            .copied()
            .unwrap_or(TRANSPARENT)
    }
}
