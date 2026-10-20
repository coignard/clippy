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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Size {
    pub width: i32,
    pub height: i32,
}

impl Size {
    pub const fn new(width: i32, height: i32) -> Self {
        Self { width, height }
    }

    #[must_use]
    pub const fn scaled(self, factor: i32) -> Self {
        Self::new(self.width * factor, self.height * factor)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    #[must_use]
    pub const fn scaled(self, factor: i32) -> Self {
        Self::new(
            self.x * factor,
            self.y * factor,
            self.width * factor,
            self.height * factor,
        )
    }

    pub const fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    pub const fn right(self) -> i32 {
        self.x + self.width
    }

    pub const fn bottom(self) -> i32 {
        self.y + self.height
    }

    #[must_use]
    pub const fn inset(self, amount: i32) -> Self {
        Self::new(
            self.x + amount,
            self.y + amount,
            self.width - 2 * amount,
            self.height - 2 * amount,
        )
    }

    pub fn overflow(self, bounds: Rect) -> i32 {
        [
            bounds.x - self.x,
            self.right() - bounds.right(),
            bounds.y - self.y,
            self.bottom() - bounds.bottom(),
        ]
        .into_iter()
        .map(|distance| distance.max(0))
        .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overflow_adds_up_what_sticks_out() {
        let bounds = Rect::new(0, 0, 100, 100);
        assert_eq!(Rect::new(10, 10, 50, 50).overflow(bounds), 0);
        assert_eq!(Rect::new(-5, 90, 20, 20).overflow(bounds), 15);
    }

    #[test]
    fn contains_its_origin_but_not_its_far_edges() {
        let rect = Rect::new(10, 20, 30, 40);
        assert!(rect.contains(10, 20));
        assert!(rect.contains(39, 59));
        assert!(!rect.contains(40, 30));
        assert!(!rect.contains(20, 60));
        assert!(!rect.contains(9, 30));
    }
}
