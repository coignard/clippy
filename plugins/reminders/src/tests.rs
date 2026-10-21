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

use clippy::testing::Harness;
use jiff::ToSpan;
use jiff::civil::{Date, date};

use super::*;
use crate::interval::Interval;
use crate::settings::Snoozes;

const DAY: Date = date(2026, 10, 19);
const STRETCH: &str = "Stand up and stretch.";
const WATER: &str = "Have a glass of water.";
const AGAIN: &str = "Don't show me this reminder again";
const NOTHING: [&str; 0] = [];

fn at(hour: i8, minute: i8) -> DateTime {
    DAY.at(hour, minute, 0, 0)
}

fn next_day(hour: i8, minute: i8) -> DateTime {
    DAY.tomorrow().unwrap().at(hour, minute, 0, 0)
}

fn settings(text: &str) -> Settings {
    toml::from_str(text).unwrap()
}

fn habits() -> Settings {
    settings(&format!(
        r#"reminders = [
            {{ text = "{STRETCH}", time = "10:00" }},
            {{ text = "{WATER}", time = "13:30" }},
        ]"#
    ))
}

fn hourly() -> Settings {
    settings(&format!(
        r#"reminders = [{{ text = "{WATER}", every = "1h", from = "9:00", until = "17:00" }}]"#
    ))
}

fn reminders(settings: Settings, now: DateTime) -> Harness {
    let mut harness = Harness::with_settings(Reminders::default(), settings);
    harness.set_time(now);
    harness
}

fn until(time: &str) -> String {
    format!("Don't show me this reminder until {time}")
}

#[test]
fn reminds_on_time_with_a_way_out() {
    let mut harness = reminders(habits(), at(9, 59));
    assert_eq!(harness.notices(), NOTHING);
    assert_eq!(harness.next_change(), Some(at(10, 0)));
    harness.set_time(at(10, 0));
    assert_eq!(harness.notices(), [STRETCH]);
    assert_eq!(harness.checkbox(STRETCH).as_deref(), Some(AGAIN));
    assert!(harness.shows_over_full_screen(STRETCH));
    assert_eq!(
        harness.shortcut(STRETCH),
        Some(Settings::default().hide_shortcut)
    );
    assert_eq!(harness.next_change(), Some(at(13, 30)));
}

#[test]
fn snoozes_stretch_from_fifteen_minutes_to_an_hour() {
    let mut harness = reminders(habits(), at(10, 0));
    let ladder = [
        (at(10, 0), at(10, 15)),
        (at(10, 15), at(10, 45)),
        (at(10, 45), at(11, 45)),
        (at(11, 45), at(12, 45)),
    ];
    for (snoozed, back) in ladder {
        harness.set_time(snoozed);
        harness.snooze(STRETCH);
        assert_eq!(harness.notices(), NOTHING);
        assert_eq!(harness.next_change(), Some(back));
    }
}

#[test]
fn the_snooze_ladder_comes_from_the_settings() {
    let mut settings = habits();
    settings.snooze = Snoozes::new(vec![Interval::minutes(5), Interval::minutes(120)]).unwrap();
    let mut harness = reminders(settings, at(10, 0));
    harness.snooze(STRETCH);
    assert_eq!(harness.next_change(), Some(at(10, 5)));
    harness.set_time(at(10, 5));
    harness.snooze(STRETCH);
    assert_eq!(harness.next_change(), Some(at(12, 5)));
}

#[test]
fn snoozes_count_from_the_double_click() {
    let mut harness = reminders(habits(), at(10, 20));
    harness.snooze(STRETCH);
    assert_eq!(harness.next_change(), Some(at(10, 35)));
    harness.set_time(at(10, 34));
    assert_eq!(harness.notices(), NOTHING);
    harness.set_time(at(10, 35));
    assert_eq!(harness.notices(), [STRETCH]);
}

#[test]
fn ticking_the_box_settles_a_daily_reminder_for_the_day() {
    let mut harness = reminders(habits(), at(10, 0));
    harness.check(STRETCH);
    assert_eq!(harness.notices(), NOTHING);
    harness.set_time(at(23, 59));
    assert_eq!(harness.notices(), [WATER]);
    harness.set_time(next_day(10, 0));
    assert_eq!(harness.notices(), [STRETCH]);
}

#[test]
fn a_new_day_starts_the_snoozes_afresh() {
    let mut harness = reminders(habits(), at(10, 0));
    for snoozed in [at(10, 0), at(10, 15), at(10, 45)] {
        harness.set_time(snoozed);
        harness.snooze(STRETCH);
    }
    harness.set_time(next_day(10, 0));
    harness.snooze(STRETCH);
    assert_eq!(harness.next_change(), Some(next_day(10, 15)));
}

#[test]
fn missed_reminders_wait_for_the_app_earliest_first() {
    let settings = settings(&format!(
        r#"reminders = [
            {{ text = "{WATER}", time = "13:30" }},
            {{ text = "{STRETCH}", time = "10:00" }},
        ]"#
    ));
    let harness = reminders(settings, at(15, 0));
    assert_eq!(harness.notices(), [STRETCH, WATER]);
}

#[test]
fn yesterday_is_forgotten() {
    let mut harness = reminders(habits(), at(23, 50));
    assert_eq!(harness.notices(), [STRETCH, WATER]);
    harness.snooze(STRETCH);
    assert_eq!(harness.next_change(), Some(next_day(0, 0)));
    harness.set_time(next_day(0, 5));
    assert_eq!(harness.notices(), NOTHING);
    assert_eq!(harness.next_change(), Some(next_day(10, 0)));
}

#[test]
fn hourly_reminders_name_the_hour_they_come_back() {
    let mut harness = reminders(hourly(), at(8, 30));
    assert_eq!(harness.next_change(), Some(at(9, 0)));
    harness.set_time(at(10, 20));
    assert_eq!(harness.notices(), [WATER]);
    assert_eq!(harness.checkbox(WATER), Some(until("11:00")));
    harness.check(WATER);
    assert_eq!(harness.notices(), NOTHING);
    assert_eq!(harness.next_change(), Some(at(11, 0)));
    harness.set_time(at(11, 0));
    assert_eq!(harness.notices(), [WATER]);
}

#[test]
fn the_last_hourly_reminder_of_the_day_says_so() {
    let mut harness = reminders(hourly(), at(17, 0));
    assert_eq!(harness.checkbox(WATER).as_deref(), Some(AGAIN));
    harness.check(WATER);
    harness.set_time(at(23, 0));
    assert_eq!(harness.notices(), NOTHING);
    harness.set_time(next_day(9, 0));
    assert_eq!(harness.notices(), [WATER]);
}

#[test]
fn snoozing_a_repeating_reminder_folds_the_hours_it_skips() {
    let settings = settings(&format!(
        r#"
        snooze = ["1h"]
        reminders = [{{ text = "{WATER}", every = "30m", from = "9:00" }}]
        "#
    ));
    let mut harness = reminders(settings, at(9, 0));
    harness.snooze(WATER);
    for quiet in [at(9, 30), at(9, 59)] {
        harness.set_time(quiet);
        assert_eq!(harness.notices(), NOTHING);
    }
    harness.set_time(at(10, 0));
    assert_eq!(harness.notices(), [WATER]);
    assert_eq!(harness.checkbox(WATER), Some(until("10:30")));
}

#[test]
fn reminders_without_a_start_count_from_when_the_day_began() {
    let settings = settings(&format!(
        r#"reminders = [{{ text = "{WATER}", every = "1h" }}]"#
    ));
    let mut harness = reminders(settings, at(9, 7));
    assert_eq!(harness.next_change(), Some(at(10, 7)));
    harness.set_time(at(9, 40));
    harness.relaunch();
    assert_eq!(harness.next_change(), Some(at(10, 7)));
    harness.set_time(at(10, 7));
    assert_eq!(harness.notices(), [WATER]);
    harness.set_time(next_day(8, 15));
    assert_eq!(harness.notices(), NOTHING);
    assert_eq!(harness.next_change(), Some(next_day(9, 15)));
}

#[test]
fn quiet_hours_hold_reminders_until_they_end() {
    let mut settings = habits();
    settings.quiet_hours = vec![
        "9:30-10:30".parse().unwrap(),
        "13:00-14:00".parse().unwrap(),
    ];
    let mut harness = reminders(settings, at(10, 0));
    assert_eq!(harness.notices(), NOTHING);
    assert_eq!(harness.next_change(), Some(at(10, 30)));
    harness.set_time(at(10, 30));
    assert_eq!(harness.notices(), [STRETCH]);
    harness.set_time(at(13, 0));
    assert_eq!(harness.notices(), NOTHING);
    harness.set_time(at(14, 0));
    assert_eq!(harness.notices(), [STRETCH, WATER]);
}

#[test]
fn the_checkbox_counts_quiet_hours_in() {
    let mut settings = hourly();
    settings.quiet_hours = vec![
        "11:00-12:30".parse().unwrap(),
        "16:30-9:00".parse().unwrap(),
    ];
    let mut harness = reminders(settings, at(10, 0));
    assert_eq!(harness.checkbox(WATER), Some(until("12:30")));
    harness.set_time(at(16, 0));
    assert_eq!(harness.checkbox(WATER).as_deref(), Some(AGAIN));
}

#[test]
fn reminders_keep_to_their_days() {
    let settings = settings(&format!(
        r#"reminders = [{{ text = "{STRETCH}", time = "10:00", days = ["Sat", "Sun"] }}]"#
    ));
    let mut harness = reminders(settings, at(11, 0));
    assert_eq!(harness.notices(), NOTHING);
    harness.set_time(date(2026, 10, 24).at(10, 0, 0, 0));
    assert_eq!(harness.notices(), [STRETCH]);
}

#[test]
fn the_same_text_twice_is_one_habit_with_two_times() {
    let settings = settings(&format!(
        r#"reminders = [
            {{ text = "{WATER}", time = "10:00" }},
            {{ text = "{WATER}", every = "2h", from = "13:00" }},
        ]"#
    ));
    let mut harness = reminders(settings, at(10, 0));
    assert_eq!(harness.notices(), [WATER]);
    assert_eq!(harness.checkbox(WATER), Some(until("13:00")));
    harness.check(WATER);
    harness.set_time(at(13, 0));
    assert_eq!(harness.notices(), [WATER]);
}

#[test]
fn reminders_can_be_paused_from_the_menu() {
    let mut harness = reminders(habits(), at(10, 0));
    assert_eq!(
        harness.menu(),
        [
            ("Pause for an Hour".to_owned(), true),
            ("Pause Until Tomorrow".to_owned(), true)
        ]
    );
    harness.choose("Pause for an Hour");
    assert_eq!(harness.notices(), NOTHING);
    assert_eq!(
        harness.menu(),
        [
            ("Paused Until 11:00".to_owned(), false),
            ("Resume Reminders".to_owned(), true)
        ]
    );
    assert_eq!(harness.next_change(), Some(at(11, 0)));
    harness.choose("Resume Reminders");
    assert_eq!(harness.notices(), [STRETCH]);
    harness.choose("Pause Until Tomorrow");
    assert_eq!(
        harness.menu()[0],
        ("Paused Until Tomorrow".to_owned(), false)
    );
    harness.set_time(at(23, 59));
    assert_eq!(harness.notices(), NOTHING);
    harness.set_time(next_day(10, 0));
    assert_eq!(harness.notices(), [STRETCH]);
    assert_eq!(harness.menu()[0].0, "Pause for an Hour");
}

#[test]
fn an_hour_paused_late_in_the_evening_ends_at_its_hour() {
    let mut harness = reminders(habits(), at(23, 30));
    harness.choose("Pause for an Hour");
    assert_eq!(harness.menu()[0], ("Paused Until 00:30".to_owned(), false));
}

#[test]
fn every_occurrence_starts_its_own_snooze_ladder() {
    let mut harness = reminders(hourly(), at(9, 0));
    harness.snooze(WATER);
    harness.set_time(at(9, 15));
    harness.snooze(WATER);
    assert_eq!(harness.next_change(), Some(at(9, 45)));
    harness.set_time(at(11, 0));
    harness.snooze(WATER);
    assert_eq!(harness.next_change(), Some(at(11, 15)));
}

#[test]
fn a_new_day_clears_what_yesterday_left_behind() {
    let mut reminders = Reminders {
        paused_until: Some(at(11, 0)),
        ..Reminders::default()
    };
    reminders
        .progress
        .entry(STRETCH.to_owned())
        .or_default()
        .snooze(at(10, 0), at(10, 15));
    reminders.begin_day(&habits(), next_day(9, 0));
    assert_eq!(reminders.paused_until, None);
    assert!(reminders.progress.is_empty());
}

#[test]
fn quiet_hours_all_day_never_promise_a_time() {
    let mut settings = habits();
    settings.quiet_hours = vec!["0:00-12:00".parse().unwrap(), "12:00-0:00".parse().unwrap()];
    let reminders = Reminders::default();
    let habits = super::habits(&settings);
    assert_eq!(
        reminders.next_showing(&settings, &habits[0], at(9, 0)),
        None
    );
}

#[test]
fn without_reminders_there_is_nothing_to_pause() {
    let harness = reminders(Settings::default(), at(10, 0));
    assert_eq!(harness.menu(), []);
}

#[test]
fn reminders_can_wait_for_full_screen_to_end() {
    let mut settings = habits();
    settings.show_in_full_screen = false;
    let harness = reminders(settings, at(10, 0));
    assert!(!harness.shows_over_full_screen(STRETCH));
}

#[test]
fn progress_outlives_restarts_as_readable_json() {
    let mut progress = Reminders::default();
    progress.begin_day(&hourly(), at(9, 0));
    progress
        .progress
        .entry(WATER.to_owned())
        .or_default()
        .snooze(at(10, 0), at(10, 15));
    let saved = serde_json::to_value(&progress).unwrap();
    assert_eq!(
        saved,
        serde_json::json!({
            "started": "2026-10-19T09:00:00",
            "progress": {
                WATER: {
                    "snoozed": {
                        "times": 1,
                        "occurrence": "2026-10-19T10:00:00",
                        "until": "2026-10-19T10:15:00"
                    }
                }
            }
        })
    );
    let restored: Reminders = serde_json::from_value(saved).unwrap();
    let mut harness = Harness::with_settings(restored, hourly());
    harness.set_time(at(10, 10));
    assert_eq!(harness.notices(), NOTHING);
    harness.set_time(at(10, 15));
    harness.snooze(WATER);
    assert_eq!(harness.next_change(), Some(at(10, 45)));
}

const MUSIC: &str = "How about some music?";

fn music() -> Settings {
    settings(&format!(
        r#"reminders = [{{ text = "{MUSIC}", random = "7:00-21:00", days = ["Mon", "Tue", "Wed", "Thu", "Fri"] }}]"#
    ))
}

#[test]
fn a_random_reminder_comes_once_at_its_minute_and_stays_put() {
    fastrand::seed(3);
    let mut harness = reminders(music(), at(6, 0));
    let picked = harness.next_change().unwrap();
    assert!(picked >= at(7, 0) && picked < at(21, 0), "{picked}");
    harness.set_time(picked.checked_sub(1.minute()).unwrap());
    assert_eq!(harness.notices(), NOTHING);
    harness.relaunch();
    assert_eq!(harness.next_change(), Some(picked));
    harness.set_time(picked);
    assert_eq!(harness.notices(), [MUSIC]);
    assert_eq!(harness.checkbox(MUSIC).as_deref(), Some(AGAIN));
    harness.check(MUSIC);
    assert_eq!(harness.notices(), NOTHING);
    assert_eq!(harness.next_change(), Some(next_day(0, 0)));
}

#[test]
fn a_random_reminder_skips_a_day_that_starts_after_its_hours() {
    let mut harness = reminders(music(), at(21, 0));
    assert_eq!(harness.notices(), NOTHING);
    assert_eq!(harness.next_change(), Some(next_day(0, 0)));
    harness.set_time(next_day(6, 0));
    let picked = harness.next_change().unwrap();
    assert_eq!(picked.date(), next_day(0, 0).date());
    assert!(picked >= next_day(7, 0));
}
