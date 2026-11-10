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

use crate::days::Days;
use crate::settings::{Reminder, Schedule, Settings};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Agenda {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) started: Option<DateTime>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    picks: Vec<Pick>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    weeks: Vec<Week>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Pick {
    text: String,
    hours: Hours,
    at: DateTime,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Week {
    text: String,
    from: Days,
    count: u8,
    monday: Date,
    days: Vec<Date>,
}

impl Agenda {
    #[cfg(test)]
    pub(crate) fn starting(started: Option<DateTime>) -> Self {
        Self {
            started,
            picks: Vec::new(),
            weeks: Vec::new(),
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
        let monday = monday_of(now.date());
        self.weeks.retain(|week| week.monday == monday);
        for reminder in &settings.reminders {
            if let Some(count) = reminder.random_days
                && self.week(reminder).is_none()
            {
                self.weeks.push(Week {
                    text: reminder.key(),
                    from: reminder.days,
                    count,
                    monday,
                    days: random_days(reminder.days, count, now.date()),
                });
            }
        }
        for reminder in &settings.reminders {
            let Schedule::Random(spans) = &reminder.schedule else {
                continue;
            };
            if !self.is_on(reminder, now.date()) {
                continue;
            }
            for &hours in spans {
                if self.pick(&reminder.key(), hours, now.date()).is_none()
                    && let Some(at) = random_time(settings, hours, now.date())
                {
                    self.picks.push(Pick {
                        text: reminder.key(),
                        hours,
                        at,
                    });
                }
            }
        }
    }

    pub(crate) fn is_on(&self, reminder: &Reminder, day: Date) -> bool {
        match reminder.random_days {
            Some(_) => self
                .week(reminder)
                .is_some_and(|week| week.days.contains(&day)),
            None => reminder.days.contains(day.weekday()),
        }
    }

    fn week(&self, reminder: &Reminder) -> Option<&Week> {
        self.weeks.iter().find(|week| {
            week.text == reminder.key()
                && week.from == reminder.days
                && Some(week.count) == reminder.random_days
        })
    }

    pub(crate) fn pick(&self, text: &str, hours: Hours, day: Date) -> Option<DateTime> {
        self.picks
            .iter()
            .find(|pick| pick.text == text && pick.hours == hours && pick.at.date() == day)
            .map(|pick| pick.at)
    }
}

fn random_days(from: Days, count: u8, today: Date) -> Vec<Date> {
    let mut left: Vec<Date> = monday_of(today)
        .series(1.day())
        .take(7)
        .filter(|&day| day >= today && from.contains(day.weekday()))
        .collect();
    fastrand::shuffle(&mut left);
    left.truncate(count.into());
    left.sort_unstable();
    left
}

pub(crate) fn monday_of(day: Date) -> Date {
    let since = i64::from(day.weekday().to_monday_zero_offset());
    day.checked_sub(since.days()).unwrap_or(day)
}

fn random_time(settings: &Settings, hours: Hours, day: Date) -> Option<DateTime> {
    let end = day.to_datetime(hours.end().time());
    let free: Vec<DateTime> = day
        .to_datetime(hours.start().time())
        .series(1.minute())
        .take_while(|&at| at < end)
        .filter(|&at| !settings.is_quiet(at.time()))
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
    fn a_late_start_still_picks_from_all_the_hours() {
        let settings = music("");
        let mut earlier = false;
        for seed in 0..50 {
            fastrand::seed(seed);
            let agenda = begun(&settings, MONDAY.at(15, 0, 30, 0));
            let at = agenda.pick("Music?", hours(), MONDAY).unwrap();
            assert!(
                at >= MONDAY.at(7, 0, 0, 0) && at < MONDAY.at(21, 0, 0, 0),
                "{at}"
            );
            earlier |= at < MONDAY.at(15, 0, 0, 0);
        }
        assert!(earlier, "some picks should fall before the start");
        let agenda = begun(&settings, MONDAY.at(23, 0, 0, 0));
        assert!(agenda.pick("Music?", hours(), MONDAY).is_some());
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

    const NO_DAYS: [Date; 0] = [];

    fn friend(extra: &str) -> Settings {
        settings(&format!(
            r#"reminders = [{{ text = "Call a friend.", random = "10:00-20:00", random_days = 2{extra} }}]"#
        ))
    }

    fn picked_days(agenda: &Agenda, settings: &Settings, monday: Date) -> Vec<Date> {
        monday
            .series(1.day())
            .take(7)
            .filter(|&day| agenda.is_on(&settings.reminders[0], day))
            .collect()
    }

    #[test]
    fn picks_that_many_days_of_the_week_within_its_days() {
        let anywhere = friend("");
        let weekends = friend(r#", days = ["Sat", "Sun"]"#);
        let mut seen = BTreeSet::new();
        for seed in 0..50 {
            fastrand::seed(seed);
            let agenda = begun(&anywhere, MONDAY.at(0, 0, 0, 0));
            let days = picked_days(&agenda, &anywhere, MONDAY);
            assert_eq!(days.len(), 2, "{days:?}");
            seen.extend(days.iter().map(|day| day.weekday().to_monday_zero_offset()));
            let agenda = begun(&weekends, MONDAY.at(0, 0, 0, 0));
            let days = picked_days(&agenda, &weekends, MONDAY);
            assert_eq!(days, [date(2026, 10, 24), date(2026, 10, 25)]);
        }
        assert_eq!(seen.len(), 7, "{seen:?}");
    }

    #[test]
    fn keeps_the_days_for_the_week_and_picks_again_the_next() {
        let settings = friend("");
        let mut agenda = begun(&settings, MONDAY.at(0, 0, 0, 0));
        let week = picked_days(&agenda, &settings, MONDAY);
        let restored: Agenda =
            serde_json::from_value(serde_json::to_value(&agenda).unwrap()).unwrap();
        assert_eq!(restored, agenda);
        for (offset, seed) in (1..7).zip(10..) {
            fastrand::seed(seed);
            let day = MONDAY.checked_add(offset.days()).unwrap();
            agenda.begin(&settings, day.at(0, 0, 0, 0));
            assert_eq!(picked_days(&agenda, &settings, MONDAY), week);
        }
        let weeks: BTreeSet<Vec<i8>> = (1..20)
            .map(|week| {
                let monday = MONDAY.checked_add((7 * week).days()).unwrap();
                agenda.begin(&settings, monday.at(0, 0, 0, 0));
                assert_eq!(picked_days(&agenda, &settings, MONDAY), NO_DAYS);
                let days = picked_days(&agenda, &settings, monday);
                assert_eq!(days.len(), 2);
                days.iter()
                    .map(|day| day.weekday().to_monday_zero_offset())
                    .collect()
            })
            .collect();
        assert!(weeks.len() > 1);
    }

    #[test]
    fn a_late_start_in_the_week_picks_from_the_days_left() {
        let settings = friend("");
        let thursday = date(2026, 10, 22);
        for seed in 0..20 {
            fastrand::seed(seed);
            let agenda = begun(&settings, thursday.at(12, 0, 0, 0));
            let days = picked_days(&agenda, &settings, MONDAY);
            assert_eq!(days.len(), 2);
            assert!(days.iter().all(|&day| day >= thursday), "{days:?}");
        }
        let settings = friend(r#", days = ["Mon", "Tue"]"#);
        let agenda = begun(&settings, thursday.at(12, 0, 0, 0));
        assert_eq!(picked_days(&agenda, &settings, MONDAY), NO_DAYS);
    }

    #[test]
    fn picks_a_random_time_only_on_the_days_picked() {
        let settings = friend("");
        let mut agenda = begun(&settings, MONDAY.at(0, 0, 0, 0));
        let hours: Hours = "10:00-20:00".parse().unwrap();
        for day in MONDAY.series(1.day()).take(7) {
            agenda.begin(&settings, day.at(0, 0, 0, 0));
            let picked = agenda.pick("Call a friend.", hours, day);
            assert_eq!(
                picked.is_some(),
                agenda.is_on(&settings.reminders[0], day),
                "{day}"
            );
        }
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
