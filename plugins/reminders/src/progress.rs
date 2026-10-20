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

use jiff::civil::{Date, DateTime};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Progress {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) done: Option<DateTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) snoozed: Option<Snooze>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Snooze {
    pub(crate) times: u32,
    pub(crate) occurrence: DateTime,
    pub(crate) until: DateTime,
}

impl Progress {
    pub(crate) fn snooze_on(&self, day: Date) -> Option<Snooze> {
        self.snoozed
            .filter(|snooze| snooze.occurrence.date() == day)
    }

    pub(crate) fn snoozes_of(&self, occurrence: DateTime) -> u32 {
        self.snoozed
            .filter(|snooze| snooze.occurrence == occurrence)
            .map_or(0, |snooze| snooze.times)
    }

    pub(crate) fn is_from(&self, day: Date) -> bool {
        self.done.is_some_and(|done| done.date() == day) || self.snooze_on(day).is_some()
    }

    pub(crate) fn snooze(&mut self, occurrence: DateTime, until: DateTime) {
        self.snoozed = Some(Snooze {
            times: self.snoozes_of(occurrence) + 1,
            occurrence,
            until,
        });
    }

    pub(crate) fn finish(&mut self, occurrence: DateTime) {
        self.done = Some(self.done.map_or(occurrence, |done| done.max(occurrence)));
        self.snoozed = None;
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;

    #[test]
    fn progress_belongs_to_the_day_it_was_made() {
        let day = date(2026, 10, 19);
        let mut progress = Progress::default();
        progress.snooze(day.at(23, 0, 0, 0), day.tomorrow().unwrap().at(0, 50, 0, 0));
        assert!(progress.is_from(day));
        assert!(!progress.is_from(day.tomorrow().unwrap()));
        assert_eq!(progress.snooze_on(day.tomorrow().unwrap()), None);
        progress.finish(day.at(10, 0, 0, 0));
        assert_eq!(progress.snoozed, None);
        progress.finish(day.at(9, 0, 0, 0));
        assert_eq!(progress.done, Some(day.at(10, 0, 0, 0)));
    }

    #[test]
    fn snoozes_climb_only_for_the_same_occurrence() {
        let day = date(2026, 10, 19);
        let nine = day.at(9, 0, 0, 0);
        let mut progress = Progress::default();
        progress.snooze(nine, day.at(9, 15, 0, 0));
        progress.snooze(nine, day.at(9, 45, 0, 0));
        assert_eq!(progress.snoozes_of(nine), 2);
        let eleven = day.at(11, 0, 0, 0);
        assert_eq!(progress.snoozes_of(eleven), 0);
        progress.snooze(eleven, day.at(11, 15, 0, 0));
        assert_eq!(progress.snoozes_of(eleven), 1);
    }
}
