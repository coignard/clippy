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

mod settings;

use std::collections::BTreeMap;
use std::ops::Not;

use clippy::calendar::{Agenda, Appointment};
use clippy::graphics::printable;
use clippy::hours::TimeOfDay;
use clippy::{Context, MenuItem, Message, Notice, Plugin, Urgency};
use jiff::SignedDuration;
use jiff::civil::{Date, DateTime, Time};
use serde::{Deserialize, Serialize};

pub use settings::Settings;

use settings::RemindWith;

const PAUSE: SignedDuration = SignedDuration::from_hours(1);
const INSTANT_EVENT: SignedDuration = SignedDuration::from_mins(5);
const SECOND: SignedDuration = SignedDuration::from_secs(1);
const MINUTE: SignedDuration = SignedDuration::from_mins(1);
const DAY: SignedDuration = SignedDuration::from_hours(24);
const SUMMARY: &str = "summary";
const SUMMARY_LENGTH: usize = 8;
const CURRENT_LENGTH: usize = 3;
const AGAIN: &str = "Don't remind me again";

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Calendar {
    #[serde(skip)]
    agenda: Agenda,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    events: BTreeMap<String, Seen>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary_asked: Option<Date>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary_seen: Option<Date>,
    #[serde(skip_serializing_if = "Option::is_none")]
    paused_until: Option<DateTime>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct Seen {
    until: DateTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dismissed: Option<DateTime>,
    #[serde(default, skip_serializing_if = "Not::not")]
    muted: bool,
    #[serde(default, skip_serializing_if = "Not::not")]
    hidden: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Dismiss {
        id: String,
        alert: DateTime,
        until: DateTime,
        ongoing: bool,
    },
    Mute {
        id: String,
        until: DateTime,
    },
    ShowSummary,
    DismissSummary,
    HideCurrent {
        events: Vec<(String, DateTime)>,
    },
    ShowCurrent,
    Pause {
        until: DateTime,
    },
    Resume,
}

impl Calendar {
    fn appointments<'a>(&'a self, settings: &'a Settings) -> impl Iterator<Item = &'a Appointment> {
        let appointments = match &self.agenda {
            Agenda::Read(appointments) => appointments.as_slice(),
            Agenda::Unread | Agenda::Denied => &[],
        };
        appointments
            .iter()
            .filter(|appointment| settings.follows(&appointment.calendar))
    }

    fn today<'a>(&'a self, settings: &'a Settings, now: DateTime) -> Vec<&'a Appointment> {
        let tomorrow = midnight_after(now);
        let mut today: Vec<&Appointment> = self
            .appointments(settings)
            .filter(|appointment| appointment.start < tomorrow && now < appointment.end)
            .collect();
        today.sort_by_key(|appointment| {
            (
                !lasts_all_day(appointment, now),
                appointment.start,
                &appointment.title,
            )
        });
        today
    }

    fn due_alert(
        &self,
        settings: &Settings,
        appointment: &Appointment,
        now: DateTime,
    ) -> Option<DateTime> {
        let started = appointment.start <= now;
        let started_earlier = started && appointment.start.date() != now.date();
        if appointment.all_day
            || started_earlier
            || now >= expiry(appointment)
            || (started && self.seen(appointment, |seen| seen.hidden))
        {
            return None;
        }
        let alert = settings
            .alerts(appointment.start)
            .filter(|&alert| alert <= now)
            .max()?;
        let dismissed = self.seen(appointment, |seen| {
            seen.dismissed.is_some_and(|dismissed| dismissed >= alert)
        });
        (!dismissed && !self.seen(appointment, |seen| seen.muted)).then_some(alert)
    }

    fn seen(&self, appointment: &Appointment, about: impl Fn(&Seen) -> bool) -> bool {
        self.events.get(&appointment.id).is_some_and(about)
    }

    fn current<'a>(&'a self, settings: &'a Settings, now: DateTime) -> Vec<&'a Appointment> {
        let mut current: Vec<&Appointment> = self
            .appointments(settings)
            .filter(|appointment| {
                appointment.start <= now
                    && now < appointment.end
                    && !lasts_all_day(appointment, now)
            })
            .collect();
        current.sort_by_key(|appointment| (appointment.start, &appointment.title));
        current
    }

    fn reminder(
        settings: &Settings,
        appointment: &Appointment,
        alert: DateTime,
        now: DateTime,
    ) -> Notice<Action> {
        let id = appointment.id.clone();
        let until = expiry(appointment);
        let text = match settings.remind_with {
            RemindWith::Countdown => {
                format!(
                    "{}\n{}",
                    countdown(appointment.start, now),
                    title(appointment)
                )
            }
            RemindWith::StartTime => {
                format!("{}\n{}", title(appointment), starting(appointment, now))
            }
        };
        let notice = Notice::new(
            &appointment.id,
            text,
            Action::Dismiss {
                id: id.clone(),
                alert,
                until,
                ongoing: appointment.start <= now,
            },
        )
        .over_full_screen(settings.show_in_full_screen)
        .shortcut(settings.hide_shortcut)
        .celebrates(false)
        .urgency(Urgency::Pressing);
        let repeats = now < appointment.start
            && settings
                .alerts(appointment.start)
                .any(|later| alert < later);
        if repeats {
            notice.checkbox(AGAIN, Action::Mute { id, until })
        } else {
            notice
        }
    }

    fn summary(&self, settings: &Settings, now: DateTime) -> Option<Notice<Action>> {
        let today = now.date();
        let scheduled = settings.daily_summary && now.time() >= settings.daily_summary_at.time();
        let asked = self.summary_asked == Some(today);
        if !(scheduled || asked) || self.summary_seen == Some(today) {
            return None;
        }
        let appointments = self.today(settings, now);
        if appointments.is_empty() {
            return None;
        }
        let mut lines = vec!["Today on your calendar:".to_owned()];
        lines.extend(
            appointments
                .iter()
                .take(SUMMARY_LENGTH)
                .map(|appointment| summary_line(appointment, now)),
        );
        if let Some(more) = appointments.len().checked_sub(SUMMARY_LENGTH)
            && more > 0
        {
            lines.push(format!("...and {more} more"));
        }
        Some(
            Notice::new(SUMMARY, lines.join("\n"), Action::DismissSummary)
                .shortcut(settings.hide_shortcut)
                .urgency(if asked {
                    Urgency::Asked
                } else {
                    Urgency::Normal
                }),
        )
    }

    fn remember(&mut self, id: String, until: DateTime) -> &mut Seen {
        let seen = self.events.entry(id).or_insert(Seen {
            until,
            dismissed: None,
            muted: false,
            hidden: false,
        });
        seen.until = seen.until.max(until);
        seen
    }

    fn tidy(&mut self, now: DateTime) {
        let today = Some(now.date());
        self.events.retain(|_, seen| now < seen.until);
        self.summary_asked = self.summary_asked.filter(|&day| Some(day) == today);
        self.summary_seen = self.summary_seen.filter(|&day| Some(day) == today);
        self.paused_until = self.paused_until.filter(|&until| now < until);
    }

    fn is_paused(&self, now: DateTime) -> bool {
        self.paused_until.is_some_and(|until| now < until)
    }

    fn pausing(&self, now: DateTime) -> Vec<MenuItem<Action>> {
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
}

impl Plugin for Calendar {
    const NAME: &'static str = "Calendar";

    const READS_CALENDAR: bool = true;

    type Action = Action;

    type Settings = Settings;

    fn begin_day(&mut self, _settings: &Settings, now: DateTime) {
        self.tidy(now);
    }

    fn read_agenda(&mut self, agenda: &Agenda) {
        self.agenda = agenda.clone();
    }

    fn menu(&self, settings: &Settings, now: DateTime) -> Vec<MenuItem<Action>> {
        match self.agenda {
            Agenda::Unread => return Vec::new(),
            Agenda::Denied => return vec![MenuItem::label("No Access to Calendars")],
            Agenda::Read(_) => {}
        }
        let today = self.today(settings, now);
        let next = today
            .iter()
            .find(|appointment| !appointment.all_day && now < appointment.start)
            .map_or_else(
                || "Nothing Else Today".to_owned(),
                |next| {
                    format!(
                        "Next: {} at {}",
                        title(next),
                        TimeOfDay::from(next.start.time())
                    )
                },
            );
        let current = self.current(settings, now);
        let mut items: Vec<MenuItem<Action>> = current
            .iter()
            .take(CURRENT_LENGTH)
            .map(|appointment| {
                let title = title(appointment);
                MenuItem::label(if appointment.end.date() == now.date() {
                    format!(
                        "Now: {title} until {}",
                        TimeOfDay::from(appointment.end.time())
                    )
                } else {
                    format!("Now: {title}")
                })
            })
            .collect();
        items.push(MenuItem::label(next));
        items.push(
            MenuItem::new("Show Today's Events", Action::ShowSummary).enabled(!today.is_empty()),
        );
        let hidden = current
            .iter()
            .filter(|appointment| self.seen(appointment, |seen| seen.hidden))
            .count();
        if settings.show_current_event && !self.is_paused(now) && hidden > 0 {
            let title = if hidden == 1 {
                "Show Current Event"
            } else {
                "Show Current Events"
            };
            items.push(MenuItem::new(title, Action::ShowCurrent));
        }
        items.extend(self.pausing(now));
        items
    }

    fn message(&self, settings: &Settings, now: DateTime) -> Option<Message<Action>> {
        if !settings.show_current_event || self.is_paused(now) {
            return None;
        }
        let current: Vec<&Appointment> = self
            .current(settings, now)
            .into_iter()
            .filter(|appointment| !self.seen(appointment, |seen| seen.hidden))
            .collect();
        if current.is_empty() {
            return None;
        }
        let mut lines = vec!["Now:".to_owned()];
        lines.extend(
            current
                .iter()
                .take(CURRENT_LENGTH)
                .map(|appointment| title(appointment)),
        );
        if let Some(more) = current.len().checked_sub(CURRENT_LENGTH)
            && more > 0
        {
            lines.push(format!("...and {more} more"));
        }
        let events = current
            .iter()
            .map(|appointment| (appointment.id.clone(), expiry(appointment)))
            .collect();
        Some(
            Message::new(lines.join("\n"))
                .on_double_click(Action::HideCurrent { events })
                .shortcut(settings.hide_shortcut),
        )
    }

    fn notices(&self, settings: &Settings, now: DateTime) -> Vec<Notice<Action>> {
        let paused = self.is_paused(now);
        let mut due: Vec<(&Appointment, DateTime)> = self
            .appointments(settings)
            .filter(|_| !paused)
            .filter_map(|appointment| {
                Some((appointment, self.due_alert(settings, appointment, now)?))
            })
            .collect();
        due.sort_by_key(|(appointment, _)| (appointment.start, &appointment.title));
        let summary = self
            .summary(settings, now)
            .filter(|_| !paused || self.summary_asked == Some(now.date()));
        due.into_iter()
            .map(|(appointment, alert)| Self::reminder(settings, appointment, alert, now))
            .chain(summary)
            .collect()
    }

    fn changes_at(&self, settings: &Settings, now: DateTime) -> Option<DateTime> {
        let appointments = self.appointments(settings).flat_map(|appointment| {
            settings
                .alerts(appointment.start)
                .filter(|_| !appointment.all_day)
                .chain([appointment.start, appointment.end, expiry(appointment)])
        });
        let countdowns = self
            .appointments(settings)
            .filter(|_| settings.remind_with == RemindWith::Countdown && !self.is_paused(now))
            .filter(|appointment| self.due_alert(settings, appointment, now).is_some())
            .filter_map(|appointment| next_tick(appointment.start, now));
        let summary = settings
            .daily_summary
            .then(|| settings.daily_summary_at.next_after(now));
        appointments
            .chain(countdowns)
            .chain(summary)
            .chain(self.paused_until)
            .chain([midnight_after(now)])
            .filter(|&at| at > now)
            .min()
    }

    fn perform(&mut self, action: Action, cx: &mut Context<'_, Self>) {
        let now = cx.now();
        self.tidy(now);
        match action {
            Action::Dismiss {
                id,
                alert,
                until,
                ongoing,
            } => {
                let seen = self.remember(id, until);
                seen.dismissed = Some(alert);
                seen.hidden |= ongoing;
            }
            Action::Mute { id, until } => self.remember(id, until).muted = true,
            Action::HideCurrent { events } => {
                for (id, until) in events {
                    self.remember(id, until).hidden = true;
                }
            }
            Action::ShowCurrent => {
                for seen in self.events.values_mut() {
                    seen.hidden = false;
                }
            }
            Action::ShowSummary => {
                self.summary_asked = Some(now.date());
                self.summary_seen = None;
            }
            Action::DismissSummary => self.summary_seen = Some(now.date()),
            Action::Pause { until } => self.paused_until = Some(until),
            Action::Resume => self.paused_until = None,
        }
    }
}

fn title(appointment: &Appointment) -> String {
    let title: Vec<&str> = appointment.title.split_whitespace().collect();
    let title = title.join(" ");
    if printable(&title).is_empty() {
        "Untitled event".to_owned()
    } else {
        title
    }
}

fn starting(appointment: &Appointment, now: DateTime) -> String {
    let at = TimeOfDay::from(appointment.start.time());
    let day = appointment.start.date();
    if appointment.start <= now {
        format!("Started at {at}")
    } else if day == now.date() {
        format!("Starts at {at}")
    } else if Some(day) == now.date().tomorrow().ok() {
        format!("Starts tomorrow at {at}")
    } else {
        format!("Starts on {} at {at}", day.strftime("%A"))
    }
}

fn countdown(start: DateTime, now: DateTime) -> String {
    let left = now.duration_until(start);
    if !left.is_positive() {
        return "Now:".to_owned();
    }
    let at = TimeOfDay::from(start.time());
    if left > DAY {
        return if Some(start.date()) == now.date().tomorrow().ok() {
            format!("Tomorrow at {at}")
        } else {
            format!("On {} at {at}", start.date().strftime("%A"))
        };
    }
    let minutes = whole(left, MINUTE);
    let left = if minutes >= 60 {
        match (minutes / 60, minutes % 60) {
            (hours, 0) => counted(hours, "hour"),
            (hours, minutes) => {
                format!("{} {}", counted(hours, "hour"), counted(minutes, "minute"))
            }
        }
    } else if minutes > 1 {
        counted(minutes, "minute")
    } else {
        match whole(left, SECOND) {
            60 => counted(1, "minute"),
            seconds => counted(seconds, "second"),
        }
    };
    format!("In {left}:")
}

fn next_tick(start: DateTime, now: DateTime) -> Option<DateTime> {
    let left = now.duration_until(start);
    if !left.is_positive() {
        return None;
    }
    if left > DAY {
        return Some(start.saturating_sub(DAY));
    }
    let unit = if left > MINUTE { MINUTE } else { SECOND };
    let steps = i32::try_from(whole(left, unit) - 1).ok()?;
    Some(start.saturating_sub(unit.checked_mul(steps)?))
}

fn whole(left: SignedDuration, unit: SignedDuration) -> i64 {
    let (left, unit) = (left.as_nanos(), unit.as_nanos());
    i64::try_from((left + unit - 1) / unit).unwrap_or(i64::MAX)
}

fn counted(count: i64, unit: &str) -> String {
    if count == 1 {
        format!("1 {unit}")
    } else {
        format!("{count} {unit}s")
    }
}

fn summary_line(appointment: &Appointment, now: DateTime) -> String {
    let title = title(appointment);
    if lasts_all_day(appointment, now) {
        format!("All day: {title}")
    } else if appointment.start.date() < now.date() {
        format!("Until {}: {title}", TimeOfDay::from(appointment.end.time()))
    } else {
        format!("{} {title}", TimeOfDay::from(appointment.start.time()))
    }
}

fn lasts_all_day(appointment: &Appointment, now: DateTime) -> bool {
    appointment.all_day
        || (appointment.start.date() < now.date() && midnight_after(now) <= appointment.end)
}

fn expiry(appointment: &Appointment) -> DateTime {
    if appointment.start < appointment.end {
        appointment.end
    } else {
        appointment.start.saturating_add(INSTANT_EVENT)
    }
}

fn midnight_after(now: DateTime) -> DateTime {
    TimeOfDay::from(Time::midnight()).next_after(now)
}

#[cfg(test)]
mod tests;
