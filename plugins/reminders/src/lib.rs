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

mod agenda;
mod days;
mod interval;
mod progress;
mod schedule;
mod settings;

use std::collections::BTreeMap;

use clippy::hours::TimeOfDay;
use clippy::{Context, MenuItem, Notice, Plugin};
use jiff::SignedDuration;
use jiff::civil::{DateTime, Time};
use serde::{Deserialize, Serialize};

pub use settings::Settings;

use agenda::Agenda;
use progress::Progress;
use settings::Reminder;

const PAUSE: SignedDuration = SignedDuration::from_hours(1);

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Reminders {
    #[serde(flatten)]
    agenda: Agenda,
    #[serde(skip_serializing_if = "Option::is_none")]
    paused_until: Option<DateTime>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    progress: BTreeMap<String, Progress>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Snooze {
        text: String,
        occurrence: DateTime,
        until: DateTime,
    },
    Done {
        text: String,
        occurrence: DateTime,
    },
    Pause {
        until: DateTime,
    },
    Resume,
}

struct Habit<'a> {
    text: &'a str,
    reminders: Vec<&'a Reminder>,
}

impl Reminders {
    fn pending(&self, habit: &Habit<'_>, now: DateTime) -> Option<DateTime> {
        let occurrence = habit.latest(now, &self.agenda)?;
        let progress = self.progress_of(habit);
        let done = progress.done.is_some_and(|done| done >= occurrence);
        let snoozed = progress
            .snooze_on(now.date())
            .is_some_and(|snooze| now < snooze.until);
        (!done && !snoozed).then_some(occurrence)
    }

    fn notice(
        &self,
        settings: &Settings,
        habit: &Habit<'_>,
        occurrence: DateTime,
        now: DateTime,
    ) -> Notice<Action> {
        let snoozes = self.progress_of(habit).snoozes_of(occurrence);
        let label = match self.next_showing(settings, habit, now) {
            Some(at) => format!(
                "Don't show me this reminder until {}",
                TimeOfDay::from(at.time())
            ),
            None => "Don't show me this reminder again".to_owned(),
        };
        let text = habit.text.to_owned();
        Notice::new(
            habit.text,
            habit.text,
            Action::Snooze {
                text: text.clone(),
                occurrence,
                until: now.saturating_add(settings.snooze.after(snoozes)),
            },
        )
        .checkbox(label, Action::Done { text, occurrence })
        .over_full_screen(settings.show_in_full_screen)
        .shortcut(settings.hide_shortcut)
        .check_shortcut(settings.check_shortcut)
    }

    fn next_showing(
        &self,
        settings: &Settings,
        habit: &Habit<'_>,
        now: DateTime,
    ) -> Option<DateTime> {
        let next = settings.after_quiet_hours(habit.next(now, &self.agenda)?)?;
        (next.date() == now.date()).then_some(next)
    }

    fn tidy(&mut self, now: DateTime) {
        self.progress
            .retain(|_, progress| progress.is_from(now.date()));
        self.paused_until = self.paused_until.filter(|&until| now < until);
    }

    fn is_resting(&self, settings: &Settings, now: DateTime) -> bool {
        self.paused_until.is_some_and(|until| now < until) || settings.is_quiet(now.time())
    }

    fn progress_of(&self, habit: &Habit<'_>) -> Progress {
        self.progress.get(habit.text).copied().unwrap_or_default()
    }
}

impl Plugin for Reminders {
    const NAME: &'static str = "Reminders";

    type Action = Action;

    type Settings = Settings;

    fn begin_day(&mut self, settings: &Settings, now: DateTime) {
        self.agenda.begin(settings, now);
        self.tidy(now);
    }

    fn menu(&self, settings: &Settings, now: DateTime) -> Vec<MenuItem<Action>> {
        if settings.reminders.is_empty() {
            return Vec::new();
        }
        match self.paused_until.filter(|&until| now < until) {
            Some(until) => {
                let when = if until == midnight_after(now) {
                    "Tomorrow".to_owned()
                } else {
                    TimeOfDay::from(until.time()).to_string()
                };
                vec![
                    MenuItem::label(format!("Paused Until {when}")),
                    MenuItem::new("Resume Reminders", Action::Resume),
                ]
            }
            None => vec![
                MenuItem::new(
                    "Pause for an Hour",
                    Action::Pause {
                        until: now.saturating_add(PAUSE),
                    },
                ),
                MenuItem::new(
                    "Pause Until Tomorrow",
                    Action::Pause {
                        until: midnight_after(now),
                    },
                ),
            ],
        }
    }

    fn notices(&self, settings: &Settings, now: DateTime) -> Vec<Notice<Action>> {
        if self.is_resting(settings, now) {
            return Vec::new();
        }
        let mut pending: Vec<(DateTime, Notice<Action>)> = habits(settings)
            .iter()
            .filter_map(|habit| {
                let occurrence = self.pending(habit, now)?;
                Some((occurrence, self.notice(settings, habit, occurrence, now)))
            })
            .collect();
        pending.sort_by_key(|&(occurrence, _)| occurrence);
        pending.into_iter().map(|(_, notice)| notice).collect()
    }

    fn changes_at(&self, settings: &Settings, now: DateTime) -> Option<DateTime> {
        let today = now.date();
        let habits = habits(settings);
        let occurrences = habits
            .iter()
            .filter_map(|habit| habit.next(now, &self.agenda));
        let snoozes = self
            .progress
            .values()
            .filter_map(|progress| progress.snooze_on(today))
            .map(|snooze| snooze.until);
        let quiet = settings
            .quiet_hours
            .iter()
            .flat_map(|hours| [hours.start_after(now), hours.end_after(now)]);
        occurrences
            .chain(snoozes)
            .chain(quiet)
            .chain(self.paused_until)
            .chain([midnight_after(now)])
            .filter(|&at| at > now)
            .min()
    }

    fn perform(&mut self, action: Action, cx: &mut Context<'_, Self>) {
        self.tidy(cx.now());
        match action {
            Action::Snooze {
                text,
                occurrence,
                until,
            } => {
                self.progress
                    .entry(text)
                    .or_default()
                    .snooze(occurrence, until);
            }
            Action::Done { text, occurrence } => {
                self.progress.entry(text).or_default().finish(occurrence);
            }
            Action::Pause { until } => self.paused_until = Some(until),
            Action::Resume => self.paused_until = None,
        }
    }
}

impl Habit<'_> {
    fn latest(&self, now: DateTime, agenda: &Agenda) -> Option<DateTime> {
        self.reminders
            .iter()
            .filter_map(|reminder| reminder.latest(now, agenda))
            .max()
    }

    fn next(&self, now: DateTime, agenda: &Agenda) -> Option<DateTime> {
        self.reminders
            .iter()
            .filter_map(|reminder| reminder.next(now, agenda))
            .min()
    }
}

fn habits(settings: &Settings) -> Vec<Habit<'_>> {
    let mut habits: Vec<Habit<'_>> = Vec::new();
    for reminder in &settings.reminders {
        match habits.iter_mut().find(|habit| habit.text == reminder.text) {
            Some(habit) => habit.reminders.push(reminder),
            None => habits.push(Habit {
                text: &reminder.text,
                reminders: vec![reminder],
            }),
        }
    }
    habits
}

fn midnight_after(now: DateTime) -> DateTime {
    TimeOfDay::from(Time::midnight()).next_after(now)
}

#[cfg(test)]
mod tests;
