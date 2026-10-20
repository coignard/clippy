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

use crate::balloon::Utf16Range;
use crate::geometry::Rect;
use crate::graphics::Canvas;

#[derive(Debug)]
pub struct Scene {
    pub clippy: Option<Canvas>,
    pub balloon: Option<BalloonScene>,
}

#[derive(Debug)]
pub struct BalloonScene {
    pub frame: Rect,
    pub canvas: Canvas,
    pub field: Option<Field>,
    pub checkbox: Option<Rect>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Field {
    pub frame: Rect,
    pub selection: Utf16Range,
    pub wants_focus: bool,
}
