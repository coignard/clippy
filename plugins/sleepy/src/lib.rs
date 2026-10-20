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

#![forbid(unsafe_code)]

use std::convert::Infallible;

use clippy::hours::Hours;
use clippy::{Context, Plugin};
use jiff::civil::DateTime;
use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
pub struct Sleepy;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    sleep_hours: Vec<Hours>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sleep_hours: vec![
                "22:00-7:00"
                    .parse()
                    .expect("the default sleep hours should be valid"),
            ],
        }
    }
}

impl Settings {
    fn is_sleep_time(&self, now: DateTime) -> bool {
        self.sleep_hours
            .iter()
            .any(|hours| hours.contains(now.time()))
    }
}

impl Plugin for Sleepy {
    const NAME: &'static str = "Sleepy";

    type Action = Infallible;

    type Settings = Settings;

    fn bedtime(&self, settings: &Settings, now: DateTime) -> bool {
        settings.is_sleep_time(now)
    }

    fn changes_at(&self, settings: &Settings, now: DateTime) -> Option<DateTime> {
        settings
            .sleep_hours
            .iter()
            .flat_map(|hours| [hours.start_after(now), hours.end_after(now)])
            .min()
    }

    fn perform(&mut self, action: Infallible, _cx: &mut Context<'_, Self>) {
        match action {}
    }
}

#[cfg(test)]
mod tests {
    use clippy::testing::Harness;
    use jiff::civil::{Date, date};

    use super::*;

    const DAY: Date = date(2026, 10, 19);

    fn with_hours(sleep_hours: &str) -> Harness {
        let settings = toml::from_str(&format!("sleep_hours = {sleep_hours}")).unwrap();
        Harness::with_settings(Sleepy, settings)
    }

    fn asleep_at(sleepy: &mut Harness, hour: i8, minute: i8) -> bool {
        sleepy.set_time(DAY.at(hour, minute, 0, 0));
        sleepy.bedtime()
    }

    #[test]
    fn sleeps_through_the_night_by_default() {
        let mut sleepy = Harness::new(Sleepy);
        assert!(!asleep_at(&mut sleepy, 21, 59));
        assert!(asleep_at(&mut sleepy, 22, 0));
        assert!(asleep_at(&mut sleepy, 23, 30));
        assert!(!asleep_at(&mut sleepy, 12, 0));
        let mut sleepy = Harness::new(Sleepy);
        assert!(asleep_at(&mut sleepy, 6, 59));
        assert!(!asleep_at(&mut sleepy, 7, 0));
    }

    #[test]
    fn keeps_to_every_sleep_hours() {
        let mut sleepy = with_hours(r#"["13:00-14:00", "23:30-6:30"]"#);
        assert!(asleep_at(&mut sleepy, 13, 15));
        assert!(!asleep_at(&mut sleepy, 14, 0));
        assert!(!asleep_at(&mut sleepy, 22, 0));
        assert!(asleep_at(&mut sleepy, 23, 45));
        let mut sleepy = with_hours(r#"["13:00-14:00", "23:30-6:30"]"#);
        assert!(asleep_at(&mut sleepy, 6, 0));
        assert!(!asleep_at(&mut sleepy, 6, 30));
    }

    #[test]
    fn never_sleeps_without_sleep_hours() {
        let mut sleepy = with_hours("[]");
        assert!(!asleep_at(&mut sleepy, 23, 0));
        assert_eq!(sleepy.next_change(), None);
    }

    #[test]
    fn wakes_the_app_when_sleep_hours_begin_and_end() {
        let mut sleepy = with_hours(r#"["13:00-14:00", "22:00-7:00"]"#);
        sleepy.set_time(DAY.at(9, 0, 0, 0));
        assert_eq!(sleepy.next_change(), Some(DAY.at(13, 0, 0, 0)));
        sleepy.set_time(DAY.at(13, 30, 0, 0));
        assert_eq!(sleepy.next_change(), Some(DAY.at(14, 0, 0, 0)));
        sleepy.set_time(DAY.at(23, 0, 0, 0));
        assert_eq!(
            sleepy.next_change(),
            Some(DAY.tomorrow().unwrap().at(7, 0, 0, 0))
        );
    }

    #[test]
    fn writes_and_reads_its_settings() {
        let written = toml::to_string(&Settings::default()).unwrap();
        assert_eq!(written, "sleep_hours = [\"22:00-07:00\"]\n");
        assert_eq!(
            toml::from_str::<Settings>(&written).unwrap(),
            Settings::default()
        );
        assert_eq!(toml::from_str::<Settings>("").unwrap(), Settings::default());
        for odd in [
            r#"sleep_hours = ["22:00"]"#,
            r#"sleep_hours = ["7:00-7:00"]"#,
            r#"bedtime = "22:00""#,
        ] {
            assert!(toml::from_str::<Settings>(odd).is_err(), "{odd}");
        }
    }
}
