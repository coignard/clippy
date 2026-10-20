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

use crate::agenda::Agenda;
use crate::settings::{Reminder, Schedule};

impl Reminder {
    pub(crate) fn latest(&self, now: DateTime, agenda: &Agenda) -> Option<DateTime> {
        let day = self.day(now.date())?;
        match &self.schedule {
            Schedule::At(times) => times
                .iter()
                .rev()
                .map(|time| day.to_datetime(time.time()))
                .find(|&at| at <= now),
            Schedule::Random(spans) => spans
                .iter()
                .filter_map(|&hours| agenda.pick(&self.text, hours, day))
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
        let day = self.day(now.date())?;
        match &self.schedule {
            Schedule::At(times) => times
                .iter()
                .map(|time| day.to_datetime(time.time()))
                .find(|&at| at > now),
            Schedule::Random(spans) => spans
                .iter()
                .filter_map(|&hours| agenda.pick(&self.text, hours, day))
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

    fn day(&self, day: Date) -> Option<Date> {
        self.days.contains(day.weekday()).then_some(day)
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
    use clippy::hours::{Hours, TimeOfDay};
    use jiff::ToSpan;
    use jiff::civil::{Weekday, date};

    use super::*;
    use crate::days::Days;
    use crate::interval::Interval;
    use crate::settings::Settings;

    const DAY: Date = date(2026, 10, 19);

    fn at(hour: i8, minute: i8) -> DateTime {
        DAY.at(hour, minute, 0, 0)
    }

    fn time(text: &str) -> TimeOfDay {
        text.parse().unwrap()
    }

    fn reminder(schedule: Schedule) -> Reminder {
        Reminder {
            text: "Stretch.".into(),
            schedule,
            days: Days::EVERY_DAY,
        }
    }

    fn hourly(from: Option<&str>, until: Option<&str>) -> Reminder {
        reminder(Schedule::Every {
            interval: Interval::minutes(60),
            from: from.map(time),
            until: until.map(time),
        })
    }

    #[test]
    fn fixed_times_come_in_order() {
        let twice = reminder(Schedule::At(vec![time("10:00"), time("15:00")]));
        assert_eq!(twice.latest(at(9, 59), &Agenda::default()), None);
        assert_eq!(twice.latest(at(10, 0), &Agenda::default()), Some(at(10, 0)));
        assert_eq!(twice.latest(at(16, 0), &Agenda::default()), Some(at(15, 0)));
        assert_eq!(twice.next(at(10, 0), &Agenda::default()), Some(at(15, 0)));
        assert_eq!(twice.next(at(15, 0), &Agenda::default()), None);
    }

    #[test]
    fn hourly_reminders_start_on_the_hour_they_are_given() {
        let hourly = hourly(Some("9:00"), Some("12:00"));
        assert_eq!(hourly.latest(at(8, 59), &Agenda::default()), None);
        assert_eq!(hourly.latest(at(9, 0), &Agenda::default()), Some(at(9, 0)));
        assert_eq!(
            hourly.latest(at(10, 59), &Agenda::default()),
            Some(at(10, 0))
        );
        assert_eq!(
            hourly.latest(at(18, 0), &Agenda::default()),
            Some(at(12, 0))
        );
        assert_eq!(hourly.next(at(8, 0), &Agenda::default()), Some(at(9, 0)));
        assert_eq!(hourly.next(at(10, 30), &Agenda::default()), Some(at(11, 0)));
        assert_eq!(hourly.next(at(12, 0), &Agenda::default()), None);
    }

    #[test]
    fn hourly_reminders_without_a_start_count_from_the_first_launch() {
        let hourly = hourly(None, None);
        assert_eq!(hourly.latest(at(9, 30), &Agenda::default()), None);
        let started = Agenda::starting(Some(at(9, 7)));
        assert_eq!(hourly.latest(at(10, 6), &started), None);
        assert_eq!(hourly.latest(at(10, 7), &started), Some(at(10, 7)));
        assert_eq!(hourly.next(at(10, 7), &started), Some(at(11, 7)));
        assert_eq!(hourly.next(at(23, 30), &started), None);
    }

    #[test]
    fn hourly_reminders_keep_their_rhythm_overnight() {
        let hourly = hourly(None, None);
        let yesterday = Agenda::starting(Some(DAY.yesterday().unwrap().at(9, 7, 0, 0)));
        assert_eq!(hourly.latest(at(0, 30), &yesterday), Some(at(0, 7)));
        assert_eq!(hourly.latest(at(0, 5), &yesterday), None);
        assert_eq!(hourly.next(at(0, 5), &yesterday), Some(at(0, 7)));
    }

    #[test]
    fn random_reminders_come_at_the_minute_picked_for_the_day() {
        let hours: Hours = "9:00-17:00".parse().unwrap();
        let music = reminder(Schedule::Random(vec![hours]));
        let settings = Settings {
            reminders: vec![music.clone()],
            ..Settings::default()
        };
        assert_eq!(music.next(at(8, 0), &Agenda::default()), None);
        let mut agenda = Agenda::default();
        agenda.begin(&settings, at(8, 0));
        let picked = agenda.pick(&music.text, hours, DAY).unwrap();
        let before = picked.checked_sub(1.minute()).unwrap();
        assert_eq!(music.latest(before, &agenda), None);
        assert_eq!(music.next(before, &agenda), Some(picked));
        assert_eq!(music.latest(picked, &agenda), Some(picked));
        assert_eq!(music.latest(at(23, 0), &agenda), Some(picked));
        assert_eq!(music.next(picked, &agenda), None);
    }

    #[test]
    fn reminders_keep_to_their_days() {
        let mut weekend = reminder(Schedule::At(vec![time("10:00")]));
        weekend.days = [Weekday::Saturday, Weekday::Sunday].into_iter().collect();
        assert_eq!(DAY.weekday(), Weekday::Monday);
        assert_eq!(weekend.latest(at(11, 0), &Agenda::default()), None);
        let saturday = date(2026, 10, 24).at(11, 0, 0, 0);
        assert_eq!(
            weekend.latest(saturday, &Agenda::default()),
            Some(date(2026, 10, 24).at(10, 0, 0, 0))
        );
    }
}
