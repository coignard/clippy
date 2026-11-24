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

use crate::character::portrait;
use crate::geometry::{Rect, Size};
use crate::graphics::{Bitmap, Canvas};

pub fn icon(size: i32) -> Canvas {
    let portrait = portrait();
    let bounds = opaque_bounds(&portrait);
    let room = size * 7 / 8;
    let longest = bounds.width.max(bounds.height);
    let (width, height) = match room / longest {
        0 => (
            bounds.width * room / longest,
            bounds.height * room / longest,
        ),
        factor => (bounds.width * factor, bounds.height * factor),
    };
    let (left, top) = ((size - width) / 2, (size - height) / 2);
    let mut icon = Canvas::new(Size::new(size, size));
    for y in 0..height {
        for x in 0..width {
            let pixel = portrait.pixel(
                bounds.x + x * bounds.width / width,
                bounds.y + y * bounds.height / height,
            );
            icon.plot(left + x, top + y, pixel);
        }
    }
    icon
}

fn opaque_bounds(canvas: &Canvas) -> Rect {
    let size = canvas.size();
    let (left, top, right, bottom) = (0..size.height)
        .flat_map(|y| (0..size.width).map(move |x| (x, y)))
        .filter(|&(x, y)| canvas.pixel(x, y)[3] != 0)
        .fold(None, |edges: Option<(i32, i32, i32, i32)>, (x, y)| {
            Some(edges.map_or((x, y, x, y), |(left, top, right, bottom)| {
                (left.min(x), top.min(y), right.max(x), bottom.max(y))
            }))
        })
        .expect("Clippy should be visible at rest");
    Rect::new(left, top, right - left + 1, bottom - top + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_icons_blow_clippy_up_by_whole_pixels() {
        let resting = opaque_bounds(&portrait());
        let icon = icon(256);
        assert_eq!(icon.size(), Size::new(256, 256));
        let shown = opaque_bounds(&icon);
        let factor = shown.height / resting.height;
        assert!(factor > 1);
        assert_eq!(shown.width, resting.width * factor);
        assert_eq!(shown.height, resting.height * factor);
    }

    #[test]
    fn icons_keep_clippy_centred_with_room_around_him() {
        for size in [16, 32, 48, 256] {
            let shown = opaque_bounds(&icon(size));
            let room = size * 7 / 8;
            assert!(shown.width <= room && shown.height <= room, "{size}");
            assert!((shown.x - (size - shown.right())).abs() <= 1, "{size}");
            assert!((shown.y - (size - shown.bottom())).abs() <= 1, "{size}");
        }
    }

    #[test]
    fn icons_too_small_to_hold_clippy_stay_empty() {
        let icon = icon(1);
        assert_eq!(icon.size(), Size::new(1, 1));
        assert!(icon.rgba().iter().all(|&byte| byte == 0));
    }
}
