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

use clippy::hours::{TimeOfDay, midnight_after};
use clippy::{Context, MenuItem, Notice, Plugin};
use jiff::ToSpan;
use jiff::civil::{Date, DateTime};
use serde::{Deserialize, Serialize};

pub use settings::Settings;

use agenda::Agenda;
use progress::Progress;
use schedule::Comeback;
use settings::Reminder;

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
        key: String,
        occurrence: DateTime,
        until: DateTime,
    },
    Done {
        key: String,
        occurrence: DateTime,
    },
    Task {
        key: String,
        occurrence: DateTime,
        task: String,
        completes: bool,
    },
    Pause {
        until: DateTime,
    },
    Resume,
}

struct Habit<'a> {
    key: String,
    text: &'a str,
    tasks: &'a [String],
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
        let progress = self.progress_of(habit);
        let snoozes = progress.snoozes_of(occurrence);
        let label = match self.next_showing(settings, habit, now) {
            Some(comeback) => format!(
                "Don't show me this reminder until {}",
                until(now.date(), comeback)
            ),
            None => "Don't show me this reminder again".to_owned(),
        };
        let key = habit.key.clone();
        let mut notice = Notice::new(
            habit.key.as_str(),
            habit.text,
            Action::Snooze {
                key: key.clone(),
                occurrence,
                until: now.saturating_add(settings.snooze.after(snoozes)),
            },
        );
        let ticked = |task: &str| progress.has_ticked(occurrence, task);
        for task in habit.tasks {
            let done = ticked(task);
            let completes = !done
                && habit
                    .tasks
                    .iter()
                    .all(|other| other == task || ticked(other));
            let toggle = Action::Task {
                key: key.clone(),
                occurrence,
                task: task.clone(),
                completes,
            };
            notice = notice.task(task.as_str(), done, toggle);
        }
        notice
            .checkbox(label, Action::Done { key, occurrence })
            .over_full_screen(settings.show_in_full_screen)
            .shortcut(settings.hide_shortcut)
            .check_shortcut(settings.check_shortcut)
    }

    fn next_showing(
        &self,
        settings: &Settings,
        habit: &Habit<'_>,
        now: DateTime,
    ) -> Option<Comeback> {
        let today = now.date();
        let later_today = habit
            .next(now, &self.agenda)
            .and_then(|at| settings.after_quiet_hours(at))
            .filter(|at| at.date() == today);
        if let Some(at) = later_today {
            return Some(Comeback::At(at));
        }
        today
            .series(1.day())
            .skip(1)
            .take(7)
            .find_map(|day| habit.first_on(settings, day, today, &self.agenda))
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
        self.progress.get(&habit.key).cloned().unwrap_or_default()
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
        let pause = |until| Action::Pause { until };
        MenuItem::pausing(self.paused_until, now, pause, Action::Resume)
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
                key,
                occurrence,
                until,
            } => {
                self.progress
                    .entry(key)
                    .or_default()
                    .snooze(occurrence, until);
            }
            Action::Done { key, occurrence } => {
                self.progress.entry(key).or_default().finish(occurrence);
            }
            Action::Task {
                key,
                occurrence,
                task,
                completes,
            } => {
                let progress = self.progress.entry(key).or_default();
                if completes {
                    progress.finish(occurrence);
                } else {
                    progress.toggle(occurrence, task);
                }
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

    fn first_on(
        &self,
        settings: &Settings,
        day: Date,
        today: Date,
        agenda: &Agenda,
    ) -> Option<Comeback> {
        let comebacks: Vec<Comeback> = self
            .reminders
            .iter()
            .filter_map(|reminder| match reminder.first_on(day, today, agenda)? {
                Comeback::At(at) => settings
                    .after_quiet_hours(at)
                    .filter(|at| at.date() == day)
                    .map(Comeback::At),
                vague => Some(vague),
            })
            .collect();
        if comebacks
            .iter()
            .all(|comeback| matches!(comeback, Comeback::At(_)))
        {
            comebacks.into_iter().min()
        } else {
            comebacks.into_iter().max()
        }
    }
}

fn habits(settings: &Settings) -> Vec<Habit<'_>> {
    let mut habits: Vec<Habit<'_>> = Vec::new();
    for reminder in &settings.reminders {
        let key = reminder.key();
        match habits.iter_mut().find(|habit| habit.key == key) {
            Some(habit) => habit.reminders.push(reminder),
            None => habits.push(Habit {
                key,
                text: &reminder.text,
                tasks: &reminder.tasks,
                reminders: vec![reminder],
            }),
        }
    }
    habits
}

fn until(today: Date, comeback: Comeback) -> String {
    let (day, time) = match comeback {
        Comeback::At(at) => (at.date(), Some(TimeOfDay::from(at.time()))),
        Comeback::On(day) => (day, None),
        Comeback::NextWeek => return "next week".to_owned(),
    };
    let day = match today.until(day).map_or(0, |span| span.get_days()) {
        0 => String::new(),
        1 => "tomorrow".to_owned(),
        2..=6 => day.strftime("%A").to_string(),
        _ => day.strftime("next %A").to_string(),
    };
    match time {
        Some(time) if day.is_empty() => time.to_string(),
        Some(time) => format!("{day} at {time}"),
        None => day,
    }
}

#[cfg(test)]
mod tests;
