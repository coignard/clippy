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

use jiff::SignedDuration;
use serde::{Deserialize, Serialize};

const SHORTEST: SignedDuration = SignedDuration::from_mins(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub(crate) struct Interval(SignedDuration);

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ParseIntervalError {
    #[error("expected a duration such as 15m, 1h or 1h30m, found `{0}`")]
    Malformed(String),
    #[error("`{0}` is too short, the shortest is one minute")]
    TooShort(String),
}

impl Interval {
    pub(crate) const fn minutes(minutes: i64) -> Self {
        Self(SignedDuration::from_mins(minutes))
    }

    pub(crate) fn duration(self) -> SignedDuration {
        self.0
    }
}

impl FromStr for Interval {
    type Err = ParseIntervalError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let duration: SignedDuration = text
            .trim()
            .parse()
            .map_err(|_| ParseIntervalError::Malformed(text.to_owned()))?;
        if duration < SHORTEST {
            return Err(ParseIntervalError::TooShort(text.to_owned()));
        }
        Ok(Self(duration))
    }
}

impl fmt::Display for Interval {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:#}", self.0)
    }
}

impl TryFrom<String> for Interval {
    type Error = ParseIntervalError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<Interval> for String {
    fn from(interval: Interval) -> Self {
        interval.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_friendly_durations() {
        for (text, minutes) in [
            ("15m", 15),
            ("1h", 60),
            ("1h30m", 90),
            ("1h 30m", 90),
            ("90m", 90),
            ("2 hours", 120),
        ] {
            assert_eq!(text.parse(), Ok(Interval::minutes(minutes)), "{text:?}");
        }
        assert_eq!(Interval::minutes(90).to_string(), "1h 30m");
    }

    #[test]
    fn refuses_nonsense_and_spam() {
        assert!(matches!(
            "soon".parse::<Interval>(),
            Err(ParseIntervalError::Malformed(_))
        ));
        assert!(matches!(
            "1d".parse::<Interval>(),
            Err(ParseIntervalError::Malformed(_))
        ));
        for text in ["0m", "30s", "-5m"] {
            assert!(
                matches!(
                    text.parse::<Interval>(),
                    Err(ParseIntervalError::TooShort(_))
                ),
                "{text:?}"
            );
        }
    }
}
