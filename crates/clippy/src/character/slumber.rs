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
use std::time::{Duration, Instant};

use fastrand::Rng;

use super::random_pause;
use crate::animation::{Animation, Clip, Frame};
use crate::assets::ANIMATIONS;
use crate::sound::Sound;

const LYING_DOWN: RangeInclusive<usize> = 17..=26;
const SNORING: RangeInclusive<usize> = 27..=37;
const OPENING_AN_EYE: RangeInclusive<usize> = 38..=45;
const PEEKING: RangeInclusive<usize> = 46..=51;
const RAISING_A_HAND: RangeInclusive<usize> = 52..=61;
const STIRS: [RangeInclusive<usize>; 3] = [OPENING_AN_EYE, PEEKING, RAISING_A_HAND];
const DRIFTING_OFF: RangeInclusive<usize> = 62..=80;
const EYE_OPEN: usize = 42;
const OPENING_THE_EYES: usize = 81;
const STIR_PAUSE: RangeInclusive<Duration> = Duration::from_mins(3)..=Duration::from_mins(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Wish {
    Sleep,
    Wake(Manner),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Manner {
    Gently,
    AtOnce,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    DozingOff,
    LyingDown,
    Snoring,
    Stirring,
    DriftingOff,
    GettingUp,
    Awake,
}

pub(super) struct Slumber {
    frame: usize,
    next_step: Instant,
    stir_at: Instant,
    waking: Option<Manner>,
}

impl Slumber {
    pub(super) fn doze_off(now: Instant, rng: &mut Rng) -> Self {
        Self {
            frame: 0,
            next_step: now + clip().frames[0].duration,
            stir_at: now + random_pause(rng, &STIR_PAUSE),
            waking: None,
        }
    }

    pub(super) fn advance(
        &mut self,
        wish: Wish,
        now: Instant,
        rng: &mut Rng,
        sounds: &mut Vec<Sound>,
    ) {
        self.waking = match wish {
            Wish::Sleep => None,
            Wish::Wake(manner) => Some(self.waking.unwrap_or(manner)),
        };
        while !self.is_awake() && now >= self.next_step {
            self.frame = self.next(rng);
            let frame = self.frame();
            self.next_step += frame.duration;
            sounds.extend(frame.sound);
        }
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        (!self.is_awake()).then_some(self.next_step)
    }

    pub(super) fn is_awake(&self) -> bool {
        self.phase() == Phase::Awake
    }

    #[cfg(test)]
    pub(super) fn is_sound_asleep(&self) -> bool {
        matches!(
            self.phase(),
            Phase::Snoring | Phase::Stirring | Phase::DriftingOff
        )
    }

    pub(super) fn frame(&self) -> &'static Frame {
        &clip().frames[self.frame]
    }

    fn phase(&self) -> Phase {
        match self.frame {
            frame if frame == last() => Phase::Awake,
            frame if frame < *LYING_DOWN.start() => Phase::DozingOff,
            frame if LYING_DOWN.contains(&frame) => Phase::LyingDown,
            frame if SNORING.contains(&frame) => Phase::Snoring,
            frame if STIRS.iter().any(|stir| stir.contains(&frame)) => Phase::Stirring,
            frame if DRIFTING_OFF.contains(&frame) => Phase::DriftingOff,
            _ => Phase::GettingUp,
        }
    }

    fn next(&mut self, rng: &mut Rng) -> usize {
        let frame = self.frame;
        let exit = self.frame().exit;
        match (self.phase(), self.waking) {
            (Phase::DozingOff, Some(_)) => frame.checked_sub(1).unwrap_or(last()),
            (Phase::GettingUp | Phase::Awake, _) => (frame + 1).min(last()),
            (Phase::LyingDown, Some(Manner::Gently)) if frame == *LYING_DOWN.end() => {
                *OPENING_AN_EYE.start()
            }
            (_, Some(Manner::AtOnce)) | (Phase::LyingDown, Some(Manner::Gently)) => {
                exit.unwrap_or(frame + 1)
            }
            (Phase::Stirring, Some(Manner::Gently)) if frame == EYE_OPEN => OPENING_THE_EYES,
            (Phase::Stirring, Some(Manner::Gently))
                if (*OPENING_AN_EYE.start()..EYE_OPEN).contains(&frame) =>
            {
                frame + 1
            }
            (_, Some(Manner::Gently)) if exit.is_some() => *OPENING_AN_EYE.start(),
            (Phase::Snoring, None) if frame == *SNORING.end() => self.snore_or_stir(rng),
            (Phase::Stirring, None) if STIRS.iter().any(|stir| frame == *stir.end()) => {
                *DRIFTING_OFF.start()
            }
            (Phase::DriftingOff, None) if frame == *DRIFTING_OFF.end() => *SNORING.start(),
            _ => self.frame().branch(rng).unwrap_or(frame + 1),
        }
    }

    fn snore_or_stir(&mut self, rng: &mut Rng) -> usize {
        if self.next_step < self.stir_at {
            return *SNORING.start();
        }
        self.stir_at = self.next_step + random_pause(rng, &STIR_PAUSE);
        *STIRS[rng.usize(..STIRS.len())].start()
    }
}

fn clip() -> &'static Clip {
    ANIMATIONS.clip(Animation::IdleSnooze)
}

fn last() -> usize {
    clip().frames.len() - 1
}

#[cfg(test)]
mod tests {
    use super::*;

    const GENTLY: Wish = Wish::Wake(Manner::Gently);
    const AT_ONCE: Wish = Wish::Wake(Manner::AtOnce);

    const GETTING_UP: usize = 83;
    const SNORING_AGAIN: RangeInclusive<usize> = 69..=74;
    const WAKING_UP: [usize; 7] = [81, 82, 83, 84, 85, 86, 87];

    struct Run {
        slumber: Slumber,
        now: Instant,
        rng: Rng,
        frames: Vec<usize>,
    }

    impl Run {
        fn new() -> Self {
            let now = Instant::now();
            let mut rng = Rng::with_seed(5);
            Self {
                slumber: Slumber::doze_off(now, &mut rng),
                now,
                rng,
                frames: Vec::new(),
            }
        }

        fn asleep() -> Self {
            let mut run = Self::new();
            run.until(Wish::Sleep, Slumber::is_sound_asleep);
            run
        }

        fn step(&mut self, wish: Wish) {
            self.now = self.slumber.deadline().expect("Clippy should still move");
            self.slumber
                .advance(wish, self.now, &mut self.rng, &mut Vec::new());
            self.frames.push(self.slumber.frame);
        }

        fn until(&mut self, wish: Wish, done: impl Fn(&Slumber) -> bool) -> Duration {
            let start = self.now;
            self.frames.clear();
            while !done(&self.slumber) {
                assert!(
                    self.now - start < Duration::from_secs(60),
                    "never got there"
                );
                self.step(wish);
            }
            self.now - start
        }

        fn for_a_while(&mut self, wish: Wish, how_long: Duration) {
            let end = self.now + how_long;
            self.frames.clear();
            while self.now < end {
                self.step(wish);
            }
        }
    }

    fn authored(from: usize, to: usize) -> bool {
        let frame = &clip().frames[from];
        to == from + 1 || frame.branches.iter().any(|branch| branch.frame == to)
    }

    fn rejoins(from: usize, to: usize) -> bool {
        (from, to) == (*DRIFTING_OFF.end(), *SNORING.start())
            || (to == *DRIFTING_OFF.start() && STIRS.iter().any(|stir| from == *stir.end()))
            || (from == *SNORING.end() && STIRS.iter().any(|stir| to == *stir.start()))
    }

    fn sleeps_as_authored(from: usize, frames: &[usize]) -> bool {
        let mut previous = from;
        frames.iter().all(|&frame| {
            let fine = authored(previous, frame) || rejoins(previous, frame);
            previous = frame;
            fine
        })
    }

    fn stir_of(frame: usize) -> Option<usize> {
        STIRS.iter().position(|stir| stir.contains(&frame))
    }

    fn picture(frame: usize) -> usize {
        clip().frames[frame].images[0].image
    }

    #[test]
    fn the_original_animation_has_the_shape_we_rely_on() {
        let frames = &clip().frames;
        let branches = |frame: usize| -> Vec<(usize, u32)> {
            frames[frame]
                .branches
                .iter()
                .map(|branch| (branch.frame, branch.weight))
                .collect()
        };
        assert_eq!(frames.len(), 88);
        assert_eq!(
            branches(*SNORING.end()),
            [
                (*SNORING.start(), 90),
                (*PEEKING.start(), 5),
                (*RAISING_A_HAND.start(), 5)
            ]
        );
        assert_eq!(
            branches(*SNORING_AGAIN.end()),
            [(*SNORING_AGAIN.start(), 20)]
        );
        assert!(
            (*OPENING_AN_EYE.start()..=*DRIFTING_OFF.end())
                .filter(|&frame| frame != *SNORING_AGAIN.end())
                .all(|frame| frames[frame].branches.is_empty())
        );
        assert_eq!(frames[*LYING_DOWN.end()].exit, Some(GETTING_UP));
        assert!(
            SNORING
                .clone()
                .filter(|&frame| frames[frame].exit.is_some())
                .all(|frame| frames[frame].exit == Some(GETTING_UP))
        );
        assert!(
            frames
                .iter()
                .flat_map(|frame| &frame.branches)
                .all(|branch| !OPENING_AN_EYE.contains(&branch.frame))
        );
        assert_eq!(picture(0), 0);
        assert_eq!(picture(last()), 0);
    }

    #[test]
    fn sleeps_the_way_the_original_does_and_stirs_only_now_and_then() {
        let mut run = Run::asleep();
        let from = run.slumber.frame;
        run.for_a_while(Wish::Sleep, Duration::from_hours(1));
        assert!(sleeps_as_authored(from, &run.frames));
        assert!(run.slumber.is_sound_asleep());
        let stirs = run
            .frames
            .windows(2)
            .filter(|pair| pair[0] == *SNORING.end() && stir_of(pair[1]).is_some())
            .count();
        assert!((4..=21).contains(&stirs), "{stirs} stirs in an hour");
        let drifts = run
            .frames
            .iter()
            .filter(|&&frame| frame == *DRIFTING_OFF.start())
            .count();
        assert!(
            drifts.abs_diff(stirs) <= 1,
            "{drifts} drifts for {stirs} stirs"
        );
        assert!(run.frames.iter().any(|frame| SNORING_AGAIN.contains(frame)));
        assert!(run.frames.iter().all(|frame| !WAKING_UP.contains(frame)));
    }

    #[test]
    fn stirs_in_one_of_the_three_ways_and_drifts_off_again() {
        let mut ways = Vec::new();
        for seed in 0..30 {
            let mut run = Run::asleep();
            run.rng = Rng::with_seed(seed);
            run.slumber.stir_at = run.now;
            run.until(Wish::Sleep, |slumber| stir_of(slumber.frame).is_some());
            let way = stir_of(run.slumber.frame).unwrap();
            ways.push(way);
            let from = run.slumber.frame;
            run.until(Wish::Sleep, |slumber| slumber.frame == *SNORING.start());
            assert!(sleeps_as_authored(from, &run.frames));
            let stir: Vec<usize> = STIRS[way].clone().skip(1).collect();
            let rest = run
                .frames
                .strip_prefix(stir.as_slice())
                .expect("the whole stir should play");
            assert_eq!(rest.first(), Some(DRIFTING_OFF.start()), "{rest:?}");
            assert!(
                rest[..rest.len() - 1]
                    .iter()
                    .all(|frame| DRIFTING_OFF.contains(frame))
            );
        }
        assert!((0..STIRS.len()).all(|way| ways.contains(&way)), "{ways:?}");
    }

    #[test]
    fn wakes_by_rubbing_an_eye_and_opening_both() {
        for snoring in 0..SNORING.count() {
            let mut run = Run::asleep();
            for _ in 0..snoring {
                run.step(Wish::Sleep);
            }
            let took = run.until(GENTLY, Slumber::is_awake);
            assert!(took <= Duration::from_secs(4), "{took:?}");
            let rubbing = run
                .frames
                .iter()
                .position(|&frame| frame == *OPENING_AN_EYE.start())
                .unwrap();
            assert!(rubbing <= 2, "{:?}", run.frames);
            assert_eq!(
                run.frames[rubbing..],
                [38, 39, 40, 41, 42, 81, 82, 83, 84, 85, 86, 87]
            );
        }
    }

    #[test]
    fn wakes_at_once_through_the_original_exits_when_something_needs_him() {
        for snoring in 0..SNORING.count() {
            let mut run = Run::asleep();
            for _ in 0..snoring {
                run.step(Wish::Sleep);
            }
            let from = run.slumber.frame;
            let took = run.until(AT_ONCE, Slumber::is_awake);
            assert!(took <= Duration::from_secs(1), "{took:?}");
            assert!(run.frames.ends_with(&[83, 84, 85, 86, 87]));
            assert!(
                run.frames
                    .iter()
                    .all(|frame| !OPENING_AN_EYE.contains(frame))
            );
            let mut previous = from;
            for &frame in &run.frames {
                let original = clip().frames[previous].exit == Some(frame) || frame == previous + 1;
                assert!(original, "{previous} -> {frame}");
                previous = frame;
            }
        }
    }

    #[test]
    fn a_gentle_wake_stays_gentle_whatever_comes_next() {
        let mut run = Run::asleep();
        run.slumber
            .advance(GENTLY, run.now, &mut Rng::with_seed(0), &mut Vec::new());
        run.until(AT_ONCE, Slumber::is_awake);
        assert!(run.frames.contains(OPENING_AN_EYE.start()));
        let mut run = Run::asleep();
        run.slumber
            .advance(AT_ONCE, run.now, &mut Rng::with_seed(0), &mut Vec::new());
        run.until(GENTLY, Slumber::is_awake);
        assert!(!run.frames.contains(OPENING_AN_EYE.start()));
    }

    #[test]
    fn wakes_from_a_stir_too() {
        for start in *OPENING_AN_EYE.start()..=*DRIFTING_OFF.end() {
            let mut run = Run::asleep();
            run.slumber.frame = start;
            run.until(GENTLY, Slumber::is_awake);
            assert!(run.frames.ends_with(&WAKING_UP), "from {start}");
        }
    }

    #[test]
    fn drifts_off_again_when_left_alone_while_opening_an_eye() {
        let mut run = Run::asleep();
        run.until(GENTLY, |slumber| slumber.frame == 39);
        run.until(Wish::Sleep, |slumber| slumber.frame == *SNORING.start());
        assert_eq!(run.frames[..7], [40, 41, 42, 43, 44, 45, 62]);
        assert!(sleeps_as_authored(39, &run.frames));
    }

    #[test]
    fn perks_up_if_woken_before_lying_down() {
        let mut run = Run::new();
        run.until(Wish::Sleep, |slumber| slumber.frame == 6);
        run.until(GENTLY, Slumber::is_awake);
        assert_eq!(run.frames, [5, 4, 3, 2, 1, 0, 87]);
    }

    #[test]
    fn lies_down_before_waking_once_falling() {
        let mut run = Run::new();
        run.until(Wish::Sleep, |slumber| slumber.frame == 18);
        run.until(GENTLY, Slumber::is_awake);
        assert_eq!(
            run.frames,
            [
                19, 20, 23, 26, 38, 39, 40, 41, 42, 81, 82, 83, 84, 85, 86, 87
            ]
        );
    }
}
