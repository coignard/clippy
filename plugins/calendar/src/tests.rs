use clippy::Urgency;
use clippy::shortcut::{Key, Modifiers, Shortcut};
use clippy::testing::Harness;
use jiff::civil::date;

use super::*;

const DAY: Date = date(2026, 10, 19);
const NOTHING: [&str; 0] = [];
const QUIET: &str = "daily_summary = false\n";

fn at(hour: i8, minute: i8) -> DateTime {
    DAY.at(hour, minute, 0, 0)
}

fn at_second(hour: i8, minute: i8, second: i8) -> DateTime {
    DAY.at(hour, minute, second, 0)
}

fn day_after(days: i64, hour: i8, minute: i8) -> DateTime {
    DAY.checked_add(jiff::Span::new().days(days))
        .unwrap()
        .at(hour, minute, 0, 0)
}

fn event(id: &str, title: &str, start: DateTime, minutes: i64) -> Appointment {
    Appointment {
        id: id.to_owned(),
        calendar: "Work".to_owned(),
        title: title.to_owned(),
        start,
        end: start.saturating_add(SignedDuration::from_mins(minutes)),
        all_day: false,
    }
}

fn all_day(id: &str, title: &str) -> Appointment {
    Appointment {
        all_day: true,
        ..event(id, title, at(0, 0), 24 * 60)
    }
}

fn titles(harness: &Harness) -> Vec<String> {
    harness.menu().into_iter().map(|(title, _)| title).collect()
}

fn calendar(settings: &str, appointments: Vec<Appointment>, now: DateTime) -> Harness {
    let mut harness =
        Harness::with_settings(Calendar::default(), toml::from_str(settings).unwrap());
    harness.read_agenda(&Agenda::Read(appointments));
    harness.set_time(now);
    harness
}

#[test]
fn reminds_at_every_lead_time_until_dismissed() {
    let settings =
        format!("{QUIET}remind_before = [\"10m\", \"0m\"]\nremind_with = \"start_time\"");
    let standup = event("standup", "Standup", at(10, 0), 30);
    let mut harness = calendar(&settings, vec![standup], at(9, 49));
    assert_eq!(harness.notices(), NOTHING);
    assert_eq!(harness.next_change(), Some(at(9, 50)));
    harness.set_time(at(9, 50));
    assert_eq!(harness.notices(), ["Standup\nStarts at 10:00"]);
    assert_eq!(
        harness.checkbox("Standup\nStarts at 10:00").as_deref(),
        Some(AGAIN)
    );
    assert!(harness.shows_over_full_screen("Standup\nStarts at 10:00"));
    assert_eq!(
        harness.urgency("Standup\nStarts at 10:00"),
        Urgency::Pressing
    );
    harness.snooze("Standup\nStarts at 10:00");
    assert_eq!(harness.notices(), NOTHING);
    assert_eq!(harness.next_change(), Some(at(10, 0)));
    harness.set_time(at(10, 0));
    assert_eq!(harness.notices(), ["Standup\nStarted at 10:00"]);
    assert_eq!(harness.checkbox("Standup\nStarted at 10:00"), None);
    harness.snooze("Standup\nStarted at 10:00");
    assert_eq!(harness.notices(), NOTHING);
}

#[test]
fn everything_it_shows_hides_with_the_shortcuts_from_the_settings() {
    let standup = event("standup", "Standup", at(9, 3), 15);
    let settings = r#"
        hide_shortcut = "Cmd+Option+H"
        check_shortcut = "Cmd+Option+J"
    "#;
    let harness = calendar(settings, vec![standup], at(9, 0));
    let shortcut = |key| Shortcut {
        modifiers: Modifiers::COMMAND.union(Modifiers::OPTION),
        key,
    };
    for text in harness.notices() {
        assert_eq!(harness.shortcut(&text), Some(shortcut(Key::H)), "{text}");
        assert_eq!(
            harness.check_shortcut(&text),
            Some(shortcut(Key::J)),
            "{text}"
        );
    }
}

#[test]
fn counts_down_in_minutes_then_seconds_then_says_now() {
    let standup = event("standup", "Standup", at(10, 0), 30);
    let mut harness = calendar(QUIET, vec![standup], at(9, 55));
    assert_eq!(harness.notices(), ["In 5 minutes:\nStandup"]);
    assert_eq!(harness.next_change(), Some(at(9, 56)));
    for (now, text, next) in [
        (at_second(9, 56, 30), "In 4 minutes:", at(9, 57)),
        (at_second(9, 58, 59), "In 2 minutes:", at(9, 59)),
        (at(9, 59), "In 1 minute:", at_second(9, 59, 1)),
        (at_second(9, 59, 1), "In 59 seconds:", at_second(9, 59, 2)),
        (at_second(9, 59, 59), "In 1 second:", at(10, 0)),
        (at(10, 0), "Now:", at(10, 30)),
    ] {
        harness.set_time(now);
        assert_eq!(harness.notices(), [format!("{text}\nStandup")], "{now}");
        assert_eq!(harness.next_change(), Some(next), "{now}");
    }
}

#[test]
fn counts_down_hours_and_names_the_day_further_ahead() {
    let settings = format!("{QUIET}remind_before = [\"48h\", \"90m\"]");
    let review = event("review", "Review", day_after(2, 10, 0), 60);
    let mut harness = calendar(&settings, vec![review], at(10, 0));
    assert_eq!(harness.notices(), ["On Wednesday at 10:00\nReview"]);
    assert_eq!(harness.next_change(), Some(day_after(1, 0, 0)));
    harness.set_time(day_after(1, 9, 59));
    assert_eq!(harness.notices(), ["Tomorrow at 10:00\nReview"]);
    assert_eq!(harness.next_change(), Some(day_after(1, 10, 0)));
    harness.set_time(day_after(1, 10, 0));
    assert_eq!(harness.notices(), ["In 24 hours:\nReview"]);
    harness.set_time(day_after(2, 8, 30));
    assert_eq!(harness.notices(), ["In 1 hour 30 minutes:\nReview"]);
    harness.set_time(day_after(2, 9, 0));
    assert_eq!(harness.notices(), ["In 1 hour:\nReview"]);
    harness.set_time(day_after(2, 9, 1));
    assert_eq!(harness.notices(), ["In 59 minutes:\nReview"]);
}

#[test]
fn a_dismissed_countdown_does_not_keep_clippy_awake() {
    let standup = event("standup", "Standup", at(10, 0), 30);
    let mut harness = calendar(QUIET, vec![standup], at(9, 55));
    harness.snooze("In 5 minutes:\nStandup");
    assert_eq!(harness.next_change(), Some(at(10, 0)));
}

#[test]
fn a_reminder_goes_away_by_itself_when_the_event_ends() {
    let mut harness = calendar(
        QUIET,
        vec![
            event("call", "Call", at(10, 0), 5),
            event("deadline", "Deadline", at(11, 0), 0),
        ],
        at(10, 4),
    );
    assert_eq!(harness.notices(), ["Now:\nCall"]);
    assert_eq!(harness.next_change(), Some(at(10, 5)));
    harness.set_time(at(10, 5));
    assert_eq!(harness.notices(), NOTHING);
    harness.set_time(at(11, 4));
    assert_eq!(harness.notices(), ["Now:\nDeadline"]);
    harness.set_time(at(11, 5));
    assert_eq!(harness.notices(), NOTHING);
}

#[test]
fn a_reminder_goes_away_when_its_event_is_deleted_or_moved() {
    let standup = event("standup", "Standup", at(10, 0), 30);
    let mut harness = calendar(QUIET, vec![standup.clone()], at(9, 55));
    harness.read_agenda(&Agenda::Read(Vec::new()));
    assert_eq!(harness.notices(), NOTHING);
    harness.read_agenda(&Agenda::Read(vec![standup.clone()]));
    assert_eq!(harness.notices(), ["In 5 minutes:\nStandup"]);
    let moved = Appointment {
        start: at(11, 0),
        end: at(11, 30),
        ..standup
    };
    harness.read_agenda(&Agenda::Read(vec![moved]));
    assert_eq!(harness.notices(), NOTHING);
    assert_eq!(harness.next_change(), Some(at(10, 55)));
    harness.read_agenda(&Agenda::Denied);
    assert_eq!(harness.notices(), NOTHING);
}

#[test]
fn ticking_the_box_silences_only_that_occurrence_of_a_repeating_event() {
    let settings = format!("{QUIET}remind_before = [\"10m\", \"0m\"]");
    let standups = vec![
        event("standup@1", "Standup", at(10, 0), 15),
        event("standup@2", "Standup", day_after(1, 10, 0), 15),
    ];
    let mut harness = calendar(&settings, standups, at(9, 50));
    harness.check("In 10 minutes:\nStandup");
    harness.set_time(at(10, 0));
    assert_eq!(harness.notices(), NOTHING);
    assert_eq!(harness.message().as_deref(), Some("Now:\nStandup"));
    harness.set_time(day_after(1, 9, 50));
    assert_eq!(harness.notices(), ["In 10 minutes:\nStandup"]);
}

#[test]
fn reminds_days_ahead_when_asked_to() {
    let settings =
        format!("{QUIET}remind_before = [\"48h\", \"24h\"]\nremind_with = \"start_time\"");
    let review = event("review", "Review", day_after(2, 10, 0), 60);
    let mut harness = calendar(&settings, vec![review], at(10, 0));
    assert_eq!(harness.notices(), ["Review\nStarts on Wednesday at 10:00"]);
    harness.set_time(day_after(1, 10, 0));
    assert_eq!(harness.notices(), ["Review\nStarts tomorrow at 10:00"]);
}

#[test]
fn leaves_out_all_day_events_ignored_calendars_and_what_began_yesterday() {
    let mut home = event("gym", "Gym", at(10, 0), 60);
    home.calendar = "Home".to_owned();
    let night = event("night", "Night shift", day_after(-1, 22, 0), 8 * 60);
    let mut harness = calendar(
        &format!("{QUIET}ignored_calendars = [\"home\"]"),
        vec![all_day("holiday", "Holiday"), home, night],
        at(1, 0),
    );
    assert_eq!(harness.notices(), NOTHING);
    harness.set_time(at(9, 55));
    assert_eq!(harness.notices(), NOTHING);
}

#[test]
fn summarises_the_rest_of_the_day_every_morning() {
    let appointments = vec![
        event("lunch", "Lunch\n  with Anna", at(13, 0), 60),
        event("breakfast", "Breakfast", at(8, 0), 30),
        event("standup", "  ", at(10, 0), 15),
        all_day("holiday", "Holiday"),
        event("conference", "Conference", day_after(-1, 9, 0), 3 * 24 * 60),
        event("night", "Night shift", day_after(-1, 22, 0), 12 * 60),
    ];
    let summary = "Today on your calendar:\n\
                   All day: Conference\n\
                   All day: Holiday\n\
                   Until 10:00: Night shift\n\
                   10:00 Untitled event\n\
                   13:00 Lunch with Anna";
    let mut harness = calendar("remind_before = []", appointments, at(8, 59));
    assert_eq!(harness.notices(), NOTHING);
    assert_eq!(harness.next_change(), Some(at(9, 0)));
    harness.set_time(at(9, 0));
    assert_eq!(harness.notices(), [summary]);
    assert!(!harness.shows_over_full_screen(summary));
    assert_eq!(harness.checkbox(summary), None);
    assert_eq!(harness.urgency(summary), Urgency::Normal);
    harness.snooze(summary);
    assert_eq!(harness.notices(), NOTHING);
    harness.choose("Show Today’s Events");
    assert_eq!(harness.notices(), [summary]);
    assert_eq!(harness.urgency(summary), Urgency::Asked);
}

#[test]
fn the_summary_can_be_turned_off_and_still_asked_for() {
    let lunch = event("lunch", "Lunch", at(13, 0), 60);
    let mut harness = calendar(
        "remind_before = []\ndaily_summary = false",
        vec![lunch],
        at(9, 0),
    );
    assert_eq!(harness.notices(), NOTHING);
    harness.choose("Show Today’s Events");
    assert_eq!(harness.notices(), ["Today on your calendar:\n13:00 Lunch"]);
    harness.set_time(at(14, 0));
    assert_eq!(harness.notices(), NOTHING);
    assert_eq!(
        harness.menu(),
        [
            ("Nothing Else Today".to_owned(), false),
            ("Show Today’s Events".to_owned(), false),
            ("Pause for an Hour".to_owned(), true),
            ("Pause Until Tomorrow".to_owned(), true),
        ]
    );
}

#[test]
fn the_summary_drops_a_short_event_as_soon_as_it_ends() {
    let appointments = vec![
        event("call", "Call", at(10, 0), 5),
        event("lunch", "Lunch", at(13, 0), 60),
    ];
    let mut harness = calendar("remind_before = []", appointments, at(10, 0));
    assert_eq!(harness.next_change(), Some(at(10, 5)));
    harness.set_time(at(10, 5));
    assert_eq!(harness.notices(), ["Today on your calendar:\n13:00 Lunch"]);
}

#[test]
fn a_busy_day_is_summarised_briefly() {
    let appointments = (0..10)
        .map(|hour| {
            event(
                &format!("{hour}"),
                &format!("Call {hour}"),
                at(10 + hour, 0),
                30,
            )
        })
        .collect();
    let harness = calendar("remind_before = []", appointments, at(9, 0));
    let summary = harness.notices().pop().unwrap();
    let lines: Vec<&str> = summary.lines().collect();
    assert_eq!(lines.len(), 1 + SUMMARY_LENGTH + 1);
    assert_eq!(lines[1], "10:00 Call 0");
    assert_eq!(lines.last(), Some(&"...and 2 more"));
}

#[test]
fn a_reminder_takes_over_from_the_summary() {
    let standup = event("standup", "Standup", at(10, 0), 15);
    let mut harness = calendar("", vec![standup], at(9, 0));
    let summary = "Today on your calendar:\n10:00 Standup";
    assert_eq!(harness.notices(), [summary]);
    harness.set_time(at(9, 55));
    assert_eq!(harness.notices(), ["In 5 minutes:\nStandup", summary]);
    assert_eq!(harness.urgency("In 5 minutes:\nStandup"), Urgency::Pressing);
    assert_eq!(harness.urgency(summary), Urgency::Normal);
}

#[test]
fn reminders_come_before_the_summary() {
    let standup = event("standup", "Standup", at(9, 3), 15);
    let harness = calendar("", vec![standup], at(9, 0));
    assert_eq!(
        harness.notices(),
        [
            "In 3 minutes:\nStandup",
            "Today on your calendar:\n09:03 Standup"
        ]
    );
}

#[test]
fn pausing_holds_everything_back_until_resumed() {
    let standup = event("standup", "Standup", at(10, 0), 60);
    let mut harness = calendar("", vec![standup], at(9, 55));
    harness.choose("Pause for an Hour");
    assert_eq!(harness.notices(), NOTHING);
    assert_eq!(harness.next_change(), Some(at(10, 0)));
    assert!(
        harness
            .menu()
            .contains(&("Paused Until 10:55".to_owned(), false))
    );
    harness.set_time(at(10, 55));
    assert_eq!(harness.notices().len(), 2);
    harness.choose("Pause Until Tomorrow");
    assert!(
        harness
            .menu()
            .contains(&("Paused Until Tomorrow".to_owned(), false))
    );
    assert_eq!(harness.notices(), NOTHING);
    harness.choose("Show Today’s Events");
    assert_eq!(
        harness.notices(),
        ["Today on your calendar:\n10:00 Standup"]
    );
    harness.choose("Resume Reminders");
    assert_eq!(harness.notices().len(), 2);
}

#[test]
fn the_menu_tells_what_comes_next() {
    let appointments = vec![
        all_day("holiday", "Holiday"),
        event("lunch", "Lunch", at(13, 0), 60),
        event("standup", "Standup", at(10, 0), 15),
    ];
    let mut harness = calendar(QUIET, appointments, at(9, 0));
    assert_eq!(
        harness.menu()[0],
        ("Next: Standup at 10:00".to_owned(), false)
    );
    harness.set_time(at(10, 0));
    assert_eq!(
        harness.menu()[..2],
        [
            ("Now: Standup until 10:15".to_owned(), false),
            ("Next: Lunch at 13:00".to_owned(), false)
        ]
    );
    harness.read_agenda(&Agenda::Denied);
    assert_eq!(
        harness.menu(),
        [("No Access to Calendars".to_owned(), false)]
    );
    assert_eq!(Harness::new(Calendar::default()).menu(), []);
}

#[test]
fn shows_what_is_going_on_until_it_is_over() {
    let appointments = vec![
        all_day("holiday", "Holiday"),
        event("standup", "Standup", at(10, 0), 30),
        event("review", "Review", at(10, 15), 45),
        event("deadline", "Deadline", at(10, 20), 0),
        event("night", "Night shift", day_after(-1, 22, 0), 11 * 60 + 30),
    ];
    let mut harness = calendar("remind_before = []", appointments, at(9, 0));
    assert_eq!(harness.message().as_deref(), Some("Now:\nNight shift"));
    harness.set_time(at(9, 59));
    assert_eq!(harness.message(), None);
    assert_eq!(harness.next_change(), Some(at(10, 0)));
    harness.set_time(at(10, 0));
    assert_eq!(harness.message().as_deref(), Some("Now:\nStandup"));
    harness.set_time(at(10, 20));
    assert_eq!(harness.message().as_deref(), Some("Now:\nStandup\nReview"));
    harness.set_time(at(10, 30));
    assert_eq!(harness.message().as_deref(), Some("Now:\nReview"));
    harness.set_time(at(11, 0));
    assert_eq!(harness.message(), None);
}

#[test]
fn a_title_with_nothing_to_draw_reads_as_untitled() {
    let party = event("party", "🎉 🥳", at(10, 0), 60);
    let harness = calendar(QUIET, vec![party], at(10, 0));
    assert_eq!(harness.message().as_deref(), Some("Now:\nUntitled event"));
    assert_eq!(harness.notices(), ["Now:\nUntitled event"]);
    let cake = event("cake", "🎂 Cake", at(10, 0), 60);
    let mut harness = calendar(QUIET, vec![cake], at(9, 0));
    assert_eq!(
        harness.menu()[0],
        ("Next: 🎂 Cake at 10:00".to_owned(), false)
    );
    harness.set_time(at(10, 0));
    assert_eq!(harness.message().as_deref(), Some("Now:\nCake"));
}

#[test]
fn the_box_shows_only_while_another_reminder_is_still_to_come() {
    let standup = event("standup", "Standup", at(10, 0), 30);
    let mut harness = calendar(QUIET, vec![standup.clone()], at(9, 55));
    assert_eq!(harness.checkbox("In 5 minutes:\nStandup"), None);
    let settings = format!("{QUIET}remind_before = [\"10m\", \"5m\"]");
    harness = calendar(&settings, vec![standup], at(9, 50));
    assert_eq!(
        harness.checkbox("In 10 minutes:\nStandup").as_deref(),
        Some(AGAIN)
    );
    harness.set_time(at(9, 55));
    assert_eq!(harness.checkbox("In 5 minutes:\nStandup"), None);
}

#[test]
fn hiding_a_reminder_once_the_event_is_on_hides_the_event_until_asked_back() {
    let settings = format!("{QUIET}remind_before = [\"0m\"]");
    let standup = event("standup", "Standup", at(10, 0), 30);
    let mut harness = calendar(&settings, vec![standup], at(10, 0));
    assert_eq!(harness.notices(), ["Now:\nStandup"]);
    assert_eq!(harness.checkbox("Now:\nStandup"), None);
    assert!(!titles(&harness).contains(&"Show Current Event".to_owned()));
    harness.snooze("Now:\nStandup");
    assert_eq!(harness.notices(), NOTHING);
    assert_eq!(harness.message(), None);
    harness.choose("Show Current Event");
    assert_eq!(harness.message().as_deref(), Some("Now:\nStandup"));
    assert_eq!(harness.notices(), NOTHING);
    assert!(!titles(&harness).contains(&"Show Current Event".to_owned()));
}

#[test]
fn what_is_going_on_hides_with_a_double_click_one_event_at_a_time() {
    let appointments = vec![
        event("standup", "Standup", at(10, 0), 30),
        event("review", "Review", at(10, 15), 30),
    ];
    let mut harness = calendar("remind_before = []", appointments, at(10, 0));
    harness.double_click();
    assert_eq!(harness.message(), None);
    assert!(titles(&harness).contains(&"Show Current Event".to_owned()));
    harness.set_time(at(10, 15));
    assert_eq!(harness.message().as_deref(), Some("Now:\nReview"));
    harness.double_click();
    assert!(titles(&harness).contains(&"Show Current Events".to_owned()));
    harness.choose("Show Current Events");
    assert_eq!(harness.message().as_deref(), Some("Now:\nStandup\nReview"));
}

#[test]
fn there_is_nothing_to_bring_back_without_the_current_event_shown() {
    let standup = event("standup", "Standup", at(10, 0), 30);
    let mut harness = calendar(
        &format!("{QUIET}remind_before = [\"0m\"]\nshow_current_event = false"),
        vec![standup],
        at(10, 0),
    );
    harness.snooze("Now:\nStandup");
    assert!(
        !titles(&harness)
            .iter()
            .any(|title| title.starts_with("Show Current"))
    );
}

#[test]
fn a_crowded_moment_is_shown_briefly() {
    let appointments = (0..5)
        .map(|index| {
            event(
                &format!("{index}"),
                &format!("Call {index}"),
                at(10, index),
                60,
            )
        })
        .collect();
    let harness = calendar("remind_before = []", appointments, at(10, 30));
    assert_eq!(
        harness.message().as_deref(),
        Some("Now:\nCall 0\nCall 1\nCall 2\n...and 2 more")
    );
}

#[test]
fn what_is_going_on_can_be_paused_or_turned_off() {
    let standup = event("standup", "Standup", at(10, 0), 30);
    let mut harness = calendar("remind_before = []", vec![standup.clone()], at(10, 0));
    harness.choose("Pause for an Hour");
    assert_eq!(harness.message(), None);
    harness.choose("Resume Reminders");
    assert_eq!(harness.message().as_deref(), Some("Now:\nStandup"));
    let harness = calendar(
        "remind_before = []\nshow_current_event = false",
        vec![standup],
        at(10, 0),
    );
    assert_eq!(harness.message(), None);
}
