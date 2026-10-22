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
const SQUINTING: RangeInclusive<usize> = 46..=51;
const LOOKING_UP: RangeInclusive<usize> = 52..=61;
const STIRS: [RangeInclusive<usize>; 3] = [OPENING_AN_EYE, SQUINTING, LOOKING_UP];
const TOSSING: RangeInclusive<usize> = 62..=80;
const STARTLED: RangeInclusive<usize> = 56..=60;
const SLITS: usize = 48;
const SLITS_HELD: Duration = Duration::from_millis(1500);
const OPENING_THE_EYES: usize = 81;
const GETTING_UP: usize = 83;
const RESTLESS_PAUSE: RangeInclusive<Duration> = Duration::from_mins(1)..=Duration::from_mins(4);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Wish {
    Sleep,
    Wake(Manner),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
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
    Tossing,
    GettingUp,
    Awake,
}

pub(super) struct Slumber {
    frame: usize,
    next_step: Instant,
    restless_at: Instant,
    waking: Option<Manner>,
    disturbed: bool,
    napping: bool,
}

impl Slumber {
    pub(super) fn doze_off(now: Instant, rng: &mut Rng) -> Self {
        Self {
            frame: 0,
            next_step: now + clip().frames[0].duration,
            restless_at: now + random_pause(rng, &RESTLESS_PAUSE),
            waking: None,
            disturbed: false,
            napping: false,
        }
    }

    pub(super) fn advance(
        &mut self,
        wish: Wish,
        now: Instant,
        rng: &mut Rng,
        sounds: &mut Vec<Sound>,
    ) {
        let waking = match wish {
            Wish::Sleep => None,
            Wish::Wake(manner) => Some(self.waking.map_or(manner, |waking| waking.max(manner))),
        };
        if self.frame == SLITS && waking == Some(Manner::AtOnce) && self.waking != waking {
            self.next_step -= SLITS_HELD.saturating_sub(self.frame().duration);
        }
        self.waking = waking;
        self.disturbed |= self.waking.is_some();
        self.napping &= self.waking.is_some();
        while !self.is_awake() && now >= self.next_step {
            self.frame = self.next(rng);
            if self.frame == *SNORING.start() {
                self.disturbed = false;
            }
            self.next_step += self.duration();
            sounds.extend(self.frame().sound);
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
            Phase::Snoring | Phase::Stirring | Phase::Tossing
        )
    }

    pub(super) fn frame(&self) -> &'static Frame {
        &clip().frames[self.frame]
    }

    fn duration(&self) -> Duration {
        if self.frame == SLITS {
            SLITS_HELD
        } else {
            self.frame().duration
        }
    }

    fn phase(&self) -> Phase {
        match self.frame {
            frame if frame == last() => Phase::Awake,
            frame if frame < *LYING_DOWN.start() => Phase::DozingOff,
            frame if LYING_DOWN.contains(&frame) => Phase::LyingDown,
            frame if SNORING.contains(&frame) => Phase::Snoring,
            frame if STIRS.iter().any(|stir| stir.contains(&frame)) => Phase::Stirring,
            frame if TOSSING.contains(&frame) => Phase::Tossing,
            _ => Phase::GettingUp,
        }
    }

    fn next(&mut self, rng: &mut Rng) -> usize {
        let frame = self.frame;
        let exit = self.frame().exit;
        match (self.phase(), self.waking) {
            (Phase::DozingOff, Some(_)) => frame.checked_sub(1).unwrap_or(last()),
            (Phase::GettingUp | Phase::Awake, _) => (frame + 1).min(last()),
            (_, Some(Manner::Gently)) if STARTLED.contains(&frame) => OPENING_THE_EYES,
            (_, Some(Manner::Gently)) if frame == *SQUINTING.end() => {
                self.napping = true;
                *TOSSING.start()
            }
            (Phase::Stirring, Some(Manner::Gently)) if frame != *LOOKING_UP.end() => frame + 1,
            (Phase::Tossing, Some(Manner::Gently)) if self.napping && frame == *TOSSING.end() => {
                *LOOKING_UP.start()
            }
            (Phase::Tossing, Some(Manner::Gently)) if self.napping => {
                self.frame().branch(rng).unwrap_or(frame + 1)
            }
            (_, Some(Manner::Gently)) if exit == Some(GETTING_UP) => *SQUINTING.start(),
            (_, Some(_)) => exit.unwrap_or(frame + 1),
            (Phase::Snoring, None) if frame == *SNORING.end() => self.snore_or_stir(rng),
            (Phase::Stirring, None)
                if frame == *OPENING_AN_EYE.end()
                    || (frame == *SQUINTING.end() && self.disturbed) =>
            {
                *TOSSING.start()
            }
            (Phase::Tossing, None) if frame == *TOSSING.end() => *SNORING.start(),
            _ => self.frame().branch(rng).unwrap_or(frame + 1),
        }
    }

    fn snore_or_stir(&mut self, rng: &mut Rng) -> usize {
        if self.next_step < self.restless_at {
            return *SNORING.start();
        }
        self.restless_at = self.next_step + random_pause(rng, &RESTLESS_PAUSE);
        match rng.u32(..10) {
            0..5 => *TOSSING.start(),
            5..7 => *SQUINTING.start(),
            7..9 => *LOOKING_UP.start(),
            _ => *OPENING_AN_EYE.start(),
        }
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

    const SNORING_AGAIN: RangeInclusive<usize> = 69..=74;
    const WAKING_UP: [usize; 7] = [81, 82, 83, 84, 85, 86, 87];
    const RESTLESS_WAYS: [usize; 4] = [62, 38, 46, 52];

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
        (from, to) == (*TOSSING.end(), *SNORING.start())
            || (from, to) == (*OPENING_AN_EYE.end(), *TOSSING.start())
            || (from == *SNORING.end() && RESTLESS_WAYS.contains(&to))
    }

    fn sleeps_as_authored(from: usize, frames: &[usize]) -> bool {
        let mut previous = from;
        frames.iter().all(|&frame| {
            let fine = authored(previous, frame) || rejoins(previous, frame);
            previous = frame;
            fine
        })
    }

    fn tosses_and_sleeps_on(frames: &[usize]) -> bool {
        let Some((last, tossing)) = frames.split_last() else {
            return false;
        };
        *last == *SNORING.start()
            && tossing.last() == Some(TOSSING.end())
            && tossing.iter().all(|frame| TOSSING.contains(frame))
    }

    fn squints_naps_and_starts_awake(frames: &[usize]) -> bool {
        let squinting: Vec<usize> = SQUINTING.collect();
        let starting: Vec<usize> = (*LOOKING_UP.start()..=*STARTLED.start())
            .chain(WAKING_UP)
            .collect();
        let Some(nap) = frames
            .strip_prefix(squinting.as_slice())
            .and_then(|rest| rest.strip_suffix(starting.as_slice()))
        else {
            return false;
        };
        nap.first() == Some(TOSSING.start())
            && nap.last() == Some(TOSSING.end())
            && nap.iter().all(|frame| TOSSING.contains(frame))
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
                (*SQUINTING.start(), 5),
                (*LOOKING_UP.start(), 5)
            ]
        );
        assert_eq!(
            branches(*SNORING_AGAIN.end()),
            [(*SNORING_AGAIN.start(), 20)]
        );
        assert!(
            (*OPENING_AN_EYE.start()..=*TOSSING.end())
                .filter(|&frame| frame != *SNORING_AGAIN.end())
                .all(|frame| frames[frame].branches.is_empty())
        );
        assert_eq!(frames[*LYING_DOWN.end()].exit, Some(GETTING_UP));
        assert!(
            SNORING
                .clone()
                .chain(TOSSING)
                .chain([*LOOKING_UP.end()])
                .filter_map(|frame| frames[frame].exit)
                .all(|exit| exit == GETTING_UP || TOSSING.contains(&exit))
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
    fn sleeps_as_authored_and_is_restless_only_now_and_then() {
        let mut run = Run::asleep();
        let from = run.slumber.frame;
        run.for_a_while(Wish::Sleep, Duration::from_hours(1));
        assert!(sleeps_as_authored(from, &run.frames));
        assert!(run.slumber.is_sound_asleep());
        let restless = |way: fn(usize) -> bool| {
            run.frames
                .windows(2)
                .filter(|pair| pair[0] == *SNORING.end() && way(pair[1]))
                .count()
        };
        let all = restless(|frame| frame != *SNORING.start());
        let tossing = restless(|frame| frame == *TOSSING.start());
        assert!((14..=60).contains(&all), "restless {all} times in an hour");
        assert!((1..all).contains(&tossing), "tossed {tossing} of {all}");
        let tosses = run
            .frames
            .iter()
            .filter(|&&frame| frame == *TOSSING.start())
            .count();
        assert!(tosses.abs_diff(all) <= 1, "{tosses} tosses for {all}");
        assert!(run.frames.iter().any(|frame| SNORING_AGAIN.contains(frame)));
        assert!(run.frames.iter().all(|frame| !WAKING_UP.contains(frame)));
    }

    #[test]
    fn is_restless_in_the_authored_ways_and_sleeps_on() {
        let mut ways = Vec::new();
        for seed in 0..60 {
            let mut run = Run::asleep();
            run.rng = Rng::with_seed(seed);
            run.slumber.restless_at = run.now;
            run.until(Wish::Sleep, |slumber| !SNORING.contains(&slumber.frame));
            let way = run.slumber.frame;
            ways.push(way);
            run.until(Wish::Sleep, |slumber| slumber.frame == *SNORING.start());
            assert!(sleeps_as_authored(way, &run.frames));
            let stir: Vec<usize> = match way {
                62 => Vec::new(),
                38 => (39..=45).collect(),
                _ => (way + 1..=61).collect(),
            };
            let rest = run
                .frames
                .strip_prefix(stir.as_slice())
                .expect("the whole stir should play");
            if way != *TOSSING.start() {
                assert_eq!(rest.first(), Some(TOSSING.start()), "{rest:?}");
            }
            assert!(tosses_and_sleeps_on(rest), "{rest:?}");
        }
        assert!(
            RESTLESS_WAYS.iter().all(|way| ways.contains(way)),
            "{ways:?}"
        );
    }

    #[test]
    fn squints_for_a_while_naps_again_and_only_then_starts_awake() {
        for snoring in 0..SNORING.count() {
            let mut run = Run::asleep();
            for _ in 0..snoring {
                run.step(Wish::Sleep);
            }
            let lying = run.until(GENTLY, |slumber| slumber.frame == OPENING_THE_EYES);
            assert!(lying >= Duration::from_millis(5500), "{lying:?}");
            let mut frames = run.frames.clone();
            let took = lying + run.until(GENTLY, Slumber::is_awake);
            assert!(took <= Duration::from_secs(10), "{took:?}");
            frames.extend(&run.frames);
            let squinting = frames
                .iter()
                .position(|&frame| frame == *SQUINTING.start())
                .unwrap();
            assert!(squinting <= 2, "{frames:?}");
            assert!(
                squints_naps_and_starts_awake(&frames[squinting..]),
                "{frames:?}"
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
                    .all(|frame| STIRS.iter().all(|stir| !stir.contains(frame)))
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
    fn hurries_up_the_moment_something_needs_him_while_he_wakes_gently() {
        for gently in 0..30 {
            let mut run = Run::asleep();
            for _ in 0..gently {
                run.step(GENTLY);
            }
            let from = run.slumber.frame;
            let start = run.now;
            run.slumber
                .advance(AT_ONCE, run.now, &mut run.rng, &mut Vec::new());
            run.until(AT_ONCE, Slumber::is_awake);
            let took = run.now - start;
            assert!(took <= Duration::from_secs(1), "{took:?} from {from}");
            assert!(run.frames.ends_with(&[83, 84, 85, 86, 87]));
            let mut previous = from;
            for &frame in &run.frames {
                let original = clip().frames[previous].exit == Some(frame) || frame == previous + 1;
                assert!(original, "{previous} -> {frame}");
                previous = frame;
            }
        }
    }

    #[test]
    fn never_slows_down_once_hurrying() {
        let mut run = Run::asleep();
        run.slumber
            .advance(AT_ONCE, run.now, &mut Rng::with_seed(0), &mut Vec::new());
        run.until(GENTLY, Slumber::is_awake);
        assert!(!run.frames.contains(SQUINTING.start()));
    }

    #[test]
    fn wakes_from_anywhere_in_his_sleep_without_snoring_on() {
        for start in *OPENING_AN_EYE.start()..=*TOSSING.end() {
            let mut run = Run::asleep();
            run.slumber.frame = start;
            run.until(GENTLY, Slumber::is_awake);
            assert!(run.frames.ends_with(&WAKING_UP), "from {start}");
            assert!(
                run.frames.iter().all(|frame| !SNORING.contains(frame)),
                "from {start}: {:?}",
                run.frames
            );
        }
    }

    #[test]
    fn gets_up_straight_away_once_startled() {
        for open in STARTLED {
            let mut run = Run::asleep();
            run.slumber.frame = open;
            run.until(GENTLY, Slumber::is_awake);
            assert_eq!(run.frames, WAKING_UP, "from {open}");
        }
    }

    #[test]
    fn closes_his_eyes_and_tosses_when_left_alone_while_squinting() {
        let mut run = Run::asleep();
        run.until(GENTLY, |slumber| slumber.frame == 47);
        run.until(Wish::Sleep, |slumber| slumber.frame == *SNORING.start());
        let rest = run.frames.strip_prefix(&[48, 49, 50, 51, 62]).unwrap();
        assert!(tosses_and_sleeps_on(rest), "{:?}", run.frames);
    }

    #[test]
    fn dozes_on_when_left_alone_during_the_nap() {
        let mut run = Run::asleep();
        run.until(GENTLY, |slumber| slumber.frame == 65);
        run.until(Wish::Sleep, |slumber| slumber.frame == *SNORING.start());
        assert!(tosses_and_sleeps_on(&run.frames), "{:?}", run.frames);
    }

    #[test]
    fn holds_the_slits_open_for_a_while_whoever_woke_him() {
        for wish in [GENTLY, Wish::Sleep] {
            let mut run = Run::asleep();
            if wish == Wish::Sleep {
                run.slumber.frame = *SQUINTING.start();
            }
            run.until(wish, |slumber| slumber.frame == SLITS);
            let held = run.until(wish, |slumber| slumber.frame != SLITS);
            assert_eq!(held, SLITS_HELD, "{wish:?}");
        }
        assert!(SLITS_HELD > clip().frames[SLITS].duration);
    }

    #[test]
    fn keeps_his_eyes_open_on_his_own_as_long_as_when_squinting_at_a_pointer() {
        let open_for = |frames: RangeInclusive<usize>| -> Duration {
            frames
                .map(|frame| {
                    if frame == SLITS {
                        SLITS_HELD
                    } else {
                        clip().frames[frame].duration
                    }
                })
                .sum()
        };
        let squinting = open_for(47..=49);
        assert!(open_for(39..=42) >= Duration::from_millis(1500));
        assert!(open_for(54..=59) >= squinting);
        assert!(squinting >= Duration::from_millis(1700));
    }

    #[test]
    fn looks_up_after_squinting_when_nobody_disturbed_him() {
        let mut run = Run::asleep();
        run.slumber.frame = *SQUINTING.start();
        run.until(Wish::Sleep, |slumber| slumber.frame == *TOSSING.start());
        assert!(run.frames.iter().copied().eq(47..=62));
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
        let (lying_down, waking) = run.frames.split_at(4);
        assert_eq!(lying_down, [19, 20, 23, 26]);
        assert!(squints_naps_and_starts_awake(waking), "{waking:?}");
    }
}
