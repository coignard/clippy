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
use crate::Event;
use crate::graphics::Canvas;

#[derive(Debug, Default)]
pub struct Hover {
    entered_on: Option<Canvas>,
}

impl Hover {
    pub fn follow(&mut self, canvas: Option<&Canvas>, x: i32, y: i32) -> Option<Event> {
        let hovered = canvas.is_some_and(|canvas| canvas.is_solid(x, y) || self.covers(x, y));
        match (self.entered_on.is_some(), hovered) {
            (false, true) => {
                self.entered_on = canvas.cloned();
                Some(Event::PointerEnter)
            }
            (true, false) => {
                self.entered_on = None;
                Some(Event::PointerLeave)
            }
            _ => None,
        }
    }

    fn covers(&self, x: i32, y: i32) -> bool {
        self.entered_on
            .as_ref()
            .is_some_and(|entered_on| entered_on.is_solid(x, y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Size;
    use crate::graphics::BLACK;

    fn frame(x: i32, y: i32) -> Canvas {
        let mut canvas = Canvas::new(Size::new(4, 4));
        canvas.plot(x, y, BLACK);
        canvas
    }

    #[test]
    fn transparent_pixels_are_not_hovered() {
        let mut hover = Hover::default();
        assert_eq!(hover.follow(Some(&frame(0, 0)), 3, 3), None);
    }

    #[test]
    fn a_frame_moving_away_from_the_pointer_keeps_the_hover() {
        let mut hover = Hover::default();
        assert_eq!(
            hover.follow(Some(&frame(1, 1)), 1, 1),
            Some(Event::PointerEnter)
        );
        assert_eq!(hover.follow(Some(&frame(1, 0)), 1, 1), None);
        assert_eq!(hover.follow(Some(&frame(1, 1)), 1, 1), None);
        assert_eq!(
            hover.follow(Some(&frame(1, 0)), 2, 2),
            Some(Event::PointerLeave)
        );
    }

    #[test]
    fn frames_shown_while_hovering_are_not_held_on_to() {
        let mut hover = Hover::default();
        assert_eq!(
            hover.follow(Some(&frame(1, 1)), 1, 1),
            Some(Event::PointerEnter)
        );
        assert_eq!(hover.follow(Some(&frame(3, 3)), 3, 3), None);
        assert_eq!(
            hover.follow(Some(&frame(1, 1)), 3, 3),
            Some(Event::PointerLeave)
        );
    }

    #[test]
    fn hiding_leaves() {
        let mut hover = Hover::default();
        hover.follow(Some(&frame(1, 1)), 1, 1);
        assert_eq!(hover.follow(None, 1, 1), Some(Event::PointerLeave));
    }
}
