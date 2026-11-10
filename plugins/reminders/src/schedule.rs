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

use clippy::hours::TimeOfDay;
use jiff::SignedDuration;
use jiff::civil::{Date, DateTime, Time};

use crate::agenda::{Agenda, monday_of};
use crate::settings::{Reminder, Schedule};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Comeback {
    At(DateTime),
    On(Date),
    NextWeek,
}

impl Reminder {
    pub(crate) fn first_on(&self, day: Date, today: Date, agenda: &Agenda) -> Option<Comeback> {
        if self.random_days.is_some() && monday_of(day) != monday_of(today) {
            return self
                .days
                .contains(day.weekday())
                .then_some(Comeback::NextWeek);
        }
        let day = self.day(day, agenda)?;
        Some(match &self.schedule {
            Schedule::At(times) => Comeback::At(day.to_datetime(times.first()?.time())),
            Schedule::Every {
                from: Some(from), ..
            } => Comeback::At(day.to_datetime(from.time())),
            Schedule::Every { from: None, .. } | Schedule::Random(_) => Comeback::On(day),
        })
    }

    pub(crate) fn latest(&self, now: DateTime, agenda: &Agenda) -> Option<DateTime> {
        let day = self.day(now.date(), agenda)?;
        match &self.schedule {
            Schedule::At(times) => times
                .iter()
                .rev()
                .map(|time| day.to_datetime(time.time()))
                .find(|&at| at <= now),
            Schedule::Random(spans) => spans
                .iter()
                .filter_map(|&hours| agenda.pick(&self.key(), hours, day))
                .filter(|&at| at <= now)
                .max(),
            Schedule::Every { .. } => {
                let grid = self.grid(day, agenda.started)?;
                let index = grid.index_at(now.min(grid.end))?;
                grid.at(index).filter(|&at| at.date() == day)
            }
        }
    }

    pub(crate) fn next(&self, now: DateTime, agenda: &Agenda) -> Option<DateTime> {
        let day = self.day(now.date(), agenda)?;
        match &self.schedule {
            Schedule::At(times) => times
                .iter()
                .map(|time| day.to_datetime(time.time()))
                .find(|&at| at > now),
            Schedule::Random(spans) => spans
                .iter()
                .filter_map(|&hours| agenda.pick(&self.key(), hours, day))
                .filter(|&at| at > now)
                .min(),
            Schedule::Every { .. } => {
                let grid = self.grid(day, agenda.started)?;
                let index = grid.index_at(now).map_or(0, |index| index + 1);
                grid.at(index)
                    .filter(|&at| at.date() == day && at <= grid.end)
            }
        }
    }

    fn day(&self, day: Date, agenda: &Agenda) -> Option<Date> {
        agenda.is_on(self, day).then_some(day)
    }

    fn grid(&self, day: Date, started: Option<DateTime>) -> Option<Grid> {
        let Schedule::Every {
            interval,
            from,
            until,
        } = &self.schedule
        else {
            return None;
        };
        let step = interval.duration();
        let first = match from {
            Some(from) => day.to_datetime(from.time()),
            None => started?.checked_add(step).ok()?,
        };
        let end = day.to_datetime(until.map_or(Time::MAX, TimeOfDay::time));
        Some(Grid { first, step, end })
    }
}

struct Grid {
    first: DateTime,
    step: SignedDuration,
    end: DateTime,
}

impl Grid {
    fn index_at(&self, at: DateTime) -> Option<i64> {
        let elapsed = at.duration_since(self.first);
        (!elapsed.is_negative()).then(|| elapsed.as_secs() / self.step.as_secs())
    }

    fn at(&self, index: i64) -> Option<DateTime> {
        let offset = SignedDuration::from_secs(self.step.as_secs().checked_mul(index)?);
        self.first.checked_add(offset).ok()
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;
    use crate::days::Days;
    use crate::interval::Interval;

    const DAY: Date = date(2026, 10, 19);

    fn at(hour: i8, minute: i8) -> DateTime {
        DAY.at(hour, minute, 0, 0)
    }

    fn hourly() -> Reminder {
        Reminder {
            text: "Stretch.".into(),
            tasks: Vec::new(),
            schedule: Schedule::Every {
                interval: Interval::minutes(60),
                from: None,
                until: None,
            },
            days: Days::EVERY_DAY,
            random_days: None,
        }
    }

    #[test]
    fn hourly_reminders_without_a_start_count_from_the_first_launch() {
        let hourly = hourly();
        assert_eq!(hourly.latest(at(9, 30), &Agenda::default()), None);
        let started = Agenda::starting(Some(at(9, 7)));
        assert_eq!(hourly.latest(at(10, 6), &started), None);
        assert_eq!(hourly.latest(at(10, 7), &started), Some(at(10, 7)));
        assert_eq!(hourly.next(at(10, 7), &started), Some(at(11, 7)));
        assert_eq!(hourly.next(at(23, 30), &started), None);
    }

    #[test]
    fn hourly_reminders_keep_their_rhythm_overnight() {
        let hourly = hourly();
        let yesterday = Agenda::starting(Some(DAY.yesterday().unwrap().at(9, 7, 0, 0)));
        assert_eq!(hourly.latest(at(0, 30), &yesterday), Some(at(0, 7)));
        assert_eq!(hourly.latest(at(0, 5), &yesterday), None);
        assert_eq!(hourly.next(at(0, 5), &yesterday), Some(at(0, 7)));
    }
}
