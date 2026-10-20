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

mod glance;
mod notepad;
mod slumber;

use std::collections::VecDeque;
use std::mem;
use std::ops::RangeInclusive;
use std::time::{Duration, Instant};

use fastrand::Rng;

use crate::animation::{Animation, Frame, IdleLevel, Playback};
use crate::assets::{ANIMATIONS, CLIPPIT};
use crate::geometry::{Rect, Size};
use crate::graphics::Canvas;
use crate::settings::Settings;
use crate::sound::Sound;

use glance::Glance;
pub(crate) use glance::{Look, Side};
use notepad::{Notepad, Wish};
use slumber::Slumber;

const ROUNDS_PER_IDLE_LEVEL: usize = 3;
const TYPING_PAUSE: Duration = Duration::from_millis(600);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Presence {
    Absent,
    Arriving,
    Present,
    Leaving,
}

pub(crate) struct Character {
    state: State,
    queue: VecDeque<Animation>,
    departure: Option<Departure>,
    pointer: Pointer,
    field: FieldState,
    typing_until: Option<Instant>,
    beckoned: Option<Animation>,
    sounds: Vec<Sound>,
    idleness: Idleness,
    bedtime: bool,
    roused: bool,
    rng: Rng,
    idle_pause: RangeInclusive<Duration>,
    animation_timeout: Duration,
    attention_span: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Departure {
    animation: Animation,
    by: Option<Instant>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FieldState {
    #[default]
    Closed,
    Open(Side),
    Focused,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Pointer {
    #[default]
    Away,
    Over {
        since: Instant,
    },
    Ignored,
}

#[derive(Default)]
struct Idleness {
    level: IdleLevel,
    rounds: usize,
    round: Vec<Animation>,
}

#[derive(Default)]
enum State {
    #[default]
    Absent,
    Arriving(Playback<'static>),
    Resting {
        pose: Playback<'static>,
        idle_at: Instant,
    },
    Performing {
        playback: Playback<'static>,
        wrap_up_at: Instant,
    },
    Idling(Playback<'static>),
    Attentive(Glance),
    Notepad(Notepad),
    Asleep(Slumber),
    Leaving(Playback<'static>),
}

impl Character {
    pub(crate) fn new(settings: &Settings, rng: Rng) -> Self {
        Self {
            state: State::Absent,
            queue: VecDeque::new(),
            departure: None,
            pointer: Pointer::Away,
            field: FieldState::Closed,
            typing_until: None,
            beckoned: None,
            sounds: Vec::new(),
            idleness: Idleness::default(),
            bedtime: false,
            roused: false,
            rng,
            idle_pause: settings.idle_pause.clone(),
            animation_timeout: settings.animation_timeout,
            attention_span: settings.attention_span,
        }
    }

    pub(crate) fn size() -> Size {
        ANIMATIONS.frame_size()
    }

    pub(crate) fn presence(&self) -> Presence {
        if self.departure.is_some() {
            return Presence::Leaving;
        }
        match self.state {
            State::Absent => Presence::Absent,
            State::Arriving(_) => Presence::Arriving,
            State::Resting { .. }
            | State::Performing { .. }
            | State::Idling(_)
            | State::Attentive(_)
            | State::Notepad(_)
            | State::Asleep(_) => Presence::Present,
            State::Leaving(_) => Presence::Leaving,
        }
    }

    pub(crate) fn arrive(&mut self, animation: Animation, now: Instant) {
        self.disturb();
        if self.departure.take().is_some() {
            return;
        }
        self.state = match mem::take(&mut self.state) {
            State::Leaving(hiding)
                if animation == Animation::Show
                    && hiding.plays(ANIMATIONS.clip(Animation::Hide)) =>
            {
                let showing = hiding.reversed(ANIMATIONS.clip(Animation::Show), now);
                self.sounds.extend(showing.frame().sound);
                State::Arriving(showing)
            }
            State::Absent | State::Leaving(_) => State::Arriving(self.start(animation, now)),
            state => state,
        };
    }

    pub(crate) fn leave(&mut self, animation: Animation, by: Option<Instant>) {
        if matches!(self.state, State::Absent | State::Leaving(_)) {
            return;
        }
        let by = match (self.departure.and_then(|departure| departure.by), by) {
            (Some(earlier), Some(later)) => Some(earlier.min(later)),
            (earlier, later) => earlier.or(later),
        };
        self.departure = Some(Departure { animation, by });
        self.queue.clear();
        if let State::Arriving(playback)
        | State::Performing { playback, .. }
        | State::Idling(playback) = &mut self.state
        {
            playback.exit();
        }
    }

    pub(crate) fn say_goodbye(&mut self) {
        let farewell = self
            .rng
            .choice(Animation::FAREWELLS)
            .expect("Clippy should know how to say goodbye");
        self.leave(farewell, None);
    }

    pub(crate) fn play(&mut self, animation: Animation) {
        if matches!(self.presence(), Presence::Arriving | Presence::Present) {
            self.queue.push_back(animation);
            self.disturb();
            self.wrap_up_idling();
        }
    }

    pub(crate) fn beckon(&mut self, balloon: Side) {
        let pointing = match balloon {
            Side::Above => Animation::GestureUp,
            Side::Below => Animation::GestureDown,
        };
        let animation = match self.beckoned {
            Some(Animation::GetAttention) => pointing,
            Some(last) if last == pointing => Animation::GetAttention,
            _ if self.rng.bool() => pointing,
            _ => Animation::GetAttention,
        };
        self.beckoned = Some(animation);
        self.play(animation);
    }

    pub(crate) fn celebrate(&mut self) {
        if let State::Performing { playback, .. } = &mut self.state {
            playback.exit();
        }
        self.queue.clear();
        self.play(Animation::Congratulate);
    }

    pub(crate) fn is_busy_with(&self, animation: Animation) -> bool {
        let performing = match &self.state {
            State::Performing { playback, .. } => playback.plays(ANIMATIONS.clip(animation)),
            _ => false,
        };
        performing || self.queue.contains(&animation)
    }

    pub(crate) fn animate(&mut self) {
        let trick = Animation::random_trick(&mut self.rng);
        self.play(trick);
    }

    pub(crate) fn set_hovered(&mut self, hovered: bool, now: Instant) {
        if hovered {
            self.pointer = Pointer::Over { since: now };
            self.rouse();
        } else {
            self.pointer = Pointer::Away;
            if let State::Resting { idle_at, .. } = &mut self.state {
                *idle_at = now + random_pause(&mut self.rng, &self.idle_pause);
            }
        }
    }

    pub(crate) fn set_field(&mut self, field: FieldState) {
        self.field = field;
        if field != FieldState::Focused {
            self.typing_until = None;
        }
    }

    pub(crate) fn set_bedtime(&mut self, bedtime: bool) {
        self.bedtime = bedtime;
    }

    pub(crate) fn scribble(&mut self, now: Instant) {
        if self.field != FieldState::Focused {
            return;
        }
        self.typing_until = Some(now + TYPING_PAUSE);
        if let State::Notepad(notepad) = &mut self.state {
            notepad.notice_typing(now);
        }
    }

    pub(crate) fn rouse(&mut self) {
        self.roused = true;
        self.disturb();
        self.wrap_up_idling();
    }

    fn disturb(&mut self) {
        self.idleness = Idleness::default();
    }

    pub(crate) fn advance(&mut self, now: Instant) {
        if let Pointer::Over { since } = self.pointer
            && now >= since + self.attention_span
        {
            self.pointer = Pointer::Ignored;
        }
        let wanted = self.wanted_look();
        let wish = self.notepad_wish(now);
        let drowsiness = self.drowsiness();
        match &mut self.state {
            State::Attentive(glance) => {
                glance.hold(wanted == Some(glance.look()), now);
                glance.advance(now);
            }
            State::Notepad(notepad) => notepad.advance(wish, now, &mut self.rng, &mut self.sounds),
            State::Asleep(slumber) => {
                slumber.advance(drowsiness, now, &mut self.rng, &mut self.sounds);
            }
            _ => {}
        }
        if self.bedtime {
            self.wrap_up_idling();
        }
        if let State::Performing {
            playback,
            wrap_up_at,
        } = &mut self.state
            && now >= *wrap_up_at
        {
            playback.exit();
        }
        if let Some(playback) = self.state.playback_mut() {
            playback.advance(now, &mut self.rng, &mut self.sounds);
        }
        if let Some(Departure {
            animation,
            by: Some(by),
        }) = self.departure
            && now >= by
        {
            self.departure = None;
            self.state = self.set_off(animation, now);
        }
        self.state = match mem::take(&mut self.state) {
            State::Leaving(playback) if playback.is_finished() => State::Absent,
            State::Arriving(playback)
            | State::Performing { playback, .. }
            | State::Idling(playback)
                if playback.is_finished() =>
            {
                let idle_at = now + random_pause(&mut self.rng, &self.idle_pause);
                self.rest(playback, idle_at, now)
            }
            State::Attentive(glance) if glance.is_over() => self.settle_down(now),
            State::Notepad(notepad) if notepad.is_away() => self.settle_down(now),
            State::Asleep(slumber) if slumber.is_awake() => self.settle_down(now),
            State::Resting { pose, idle_at } => self.rest(pose, idle_at, now),
            state => state,
        };
        self.roused = false;
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        let departure = self.departure.and_then(|departure| departure.by);
        self.state_deadline().into_iter().chain(departure).min()
    }

    fn state_deadline(&self) -> Option<Instant> {
        match &self.state {
            State::Absent => None,
            State::Resting { idle_at, .. } => Some(*idle_at),
            State::Attentive(glance) => {
                let ignoring = match (glance.look(), self.pointer) {
                    (Look::AtThePointer, Pointer::Over { since }) => {
                        Some(since + self.attention_span)
                    }
                    _ => None,
                };
                glance.deadline().into_iter().chain(ignoring).min()
            }
            State::Notepad(notepad) => notepad.deadline(),
            State::Asleep(slumber) => slumber.deadline(),
            State::Performing {
                playback,
                wrap_up_at,
            } => {
                let wrap_up = (!playback.is_exiting()).then_some(*wrap_up_at);
                playback.deadline().into_iter().chain(wrap_up).min()
            }
            State::Arriving(playback) | State::Idling(playback) | State::Leaving(playback) => {
                playback.deadline()
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn is_writing(&self) -> bool {
        matches!(&self.state, State::Notepad(notepad) if notepad.is_writing())
    }

    #[cfg(test)]
    pub(crate) fn is_waiting_with_the_pad(&self) -> bool {
        matches!(&self.state, State::Notepad(notepad) if notepad.is_waiting())
    }

    #[cfg(test)]
    pub(crate) fn look(&self) -> Option<Look> {
        match &self.state {
            State::Attentive(glance) => Some(glance.look()),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn is_asleep(&self) -> bool {
        matches!(self.state, State::Asleep(_))
    }

    #[cfg(test)]
    pub(crate) fn is_performing(&self) -> bool {
        matches!(self.state, State::Performing { .. })
    }

    pub(crate) fn take_sounds(&mut self) -> Vec<Sound> {
        mem::take(&mut self.sounds)
    }

    pub(crate) fn render(&self) -> Option<Canvas> {
        self.frame().map(draw)
    }

    fn frame(&self) -> Option<&'static Frame> {
        match &self.state {
            State::Attentive(glance) => Some(glance.frame()),
            State::Notepad(notepad) => Some(notepad.frame()),
            State::Asleep(slumber) => Some(slumber.frame()),
            state => state.playback().map(Playback::frame),
        }
    }

    fn wanted_look(&self) -> Option<Look> {
        if !self.queue.is_empty() || self.departure.is_some() {
            return None;
        }
        match (self.field, self.pointer) {
            (FieldState::Open(side), _) => Some(Look::AtTheBalloon(side)),
            (FieldState::Closed, Pointer::Over { .. }) => Some(Look::AtThePointer),
            (FieldState::Focused, _) | (FieldState::Closed, Pointer::Away | Pointer::Ignored) => {
                None
            }
        }
    }

    fn notepad_wish(&self, now: Instant) -> Wish {
        if !self.queue.is_empty() || self.departure.is_some() || self.field != FieldState::Focused {
            Wish::PutAway
        } else if self.typing_until.is_some_and(|until| now < until) {
            Wish::Write
        } else {
            Wish::Wait
        }
    }

    fn drowsiness(&self) -> slumber::Wish {
        let needed = !self.queue.is_empty()
            || self.departure.is_some()
            || self.field == FieldState::Focused
            || self.wanted_look().is_some();
        match (needed, self.bedtime) {
            (false, true) => slumber::Wish::Sleep,
            (true, _) if !self.roused => slumber::Wish::Wake(slumber::Manner::AtOnce),
            _ => slumber::Wish::Wake(slumber::Manner::Gently),
        }
    }

    fn settle_down(&mut self, now: Instant) -> State {
        let idle_at = now + random_pause(&mut self.rng, &self.idle_pause);
        let pose = self.start(Animation::RestPose, now);
        self.rest(pose, idle_at, now)
    }

    fn rest(&mut self, pose: Playback<'static>, idle_at: Instant, now: Instant) -> State {
        if let Some(Departure { animation, .. }) = self.departure.take() {
            self.set_off(animation, now)
        } else if let Some(animation) = self.queue.pop_front() {
            State::Performing {
                playback: self.start(animation, now),
                wrap_up_at: now + self.animation_timeout,
            }
        } else if self.field == FieldState::Focused {
            let notepad = Notepad::take_out(now);
            self.sounds.extend(notepad.frame().sound);
            State::Notepad(notepad)
        } else if let Some(look) = self.wanted_look() {
            State::Attentive(Glance::new(look, now))
        } else if self.bedtime {
            let slumber = Slumber::doze_off(now, &mut self.rng);
            self.sounds.extend(slumber.frame().sound);
            State::Asleep(slumber)
        } else if now >= idle_at {
            let idle = self.idleness.next(&mut self.rng);
            State::Idling(self.start(idle, now))
        } else {
            State::Resting { pose, idle_at }
        }
    }

    fn set_off(&mut self, animation: Animation, now: Instant) -> State {
        self.pointer = Pointer::Away;
        State::Leaving(self.start(animation, now).vanishing())
    }

    fn start(&mut self, animation: Animation, now: Instant) -> Playback<'static> {
        let playback = Playback::start(ANIMATIONS.clip(animation), now);
        self.sounds.extend(playback.frame().sound);
        playback
    }

    fn wrap_up_idling(&mut self) {
        if let State::Idling(playback) = &mut self.state {
            playback.exit();
        }
    }
}

impl State {
    fn playback(&self) -> Option<&Playback<'static>> {
        match self {
            State::Absent | State::Attentive(_) | State::Notepad(_) | State::Asleep(_) => None,
            State::Arriving(playback)
            | State::Resting { pose: playback, .. }
            | State::Performing { playback, .. }
            | State::Idling(playback)
            | State::Leaving(playback) => Some(playback),
        }
    }

    fn playback_mut(&mut self) -> Option<&mut Playback<'static>> {
        match self {
            State::Absent | State::Attentive(_) | State::Notepad(_) | State::Asleep(_) => None,
            State::Arriving(playback)
            | State::Resting { pose: playback, .. }
            | State::Performing { playback, .. }
            | State::Idling(playback)
            | State::Leaving(playback) => Some(playback),
        }
    }
}

impl Idleness {
    fn next(&mut self, rng: &mut Rng) -> Animation {
        if self.round.is_empty() {
            if self.rounds == ROUNDS_PER_IDLE_LEVEL {
                self.level = self.level.deeper();
                self.rounds = 0;
            }
            self.round = ANIMATIONS.idles(self.level).to_vec();
            rng.shuffle(&mut self.round);
            self.rounds += 1;
        }
        self.round
            .pop()
            .expect("every idle level should have animations")
    }
}

pub fn portrait() -> Canvas {
    draw(&ANIMATIONS.clip(Animation::RestPose).frames[0])
}

fn draw(frame: &Frame) -> Canvas {
    let mut canvas = Canvas::new(Character::size());
    for placed in frame.images.iter().rev() {
        let image = &CLIPPIT.images[placed.image];
        let size = image.size();
        canvas.draw(
            image,
            Rect::new(0, 0, size.width, size.height),
            placed.x,
            placed.y,
        );
    }
    canvas
}

fn random_pause(rng: &mut Rng, range: &RangeInclusive<Duration>) -> Duration {
    let (shortest, longest) = (*range.start(), *range.end());
    shortest + longest.saturating_sub(shortest).mul_f64(rng.f64())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::animation::Clip;

    fn character(settings: &Settings) -> Character {
        Character::new(settings, Rng::with_seed(42))
    }

    fn settle(
        character: &mut Character,
        mut now: Instant,
        until: impl Fn(&Character) -> bool,
    ) -> Instant {
        character.advance(now);
        while !until(character) {
            now = character
                .deadline()
                .expect("Clippy should have something scheduled");
            character.advance(now);
        }
        now
    }

    fn is_idling(character: &Character) -> bool {
        matches!(character.state, State::Idling(_))
    }

    fn is_resting(character: &Character) -> bool {
        matches!(character.state, State::Resting { .. })
    }

    fn is_present(character: &Character) -> bool {
        character.presence() == Presence::Present
    }

    fn is_wrapping_up(character: &Character) -> bool {
        matches!(
            &character.state,
            State::Performing { playback, .. } if playback.is_exiting()
        )
    }

    fn idle_levels(clippy: &mut Character, mut now: Instant, count: usize) -> Vec<IdleLevel> {
        let mut levels = Vec::new();
        for _ in 0..count {
            now = settle(clippy, now, is_idling);
            let State::Idling(playback) = &clippy.state else {
                unreachable!("settle stops once Clippy idles")
            };
            let level = clippy.idleness.level;
            let played = ANIMATIONS
                .idles(level)
                .iter()
                .any(|&idle| std::ptr::eq(ANIMATIONS.clip(idle), playback.clip()));
            assert!(played, "an idle outside {level:?}");
            levels.push(level);
            now = settle(clippy, now, is_resting);
        }
        levels
    }

    #[test]
    fn idling_deepens_round_by_round_until_disturbed() {
        let settings = Settings {
            idle_pause: Duration::from_secs(1)..=Duration::from_secs(1),
            ..Settings::default()
        };
        let mut clippy = character(&settings);
        clippy.arrive(Animation::Show, Instant::now());
        let now = settle(&mut clippy, Instant::now(), is_present);
        let levels = idle_levels(&mut clippy, now, 30);
        assert!(levels[..9].iter().all(|&level| level == IdleLevel::Short));
        assert!(levels[9..21].iter().all(|&level| level == IdleLevel::Long));
        assert!(levels[21..].iter().all(|&level| level == IdleLevel::Deep));
        clippy.rouse();
        assert_eq!(idle_levels(&mut clippy, now, 1), [IdleLevel::Short]);
    }

    #[test]
    fn greets_then_rests_then_idles() {
        let mut clippy = character(&Settings::default());
        clippy.arrive(Animation::Greeting, Instant::now());
        let now = settle(&mut clippy, Instant::now(), is_present);
        assert!(is_resting(&clippy));
        settle(&mut clippy, now, is_idling);
    }

    #[test]
    fn requested_animation_interrupts_idling() {
        let mut clippy = character(&Settings::default());
        clippy.arrive(Animation::Show, Instant::now());
        let now = settle(&mut clippy, Instant::now(), is_idling);
        clippy.play(Animation::Wave);
        settle(&mut clippy, now, |clippy| {
            matches!(clippy.state, State::Performing { .. })
        });
        assert!(clippy.queue.is_empty());
    }

    #[test]
    fn endless_animations_wrap_up_after_the_timeout() {
        let settings = Settings {
            animation_timeout: Duration::from_secs(2),
            ..Settings::default()
        };
        let mut clippy = character(&settings);
        clippy.arrive(Animation::Show, Instant::now());
        let now = settle(&mut clippy, Instant::now(), is_present);
        clippy.play(Animation::Thinking);
        let wrap_up = settle(&mut clippy, now, is_wrapping_up);
        assert_eq!(wrap_up, now + Duration::from_secs(2));
        settle(&mut clippy, wrap_up, is_resting);
    }

    fn present(settings: &Settings) -> (Character, Instant) {
        let mut clippy = character(settings);
        clippy.arrive(Animation::Show, Instant::now());
        let now = settle(&mut clippy, Instant::now(), is_present);
        (clippy, now)
    }

    fn image(character: &Character) -> Option<usize> {
        character.frame()?.images.first().map(|placed| placed.image)
    }

    fn looks(character: &Character, look: Look) -> bool {
        matches!(&character.state, State::Attentive(glance) if glance.look() == look)
    }

    fn holds(character: &Character, look: Look, held: usize) -> bool {
        looks(character, look) && image(character) == Some(held)
    }

    #[test]
    fn hovering_raises_the_brows_until_the_pointer_leaves() {
        let (mut clippy, entered) = present(&Settings::default());
        clippy.set_hovered(true, entered);
        let now = settle(&mut clippy, entered, |clippy| {
            holds(clippy, Look::AtThePointer, 91)
        });
        assert_eq!(now - entered, Duration::from_millis(300));
        assert_eq!(clippy.deadline(), Some(entered + Duration::from_mins(1)));
        clippy.set_hovered(false, now);
        let lowered = settle(&mut clippy, now, is_resting);
        assert_eq!(lowered - now, Duration::from_millis(300));
        assert_eq!(image(&clippy), Some(0));
    }

    #[test]
    fn hovering_wraps_up_idling_before_raising_the_brows() {
        let (mut clippy, now) = present(&Settings::default());
        let now = settle(&mut clippy, now, is_idling);
        clippy.set_hovered(true, now);
        settle(&mut clippy, now, |clippy| {
            holds(clippy, Look::AtThePointer, 91)
        });
    }

    #[test]
    fn a_pointer_that_stays_too_long_gets_ignored() {
        let settings = Settings {
            attention_span: Duration::from_secs(10),
            ..Settings::default()
        };
        let (mut clippy, now) = present(&settings);
        clippy.set_hovered(true, now);
        let bored = settle(&mut clippy, now, is_resting);
        assert!(bored >= now + Duration::from_secs(10));
        let idling = settle(&mut clippy, bored, is_idling);
        clippy.set_hovered(false, idling);
        clippy.set_hovered(true, idling);
        settle(&mut clippy, idling, |clippy| {
            holds(clippy, Look::AtThePointer, 91)
        });
    }

    #[test]
    fn requested_animations_lower_the_brows_first() {
        let (mut clippy, now) = present(&Settings::default());
        clippy.set_hovered(true, now);
        let now = settle(&mut clippy, now, |clippy| {
            holds(clippy, Look::AtThePointer, 91)
        });
        clippy.play(Animation::Wave);
        let now = settle(&mut clippy, now, |clippy| {
            matches!(clippy.state, State::Performing { .. })
        });
        settle(&mut clippy, now, |clippy| {
            holds(clippy, Look::AtThePointer, 91)
        });
    }

    #[test]
    fn an_open_field_draws_the_eyes_more_than_the_pointer() {
        let (mut clippy, now) = present(&Settings::default());
        clippy.set_hovered(true, now);
        clippy.set_field(FieldState::Open(Side::Above));
        let now = settle(&mut clippy, now, |clippy| {
            holds(clippy, Look::AtTheBalloon(Side::Above), 169)
        });
        assert_eq!(clippy.deadline(), None);
        clippy.set_field(FieldState::Closed);
        settle(&mut clippy, now, |clippy| {
            holds(clippy, Look::AtThePointer, 91)
        });
    }

    #[test]
    fn looking_down_at_the_balloon_looks_back_up_the_same_way() {
        let (mut clippy, now) = present(&Settings::default());
        clippy.set_field(FieldState::Open(Side::Below));
        let mut now = settle(&mut clippy, now, |clippy| {
            holds(clippy, Look::AtTheBalloon(Side::Below), 112)
        });
        clippy.set_field(FieldState::Closed);
        let looking_down: Vec<Option<usize>> = ANIMATIONS
            .clip(Animation::LookDown)
            .frames
            .iter()
            .map(|frame| frame.images.first().map(|placed| placed.image))
            .collect();
        clippy.advance(now);
        while !is_resting(&clippy) {
            assert!(looking_down.contains(&image(&clippy)));
            now = clippy.deadline().unwrap();
            clippy.advance(now);
        }
    }

    #[test]
    fn a_balloon_moving_to_the_other_side_turns_the_eyes_through_the_rest_pose() {
        let (mut clippy, now) = present(&Settings::default());
        clippy.set_field(FieldState::Open(Side::Below));
        let now = settle(&mut clippy, now, |clippy| {
            holds(clippy, Look::AtTheBalloon(Side::Below), 112)
        });
        clippy.set_field(FieldState::Open(Side::Above));
        settle(&mut clippy, now, |clippy| image(clippy) == Some(0));
        settle(&mut clippy, now, |clippy| {
            holds(clippy, Look::AtTheBalloon(Side::Above), 169)
        });
    }

    #[test]
    fn the_notepad_comes_out_while_the_field_has_the_keyboard() {
        let (mut clippy, now) = present(&Settings::default());
        clippy.set_field(FieldState::Focused);
        let now = settle(&mut clippy, now, Character::is_waiting_with_the_pad);
        clippy.scribble(now);
        let wrote = settle(&mut clippy, now, Character::is_writing);
        assert!(wrote - now < Duration::from_secs(1));
        let paused = settle(&mut clippy, wrote, Character::is_waiting_with_the_pad);
        assert!(paused - wrote < TYPING_PAUSE + Duration::from_secs(1));
    }

    #[test]
    fn writing_keeps_up_with_every_keystroke() {
        let (mut clippy, now) = present(&Settings::default());
        clippy.set_field(FieldState::Focused);
        let mut now = settle(&mut clippy, now, Character::is_waiting_with_the_pad);
        clippy.scribble(now);
        clippy.advance(now);
        let started = settle(&mut clippy, now, Character::is_writing);
        assert!(started - now <= Duration::from_millis(200));
        now = started;
        let typing_ends = now + Duration::from_secs(5);
        while now < typing_ends {
            clippy.scribble(now);
            now += Duration::from_millis(250);
            clippy.advance(now);
            assert!(
                clippy.is_writing(),
                "Clippy stopped writing while you typed"
            );
        }
    }

    #[test]
    fn losing_the_keyboard_puts_the_notepad_away_and_looks_up() {
        let (mut clippy, now) = present(&Settings::default());
        clippy.set_field(FieldState::Focused);
        let now = settle(&mut clippy, now, Character::is_waiting_with_the_pad);
        clippy.scribble(now);
        let now = settle(&mut clippy, now, Character::is_writing);
        clippy.set_field(FieldState::Open(Side::Above));
        let looking = settle(&mut clippy, now, |clippy| {
            holds(clippy, Look::AtTheBalloon(Side::Above), 169)
        });
        assert!(looking - now < Duration::from_secs(3));
    }

    #[test]
    fn requested_animations_wait_for_the_notepad_to_go_away() {
        let (mut clippy, now) = present(&Settings::default());
        clippy.set_field(FieldState::Focused);
        let now = settle(&mut clippy, now, Character::is_waiting_with_the_pad);
        clippy.play(Animation::Wave);
        let now = settle(&mut clippy, now, |clippy| {
            matches!(clippy.state, State::Performing { .. })
        });
        settle(&mut clippy, now, Character::is_waiting_with_the_pad);
    }

    #[test]
    fn celebrating_waits_for_the_notepad_to_go_away() {
        let (mut clippy, now) = present(&Settings::default());
        clippy.set_field(FieldState::Focused);
        let now = settle(&mut clippy, now, Character::is_waiting_with_the_pad);
        clippy.set_field(FieldState::Closed);
        clippy.celebrate();
        clippy.advance(now);
        assert!(matches!(clippy.state, State::Notepad(_)));
        settle(&mut clippy, now, |clippy| {
            matches!(&clippy.state, State::Performing { playback, .. }
                if playback.plays(ANIMATIONS.clip(Animation::Congratulate)))
        });
    }

    #[test]
    fn beckoning_points_at_the_balloon_and_never_repeats_itself() {
        let (mut clippy, _) = present(&Settings::default());
        let mut seen = Vec::new();
        for _ in 0..10 {
            clippy.beckon(Side::Above);
            let played = clippy.queue.pop_back().unwrap();
            assert_ne!(seen.last(), Some(&played));
            seen.push(played);
        }
        assert!(
            seen.iter()
                .all(|&played| matches!(played, Animation::GetAttention | Animation::GestureUp))
        );
        clippy.beckon(Side::Below);
        clippy.beckon(Side::Below);
        assert!(clippy.queue.contains(&Animation::GestureDown));
    }

    #[test]
    fn idle_pause_follows_settings() {
        let settings = Settings {
            idle_pause: Duration::from_secs(60)..=Duration::from_secs(60),
            ..Settings::default()
        };
        let mut clippy = character(&settings);
        clippy.arrive(Animation::Show, Instant::now());
        let now = settle(&mut clippy, Instant::now(), is_present);
        assert_eq!(clippy.deadline(), Some(now + Duration::from_secs(60)));
    }

    #[test]
    fn leaving_forgets_requested_animations() {
        let mut clippy = character(&Settings::default());
        let now = Instant::now();
        clippy.arrive(Animation::Show, now);
        clippy.play(Animation::Wave);
        clippy.leave(Animation::Hide, None);
        assert!(clippy.queue.is_empty());
        clippy.play(Animation::Wave);
        assert!(clippy.queue.is_empty());
    }

    #[test]
    fn arriving_while_leaving_turns_clippy_back() {
        let (mut clippy, now) = present(&Settings::default());
        clippy.play(Animation::Wave);
        let now = settle(&mut clippy, now, Character::is_performing);
        clippy.leave(Animation::Hide, None);
        assert_eq!(clippy.presence(), Presence::Leaving);
        clippy.arrive(Animation::Show, now);
        assert_eq!(clippy.presence(), Presence::Present);
        clippy.leave(Animation::Hide, None);
        let now = settle(&mut clippy, now, is_leaving);
        clippy.arrive(Animation::Show, now);
        assert_eq!(clippy.presence(), Presence::Arriving);
        settle(&mut clippy, now, is_present);
    }

    fn is_leaving(character: &Character) -> bool {
        matches!(character.state, State::Leaving(_))
    }

    #[test]
    fn leaving_waits_for_the_current_animation_to_end() {
        let (mut clippy, now) = present(&Settings::default());
        clippy.play(Animation::GetAttention);
        let started = settle(&mut clippy, now, Character::is_performing);
        let length: Duration = ANIMATIONS
            .clip(Animation::GetAttention)
            .frames
            .iter()
            .map(|frame| frame.duration)
            .sum();
        clippy.leave(Animation::Hide, None);
        assert_eq!(clippy.presence(), Presence::Leaving);
        let leaving = settle(&mut clippy, started, is_leaving);
        assert!(leaving >= started + length);
        assert_eq!(image(&clippy), Some(0));
    }

    #[test]
    fn leaving_lowers_the_brows_and_puts_the_notepad_away_first() {
        let (mut clippy, now) = present(&Settings::default());
        clippy.set_hovered(true, now);
        let now = settle(&mut clippy, now, |clippy| {
            holds(clippy, Look::AtThePointer, 91)
        });
        clippy.leave(Animation::Hide, None);
        let leaving = settle(&mut clippy, now, is_leaving);
        assert_eq!(leaving - now, Duration::from_millis(300));

        let (mut clippy, now) = present(&Settings::default());
        clippy.set_field(FieldState::Focused);
        let now = settle(&mut clippy, now, Character::is_waiting_with_the_pad);
        clippy.leave(Animation::Hide, None);
        let leaving = settle(&mut clippy, now, is_leaving);
        assert!(leaving - now >= Duration::from_millis(500));
    }

    #[test]
    fn quitting_overrides_a_pending_hide() {
        let (mut clippy, now) = present(&Settings::default());
        clippy.play(Animation::Wave);
        let now = settle(&mut clippy, now, Character::is_performing);
        clippy.leave(Animation::Hide, None);
        clippy.leave(Animation::GoodBye, None);
        settle(&mut clippy, now, is_leaving);
        let State::Leaving(playback) = &clippy.state else {
            unreachable!("settle stops once Clippy leaves")
        };
        assert!(playback.plays(ANIMATIONS.clip(Animation::GoodBye)));
    }

    #[test]
    fn a_hurried_departure_cuts_a_long_trick_short() {
        let (mut clippy, now) = present(&Settings::default());
        clippy.play(Animation::SendMail);
        let started = settle(&mut clippy, now, Character::is_performing);
        let by = started + Duration::from_secs(1);
        clippy.leave(Animation::Hide, Some(by));
        assert_eq!(clippy.deadline(), Some(by).min(clippy.state_deadline()));
        let leaving = settle(&mut clippy, started, is_leaving);
        assert!(leaving <= by);
    }

    #[test]
    fn showing_while_hiding_turns_back_from_the_same_picture() {
        let hide = ANIMATIONS.clip(Animation::Hide);
        let show = ANIMATIONS.clip(Animation::Show);
        let pictures = |clip: &Clip| -> Vec<Option<usize>> {
            clip.frames
                .iter()
                .map(|frame| frame.images.first().map(|placed| placed.image))
                .collect()
        };
        let mut reversed = pictures(hide);
        reversed.reverse();
        assert_eq!(pictures(show), reversed);

        for steps in 0..hide.frames.len() - 1 {
            let (mut clippy, now) = present(&Settings::default());
            clippy.leave(Animation::Hide, None);
            let mut now = settle(&mut clippy, now, is_leaving);
            for _ in 0..steps {
                now = clippy.deadline().unwrap();
                clippy.advance(now);
            }
            let before = image(&clippy);
            clippy.arrive(Animation::Show, now);
            assert_eq!(clippy.presence(), Presence::Arriving);
            assert_eq!(image(&clippy), before);
            settle(&mut clippy, now, is_present);
        }
    }

    #[test]
    fn turning_back_before_leaving_keeps_watching_the_pointer() {
        let (mut clippy, now) = present(&Settings::default());
        clippy.set_hovered(true, now);
        let now = settle(&mut clippy, now, |clippy| {
            holds(clippy, Look::AtThePointer, 91)
        });
        clippy.leave(Animation::Hide, None);
        clippy.advance(now);
        clippy.arrive(Animation::Show, now);
        settle(&mut clippy, now, |clippy| {
            holds(clippy, Look::AtThePointer, 91)
        });
    }

    #[test]
    fn greeting_and_goodbye_make_sounds_once() {
        let mut clippy = character(&Settings::default());
        clippy.arrive(Animation::Greeting, Instant::now());
        let now = settle(&mut clippy, Instant::now(), is_present);
        assert_ne!(clippy.take_sounds(), Vec::new());
        assert_eq!(clippy.take_sounds(), Vec::new());
        clippy.leave(Animation::GoodBye, None);
        settle(&mut clippy, now, is_absent);
        assert_ne!(clippy.take_sounds(), Vec::new());
    }

    fn is_absent(character: &Character) -> bool {
        character.presence() == Presence::Absent
    }

    #[test]
    fn a_farewell_ends_once_clippy_is_out_of_sight() {
        for animation in [Animation::SendMail, Animation::EmptyTrash] {
            let frames = &ANIMATIONS.clip(animation).frames;
            let vanishes = frames
                .iter()
                .position(|frame| !frame.shows_clippy())
                .unwrap();
            let gone: Duration = frames[..vanishes].iter().map(|frame| frame.duration).sum();
            let back = gone + frames[vanishes].duration;

            let (mut clippy, now) = present(&Settings::default());
            clippy.leave(animation, None);
            let leaving = settle(&mut clippy, now, is_leaving);
            let absent = settle(&mut clippy, leaving, is_absent);
            assert_eq!(absent - leaving, gone, "{animation:?}");

            let (mut clippy, now) = present(&Settings::default());
            clippy.leave(animation, None);
            let leaving = settle(&mut clippy, now, is_leaving);
            clippy.advance(leaving + back);
            assert!(is_absent(&clippy), "{animation:?}");
        }
    }

    const RUBBING_AN_EYE: usize = 605;
    const GETTING_UP: usize = 647;

    fn settle_within(
        character: &mut Character,
        mut now: Instant,
        limit: Duration,
        until: impl Fn(&Character) -> bool,
    ) -> Instant {
        let start = now;
        character.advance(now);
        while !until(character) {
            now = character.deadline().unwrap();
            assert!(now <= start + limit, "Clippy took longer than {limit:?}");
            character.advance(now);
        }
        now
    }

    fn sound_asleep(character: &Character) -> bool {
        matches!(&character.state, State::Asleep(slumber) if slumber.is_sound_asleep())
    }

    fn sleepy() -> (Character, Instant) {
        let settings = Settings {
            idle_pause: Duration::from_hours(1)..=Duration::from_hours(1),
            ..Settings::default()
        };
        let (mut clippy, now) = present(&settings);
        clippy.set_bedtime(true);
        (clippy, now)
    }

    #[test]
    fn bedtime_sends_clippy_to_sleep_at_once_and_keeps_him_there() {
        let (mut clippy, now) = sleepy();
        let asleep = settle_within(&mut clippy, now, Duration::from_secs(5), sound_asleep);
        let mut later = asleep;
        while later < asleep + Duration::from_mins(30) {
            later = clippy.deadline().unwrap();
            clippy.advance(later);
            assert!(clippy.is_asleep());
        }
        clippy.advance(asleep + Duration::from_hours(5));
        assert!(clippy.is_asleep());
    }

    #[test]
    fn a_pointer_wakes_clippy_gently_and_he_dozes_off_once_it_leaves() {
        let (mut clippy, now) = sleepy();
        let mut now = settle(&mut clippy, now, sound_asleep);
        clippy.set_hovered(true, now);
        let mut pictures = Vec::new();
        while clippy.look() != Some(Look::AtThePointer) {
            now = clippy.deadline().unwrap();
            clippy.advance(now);
            pictures.extend(image(&clippy));
        }
        let rubbing = pictures
            .iter()
            .position(|&picture| picture == RUBBING_AN_EYE);
        let getting_up = pictures.iter().position(|&picture| picture == GETTING_UP);
        assert!(rubbing < getting_up && rubbing.is_some(), "{pictures:?}");
        clippy.set_hovered(false, now);
        settle_within(&mut clippy, now, Duration::from_secs(10), sound_asleep);
    }

    #[test]
    fn clippy_wakes_up_once_bedtime_is_over() {
        let (mut clippy, now) = sleepy();
        let now = settle(&mut clippy, now, sound_asleep);
        clippy.set_bedtime(false);
        settle_within(&mut clippy, now, Duration::from_secs(5), is_resting);
    }

    #[test]
    fn a_reminder_wakes_clippy_and_he_goes_back_to_sleep() {
        let (mut clippy, now) = sleepy();
        let now = settle(&mut clippy, now, sound_asleep);
        clippy.beckon(Side::Above);
        let now = settle_within(
            &mut clippy,
            now,
            Duration::from_secs(5),
            Character::is_performing,
        );
        settle_within(&mut clippy, now, Duration::from_secs(10), sound_asleep);
    }

    #[test]
    fn a_reminder_wakes_clippy_at_once() {
        let (mut clippy, now) = sleepy();
        let mut now = settle(&mut clippy, now, sound_asleep);
        let called = now;
        clippy.beckon(Side::Above);
        let mut pictures = Vec::new();
        while !clippy.is_performing() {
            now = clippy.deadline().unwrap();
            clippy.advance(now);
            pictures.extend(image(&clippy));
        }
        assert!(now - called <= Duration::from_secs(1), "{:?}", now - called);
        assert!(pictures.contains(&GETTING_UP), "{pictures:?}");
        assert!(!pictures.contains(&RUBBING_AN_EYE), "{pictures:?}");
    }

    #[test]
    fn bedtime_waits_for_a_glance_and_cuts_an_idle_short() {
        let (mut clippy, now) = sleepy();
        clippy.set_hovered(true, now);
        settle(&mut clippy, now, |clippy| {
            holds(clippy, Look::AtThePointer, 91)
        });
        assert!(!matches!(clippy.state, State::Asleep(_)));

        let (mut clippy, now) = present(&Settings::default());
        let now = settle(&mut clippy, now, is_idling);
        clippy.set_bedtime(true);
        settle_within(&mut clippy, now, Duration::from_secs(15), sound_asleep);
    }

    #[test]
    fn goodbyes_vary_between_every_farewell() {
        let mut said = HashSet::new();
        for seed in 0..30 {
            let mut clippy = Character::new(&Settings::default(), Rng::with_seed(seed));
            clippy.arrive(Animation::Show, Instant::now());
            let now = settle(&mut clippy, Instant::now(), is_present);
            clippy.say_goodbye();
            settle(&mut clippy, now, is_leaving);
            let State::Leaving(playback) = &clippy.state else {
                unreachable!("settle stops once Clippy leaves")
            };
            said.extend(
                Animation::FAREWELLS
                    .into_iter()
                    .filter(|&farewell| playback.plays(ANIMATIONS.clip(farewell))),
            );
        }
        assert_eq!(said, HashSet::from(Animation::FAREWELLS));
    }
}
