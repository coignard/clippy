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

use std::ops::RangeInclusive;
use std::time::Instant;

use fastrand::Rng;

use crate::animation::{Animation, Clip, Frame};
use crate::assets::ANIMATIONS;
use crate::sound::Sound;

const PAD_IN_HAND: usize = 12;
const STROKES: [usize; 3] = [13, 32, 42];
const STROKE_ENDS: [usize; 3] = [25, 41, 53];
const TURNING_PAGE: RangeInclusive<usize> = 45..=51;
const LOOKING_UP: usize = 26;
const LOOKED_UP: usize = 27;
const WAITING: usize = 28;
const BLINKING: usize = 29;
const WAITING_AGAIN: usize = 30;
const BACK_TO_WRITING: usize = 31;
const PUTTING_AWAY: usize = 54;
const LINGER: f32 = 0.8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Wish {
    Write,
    Wait,
    PutAway,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    TakingOut,
    InHand,
    Writing,
    StrokeEnd,
    TurningPage,
    LookingUp,
    Waiting,
    Blinking,
    BackToWriting,
    PuttingAway,
    Away,
}

pub(super) struct Notepad {
    frame: usize,
    next_step: Instant,
}

impl Notepad {
    pub(super) fn take_out(now: Instant) -> Self {
        Self {
            frame: 0,
            next_step: now + clip().frames[0].duration,
        }
    }

    pub(super) fn advance(
        &mut self,
        wish: Wish,
        now: Instant,
        rng: &mut Rng,
        sounds: &mut Vec<Sound>,
    ) {
        while !self.is_away() && now >= self.next_step {
            self.frame = self.next(wish, rng);
            let frame = self.frame();
            self.next_step += frame.duration;
            sounds.extend(frame.sound);
        }
    }

    pub(super) fn notice_typing(&mut self, now: Instant) {
        if matches!(
            self.phase(),
            Phase::LookingUp | Phase::Waiting | Phase::Blinking
        ) {
            self.next_step = self.next_step.min(now);
        }
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        (!self.is_away()).then_some(self.next_step)
    }

    pub(super) fn is_away(&self) -> bool {
        self.phase() == Phase::Away
    }

    #[cfg(test)]
    pub(super) fn is_writing(&self) -> bool {
        matches!(
            self.phase(),
            Phase::Writing | Phase::StrokeEnd | Phase::TurningPage
        )
    }

    #[cfg(test)]
    pub(super) fn is_waiting(&self) -> bool {
        matches!(self.phase(), Phase::Waiting | Phase::Blinking)
    }

    pub(super) fn frame(&self) -> &'static Frame {
        &clip().frames[self.frame]
    }

    fn phase(&self) -> Phase {
        match self.frame {
            frame if frame < PAD_IN_HAND => Phase::TakingOut,
            PAD_IN_HAND => Phase::InHand,
            LOOKING_UP | LOOKED_UP => Phase::LookingUp,
            WAITING | WAITING_AGAIN => Phase::Waiting,
            BLINKING => Phase::Blinking,
            BACK_TO_WRITING => Phase::BackToWriting,
            frame if frame == last() => Phase::Away,
            frame if frame >= PUTTING_AWAY => Phase::PuttingAway,
            frame if TURNING_PAGE.contains(&frame) => Phase::TurningPage,
            frame if STROKE_ENDS.contains(&frame) => Phase::StrokeEnd,
            _ => Phase::Writing,
        }
    }

    fn next(&self, wish: Wish, rng: &mut Rng) -> usize {
        let frame = self.frame;
        match (self.phase(), wish) {
            (Phase::TakingOut, Wish::PutAway) => frame.checked_sub(1).unwrap_or(last()),
            (Phase::TakingOut | Phase::TurningPage | Phase::PuttingAway | Phase::Away, _) => {
                (frame + 1).min(last())
            }
            (_, Wish::PutAway) => self.frame().exit.unwrap_or(frame + 1),
            (Phase::Waiting, Wish::Wait) if rng.f32() < LINGER => frame,
            (Phase::Waiting, Wish::Wait) if frame == WAITING_AGAIN => WAITING,
            (Phase::LookingUp | Phase::Waiting | Phase::Blinking, Wish::Wait)
            | (Phase::Writing, Wish::Write) => frame + 1,
            (Phase::BackToWriting, Wish::Wait) => WAITING_AGAIN,
            (Phase::InHand | Phase::Writing | Phase::StrokeEnd, Wish::Wait) => LOOKING_UP,
            (Phase::LookingUp | Phase::Waiting | Phase::Blinking, Wish::Write) => BACK_TO_WRITING,
            (Phase::InHand | Phase::StrokeEnd | Phase::BackToWriting, Wish::Write) => {
                STROKES[rng.usize(..STROKES.len())]
            }
        }
    }
}

fn clip() -> &'static Clip {
    ANIMATIONS.clip(Animation::Writing)
}

fn last() -> usize {
    clip().frames.len() - 1
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    struct Run {
        notepad: Notepad,
        now: Instant,
        rng: Rng,
        frames: Vec<usize>,
    }

    impl Run {
        fn new() -> Self {
            let now = Instant::now();
            Self {
                notepad: Notepad::take_out(now),
                now,
                rng: Rng::with_seed(3),
                frames: Vec::new(),
            }
        }

        fn until(&mut self, wish: Wish, done: impl Fn(&Notepad) -> bool) -> Duration {
            let start = self.now;
            self.frames.clear();
            while !done(&self.notepad) {
                assert!(
                    self.now - start < Duration::from_secs(60),
                    "never got there"
                );
                self.now = self.notepad.deadline().expect("the pad should still move");
                self.notepad
                    .advance(wish, self.now, &mut self.rng, &mut Vec::new());
                self.frames.push(self.notepad.frame);
            }
            self.now - start
        }

        fn for_a_while(&mut self, wish: Wish, how_long: Duration) {
            let end = self.now + how_long;
            self.frames.clear();
            while self.now < end {
                self.now = self.notepad.deadline().expect("the pad should still move");
                self.notepad
                    .advance(wish, self.now, &mut self.rng, &mut Vec::new());
                self.frames.push(self.notepad.frame);
            }
        }
    }

    #[test]
    fn the_original_animation_has_the_shape_we_rely_on() {
        let frames = &clip().frames;
        let branches = |frame: usize| -> Vec<usize> {
            frames[frame]
                .branches
                .iter()
                .map(|branch| branch.frame)
                .collect()
        };
        assert_eq!(frames.len(), 61);
        assert_eq!(branches(PAD_IN_HAND), [LOOKING_UP, STROKES[1], STROKES[2]]);
        assert_eq!(branches(WAITING), [WAITING]);
        assert_eq!(branches(WAITING_AGAIN), [WAITING_AGAIN]);
        assert!(branches(BACK_TO_WRITING).contains(&STROKES[0]));
        assert!(STROKE_ENDS.iter().all(|&end| !branches(end).is_empty()));
        assert_eq!(frames[STROKES[0]].exit, Some(PUTTING_AWAY + 1));
        assert_eq!(frames[WAITING_AGAIN].exit, Some(PUTTING_AWAY + 1));
        assert_eq!(frames[last()].images[0].image, 0);
    }

    #[test]
    fn takes_the_pad_out_and_waits_looking_at_you() {
        let mut run = Run::new();
        run.until(Wish::Wait, Notepad::is_waiting);
        run.for_a_while(Wish::Wait, Duration::from_secs(30));
        assert!(
            run.frames
                .iter()
                .all(|&frame| (WAITING..=WAITING_AGAIN).contains(&frame))
        );
        assert!(run.frames.contains(&BLINKING));
    }

    #[test]
    fn writes_only_while_you_type() {
        let mut run = Run::new();
        run.until(Wish::Wait, Notepad::is_waiting);
        assert!(run.until(Wish::Write, Notepad::is_writing) <= Duration::from_millis(1000));
        run.for_a_while(Wish::Write, Duration::from_secs(30));
        assert!(
            run.frames
                .iter()
                .all(|&frame| frame >= STROKES[0] && frame < PUTTING_AWAY && frame != WAITING)
        );
        assert!(run.frames.contains(TURNING_PAGE.start()));
        assert!(run.until(Wish::Wait, Notepad::is_waiting) <= Duration::from_millis(1000));
    }

    #[test]
    fn puts_the_pad_away_from_anywhere_the_way_the_original_does() {
        let frames = &clip().frames;
        for start in PAD_IN_HAND..PUTTING_AWAY {
            let mut run = Run::new();
            run.notepad.frame = start;
            assert!(run.until(Wish::PutAway, Notepad::is_away) <= Duration::from_secs(2));
            let mut from = start;
            for &to in &run.frames {
                assert!(
                    to == from + 1 || frames[from].exit == Some(to),
                    "{from} -> {to} is not in the original"
                );
                from = to;
            }
        }
    }

    #[test]
    fn folds_the_pad_back_if_you_leave_while_it_comes_out() {
        let mut run = Run::new();
        run.until(Wish::Wait, |notepad| notepad.frame == 6);
        run.until(Wish::PutAway, Notepad::is_away);
        assert_eq!(run.frames, [5, 4, 3, 2, 1, 0, 60]);
    }
}
