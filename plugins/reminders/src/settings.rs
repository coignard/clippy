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
use std::marker::PhantomData;
use std::str::FromStr;

use clippy::hours::{Hours, TimeOfDay};
use clippy::shortcut::{Key, Modifiers, Shortcut};
use jiff::SignedDuration;
use jiff::civil::{DateTime, Time};
use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::days::Days;
use crate::interval::Interval;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub(crate) reminders: Vec<Reminder>,
    pub(crate) quiet_hours: Vec<Hours>,
    pub(crate) snooze: Snoozes,
    pub(crate) show_in_full_screen: bool,
    pub(crate) hide_shortcut: Shortcut,
    pub(crate) check_shortcut: Shortcut,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawReminder", into = "RawReminder")]
pub(crate) struct Reminder {
    pub(crate) text: String,
    pub(crate) schedule: Schedule,
    pub(crate) days: Days,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Schedule {
    At(Vec<TimeOfDay>),
    Every {
        interval: Interval,
        from: Option<TimeOfDay>,
        until: Option<TimeOfDay>,
    },
    Random(Vec<Hours>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<Interval>", into = "Vec<Interval>")]
pub(crate) struct Snoozes(Vec<Interval>);

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum SettingsError {
    #[error("a reminder needs some text")]
    NoText,
    #[error("`{0}` needs a `time`, an `every` or a `random`")]
    NoSchedule(String),
    #[error("`{0}` has more than one of `time`, `every` and `random`, keep one of them")]
    TwoSchedules(String),
    #[error("`{0}` has `from` or `until` without `every`")]
    BoundsWithoutEvery(String),
    #[error("`{0}` ends before it starts")]
    Backwards(String),
    #[error("`{0}` needs at least one time")]
    NoTimes(String),
    #[error("`{0}` needs at least one span of hours to pick a random time in")]
    NoHours(String),
    #[error("`{0}` picks a random time across midnight, keep its hours within a day")]
    Overnight(String),
    #[error("snoozing needs at least one duration")]
    NoSnoozes,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawReminder {
    text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    time: Option<OneOrMany<TimeOfDay>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    every: Option<Interval>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    random: Option<OneOrMany<Hours>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    from: Option<TimeOfDay>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    until: Option<TimeOfDay>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    days: Option<Days>,
}

struct OneOrMany<T>(Vec<T>);

impl Default for Settings {
    fn default() -> Self {
        Self {
            reminders: Vec::new(),
            quiet_hours: Vec::new(),
            snooze: Snoozes::default(),
            show_in_full_screen: true,
            hide_shortcut: Shortcut {
                modifiers: Modifiers::COMMAND.union(Modifiers::SHIFT),
                key: Key::Delete,
            },
            check_shortcut: Shortcut {
                modifiers: Modifiers::COMMAND
                    .union(Modifiers::OPTION)
                    .union(Modifiers::SHIFT),
                key: Key::Delete,
            },
        }
    }
}

impl Settings {
    pub(crate) fn is_quiet(&self, time: Time) -> bool {
        self.quiet_hours.iter().any(|hours| hours.contains(time))
    }

    pub(crate) fn after_quiet_hours(&self, mut at: DateTime) -> Option<DateTime> {
        for _ in 0..=self.quiet_hours.len() {
            match self
                .quiet_hours
                .iter()
                .find(|hours| hours.contains(at.time()))
            {
                Some(hours) => at = hours.end_after(at),
                None => return Some(at),
            }
        }
        None
    }
}

impl Snoozes {
    pub(crate) fn new(snoozes: Vec<Interval>) -> Result<Self, SettingsError> {
        if snoozes.is_empty() {
            Err(SettingsError::NoSnoozes)
        } else {
            Ok(Self(snoozes))
        }
    }

    pub(crate) fn after(&self, times: u32) -> SignedDuration {
        let last = self.0.len() - 1;
        let step = usize::try_from(times).map_or(last, |times| times.min(last));
        self.0[step].duration()
    }
}

impl Default for Snoozes {
    fn default() -> Self {
        Self(vec![
            Interval::minutes(15),
            Interval::minutes(30),
            Interval::minutes(60),
        ])
    }
}

impl TryFrom<Vec<Interval>> for Snoozes {
    type Error = SettingsError;

    fn try_from(snoozes: Vec<Interval>) -> Result<Self, Self::Error> {
        Self::new(snoozes)
    }
}

impl From<Snoozes> for Vec<Interval> {
    fn from(snoozes: Snoozes) -> Self {
        snoozes.0
    }
}

impl TryFrom<RawReminder> for Reminder {
    type Error = SettingsError;

    fn try_from(raw: RawReminder) -> Result<Self, Self::Error> {
        let text = raw.text.trim().to_owned();
        if text.is_empty() {
            return Err(SettingsError::NoText);
        }
        let bounded = raw.from.is_some() || raw.until.is_some();
        let schedule = match (raw.time, raw.every, raw.random) {
            (None, None, None) => return Err(SettingsError::NoSchedule(text)),
            (Some(_), None, None) | (None, None, Some(_)) if bounded => {
                return Err(SettingsError::BoundsWithoutEvery(text));
            }
            (Some(OneOrMany(times)), None, None) if times.is_empty() => {
                return Err(SettingsError::NoTimes(text));
            }
            (Some(OneOrMany(mut times)), None, None) => {
                times.sort_unstable();
                times.dedup();
                Schedule::At(times)
            }
            (None, None, Some(OneOrMany(hours))) if hours.is_empty() => {
                return Err(SettingsError::NoHours(text));
            }
            (None, None, Some(OneOrMany(hours))) => {
                if hours.iter().any(|hours| hours.end() <= hours.start()) {
                    return Err(SettingsError::Overnight(text));
                }
                Schedule::Random(hours)
            }
            (None, Some(interval), None) => {
                if let (Some(from), Some(until)) = (raw.from, raw.until)
                    && until < from
                {
                    return Err(SettingsError::Backwards(text));
                }
                Schedule::Every {
                    interval,
                    from: raw.from,
                    until: raw.until,
                }
            }
            _ => return Err(SettingsError::TwoSchedules(text)),
        };
        Ok(Self {
            text,
            schedule,
            days: raw.days.unwrap_or_default(),
        })
    }
}

impl From<Reminder> for RawReminder {
    fn from(reminder: Reminder) -> Self {
        let (time, every, random, from, until) = match reminder.schedule {
            Schedule::At(times) => (Some(OneOrMany(times)), None, None, None, None),
            Schedule::Every {
                interval,
                from,
                until,
            } => (None, Some(interval), None, from, until),
            Schedule::Random(hours) => (None, None, Some(OneOrMany(hours)), None, None),
        };
        Self {
            text: reminder.text,
            time,
            every,
            random,
            from,
            until,
            days: (!reminder.days.is_every_day()).then_some(reminder.days),
        }
    }
}

impl<T: Serialize> Serialize for OneOrMany<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0.as_slice() {
            [one] => one.serialize(serializer),
            many => many.serialize(serializer),
        }
    }
}

impl<'de, T> Deserialize<'de> for OneOrMany<T>
where
    T: Deserialize<'de> + FromStr,
    T::Err: fmt::Display,
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OneOrManyVisitor<T>(PhantomData<T>);

        impl<'de, T> Visitor<'de> for OneOrManyVisitor<T>
        where
            T: Deserialize<'de> + FromStr,
            T::Err: fmt::Display,
        {
            type Value = OneOrMany<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("one value or a list of them")
            }

            fn visit_str<E: de::Error>(self, text: &str) -> Result<OneOrMany<T>, E> {
                text.parse()
                    .map(|one| OneOrMany(vec![one]))
                    .map_err(E::custom)
            }

            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<OneOrMany<T>, A::Error> {
                let mut many = Vec::new();
                while let Some(one) = sequence.next_element()? {
                    many.push(one);
                }
                Ok(OneOrMany(many))
            }
        }

        deserializer.deserialize_any(OneOrManyVisitor(PhantomData))
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::Weekday;

    use super::*;

    fn read(text: &str) -> Result<Settings, String> {
        toml::from_str(text).map_err(|error| error.to_string())
    }

    fn at(text: &str) -> TimeOfDay {
        text.parse().unwrap()
    }

    #[test]
    fn defaults_explain_themselves() {
        assert_eq!(
            toml::to_string(&Settings::default()).unwrap(),
            "reminders = []\n\
             quiet_hours = []\n\
             snooze = [\"15m\", \"30m\", \"1h\"]\n\
             show_in_full_screen = true\n\
             hide_shortcut = \"Cmd+Shift+Delete\"\n\
             check_shortcut = \"Cmd+Option+Shift+Delete\"\n"
        );
    }

    #[test]
    fn reads_every_kind_of_reminder() {
        let settings = read(
            r#"
            quiet_hours = ["22:00-9:00", "13:00-14:00"]
            snooze = ["10m", "1h"]
            show_in_full_screen = false
            reminders = [
                { text = "Stand up and stretch.", time = "10:00" },
                { text = "Take your vitamins.", time = ["9:00", "21:00"], days = ["Mon", "Thu"] },
                { text = "Have a glass of water.", every = "1h" },
                { text = "Look away from the screen.", every = "20m", from = "9:00", until = "18:00" },
                { text = "How about some music?", random = ["9:00-12:00", "14:00-18:00"], days = ["Mon", "Fri"] },
            ]
            "#,
        )
        .unwrap();
        assert_eq!(settings.quiet_hours.len(), 2);
        assert_eq!(settings.snooze.after(5), SignedDuration::from_hours(1));
        assert!(!settings.show_in_full_screen);
        assert_eq!(
            settings.reminders[0].schedule,
            Schedule::At(vec![at("10:00")])
        );
        assert_eq!(
            settings.reminders[1].schedule,
            Schedule::At(vec![at("9:00"), at("21:00")])
        );
        assert!(settings.reminders[1].days.contains(Weekday::Thursday));
        assert!(!settings.reminders[1].days.contains(Weekday::Friday));
        assert_eq!(
            settings.reminders[2].schedule,
            Schedule::Every {
                interval: Interval::minutes(60),
                from: None,
                until: None
            }
        );
        assert_eq!(
            settings.reminders[3].schedule,
            Schedule::Every {
                interval: Interval::minutes(20),
                from: Some(at("9:00")),
                until: Some(at("18:00"))
            }
        );
        assert_eq!(
            settings.reminders[4].schedule,
            Schedule::Random(vec![
                "9:00-12:00".parse().unwrap(),
                "14:00-18:00".parse().unwrap()
            ])
        );
        let written = toml::to_string(&settings).unwrap();
        assert_eq!(read(&written), Ok(settings));
    }

    #[test]
    fn explains_what_is_wrong_with_a_reminder() {
        for (text, problem) in [
            (r#"{ text = " ", time = "10:00" }"#, "needs some text"),
            (
                r#"{ text = "Stretch." }"#,
                "needs a `time`, an `every` or a `random`",
            ),
            (
                r#"{ text = "Music?", random = "7:00-21:00", time = "10:00" }"#,
                "keep one of them",
            ),
            (
                r#"{ text = "Music?", random = "7:00-21:00", from = "9:00" }"#,
                "without `every`",
            ),
            (
                r#"{ text = "Music?", random = "21:00-7:00" }"#,
                "across midnight",
            ),
            (
                r#"{ text = "Music?", random = [] }"#,
                "at least one span of hours",
            ),
            (r#"{ text = "Music?", random = "evening" }"#, "22:00-9:00"),
            (
                r#"{ text = "Stretch.", time = "10:00", every = "1h" }"#,
                "keep one of them",
            ),
            (
                r#"{ text = "Stretch.", time = "10:00", until = "11:00" }"#,
                "without `every`",
            ),
            (
                r#"{ text = "Stretch.", every = "1h", from = "18:00", until = "9:00" }"#,
                "ends before it starts",
            ),
            (r#"{ text = "Stretch.", time = "25:00" }"#, "9:30 or 17:45"),
            (r#"{ text = "Stretch.", time = [] }"#, "at least one time"),
            (r#"{ text = "Stretch.", every = "10s" }"#, "too short"),
            (
                r#"{ text = "Stretch.", time = "10:00", days = ["Caturday"] }"#,
                "Caturday",
            ),
            (
                r#"{ text = "Stretch.", time = "10:00", when = "always" }"#,
                "unknown field",
            ),
        ] {
            let error = read(&format!("reminders = [{text}]")).unwrap_err();
            assert!(error.contains(problem), "{text}: {error}");
        }
        assert!(
            read("snooze = []")
                .unwrap_err()
                .contains("at least one duration")
        );
    }

    #[test]
    fn quiet_hours_push_times_past_their_end() {
        let settings = read(r#"quiet_hours = ["12:00-13:00", "13:00-14:00"]"#).unwrap();
        let day = jiff::civil::date(2026, 10, 19);
        assert!(settings.is_quiet(jiff::civil::time(12, 30, 0, 0)));
        let always = read(r#"quiet_hours = ["0:00-12:00", "12:00-0:00"]"#).unwrap();
        assert_eq!(always.after_quiet_hours(day.at(15, 0, 0, 0)), None);
        assert_eq!(
            settings.after_quiet_hours(day.at(12, 30, 0, 0)),
            Some(day.at(14, 0, 0, 0))
        );
        assert_eq!(
            settings.after_quiet_hours(day.at(15, 0, 0, 0)),
            Some(day.at(15, 0, 0, 0))
        );
    }
}
