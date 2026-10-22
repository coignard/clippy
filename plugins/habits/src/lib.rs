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

#![forbid(unsafe_code)]

use clippy::shortcut::{Key, Modifiers, Shortcut};
use clippy::{Checklist, Context, MenuItem, Plugin};
use jiff::civil::{Date, DateTime, Time};
use serde::{Deserialize, Serialize};

const COMMAND_OPTION_SHIFT: Modifiers = Modifiers::COMMAND
    .union(Modifiers::OPTION)
    .union(Modifiers::SHIFT);

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Habits {
    #[serde(skip_serializing_if = "Option::is_none")]
    day: Option<Date>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    done: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    show_shortcut: Shortcut,
    habits: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Show,
    Tick { habit: String, completes: bool },
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show_shortcut: Shortcut {
                modifiers: COMMAND_OPTION_SHIFT,
                key: Key::H,
            },
            habits: Vec::new(),
        }
    }
}

impl Plugin for Habits {
    const NAME: &'static str = "Habits";

    type Action = Action;

    type Settings = Settings;

    fn begin_day(&mut self, _settings: &Settings, now: DateTime) {
        self.begin(now.date());
    }

    fn menu(&self, settings: &Settings, _now: DateTime) -> Vec<MenuItem<Action>> {
        vec![
            MenuItem::new("Show Habits", Action::Show)
                .shortcut(settings.show_shortcut)
                .enabled(!settings.habits.is_empty()),
        ]
    }

    fn checklist(&self, settings: &Settings, now: DateTime) -> Option<Checklist<Action>> {
        let done = self.done_on(now.date());
        let left = settings
            .habits
            .iter()
            .filter(|habit| !done.contains(habit))
            .count();
        let checklist =
            settings
                .habits
                .iter()
                .fold(Checklist::new("Today's habits:"), |checklist, habit| {
                    let checked = done.contains(habit);
                    let tick = Action::Tick {
                        habit: habit.clone(),
                        completes: !checked && left == 1,
                    };
                    checklist.item(habit, checked, tick)
                });
        Some(checklist)
    }

    fn changes_at(&self, _settings: &Settings, now: DateTime) -> Option<DateTime> {
        let today = now.date();
        if self.done_on(today).is_empty() {
            return None;
        }
        Some(today.tomorrow().ok()?.to_datetime(Time::midnight()))
    }

    fn perform(&mut self, action: Action, cx: &mut Context<'_, Self>) {
        match action {
            Action::Show => cx.toggle_checklist(),
            Action::Tick { habit, completes } => {
                self.begin(cx.now().date());
                if let Some(index) = self.done.iter().position(|done| *done == habit) {
                    self.done.remove(index);
                } else {
                    self.done.push(habit);
                    if completes {
                        cx.celebrate();
                    }
                }
            }
        }
    }
}

impl Habits {
    fn begin(&mut self, today: Date) {
        if self.day != Some(today) {
            self.day = Some(today);
            self.done.clear();
        }
    }

    fn done_on(&self, date: Date) -> &[String] {
        if self.day == Some(date) {
            &self.done
        } else {
            &[]
        }
    }
}

#[cfg(test)]
mod tests {
    use clippy::testing::Harness;

    use super::*;

    const HABITS: [&str; 3] = ["Stretch", "Read", "Walk"];

    fn habits() -> Harness {
        let settings = Settings {
            habits: HABITS.map(str::to_owned).to_vec(),
            ..Settings::default()
        };
        Harness::with_settings(Habits::default(), settings)
    }

    fn ticked(harness: &mut Harness) -> Vec<bool> {
        let (_, items) = harness.checklist().expect("the habits should be shown");
        items.into_iter().map(|(_, checked)| checked).collect()
    }

    #[test]
    fn the_shortcut_shows_and_hides_the_habits() {
        let mut habits = habits();
        assert_eq!(habits.checklist(), None);
        habits.press(Settings::default().show_shortcut);
        let (title, items) = habits.checklist().unwrap();
        assert_eq!(title, "Today's habits:");
        assert_eq!(
            items,
            HABITS.map(|habit| (habit.to_owned(), false)).to_vec()
        );
        habits.press(Settings::default().show_shortcut);
        assert_eq!(habits.checklist(), None);
    }

    #[test]
    fn ticking_a_habit_again_unticks_it() {
        let mut habits = habits();
        habits.choose("Show Habits");
        habits.tick("Read");
        assert_eq!(ticked(&mut habits), [false, true, false]);
        habits.tick("Read");
        assert_eq!(ticked(&mut habits), [false, false, false]);
    }

    #[test]
    fn ticking_the_last_habit_off_is_celebrated() {
        let mut habits = habits();
        habits.choose("Show Habits");
        habits.tick("Stretch");
        habits.tick("Walk");
        assert!(!habits.celebrates());
        habits.tick("Read");
        assert!(habits.celebrates());
        assert_eq!(ticked(&mut habits), [true, true, true]);
    }

    #[test]
    fn unticking_is_not_celebrated() {
        let mut habits = habits();
        habits.choose("Show Habits");
        habits.tick("Stretch");
        habits.tick("Walk");
        habits.tick("Walk");
        assert!(!habits.celebrates());
    }

    #[test]
    fn every_day_starts_unticked() {
        let mut habits = habits();
        habits.set_time(habits.now());
        habits.choose("Show Habits");
        habits.tick("Stretch");
        habits.tick("Walk");
        let tomorrow = habits.now().date().tomorrow().unwrap().at(9, 0, 0, 0);
        habits.set_time(tomorrow);
        assert_eq!(ticked(&mut habits), [false, false, false]);
    }

    #[test]
    fn ticks_vanish_at_midnight_even_while_shown() {
        let mut habits = habits();
        let today = habits.now().date();
        habits.set_time(today.at(23, 59, 0, 0));
        habits.choose("Show Habits");
        assert_eq!(habits.next_change(), None);
        habits.tick("Read");
        let midnight = today.tomorrow().unwrap().at(0, 0, 0, 0);
        assert_eq!(habits.next_change(), Some(midnight));
        habits.set_time(midnight);
        assert_eq!(ticked(&mut habits), [false, false, false]);
        assert_eq!(habits.next_change(), None);
    }

    #[test]
    fn nothing_to_show_without_habits() {
        let mut habits = Harness::new(Habits::default());
        assert_eq!(habits.menu(), [("Show Habits".to_owned(), false)]);
        habits.press(Settings::default().show_shortcut);
        assert_eq!(habits.checklist(), None);
    }

    #[test]
    fn settings_list_the_habits_and_the_shortcut() {
        assert_eq!(
            toml::to_string(&Settings::default()).unwrap(),
            "show_shortcut = \"Cmd+Option+Shift+H\"\nhabits = []\n"
        );
        let settings: Settings = toml::from_str("habits = [\"Stretch\", \"Read\"]\n").unwrap();
        assert_eq!(settings.habits, ["Stretch", "Read"]);
        assert_eq!(settings.show_shortcut, Settings::default().show_shortcut);
    }
}
