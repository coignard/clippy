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

use std::time::{Duration, Instant};

use crate::animation::{Animation, Clip, Frame};
use crate::assets::ANIMATIONS;
use crate::balloon::Placement;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Look {
    AtThePointer,
    AtTheBalloon(Side),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Side {
    Above,
    Below,
}

impl Side {
    pub(crate) fn of(placement: Placement) -> Self {
        if placement.is_above() {
            Self::Above
        } else {
            Self::Below
        }
    }
}

impl Look {
    fn animation(self) -> Animation {
        match self {
            Self::AtThePointer => Animation::IdleEyeBrowRaise,
            Self::AtTheBalloon(Side::Above) => Animation::LookUp,
            Self::AtTheBalloon(Side::Below) => Animation::LookDown,
        }
    }
}

pub(super) struct Glance {
    look: Look,
    clip: &'static Clip,
    frame: usize,
    held: bool,
    next_step: Instant,
}

impl Glance {
    pub(super) fn new(look: Look, now: Instant) -> Self {
        let clip = ANIMATIONS.clip(look.animation());
        Self {
            look,
            clip,
            frame: 0,
            held: true,
            next_step: now + step(clip, 0),
        }
    }

    pub(super) fn skip_first_hold(&mut self, now: Instant) {
        self.next_step = now;
    }

    pub(super) fn look(&self) -> Look {
        self.look
    }

    pub(super) fn hold(&mut self, held: bool, now: Instant) {
        if self.held == held {
            return;
        }
        self.held = held;
        let peak = self.peak();
        let crossing = if held {
            self.frame > peak
        } else {
            self.frame < peak
        };
        if crossing {
            self.frame = self.last() - self.frame;
        }
        self.next_step = now + step(self.clip, self.frame);
    }

    pub(super) fn advance(&mut self, now: Instant) {
        while self.is_moving() && now >= self.next_step {
            self.frame += 1;
            self.next_step += step(self.clip, self.frame);
        }
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        self.is_moving().then_some(self.next_step)
    }

    pub(super) fn is_over(&self) -> bool {
        !self.held && self.frame == self.last()
    }

    pub(super) fn frame(&self) -> &'static Frame {
        &self.clip.frames[self.frame]
    }

    fn is_moving(&self) -> bool {
        let goal = if self.held { self.peak() } else { self.last() };
        self.frame < goal
    }

    fn peak(&self) -> usize {
        peak(self.clip)
    }

    fn last(&self) -> usize {
        self.clip.frames.len() - 1
    }
}

fn peak(clip: &Clip) -> usize {
    clip.frames.len() / 2
}

fn step(clip: &Clip, frame: usize) -> Duration {
    let leaving = if frame == peak(clip) {
        frame + 1
    } else {
        frame
    };
    clip.frames[leaving].duration
}

#[cfg(test)]
mod tests {
    use super::*;

    fn images(animation: Animation) -> Vec<Option<usize>> {
        ANIMATIONS
            .clip(animation)
            .frames
            .iter()
            .map(|frame| frame.images.first().map(|placed| placed.image))
            .collect()
    }

    fn durations(animation: Animation) -> Vec<Duration> {
        ANIMATIONS
            .clip(animation)
            .frames
            .iter()
            .map(|frame| frame.duration)
            .collect()
    }

    fn shown(glance: &Glance) -> Option<usize> {
        glance.frame().images.first().map(|placed| placed.image)
    }

    #[test]
    fn glances_rise_to_the_middle_frame_and_mirror_back() {
        for animation in [
            Animation::IdleEyeBrowRaise,
            Animation::LookUp,
            Animation::LookDown,
        ] {
            let clip = ANIMATIONS.clip(animation);
            let last = clip.frames.len() - 1;
            let peak = clip.frames.len() / 2;
            assert_eq!(clip.frames.len() % 2, 1, "{animation:?}");
            assert_eq!(
                images(animation)[0],
                images(animation)[last],
                "{animation:?}"
            );
            assert!(
                (0..peak).all(|frame| clip.frames[frame]
                    .exit
                    .is_none_or(|exit| exit == last - frame || frame == 0)),
                "{animation:?}"
            );
        }
        assert_eq!(
            images(Animation::IdleEyeBrowRaise),
            [
                Some(0),
                Some(89),
                Some(90),
                Some(91),
                Some(90),
                Some(89),
                Some(0)
            ]
        );
        assert_eq!(durations(Animation::LookUp), durations(Animation::LookDown));
    }

    #[test]
    fn brows_rise_hold_and_fall() {
        let now = Instant::now();
        let mut glance = Glance::new(Look::AtThePointer, now);
        let raised = glance
            .deadline()
            .map(|at| at + Duration::from_millis(200))
            .unwrap();
        glance.advance(raised);
        assert_eq!(shown(&glance), Some(91));
        assert_eq!(glance.deadline(), None);
        glance.hold(false, raised);
        glance.advance(raised + Duration::from_secs(1));
        assert!(glance.is_over());
        assert_eq!(shown(&glance), Some(0));
    }

    #[test]
    fn brows_turn_back_from_where_they_are() {
        let now = Instant::now();
        let mut glance = Glance::new(Look::AtThePointer, now);
        glance.advance(glance.deadline().unwrap());
        assert_eq!(shown(&glance), Some(89));
        glance.hold(false, now);
        assert_eq!(shown(&glance), Some(89));
        glance.hold(true, now);
        assert_eq!(shown(&glance), Some(89));
        glance.advance(now + Duration::from_secs(1));
        assert_eq!(shown(&glance), Some(91));
    }

    #[test]
    fn the_balloon_decides_whether_clippy_looks_up_or_down() {
        let now = Instant::now();
        let peak = |side| {
            let mut glance = Glance::new(Look::AtTheBalloon(side), now);
            glance.advance(now + Duration::from_secs(1));
            shown(&glance)
        };
        assert_eq!(
            (peak(Side::Above), peak(Side::Below)),
            (Some(169), Some(112))
        );
    }
}
