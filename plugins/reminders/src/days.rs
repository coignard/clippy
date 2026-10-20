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

use jiff::civil::Weekday;
use jiff::fmt::strtime;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<String>", into = "Vec<String>")]
pub(crate) struct Days([bool; 7]);

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ParseDaysError {
    #[error("expected days such as Mon or Friday, found `{0}`")]
    Unknown(String),
    #[error("a reminder needs at least one day")]
    None,
}

impl Days {
    pub(crate) const EVERY_DAY: Self = Self([true; 7]);

    pub(crate) fn contains(self, day: Weekday) -> bool {
        self.0[index(day)]
    }

    pub(crate) fn is_every_day(self) -> bool {
        self == Self::EVERY_DAY
    }

    fn iter(self) -> impl Iterator<Item = Weekday> {
        Weekday::Monday
            .cycle_forward()
            .take(self.0.len())
            .filter(move |&day| self.contains(day))
    }
}

impl Default for Days {
    fn default() -> Self {
        Self::EVERY_DAY
    }
}

impl FromIterator<Weekday> for Days {
    fn from_iter<I: IntoIterator<Item = Weekday>>(days: I) -> Self {
        let mut set = [false; 7];
        for day in days {
            set[index(day)] = true;
        }
        Self(set)
    }
}

impl TryFrom<Vec<String>> for Days {
    type Error = ParseDaysError;

    fn try_from(names: Vec<String>) -> Result<Self, Self::Error> {
        let days: Days = names
            .iter()
            .map(|name| parse(name.trim()))
            .collect::<Result<_, _>>()?;
        if days.0.contains(&true) {
            Ok(days)
        } else {
            Err(ParseDaysError::None)
        }
    }
}

impl From<Days> for Vec<String> {
    fn from(days: Days) -> Self {
        days.iter().map(|day| name(day).to_owned()).collect()
    }
}

fn name(day: Weekday) -> &'static str {
    match day {
        Weekday::Monday => "Mon",
        Weekday::Tuesday => "Tue",
        Weekday::Wednesday => "Wed",
        Weekday::Thursday => "Thu",
        Weekday::Friday => "Fri",
        Weekday::Saturday => "Sat",
        Weekday::Sunday => "Sun",
    }
}

fn parse(name: &str) -> Result<Weekday, ParseDaysError> {
    ["%a", "%A"]
        .into_iter()
        .find_map(|format| strtime::parse(format, name).ok()?.weekday())
        .ok_or_else(|| ParseDaysError::Unknown(name.to_owned()))
}

fn index(day: Weekday) -> usize {
    usize::from(day.to_monday_zero_offset().unsigned_abs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn days(names: &[&str]) -> Result<Days, ParseDaysError> {
        Days::try_from(
            names
                .iter()
                .map(|&name| name.to_owned())
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn reads_short_and_long_names_in_any_case() {
        let workdays = days(&["mon", "Tuesday", "WED", "thu", "Fri"]).unwrap();
        assert!(workdays.contains(Weekday::Monday));
        assert!(workdays.contains(Weekday::Friday));
        assert!(!workdays.contains(Weekday::Saturday));
        assert_eq!(
            Vec::<String>::from(workdays),
            ["Mon", "Tue", "Wed", "Thu", "Fri"]
        );
    }

    #[test]
    fn refuses_unknown_or_no_days() {
        assert_eq!(
            days(&["Funday"]),
            Err(ParseDaysError::Unknown("Funday".into()))
        );
        assert_eq!(days(&[]), Err(ParseDaysError::None));
    }
}
