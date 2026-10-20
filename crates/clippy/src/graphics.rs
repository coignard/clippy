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

mod canvas;
mod font;
mod image;
mod nine_slice;
mod paletted_image;

use crate::geometry::Size;

pub use canvas::Canvas;
pub(crate) use font::Font;
pub(crate) use image::Image;
pub(crate) use nine_slice::NineSlice;
pub(crate) use paletted_image::PalettedImage;

pub(crate) type Color = [u8; 4];

pub(crate) const TRANSPARENT: Color = [0, 0, 0, 0];
pub(crate) const BLACK: Color = [0, 0, 0, 255];
pub(crate) const WHITE: Color = [255, 255, 255, 255];
pub(crate) const NAVY: Color = [0, 0, 128, 255];

pub(crate) trait Bitmap {
    fn pixel(&self, x: i32, y: i32) -> Color;
}

fn index(size: Size, x: i32, y: i32) -> Option<usize> {
    let inside = (0..size.width).contains(&x) && (0..size.height).contains(&y);
    if !inside {
        return None;
    }
    usize::try_from(y * size.width + x).ok()
}
