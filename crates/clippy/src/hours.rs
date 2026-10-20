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

use std::fmt;
use std::str::FromStr;

use jiff::civil::{DateTime, Time};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TimeOfDay(Time);

#[derive(Debug, thiserror::Error)]
#[error("expected a time of day such as 9:30 or 17:45, found `{text}`")]
pub struct ParseTimeOfDayError {
    text: String,
    source: jiff::Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Hours {
    from: TimeOfDay,
    until: TimeOfDay,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ParseHoursError {
    #[error("expected hours such as 22:00-9:00, found `{0}`")]
    Malformed(String),
    #[error("`{0}` starts and ends at the same time")]
    Empty(String),
}

impl TimeOfDay {
    pub fn time(self) -> Time {
        self.0
    }

    pub fn next_after(self, at: DateTime) -> DateTime {
        let today = at.date().to_datetime(self.0);
        if today > at {
            today
        } else {
            at.date()
                .tomorrow()
                .map_or(DateTime::MAX, |tomorrow| tomorrow.to_datetime(self.0))
        }
    }
}

impl From<Time> for TimeOfDay {
    fn from(time: Time) -> Self {
        Self(time)
    }
}

impl FromStr for TimeOfDay {
    type Err = ParseTimeOfDayError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Time::strptime("%H:%M", text.trim())
            .map(Self)
            .map_err(|source| ParseTimeOfDayError {
                text: text.to_owned(),
                source,
            })
    }
}

impl fmt::Display for TimeOfDay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0.strftime("%H:%M"))
    }
}

impl TryFrom<String> for TimeOfDay {
    type Error = ParseTimeOfDayError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<TimeOfDay> for String {
    fn from(time: TimeOfDay) -> Self {
        time.to_string()
    }
}

impl Hours {
    pub fn start(self) -> TimeOfDay {
        self.from
    }

    pub fn end(self) -> TimeOfDay {
        self.until
    }

    pub fn contains(self, time: Time) -> bool {
        let (from, until) = (self.from.time(), self.until.time());
        if from < until {
            from <= time && time < until
        } else {
            time >= from || time < until
        }
    }

    pub fn start_after(self, at: DateTime) -> DateTime {
        self.from.next_after(at)
    }

    pub fn end_after(self, at: DateTime) -> DateTime {
        self.until.next_after(at)
    }
}

impl FromStr for Hours {
    type Err = ParseHoursError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let malformed = || ParseHoursError::Malformed(text.to_owned());
        let (from, until) = text.split_once(['-', '–', '—']).ok_or_else(malformed)?;
        let from: TimeOfDay = from.parse().map_err(|_| malformed())?;
        let until: TimeOfDay = until.parse().map_err(|_| malformed())?;
        if from == until {
            return Err(ParseHoursError::Empty(text.to_owned()));
        }
        Ok(Self { from, until })
    }
}

impl fmt::Display for Hours {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}-{}", self.from, self.until)
    }
}

impl TryFrom<String> for Hours {
    type Error = ParseHoursError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<Hours> for String {
    fn from(hours: Hours) -> Self {
        hours.to_string()
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::{date, time};

    use super::*;

    fn parse(text: &str) -> Option<Time> {
        text.parse::<TimeOfDay>().ok().map(TimeOfDay::time)
    }

    fn hours(text: &str) -> Hours {
        text.parse().unwrap()
    }

    #[test]
    fn reads_hours_and_minutes() {
        assert_eq!(parse("9:30"), Some(time(9, 30, 0, 0)));
        assert_eq!(parse("09:30"), Some(time(9, 30, 0, 0)));
        assert_eq!(parse(" 17:45 "), Some(time(17, 45, 0, 0)));
        assert_eq!(parse("0:00"), Some(Time::midnight()));
    }

    #[test]
    fn rejects_anything_else() {
        for text in ["", "noon", "9", "9:300", "24:00", "9:60", "-1:00", "9.30"] {
            assert_eq!(parse(text), None, "{text:?}");
        }
    }

    #[test]
    fn prints_with_two_digit_hours() {
        assert_eq!("9:05".parse::<TimeOfDay>().unwrap().to_string(), "09:05");
    }

    #[test]
    fn the_next_time_of_day_is_always_ahead() {
        let nine = TimeOfDay::from(time(9, 0, 0, 0));
        let day = date(2026, 10, 19);
        assert_eq!(nine.next_after(day.at(8, 0, 0, 0)), day.at(9, 0, 0, 0));
        assert_eq!(
            nine.next_after(day.at(9, 0, 0, 0)),
            day.tomorrow().unwrap().at(9, 0, 0, 0)
        );
        assert_eq!(nine.next_after(DateTime::MAX), DateTime::MAX);
    }

    #[test]
    fn hours_may_wrap_around_midnight() {
        let night = hours("22:00-9:00");
        assert!(night.contains(time(23, 0, 0, 0)));
        assert!(night.contains(time(3, 0, 0, 0)));
        assert!(!night.contains(time(9, 0, 0, 0)));
        assert!(!night.contains(time(21, 59, 0, 0)));
        let lunch = hours("13:00 – 14:00");
        assert!(lunch.contains(time(13, 0, 0, 0)));
        assert!(!lunch.contains(time(14, 0, 0, 0)));
        assert_eq!(night.to_string(), "22:00-09:00");
        assert_eq!(night.start(), TimeOfDay::from(time(22, 0, 0, 0)));
        assert_eq!(night.end(), TimeOfDay::from(time(9, 0, 0, 0)));
    }

    #[test]
    fn hours_start_and_end_on_the_right_day() {
        let night = hours("22:00-9:00");
        let day = date(2026, 10, 19);
        let tomorrow = day.tomorrow().unwrap();
        assert_eq!(
            night.end_after(day.at(23, 0, 0, 0)),
            tomorrow.at(9, 0, 0, 0)
        );
        assert_eq!(night.end_after(day.at(3, 0, 0, 0)), day.at(9, 0, 0, 0));
        assert_eq!(night.start_after(day.at(3, 0, 0, 0)), day.at(22, 0, 0, 0));
        assert_eq!(
            night.start_after(day.at(23, 0, 0, 0)),
            tomorrow.at(22, 0, 0, 0)
        );
    }

    #[test]
    fn refuses_odd_hours() {
        assert!(matches!(
            "22:00".parse::<Hours>(),
            Err(ParseHoursError::Malformed(_))
        ));
        assert!(matches!(
            "9:00-9:00".parse::<Hours>(),
            Err(ParseHoursError::Empty(_))
        ));
    }
}
