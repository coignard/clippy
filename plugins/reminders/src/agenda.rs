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

use clippy::hours::Hours;
use jiff::ToSpan;
use jiff::civil::{Date, DateTime};
use serde::{Deserialize, Serialize};

use crate::settings::{Schedule, Settings};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Agenda {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) started: Option<DateTime>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    picks: Vec<Pick>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Pick {
    text: String,
    hours: Hours,
    at: DateTime,
}

impl Agenda {
    #[cfg(test)]
    pub(crate) fn starting(started: Option<DateTime>) -> Self {
        Self {
            started,
            picks: Vec::new(),
        }
    }

    pub(crate) fn begin(&mut self, settings: &Settings, now: DateTime) {
        if self
            .started
            .is_none_or(|started| started.date() != now.date())
        {
            self.started = Some(now);
        }
        self.picks.retain(|pick| pick.at.date() == now.date());
        for reminder in &settings.reminders {
            let Schedule::Random(spans) = &reminder.schedule else {
                continue;
            };
            if !reminder.days.contains(now.weekday()) {
                continue;
            }
            for &hours in spans {
                if self.pick(&reminder.text, hours, now.date()).is_none()
                    && let Some(at) = random_time(settings, hours, now)
                {
                    self.picks.push(Pick {
                        text: reminder.text.clone(),
                        hours,
                        at,
                    });
                }
            }
        }
    }

    pub(crate) fn pick(&self, text: &str, hours: Hours, day: Date) -> Option<DateTime> {
        self.picks
            .iter()
            .find(|pick| pick.text == text && pick.hours == hours && pick.at.date() == day)
            .map(|pick| pick.at)
    }
}

fn random_time(settings: &Settings, hours: Hours, now: DateTime) -> Option<DateTime> {
    let day = now.date();
    let end = day.to_datetime(hours.end().time());
    let free: Vec<DateTime> = day
        .to_datetime(hours.start().time())
        .series(1.minute())
        .take_while(|&at| at < end)
        .filter(|&at| at > now && !settings.is_quiet(at.time()))
        .collect();
    fastrand::choice(free)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use jiff::civil::{Weekday, date};

    use super::*;

    fn settings(text: &str) -> Settings {
        toml::from_str(text).unwrap()
    }

    fn music(extra: &str) -> Settings {
        settings(&format!(
            r#"{extra}
            reminders = [{{ text = "Music?", random = "7:00-21:00", days = ["Mon", "Tue", "Wed", "Thu", "Fri"] }}]"#
        ))
    }

    fn hours() -> Hours {
        "7:00-21:00".parse().unwrap()
    }

    fn begun(settings: &Settings, now: DateTime) -> Agenda {
        let mut agenda = Agenda::default();
        agenda.begin(settings, now);
        agenda
    }

    const MONDAY: Date = date(2026, 10, 19);

    #[test]
    fn picks_a_minute_within_the_hours() {
        let settings = music("");
        for seed in 0..50 {
            fastrand::seed(seed);
            let agenda = begun(&settings, MONDAY.at(6, 0, 0, 0));
            let at = agenda.pick("Music?", hours(), MONDAY).unwrap();
            assert!(
                at >= MONDAY.at(7, 0, 0, 0) && at < MONDAY.at(21, 0, 0, 0),
                "{at}"
            );
            assert_eq!((at.second(), at.subsec_nanosecond()), (0, 0));
        }
    }

    #[test]
    fn a_late_start_picks_from_what_is_left() {
        let settings = music("");
        for seed in 0..50 {
            fastrand::seed(seed);
            let agenda = begun(&settings, MONDAY.at(15, 0, 30, 0));
            let at = agenda.pick("Music?", hours(), MONDAY).unwrap();
            assert!(at > MONDAY.at(15, 0, 30, 0), "{at}");
        }
        let agenda = begun(&settings, MONDAY.at(20, 59, 0, 0));
        assert_eq!(agenda.pick("Music?", hours(), MONDAY), None);
    }

    #[test]
    fn keeps_its_pick_for_the_rest_of_the_day() {
        let settings = music("");
        let mut agenda = begun(&settings, MONDAY.at(6, 0, 0, 0));
        let picked = agenda.pick("Music?", hours(), MONDAY);
        for seed in 0..10 {
            fastrand::seed(seed);
            agenda.begin(&settings, MONDAY.at(18, 0, 0, 0));
            assert_eq!(agenda.pick("Music?", hours(), MONDAY), picked);
        }
        assert_eq!(agenda.started, Some(MONDAY.at(6, 0, 0, 0)));
        let tuesday = MONDAY.tomorrow().unwrap();
        agenda.begin(&settings, tuesday.at(0, 0, 0, 0));
        assert_eq!(agenda.pick("Music?", hours(), MONDAY), None);
        assert!(agenda.pick("Music?", hours(), tuesday).is_some());
    }

    #[test]
    fn leaves_out_quiet_hours() {
        let settings = music(r#"quiet_hours = ["0:00-20:59"]"#);
        let agenda = begun(&settings, MONDAY.at(6, 0, 0, 0));
        assert_eq!(
            agenda.pick("Music?", hours(), MONDAY),
            Some(MONDAY.at(20, 59, 0, 0))
        );
    }

    #[test]
    fn picks_only_on_its_days() {
        let settings = music("");
        let saturday = date(2026, 10, 24);
        assert_eq!(saturday.weekday(), Weekday::Saturday);
        let agenda = begun(&settings, saturday.at(6, 0, 0, 0));
        assert_eq!(agenda.pick("Music?", hours(), saturday), None);
    }

    #[test]
    fn days_go_their_own_way() {
        let settings = music("");
        fastrand::seed(7);
        let mut agenda = Agenda::default();
        let picks: BTreeSet<_> = (0..5)
            .map(|offset| {
                let day = MONDAY.checked_add(offset.days()).unwrap();
                agenda.begin(&settings, day.at(0, 0, 0, 0));
                agenda.pick("Music?", hours(), day).unwrap().time()
            })
            .collect();
        assert!(picks.len() > 1);
    }
}
