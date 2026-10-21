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

use clippy::hours::TimeOfDay;
use clippy::shortcut::{Key, Modifiers, Shortcut};
use jiff::civil::{DateTime, Time};
use jiff::{SignedDuration, Span, SpanRelativeTo};
use serde::{Deserialize, Serialize};

const LONGEST_LEAD: SignedDuration = SignedDuration::from_hours(7 * 24);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub(crate) remind_before: Vec<Lead>,
    pub(crate) remind_with: RemindWith,
    pub(crate) show_current_event: bool,
    pub(crate) daily_summary: bool,
    pub(crate) daily_summary_at: TimeOfDay,
    pub(crate) only_calendars: Vec<String>,
    pub(crate) ignored_calendars: Vec<String>,
    pub(crate) show_in_full_screen: bool,
    pub(crate) hide_shortcut: Shortcut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RemindWith {
    Countdown,
    StartTime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub(crate) struct Lead(SignedDuration);

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ParseLeadError {
    #[error("expected how long before an event to remind, such as 10m, 1h, 1d or 0m, found `{0}`")]
    Malformed(String),
    #[error("`{0}` is after the event starts, use 0m to be reminded as it starts")]
    Late(String),
    #[error("`{0}` is too far ahead, the longest is a week")]
    TooEarly(String),
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            remind_before: vec![Lead::minutes(5)],
            remind_with: RemindWith::Countdown,
            show_current_event: true,
            daily_summary: true,
            daily_summary_at: TimeOfDay::from(Time::constant(9, 0, 0, 0)),
            only_calendars: Vec::new(),
            ignored_calendars: Vec::new(),
            show_in_full_screen: true,
            hide_shortcut: Shortcut {
                modifiers: Modifiers::COMMAND.union(Modifiers::SHIFT),
                key: Key::Delete,
            },
        }
    }
}

impl Settings {
    pub(crate) fn follows(&self, calendar: &str) -> bool {
        let calendar = calendar.trim().to_lowercase();
        let names = |names: &[String]| {
            names
                .iter()
                .any(|name| name.trim().to_lowercase() == calendar)
        };
        (self.only_calendars.is_empty() || names(&self.only_calendars))
            && !names(&self.ignored_calendars)
    }

    pub(crate) fn alerts(&self, start: DateTime) -> impl Iterator<Item = DateTime> + '_ {
        self.remind_before
            .iter()
            .map(move |lead| start.saturating_sub(lead.0))
    }
}

impl Lead {
    pub(crate) const fn minutes(minutes: i64) -> Self {
        Self(SignedDuration::from_mins(minutes))
    }
}

impl FromStr for Lead {
    type Err = ParseLeadError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let trimmed = text.trim();
        let duration = if trimmed == "0" {
            SignedDuration::ZERO
        } else {
            trimmed
                .parse::<Span>()
                .ok()
                .and_then(|span| span.to_duration(SpanRelativeTo::days_are_24_hours()).ok())
                .ok_or_else(|| ParseLeadError::Malformed(text.to_owned()))?
        };
        if duration.is_negative() {
            return Err(ParseLeadError::Late(text.to_owned()));
        }
        if duration > LONGEST_LEAD {
            return Err(ParseLeadError::TooEarly(text.to_owned()));
        }
        Ok(Self(duration))
    }
}

impl fmt::Display for Lead {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_zero() {
            formatter.write_str("0m")
        } else {
            write!(formatter, "{:#}", self.0)
        }
    }
}

impl TryFrom<String> for Lead {
    type Error = ParseLeadError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<Lead> for String {
    fn from(lead: Lead) -> Self {
        lead.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> Result<Settings, String> {
        toml::from_str(text).map_err(|error| error.to_string())
    }

    #[test]
    fn defaults_explain_themselves() {
        let written = toml::to_string(&Settings::default()).unwrap();
        assert_eq!(
            written,
            "remind_before = [\"5m\"]\n\
             remind_with = \"countdown\"\n\
             show_current_event = true\n\
             daily_summary = true\n\
             daily_summary_at = \"09:00\"\n\
             only_calendars = []\n\
             ignored_calendars = []\n\
             show_in_full_screen = true\n\
             hide_shortcut = \"Cmd+Shift+Delete\"\n"
        );
        assert_eq!(read(&written), Ok(Settings::default()));
        assert_eq!(read(""), Ok(Settings::default()));
    }

    #[test]
    fn reminds_at_any_number_of_lead_times_including_the_start() {
        let settings = read(r#"remind_before = ["1w", "1d", "1h 30m", "10m", "0m", "0"]"#).unwrap();
        assert_eq!(
            settings.remind_before,
            [
                Lead::minutes(7 * 24 * 60),
                Lead::minutes(24 * 60),
                Lead::minutes(90),
                Lead::minutes(10),
                Lead::minutes(0),
                Lead::minutes(0)
            ]
        );
        assert_eq!(
            toml::to_string(&settings).unwrap().lines().next(),
            Some(r#"remind_before = ["168h", "24h", "1h 30m", "10m", "0m", "0m"]"#)
        );
        assert_eq!(read("remind_before = []").unwrap().remind_before, []);
    }

    #[test]
    fn explains_what_is_wrong() {
        for (text, problem) in [
            (r#"remind_before = ["soon"]"#, "such as 10m, 1h, 1d or 0m"),
            (r#"remind_before = ["-5m"]"#, "use 0m"),
            (r#"remind_before = ["8d"]"#, "the longest is a week"),
            (r#"daily_summary_at = "9am""#, "9:30 or 17:45"),
            (r#"remind_with = "clock""#, "`countdown` or `start_time`"),
            (r#"calendars = ["Work"]"#, "unknown field"),
        ] {
            let error = read(text).unwrap_err();
            assert!(error.contains(problem), "{text}: {error}");
        }
    }

    #[test]
    fn calendars_are_matched_by_name_regardless_of_case() {
        let settings = read(
            r#"
            only_calendars = ["Работа", "Home"]
            ignored_calendars = ["home"]
            "#,
        )
        .unwrap();
        assert!(settings.follows("работа"));
        assert!(settings.follows(" Работа "));
        assert!(!settings.follows("Home"));
        assert!(!settings.follows("Birthdays"));
        let ignoring = read(r#"ignored_calendars = ["Birthdays"]"#).unwrap();
        assert!(ignoring.follows("Work"));
        assert!(!ignoring.follows("BIRTHDAYS"));
    }
}
