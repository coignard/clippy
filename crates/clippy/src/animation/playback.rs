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
use std::time::{Duration, Instant};

use fastrand::Rng;

use crate::animation::{Clip, Frame};
use crate::sound::Sound;

const HURRIED_HOLD: Duration = Duration::from_millis(100);

pub(crate) struct Playback<'a> {
    clip: &'a Clip,
    frame: usize,
    pace: Pace,
    vanishing: bool,
    whole: bool,
    next_step: Option<Instant>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pace {
    Playing,
    Exiting,
    Hurrying,
    Rewinding,
}

impl<'a> Playback<'a> {
    pub(crate) fn start(clip: &'a Clip, now: Instant) -> Self {
        Self {
            clip,
            frame: 0,
            pace: Pace::Playing,
            vanishing: false,
            whole: false,
            next_step: Some(now + clip.frames[0].duration),
        }
    }

    #[must_use]
    pub(crate) fn whole(self) -> Self {
        Self {
            whole: true,
            ..self
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
            pace: Pace::Playing,
            vanishing: false,
            whole: false,
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
        if self.pace == Pace::Playing {
            self.pace = Pace::Exiting;
        }
    }

    pub(crate) fn hurry(&mut self) {
        if self.is_hurried() {
            return;
        }
        self.pace = Pace::Hurrying;
        if self.rewinds_sooner() {
            self.pace = Pace::Rewinding;
        }
        let duration = self.frame().duration;
        if let Some(at) = &mut self.next_step
            && let Some(cut) = duration.checked_sub(HURRIED_HOLD)
        {
            *at = at.checked_sub(cut).unwrap_or(*at);
        }
    }

    pub(crate) fn is_exiting(&self) -> bool {
        self.pace != Pace::Playing
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
            let Some(next) = self.following(rng) else {
                self.next_step = None;
                break;
            };
            self.frame = next;
            let frame = self.frame();
            if self.pace != Pace::Rewinding {
                sounds.extend(frame.sound);
            }
            let hold = self.hold(self.frame);
            let on_time = at + hold;
            let next = if on_time > now { on_time } else { now + hold };
            self.next_step = (!self.vanishing || frame.shows_clippy()).then_some(next);
        }
    }

    fn following(&self, rng: &mut Rng) -> Option<usize> {
        if self.pace == Pace::Rewinding {
            self.frame.checked_sub(1)
        } else {
            (self.frame != self.last_frame()).then(|| self.next_frame(rng))
        }
    }

    fn hold(&self, frame: usize) -> Duration {
        let duration = self.clip.frames[frame].duration;
        if self.is_hurried() {
            duration.min(HURRIED_HOLD)
        } else {
            duration
        }
    }

    fn is_hurried(&self) -> bool {
        matches!(self.pace, Pace::Hurrying | Pace::Rewinding)
    }

    fn rewinds_sooner(&self) -> bool {
        let frames = &self.clip.frames;
        if frames[0].images != frames[self.last_frame()].images {
            return false;
        }
        let rewinding: Duration = (0..self.frame).map(|frame| self.hold(frame)).sum();
        let mut finishing = Duration::ZERO;
        let mut frame = self.frame;
        while frame != self.last_frame() && finishing <= rewinding {
            frame = frames[frame].exit.unwrap_or(frame + 1);
            finishing += self.hold(frame);
        }
        rewinding < finishing
    }

    fn next_frame(&self, rng: &mut Rng) -> usize {
        let frame = self.frame();
        if self.is_exiting() {
            return frame.exit.unwrap_or(self.frame + 1);
        }
        let next = frame.branch(rng).unwrap_or(self.frame + 1);
        if !self.whole || next > self.frame {
            return next;
        }
        if !frame.always_branches() {
            return self.frame + 1;
        }
        frame
            .branches
            .iter()
            .map(|branch| branch.frame)
            .chain(frame.exit)
            .find(|&ahead| ahead > self.frame)
            .unwrap_or(self.frame + 1)
    }

    fn last_frame(&self) -> usize {
        self.clip.frames.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::animation::{Animation, Branch, FrameImage};
    use crate::assets::ANIMATIONS;

    #[test]
    fn a_whole_playback_goes_through_once_without_looping_back() {
        let clip = ANIMATIONS.clip(Animation::Searching);
        let path: Vec<usize> = (0..=40).chain(55..=60).collect();
        for seed in 0..50 {
            let mut rng = Rng::with_seed(seed);
            let mut playback = Playback::start(clip, Instant::now()).whole();
            let mut shown = vec![playback.frame];
            while let Some(at) = playback.deadline() {
                playback.advance(at, &mut rng, &mut Vec::new());
                if shown.last() != Some(&playback.frame) {
                    shown.push(playback.frame);
                }
            }
            assert_eq!(shown, path, "seed {seed}");
        }
    }

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
    fn keeps_to_the_schedule_when_woken_a_little_late() {
        let clip = Clip {
            frames: vec![frame(100), frame(100), frame(100), frame(100)],
        };
        let start = Instant::now();
        let mut playback = Playback::start(&clip, start);
        playback.advance(
            start + Duration::from_millis(150),
            &mut Rng::with_seed(0),
            &mut Vec::new(),
        );
        assert_eq!(playback.frame, 1);
        assert_eq!(
            playback.deadline(),
            Some(start + Duration::from_millis(200))
        );
    }

    #[test]
    fn never_skips_frames_when_woken_very_late() {
        let clip = Clip {
            frames: vec![frame(100), frame(100), frame(100), frame(100)],
        };
        let start = Instant::now();
        let mut playback = Playback::start(&clip, start);
        let late = start + Duration::from_millis(450);
        playback.advance(late, &mut Rng::with_seed(0), &mut Vec::new());
        assert_eq!(playback.frame, 1);
        assert_eq!(playback.deadline(), Some(late + Duration::from_millis(100)));
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

    fn noisy(milliseconds: u64) -> Frame {
        Frame {
            sound: Some(Sound::new(0)),
            ..frame(milliseconds)
        }
    }

    #[test]
    fn hurrying_rewinds_when_the_start_is_closer() {
        let clip = Clip {
            frames: vec![
                frame(10),
                noisy(10),
                frame(10),
                frame(10),
                frame(10),
                frame(10),
            ],
        };
        let now = Instant::now();
        let mut playback = Playback::start(&clip, now);
        walk(&mut playback, now, 2);
        playback.hurry();
        let mut sounds = Vec::new();
        let mut rng = Rng::with_seed(0);
        let mut visited = vec![playback.frame];
        while let Some(at) = playback.deadline() {
            playback.advance(at, &mut rng, &mut sounds);
            visited.push(playback.frame);
        }
        assert_eq!(visited, [2, 1, 0, 0]);
        assert_eq!(sounds, []);
    }

    #[test]
    fn hurrying_follows_the_exit_when_the_end_is_closer() {
        let clip = Clip {
            frames: vec![frame(10), frame(10), frame(10), frame(10), frame(10)],
        };
        let now = Instant::now();
        let mut playback = Playback::start(&clip, now);
        walk(&mut playback, now, 3);
        playback.hurry();
        assert_eq!(walk(&mut playback, now, 2), [3, 4]);
        assert!(playback.is_finished());
    }

    #[test]
    fn hurrying_never_rewinds_into_a_different_pose() {
        let ending = Frame {
            images: vec![FrameImage {
                image: 1,
                x: 0,
                y: 0,
            }],
            ..frame(10)
        };
        let clip = Clip {
            frames: vec![frame(10), frame(10), frame(10), frame(10), ending],
        };
        let now = Instant::now();
        let mut playback = Playback::start(&clip, now);
        walk(&mut playback, now, 1);
        playback.hurry();
        assert_eq!(walk(&mut playback, now, 3), [1, 2, 3, 4]);
    }

    #[test]
    fn hurrying_cuts_long_holds() {
        let clip = Clip {
            frames: vec![frame(100), frame(2000), frame(2000), frame(100)],
        };
        let start = Instant::now();
        let mut playback = Playback::start(&clip, start);
        walk(&mut playback, start, 2);
        playback.hurry();
        assert_eq!(
            playback.deadline(),
            Some(start + Duration::from_millis(2100) + HURRIED_HOLD)
        );
        let mut rng = Rng::with_seed(0);
        playback.advance(playback.deadline().unwrap(), &mut rng, &mut Vec::new());
        assert_eq!(playback.frame, 3);
    }

    #[test]
    fn hurrying_never_skips_frames_to_catch_up() {
        let clip = Clip {
            frames: vec![
                frame(10),
                frame(10),
                frame(10),
                frame(1000),
                frame(10),
                frame(10),
            ],
        };
        let start = Instant::now();
        let mut playback = Playback::start(&clip, start);
        walk(&mut playback, start, 3);
        playback.hurry();
        let late = start + Duration::from_millis(900);
        playback.advance(late, &mut Rng::with_seed(0), &mut Vec::new());
        assert_eq!(playback.frame, 4);
        assert_eq!(playback.deadline(), Some(late + Duration::from_millis(10)));
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
