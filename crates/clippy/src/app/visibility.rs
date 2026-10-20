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

use crate::animation::Animation;
use crate::settings::AfterSummons;

#[derive(Default)]
pub(super) struct Visibility {
    debut: Debut,
    hidden: bool,
    full_screen: FullScreen,
    summons: Option<Summons>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Summons {
    pub(super) owner: usize,
    pub(super) lasts: Lasts,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Lasts {
    WhileAsking,
    WhileShowing,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Need {
    #[default]
    None,
    Polite,
    Urgent,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Debut {
    #[default]
    Waiting,
    Due,
    Done,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum FullScreen {
    #[default]
    Off,
    SteppingAside,
    ShownAnyway,
}

impl Visibility {
    pub(super) fn launch(&mut self) {
        if self.debut == Debut::Waiting {
            self.debut = Debut::Due;
        }
    }

    pub(super) fn hide(&mut self) {
        self.hidden = true;
        self.summons = None;
        if self.full_screen == FullScreen::ShownAnyway {
            self.full_screen = FullScreen::SteppingAside;
        }
    }

    pub(super) fn show(&mut self) {
        self.hidden = false;
        self.summons = None;
        if self.full_screen != FullScreen::Off {
            self.full_screen = FullScreen::ShownAnyway;
        }
    }

    pub(super) fn summon(&mut self, owner: usize, after: AfterSummons) {
        if after == AfterSummons::Stay {
            self.hidden = false;
        }
        if self.is_welcome() {
            return;
        }
        let lasts = if self.is_stepping_aside() {
            Lasts::WhileAsking
        } else {
            Lasts::WhileShowing
        };
        self.summons = Some(Summons { owner, lasts });
    }

    pub(super) fn keep(&mut self, after: AfterSummons) {
        if after == AfterSummons::Stay {
            self.hidden = false;
        }
    }

    pub(super) fn summons(&self) -> Option<Summons> {
        self.summons
    }

    pub(super) fn end_summons(&mut self) {
        self.summons = None;
    }

    pub(super) fn set_full_screen(&mut self, full_screen: bool) {
        match (full_screen, self.full_screen) {
            (true, FullScreen::Off) => {
                self.full_screen = FullScreen::SteppingAside;
                self.summons = None;
            }
            (false, FullScreen::SteppingAside | FullScreen::ShownAnyway) => {
                self.full_screen = FullScreen::Off;
            }
            _ => {}
        }
    }

    pub(super) fn is_stepping_aside(&self) -> bool {
        self.full_screen == FullScreen::SteppingAside
    }

    pub(super) fn wants_clippy(&self, need: Need) -> bool {
        let needed = match need {
            Need::None => false,
            Need::Polite => !self.is_stepping_aside(),
            Need::Urgent => true,
        };
        self.debut != Debut::Waiting && (self.is_welcome() || self.summons.is_some() || needed)
    }

    pub(super) fn arrival(&mut self) -> Animation {
        if self.debut == Debut::Due {
            self.debut = Debut::Done;
            Animation::Greeting
        } else {
            Animation::Show
        }
    }

    fn is_welcome(&self) -> bool {
        !self.hidden && !self.is_stepping_aside()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launched() -> Visibility {
        let mut visibility = Visibility::default();
        visibility.launch();
        visibility
    }

    fn hidden() -> Visibility {
        let mut visibility = launched();
        visibility.hide();
        visibility
    }

    #[test]
    fn waits_for_launch_and_greets_once() {
        let mut visibility = Visibility::default();
        assert!(!visibility.wants_clippy(Need::None));
        visibility.launch();
        assert!(visibility.wants_clippy(Need::None));
        assert_eq!(visibility.arrival(), Animation::Greeting);
        assert_eq!(visibility.arrival(), Animation::Show);
    }

    #[test]
    fn full_screen_hides_clippy_unless_the_user_shows_him() {
        let mut visibility = launched();
        visibility.set_full_screen(true);
        assert!(!visibility.wants_clippy(Need::None));
        visibility.show();
        assert!(visibility.wants_clippy(Need::None));
        visibility.set_full_screen(true);
        assert!(visibility.wants_clippy(Need::None));
        visibility.set_full_screen(false);
        visibility.set_full_screen(true);
        assert!(!visibility.wants_clippy(Need::None));
        visibility.set_full_screen(false);
        assert!(visibility.wants_clippy(Need::None));
    }

    #[test]
    fn hiding_outlasts_full_screen() {
        let mut visibility = hidden();
        visibility.set_full_screen(true);
        visibility.set_full_screen(false);
        assert!(!visibility.wants_clippy(Need::None));
    }

    #[test]
    fn a_summons_brings_clippy_back_for_good_by_default() {
        let mut visibility = hidden();
        visibility.summon(0, AfterSummons::Stay);
        assert_eq!(visibility.summons(), None);
        assert!(visibility.wants_clippy(Need::None));
    }

    #[test]
    fn a_summons_can_last_only_while_the_plugin_shows_something() {
        let mut visibility = hidden();
        visibility.summon(1, AfterSummons::Hide);
        assert_eq!(
            visibility.summons(),
            Some(Summons {
                owner: 1,
                lasts: Lasts::WhileShowing
            })
        );
        assert!(visibility.wants_clippy(Need::None));
        visibility.end_summons();
        assert!(!visibility.wants_clippy(Need::None));
    }

    #[test]
    fn full_screen_summons_last_only_while_asking() {
        for after in [AfterSummons::Stay, AfterSummons::Hide] {
            let mut visibility = launched();
            visibility.set_full_screen(true);
            visibility.summon(2, after);
            assert_eq!(
                visibility.summons(),
                Some(Summons {
                    owner: 2,
                    lasts: Lasts::WhileAsking
                })
            );
            visibility.end_summons();
            assert!(!visibility.wants_clippy(Need::None));
        }
    }

    #[test]
    fn staying_after_a_summons_in_full_screen_shows_clippy_once_it_ends() {
        let mut visibility = hidden();
        visibility.set_full_screen(true);
        visibility.summon(0, AfterSummons::Stay);
        visibility.end_summons();
        assert!(!visibility.wants_clippy(Need::None));
        visibility.set_full_screen(false);
        assert!(visibility.wants_clippy(Need::None));
    }

    #[test]
    fn summoning_a_welcome_clippy_changes_nothing() {
        let mut visibility = launched();
        visibility.summon(0, AfterSummons::Hide);
        assert_eq!(visibility.summons(), None);
    }

    #[test]
    fn polite_needs_wait_for_full_screen_and_urgent_ones_do_not() {
        let mut visibility = hidden();
        assert!(visibility.wants_clippy(Need::Polite));
        visibility.set_full_screen(true);
        assert!(!visibility.wants_clippy(Need::Polite));
        assert!(visibility.wants_clippy(Need::Urgent));
        visibility.show();
        assert!(visibility.wants_clippy(Need::Polite));
    }

    #[test]
    fn keeping_clippy_after_a_notice_follows_the_settings() {
        let mut visibility = hidden();
        visibility.keep(AfterSummons::Hide);
        assert!(!visibility.wants_clippy(Need::None));
        visibility.keep(AfterSummons::Stay);
        assert!(visibility.wants_clippy(Need::None));
    }

    #[test]
    fn entering_full_screen_ends_a_summons() {
        let mut visibility = hidden();
        visibility.summon(0, AfterSummons::Hide);
        visibility.set_full_screen(true);
        assert_eq!(visibility.summons(), None);
        assert!(!visibility.wants_clippy(Need::None));
    }
}
