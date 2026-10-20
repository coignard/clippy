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

use std::ptr;
use std::time::Instant;

use fastrand::Rng;

use crate::animation::{Clip, Frame};
use crate::sound::Sound;

pub(crate) struct Playback<'a> {
    clip: &'a Clip,
    frame: usize,
    exiting: bool,
    vanishing: bool,
    next_step: Option<Instant>,
}

impl<'a> Playback<'a> {
    pub(crate) fn start(clip: &'a Clip, now: Instant) -> Self {
        Self {
            clip,
            frame: 0,
            exiting: false,
            vanishing: false,
            next_step: Some(now + clip.frames[0].duration),
        }
    }

    #[must_use]
    pub(crate) fn vanishing(self) -> Self {
        Self {
            vanishing: true,
            ..self
        }
    }

    pub(crate) fn reversed<'b>(&self, clip: &'b Clip, now: Instant) -> Playback<'b> {
        let frame = clip.frames.len().saturating_sub(self.frame + 1);
        Playback {
            clip,
            frame,
            exiting: false,
            vanishing: false,
            next_step: Some(now + clip.frames[frame].duration),
        }
    }

    pub(crate) fn plays(&self, clip: &Clip) -> bool {
        ptr::eq(self.clip, clip)
    }

    #[cfg(test)]
    pub(crate) fn clip(&self) -> &'a Clip {
        self.clip
    }

    pub(crate) fn frame(&self) -> &'a Frame {
        &self.clip.frames[self.frame]
    }

    pub(crate) fn exit(&mut self) {
        self.exiting = true;
    }

    pub(crate) fn is_exiting(&self) -> bool {
        self.exiting
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.next_step.is_none()
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.next_step
    }

    pub(crate) fn advance(&mut self, now: Instant, rng: &mut Rng, sounds: &mut Vec<Sound>) {
        while let Some(at) = self.next_step
            && now >= at
        {
            if self.frame == self.last_frame() {
                self.next_step = None;
            } else {
                self.frame = self.next_frame(rng);
                let frame = self.frame();
                sounds.extend(frame.sound);
                self.next_step =
                    (!self.vanishing || frame.shows_clippy()).then(|| at + frame.duration);
            }
        }
    }

    fn next_frame(&self, rng: &mut Rng) -> usize {
        let frame = self.frame();
        if self.exiting {
            frame.exit.unwrap_or(self.frame + 1)
        } else {
            frame.branch(rng).unwrap_or(self.frame + 1)
        }
    }

    fn last_frame(&self) -> usize {
        self.clip.frames.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::animation::Branch;

    fn frame(milliseconds: u64) -> Frame {
        Frame {
            duration: Duration::from_millis(milliseconds),
            images: Vec::new(),
            sound: None,
            exit: None,
            branches: Vec::new(),
        }
    }

    fn walk(playback: &mut Playback<'_>, mut now: Instant, steps: usize) -> Vec<usize> {
        let mut rng = Rng::with_seed(0);
        let mut visited = vec![playback.frame];
        for _ in 0..steps {
            now = playback.deadline().unwrap_or(now);
            let before = playback.frame;
            playback.advance(now, &mut rng, &mut Vec::new());
            if playback.frame != before {
                visited.push(playback.frame);
            }
        }
        visited
    }

    #[test]
    fn plays_frames_in_order_then_finishes() {
        let clip = Clip {
            frames: vec![frame(100), frame(200), frame(300)],
        };
        let start = Instant::now();
        let mut playback = Playback::start(&clip, start);
        assert_eq!(
            playback.deadline(),
            Some(start + Duration::from_millis(100))
        );
        assert_eq!(walk(&mut playback, start, 2), [0, 1, 2]);
        assert!(!playback.is_finished());
        assert_eq!(walk(&mut playback, start, 1), [2]);
        assert!(playback.is_finished());
        assert_eq!(playback.deadline(), None);
    }

    #[test]
    fn waits_for_the_frame_to_end() {
        let clip = Clip {
            frames: vec![frame(100), frame(100)],
        };
        let start = Instant::now();
        let mut playback = Playback::start(&clip, start);
        let mut rng = Rng::with_seed(0);
        playback.advance(start + Duration::from_millis(99), &mut rng, &mut Vec::new());
        assert_eq!(playback.frame, 0);
        playback.advance(
            start + Duration::from_millis(100),
            &mut rng,
            &mut Vec::new(),
        );
        assert_eq!(playback.frame, 1);
    }

    #[test]
    fn keeps_to_the_schedule_when_woken_late() {
        let clip = Clip {
            frames: vec![frame(100), frame(100), frame(100), frame(100)],
        };
        let start = Instant::now();
        let mut playback = Playback::start(&clip, start);
        playback.advance(
            start + Duration::from_millis(250),
            &mut Rng::with_seed(0),
            &mut Vec::new(),
        );
        assert_eq!(playback.frame, 2);
        assert_eq!(
            playback.deadline(),
            Some(start + Duration::from_millis(300))
        );
    }

    #[test]
    fn passes_through_frames_without_a_duration_at_once() {
        let looping = Frame {
            branches: vec![Branch {
                frame: 1,
                weight: 90,
            }],
            ..frame(0)
        };
        let clip = Clip {
            frames: vec![frame(100), looping, frame(100)],
        };
        let start = Instant::now();
        let mut playback = Playback::start(&clip, start);
        playback.advance(
            start + Duration::from_millis(100),
            &mut Rng::with_seed(0),
            &mut Vec::new(),
        );
        assert_eq!(playback.frame, 2);
        assert_eq!(
            playback.deadline(),
            Some(start + Duration::from_millis(200))
        );
    }

    #[test]
    fn loops_through_a_certain_branch_until_asked_to_exit() {
        let looping = Frame {
            exit: Some(3),
            branches: vec![Branch {
                frame: 0,
                weight: 100,
            }],
            ..frame(10)
        };
        let clip = Clip {
            frames: vec![frame(10), looping, frame(10), frame(10)],
        };
        let now = Instant::now();
        let mut playback = Playback::start(&clip, now);
        assert_eq!(walk(&mut playback, now, 4), [0, 1, 0, 1, 0]);
        walk(&mut playback, now, 1);
        playback.exit();
        assert_eq!(walk(&mut playback, now, 1), [1, 3]);
    }

    #[test]
    fn stops_branching_once_asked_to_exit() {
        let looping = Frame {
            branches: vec![Branch {
                frame: 0,
                weight: 100,
            }],
            ..frame(10)
        };
        let clip = Clip {
            frames: vec![frame(10), looping, frame(10)],
        };
        let now = Instant::now();
        let mut playback = Playback::start(&clip, now);
        playback.exit();
        assert_eq!(walk(&mut playback, now, 3), [0, 1, 2]);
        assert!(playback.is_finished());
    }
}
