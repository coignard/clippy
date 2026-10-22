use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::num::NonZeroU8;
use std::time::Duration;

use jiff::civil::{Date, date, time};
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp, Zoned};
use serde::{Deserialize, Serialize};
use tempfile::TempDir;

use super::*;
use crate::assets::ANIMATIONS;
use crate::balloon::{Checkbox, Motion};
use crate::calendar::Appointment;
use crate::character::Look;
use crate::plugin::{Context, MenuItem, Message, NoSettings, Notice, Plugin, Question, Urgency};
use crate::settings::AfterSummons;
use crate::shortcut::{Key, Modifiers};
use crate::storage::Storage;

const SCREEN: Rect = Rect::new(0, 0, 1440, 900);
const CLIPPY: Rect = Rect::new(1300, 800, 124, 93);
const QUICK: Duration = Duration::from_millis(200);
const WRITE: Shortcut = Shortcut {
    modifiers: Modifiers::COMMAND,
    key: Key::F5,
};
const ERASE: Shortcut = Shortcut {
    modifiers: Modifiers::COMMAND,
    key: Key::F6,
};
const TICK: Shortcut = Shortcut {
    modifiers: Modifiers::COMMAND,
    key: Key::F7,
};
const STRETCH: &str = "Time to stretch!";
const WATER: &str = "Have some water.";
const DONE: &str = "Don't show me this reminder again";
const CHORES: [(&str, i8, i8); 2] = [(STRETCH, 10, 0), (WATER, 10, 30)];
const SNOOZE: SignedDuration = SignedDuration::from_mins(15);
const UNTICKED: Checkbox = Checkbox {
    checked: false,
    pressed: false,
};
const PRESSED: Checkbox = Checkbox {
    checked: false,
    pressed: true,
};
const TICKED: Checkbox = Checkbox {
    checked: true,
    pressed: false,
};

#[derive(Default, Serialize, Deserialize)]
struct Note {
    text: Option<String>,
}

enum NoteAction {
    Write,
    Keep(String),
    Erase,
}

impl Plugin for Note {
    const NAME: &'static str = "Note";

    type Action = NoteAction;

    type Settings = NoSettings;

    fn menu(&self, _: &NoSettings, _: DateTime) -> Vec<MenuItem<NoteAction>> {
        vec![
            MenuItem::new("Write...", NoteAction::Write).shortcut(WRITE),
            MenuItem::new("Erase", NoteAction::Erase)
                .enabled(self.text.is_some())
                .shortcut(ERASE),
        ]
    }

    fn message(&self, _: &NoSettings, _: DateTime) -> Option<Message<NoteAction>> {
        let text = self.text.as_ref()?;
        Some(Message::new(text).on_double_click(NoteAction::Write))
    }

    fn perform(&mut self, action: NoteAction, cx: &mut Context<'_, Self>) {
        match action {
            NoteAction::Write => cx.ask(
                Question::new("What should I remember?", NoteAction::Keep)
                    .draft(self.text.as_deref().unwrap_or_default()),
            ),
            NoteAction::Keep(text) => self.text = (!text.is_empty()).then_some(text),
            NoteAction::Erase => {
                if self.text.take().is_some() {
                    cx.celebrate();
                }
            }
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Greeter {
    name: Option<String>,
}

enum GreeterAction {
    Ask,
    Meet(String),
}

impl Plugin for Greeter {
    const NAME: &'static str = "Greeter";

    type Action = GreeterAction;

    type Settings = NoSettings;

    fn menu(&self, _: &NoSettings, _: DateTime) -> Vec<MenuItem<GreeterAction>> {
        vec![MenuItem::new("Introduce Yourself...", GreeterAction::Ask)]
    }

    fn message(&self, _: &NoSettings, _: DateTime) -> Option<Message<GreeterAction>> {
        let name = self.name.as_ref()?;
        Some(Message::new(format!("Hello, {name}!")))
    }

    fn perform(&mut self, action: GreeterAction, cx: &mut Context<'_, Self>) {
        match action {
            GreeterAction::Ask => {
                cx.ask(Question::new("What is your name?", GreeterAction::Meet));
            }
            GreeterAction::Meet(name) => self.name = Some(name),
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Chores {
    later: BTreeMap<String, DateTime>,
    done: BTreeSet<String>,
}

enum ChoreAction {
    Snooze(String),
    Done(String),
}

impl Plugin for Chores {
    const NAME: &'static str = "Chores";

    type Action = ChoreAction;

    type Settings = NoSettings;

    fn notices(&self, _: &NoSettings, now: DateTime) -> Vec<Notice<ChoreAction>> {
        CHORES
            .into_iter()
            .filter(|&(text, hour, minute)| {
                now.time() >= time(hour, minute, 0, 0)
                    && !self.done.contains(text)
                    && self.later.get(text).is_none_or(|until| now >= *until)
            })
            .map(|(text, ..)| {
                Notice::new(text, text, ChoreAction::Snooze(text.to_owned()))
                    .checkbox(DONE, ChoreAction::Done(text.to_owned()))
                    .over_full_screen(text == WATER)
                    .shortcut(ERASE)
                    .check_shortcut(TICK)
            })
            .collect()
    }

    fn changes_at(&self, _: &NoSettings, now: DateTime) -> Option<DateTime> {
        CHORES
            .into_iter()
            .map(|(_, hour, minute)| now.date().at(hour, minute, 0, 0))
            .chain(self.later.values().copied())
            .filter(|&at| at > now)
            .min()
    }

    fn perform(&mut self, action: ChoreAction, cx: &mut Context<'_, Self>) {
        match action {
            ChoreAction::Snooze(text) => {
                self.later.insert(text, cx.now() + SNOOZE);
            }
            ChoreAction::Done(text) => {
                self.done.insert(text);
            }
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Shift {
    chosen: Vec<String>,
    days: Vec<DateTime>,
}

impl Plugin for Shift {
    const NAME: &'static str = "Shift";

    type Action = &'static str;

    type Settings = NoSettings;

    fn menu(&self, _: &NoSettings, now: DateTime) -> Vec<MenuItem<&'static str>> {
        if now.hour() < 15 {
            vec![
                MenuItem::new("Day", "day").enabled(false),
                MenuItem::new("Evening", "evening"),
            ]
        } else {
            vec![
                MenuItem::new("Evening", "evening"),
                MenuItem::new("Night", "night"),
            ]
        }
    }

    fn begin_day(&mut self, _: &NoSettings, now: DateTime) {
        self.days.push(now);
    }

    fn perform(&mut self, action: &'static str, _: &mut Context<'_, Self>) {
        self.chosen.push(action.to_owned());
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Tally {
    count: u32,
}

impl Plugin for Tally {
    const NAME: &'static str = "Tally";

    type Action = ();

    type Settings = NoSettings;

    fn menu(&self, _: &NoSettings, _: DateTime) -> Vec<MenuItem<()>> {
        vec![MenuItem::new("Count", ())]
    }

    fn perform(&mut self, (): (), _: &mut Context<'_, Self>) {
        self.count += 1;
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Diary {
    #[serde(skip)]
    titles: Vec<String>,
    opened: u32,
}

impl Plugin for Diary {
    const NAME: &'static str = "Diary";

    const READS_CALENDAR: bool = true;

    type Action = ();

    type Settings = NoSettings;

    fn read_agenda(&mut self, agenda: &Agenda) {
        self.titles = match agenda {
            Agenda::Read(appointments) => appointments
                .iter()
                .map(|appointment| appointment.title.clone())
                .collect(),
            Agenda::Unread | Agenda::Denied => Vec::new(),
        };
    }

    fn menu(&self, _: &NoSettings, _: DateTime) -> Vec<MenuItem<()>> {
        self.titles
            .iter()
            .map(|title| MenuItem::new(title.clone(), ()))
            .collect()
    }

    fn perform(&mut self, (): (), _: &mut Context<'_, Self>) {
        self.opened += 1;
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Meeting {
    skipped: bool,
    hidden: bool,
}

enum MeetingAction {
    Skip,
    Hide,
}

impl Plugin for Meeting {
    const NAME: &'static str = "Meeting";

    type Action = MeetingAction;

    type Settings = NoSettings;

    fn message(&self, _: &NoSettings, _: DateTime) -> Option<Message<MeetingAction>> {
        (!self.hidden).then(|| {
            Message::new("Busy")
                .on_double_click(MeetingAction::Hide)
                .shortcut(ERASE)
        })
    }

    fn notices(&self, _: &NoSettings, _: DateTime) -> Vec<Notice<MeetingAction>> {
        (!self.skipped)
            .then(|| {
                Notice::new("standup", "Standup", MeetingAction::Skip)
                    .checkbox("Skip", MeetingAction::Skip)
                    .celebrates(false)
            })
            .into_iter()
            .collect()
    }

    fn perform(&mut self, action: MeetingAction, _: &mut Context<'_, Self>) {
        match action {
            MeetingAction::Skip => self.skipped = true,
            MeetingAction::Hide => self.hidden = true,
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Alarm {
    off: bool,
}

impl Plugin for Alarm {
    const NAME: &'static str = "Alarm";

    type Action = ();

    type Settings = NoSettings;

    fn notices(&self, _: &NoSettings, now: DateTime) -> Vec<Notice<()>> {
        (now.time() >= time(10, 5, 0, 0) && !self.off)
            .then(|| {
                Notice::new("alarm", "Leave now!", ())
                    .check_shortcut(TICK)
                    .urgency(Urgency::Pressing)
            })
            .into_iter()
            .collect()
    }

    fn changes_at(&self, _: &NoSettings, now: DateTime) -> Option<DateTime> {
        Some(now.date().at(10, 5, 0, 0)).filter(|&at| at > now)
    }

    fn perform(&mut self, (): (), _: &mut Context<'_, Self>) {
        self.off = true;
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Sleeper;

impl Plugin for Sleeper {
    const NAME: &'static str = "Sleeper";

    type Action = ();

    type Settings = NoSettings;

    fn bedtime(&self, _: &NoSettings, now: DateTime) -> bool {
        now.hour() >= 22
    }

    fn changes_at(&self, _: &NoSettings, now: DateTime) -> Option<DateTime> {
        Some(now.date().at(22, 0, 0, 0)).filter(|&bedtime| bedtime > now)
    }

    fn perform(&mut self, (): (), _: &mut Context<'_, Self>) {}
}

struct Day {
    origin: Instant,
    start: Zoned,
}

impl Day {
    fn starting_at(hour: i8, minute: i8) -> Self {
        Self::of(date(2026, 10, 19), TimeZone::UTC, hour, minute)
    }

    fn of(day: Date, zone: TimeZone, hour: i8, minute: i8) -> Self {
        Self {
            origin: Instant::now(),
            start: day.at(hour, minute, 0, 0).to_zoned(zone).unwrap(),
        }
    }

    fn at(&self, hour: i8, minute: i8) -> Instant {
        let target = self.start.with().time(time(hour, minute, 0, 0)).build();
        let elapsed = self.start.duration_until(&target.unwrap());
        self.origin + Duration::try_from(elapsed).unwrap()
    }

    fn clock(&self) -> impl Fn(Instant) -> Zoned + 'static {
        let (origin, start) = (self.origin, self.start.clone());
        move |now| {
            let elapsed = SignedDuration::try_from(now.saturating_duration_since(origin));
            start.checked_add(elapsed.unwrap()).unwrap()
        }
    }
}

fn with_chores(directory: &TempDir, day: &Day) -> (App, Instant) {
    with_chores_and(directory, day, AfterSummons::Stay)
}

fn with_chores_and(directory: &TempDir, day: &Day, after_summons: AfterSummons) -> (App, Instant) {
    fastrand::seed(7);
    let day_long = Duration::from_hours(24);
    let settings = Settings {
        idle_pause: day_long..=day_long,
        plugins: enabling(&["Note", "Chores"]).plugins,
        after_summons,
        ..Settings::default()
    };
    let mut app = App::builder()
        .plugin::<Note>()
        .plugin::<Chores>()
        .clock(day.clock())
        .build(settings, Storage::new(directory.path()));
    app.place(CLIPPY, SCREEN);
    app.handle(Event::Launch, day.origin);
    let now = settle(&mut app, day.origin, App::is_present);
    (app, now)
}

fn assemble(directory: &TempDir, settings: Settings) -> App {
    fastrand::seed(7);
    let mut app = App::builder()
        .plugin::<Note>()
        .plugin::<Greeter>()
        .build(settings, Storage::new(directory.path()));
    app.place(CLIPPY, SCREEN);
    app
}

fn enabling(plugins: &[&str]) -> Settings {
    Settings {
        plugins: Some(plugins.iter().map(|&name| name.to_owned()).collect()),
        ..Settings::default()
    }
}

fn with_note(directory: &TempDir, settings: Settings) -> App {
    assemble(
        directory,
        Settings {
            plugins: enabling(&["Note"]).plugins,
            ..settings
        },
    )
}

fn launch(directory: &TempDir, settings: Settings) -> (App, Instant) {
    start(with_note(directory, settings))
}

fn start(mut app: App) -> (App, Instant) {
    let now = Instant::now();
    app.handle(Event::Launch, now);
    let now = settle(&mut app, now, App::is_present);
    (app, now)
}

fn settle(app: &mut App, mut now: Instant, until: impl Fn(&App) -> bool) -> Instant {
    while !until(app) {
        now = app
            .deadline()
            .expect("the app should have something scheduled");
        app.handle(Event::Tick, now);
    }
    now
}

fn is_absent(app: &App) -> bool {
    app.character.presence() == Presence::Absent
}

fn titles(entries: &[Entry]) -> Vec<&str> {
    entries
        .iter()
        .map(|entry| match entry {
            Entry::Item { title, .. } | Entry::Submenu { title, .. } => title.as_str(),
            Entry::Separator => "-",
        })
        .collect()
}

fn submenu(app: &App, title: &str) -> Vec<(String, bool)> {
    let entries = app
        .menu(Instant::now())
        .into_iter()
        .find_map(|entry| match entry {
            Entry::Submenu {
                title: name,
                entries,
            } if name == title => Some(entries),
            _ => None,
        })
        .expect("the menu should have the plugin's submenu");
    entries
        .into_iter()
        .filter_map(|entry| match entry {
            Entry::Item { title, enabled, .. } => Some((title, enabled)),
            Entry::Submenu { .. } | Entry::Separator => None,
        })
        .collect()
}

fn choose(app: &mut App, title: &str, now: Instant) {
    let command = app
        .menu(now)
        .into_iter()
        .flat_map(|entry| match entry {
            Entry::Submenu { entries, .. } => entries,
            entry => vec![entry],
        })
        .find_map(|entry| match entry {
            Entry::Item {
                title: item,
                command,
                ..
            } if item == title => Some(command),
            _ => None,
        })
        .expect("the menu should have the item");
    app.handle(Event::Command(command), now);
}

fn edit(app: &mut App, edit: Edit, now: Instant) {
    app.handle(Event::Edit(edit), now);
}

fn answer(app: &mut App, text: &str, now: Instant) {
    edit(app, Edit::SelectAll, now);
    edit(app, Edit::Insert(text.into()), now);
    app.handle(Event::Submit, now);
}

fn shown(app: &App) -> Option<String> {
    let message = app.message();
    match app.balloon(message.as_deref())? {
        Content::Message(text) => Some(text.to_owned()),
        Content::Prompt { .. } | Content::Notice { .. } => None,
    }
}

fn notice(app: &App) -> Option<(&str, Checkbox)> {
    match app.balloon(None)? {
        Content::Notice {
            text,
            checkbox: Some((DONE, checkbox)),
        } => Some((text, checkbox)),
        _ => None,
    }
}

fn beckons(app: &App) -> bool {
    [
        Animation::GetAttention,
        Animation::GestureUp,
        Animation::GestureDown,
        Animation::GestureLeft,
        Animation::GestureRight,
        Animation::Searching,
        Animation::Knock,
        Animation::Exclaim,
    ]
    .into_iter()
    .any(|animation| app.character.is_busy_with(animation))
}

fn celebrates(app: &App) -> bool {
    app.character.is_busy_with(Animation::Congratulate)
}

fn noticed(app: &App) -> bool {
    notice(app).is_some()
}

fn quiet(app: &App) -> bool {
    notice(app).is_none()
}

fn draft(app: &App) -> Option<&str> {
    let prompt = app.plugins.prompt().filter(|_| app.is_present())?;
    Some(prompt.field().text())
}

fn field(app: &App) -> Option<Field> {
    app.scene().balloon?.field
}

#[test]
fn menu_matches_the_original_wording() {
    let directory = TempDir::new().unwrap();
    let (app, _) = launch(&directory, Settings::default());
    assert_eq!(
        titles(&app.menu(Instant::now())),
        ["Hide", "Animate!", "-", "Note", "-", "Quit"]
    );
    assert_eq!(
        submenu(&app, "Note"),
        [("Write...".to_owned(), true), ("Erase".to_owned(), false)]
    );
}

#[test]
fn menu_without_plugins_has_no_extra_separator() {
    let directory = TempDir::new().unwrap();
    let app = assemble(&directory, enabling(&[]));
    assert_eq!(
        titles(&app.menu(Instant::now())),
        ["Hide", "Animate!", "-", "Quit"]
    );
}

#[test]
fn answers_become_the_message_and_outlive_restarts() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    choose(&mut app, "Write...", now);
    assert!(field(&app).is_some());
    answer(&mut app, "  Ship the plan  ", now);
    assert_eq!(shown(&app).as_deref(), Some("Ship the plan"));
    assert_eq!(submenu(&app, "Note")[1], ("Erase".to_owned(), true));

    let (mut restarted, now) = launch(&directory, Settings::default());
    assert_eq!(shown(&restarted).as_deref(), Some("Ship the plan"));
    choose(&mut restarted, "Erase", now);
    assert_eq!(shown(&restarted), None);

    let (again, _) = launch(&directory, Settings::default());
    assert_eq!(shown(&again), None);
}

#[test]
fn balloon_waits_for_clippy_to_arrive() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    choose(&mut app, "Write...", now);
    answer(&mut app, "Ship it", now);

    let mut restarted = with_note(&directory, Settings::default());
    let now = Instant::now();
    restarted.handle(Event::Launch, now);
    assert!(restarted.scene().balloon.is_none());
    settle(&mut restarted, now, App::is_present);
    assert_eq!(shown(&restarted).as_deref(), Some("Ship it"));
}

#[test]
fn editing_preselects_the_current_message() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    choose(&mut app, "Write...", now);
    answer(&mut app, "Write tests", now);
    choose(&mut app, "Write...", now);
    assert_eq!(draft(&app), Some("Write tests"));
    edit(&mut app, Edit::Insert("Ship it".into()), now);
    app.handle(Event::Submit, now);
    assert_eq!(shown(&app).as_deref(), Some("Ship it"));
}

#[test]
fn double_clicking_the_message_opens_it_for_editing() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    app.handle(Event::MessageDoubleClick, now);
    assert_eq!(draft(&app), None);
    choose(&mut app, "Write...", now);
    answer(&mut app, "Ship it", now);
    app.handle(Event::MessageDoubleClick, now);
    assert_eq!(draft(&app), Some("Ship it"));
    edit(&mut app, Edit::Move(Motion::End), now);
    edit(&mut app, Edit::Insert("!".into()), now);
    app.handle(Event::MessageDoubleClick, now);
    assert_eq!(draft(&app), Some("Ship it!"));
    app.handle(Event::Submit, now);
    assert_eq!(shown(&app).as_deref(), Some("Ship it!"));
}

#[test]
fn cancelling_keeps_the_message() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    choose(&mut app, "Write...", now);
    answer(&mut app, "Write tests", now);
    app.handle(Event::MessageDoubleClick, now);
    edit(&mut app, Edit::Insert("Something else".into()), now);
    app.handle(Event::Cancel, now);
    assert_eq!(shown(&app).as_deref(), Some("Write tests"));
}

#[test]
fn blank_answers_reach_the_plugin_trimmed() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    choose(&mut app, "Write...", now);
    answer(&mut app, "Write tests", now);
    choose(&mut app, "Write...", now);
    answer(&mut app, "   ", now);
    assert_eq!(shown(&app), None);
    assert_eq!(draft(&app), None);
}

#[test]
fn disabled_items_do_nothing() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    let state = directory.path().join("Note.json");
    fs::remove_file(&state).unwrap();
    choose(&mut app, "Erase", now);
    assert_eq!(shown(&app), None);
    assert!(!state.exists());
}

#[test]
fn messages_stack_and_a_double_click_reaches_the_one_that_answers_it() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = start(assemble(&directory, enabling(&["Greeter", "Note"])));
    choose(&mut app, "Introduce Yourself...", now);
    answer(&mut app, "Ada", now);
    choose(&mut app, "Write...", now);
    answer(&mut app, "Ship it", now);
    assert_eq!(shown(&app).as_deref(), Some("Hello, Ada!\n\nShip it"));
    app.handle(Event::MessageDoubleClick, now);
    assert_eq!(draft(&app), Some("Ship it"));
}

#[test]
fn a_message_with_nothing_to_draw_is_not_shown() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    choose(&mut app, "Write...", now);
    answer(&mut app, "🎯", now);
    assert_eq!(shown(&app), None);
    assert!(app.scene().balloon.is_none());
}

#[test]
fn latest_question_gets_the_answer() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = start(assemble(&directory, enabling(&["Note", "Greeter"])));
    choose(&mut app, "Write...", now);
    choose(&mut app, "Introduce Yourself...", now);
    answer(&mut app, "Ada", now);
    assert_eq!(shown(&app).as_deref(), Some("Hello, Ada!"));
    app.handle(Event::Submit, now);
    assert_eq!(submenu(&app, "Note")[1], ("Erase".to_owned(), false));
}

#[test]
fn corrupt_plugin_state_starts_afresh() {
    let directory = TempDir::new().unwrap();
    fs::write(directory.path().join("Note.json"), "not json").unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    assert_eq!(shown(&app), None);
    choose(&mut app, "Write...", now);
    answer(&mut app, "Ship it", now);
    let (restarted, _) = launch(&directory, Settings::default());
    assert_eq!(shown(&restarted).as_deref(), Some("Ship it"));
}

#[test]
fn copying_hands_the_selection_to_the_clipboard() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    choose(&mut app, "Write...", now);
    edit(&mut app, Edit::Insert("Ship it".into()), now);
    edit(&mut app, Edit::Select(Motion::WordBackward), now);
    app.take_effects();
    edit(&mut app, Edit::Copy, now);
    edit(&mut app, Edit::Cut, now);
    edit(&mut app, Edit::Copy, now);
    let copied: Vec<Effect> = app
        .take_effects()
        .into_iter()
        .filter(|effect| matches!(effect, Effect::Copy(_)))
        .collect();
    assert_eq!(
        copied,
        [Effect::Copy("it".into()), Effect::Copy("it".into())]
    );
    assert_eq!(draft(&app), Some("Ship "));
}

#[test]
fn questions_claim_focus_until_they_get_it() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    choose(&mut app, "Write...", now);
    assert!(field(&app).is_some_and(|field| field.wants_focus));
    app.handle(Event::Focus, now);
    assert!(field(&app).is_some_and(|field| !field.wants_focus));
    app.handle(Event::Blur, now);
    assert!(field(&app).is_some_and(|field| !field.wants_focus));
    answer(&mut app, "Ship it", now);
    app.handle(Event::MessageDoubleClick, now);
    assert!(field(&app).is_some_and(|field| field.wants_focus));
}

#[test]
fn caret_blinks_only_while_focused() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    choose(&mut app, "Write...", now);
    assert_eq!(app.caret_deadline(), None);
    app.handle(Event::Focus, now);
    let blink = Duration::from_secs(1);
    assert!(app.caret_deadline().is_some_and(|at| at <= now + blink));
    edit(&mut app, Edit::Insert("Ship it".into()), now);
    edit(&mut app, Edit::SelectAll, now);
    assert_eq!(app.caret_deadline(), None);
}

#[test]
fn field_presses_land_on_the_same_character_at_any_scale() {
    let directory = TempDir::new().unwrap();
    let carets = |scale: u8| {
        let settings = Settings {
            scale: NonZeroU8::new(scale).unwrap(),
            ..Settings::default()
        };
        let (mut app, now) = launch(&directory, settings);
        choose(&mut app, "Write...", now);
        edit(&mut app, Edit::Insert("Ship the plan".into()), now);
        let factor = i32::from(scale);
        (0..80)
            .map(|x| {
                app.handle(
                    Event::FieldPress {
                        x: x * factor,
                        clicks: 1,
                        extend: false,
                    },
                    now,
                );
                app.plugins.prompt().map(|prompt| prompt.field().caret())
            })
            .collect::<Vec<_>>()
    };
    let unscaled = carets(1);
    assert!(
        unscaled
            .iter()
            .any(|caret| caret.is_some_and(|caret| caret > 0))
    );
    assert_eq!(carets(3), unscaled);
}

#[test]
fn hiding_dismisses_the_question() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    choose(&mut app, "Write...", now);
    answer(&mut app, "Focus", now);
    choose(&mut app, "Write...", now);
    app.handle(Event::Command(Command::Toggle), now);
    let now = settle(&mut app, now, is_absent);
    let hidden = app.scene();
    assert!(hidden.clippy.is_none() && hidden.balloon.is_none());
    app.handle(Event::Command(Command::Toggle), now);
    settle(&mut app, now, App::is_present);
    assert_eq!(shown(&app).as_deref(), Some("Focus"));
}

#[test]
fn changing_your_mind_about_hiding_keeps_the_draft() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    app.handle(Event::Command(Command::Animate), now);
    choose(&mut app, "Write...", now);
    edit(&mut app, Edit::Insert("Half a thought".into()), now);
    app.handle(Event::Command(Command::Toggle), now);
    assert!(app.scene().balloon.is_none());
    app.handle(Event::Command(Command::Toggle), now);
    let prompt = app.plugins.prompt().expect("the question should survive");
    assert_eq!(prompt.field().text(), "Half a thought");
}

#[test]
fn stepping_aside_for_full_screen_is_quick() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    app.handle(Event::Command(Command::Animate), now);
    app.handle(Event::FullScreen(true), now);
    let gone = settle(&mut app, now, is_absent);
    assert!(gone - now <= STEPPING_ASIDE + QUICK);
}

#[test]
fn a_menu_item_does_what_it_said_when_the_menu_was_opened() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(14, 59);
    let mut app = App::builder()
        .plugin::<Shift>()
        .clock(day.clock())
        .build(Settings::default(), Storage::new(directory.path()));
    app.place(CLIPPY, SCREEN);
    app.handle(Event::Launch, day.origin);
    let opened = day.origin + Duration::from_secs(50);
    let evening = app
        .menu(opened)
        .into_iter()
        .flat_map(|entry| match entry {
            Entry::Submenu { entries, .. } => entries,
            entry => vec![entry],
        })
        .find_map(|entry| match entry {
            Entry::Item { title, command, .. } if title == "Evening" => Some(command),
            _ => None,
        })
        .unwrap();
    app.handle(
        Event::Command(evening),
        day.at(15, 0) + Duration::from_secs(5),
    );
    let saved = fs::read_to_string(directory.path().join("Shift.json")).unwrap();
    let shift: Shift = serde_json::from_str(&saved).unwrap();
    assert_eq!(shift.chosen, ["evening"]);
}

#[test]
fn nothing_is_said_before_the_shell_places_clippy() {
    let directory = TempDir::new().unwrap();
    let mut app = App::builder()
        .plugin::<Note>()
        .build(Settings::default(), Storage::new(directory.path()));
    let now = Instant::now();
    app.handle(Event::Launch, now);
    let now = settle(&mut app, now, App::is_present);
    choose(&mut app, "Write...", now);
    let scene = app.scene();
    assert!(scene.clippy.is_some() && scene.balloon.is_none());
    app.place(CLIPPY, SCREEN);
    assert!(app.scene().balloon.is_some());
}

#[test]
fn plugins_hear_when_a_new_day_begins() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(23, 0);
    let mut app = App::builder()
        .plugin::<Shift>()
        .clock(day.clock())
        .build(Settings::default(), Storage::new(directory.path()));
    app.place(CLIPPY, SCREEN);
    app.handle(Event::Launch, day.origin);
    app.handle(Event::Tick, day.at(23, 30));
    let after_midnight = day.origin + Duration::from_hours(9);
    app.handle(Event::Tick, after_midnight);
    app.handle(Event::Tick, after_midnight + Duration::from_mins(5));
    let saved = fs::read_to_string(directory.path().join("Shift.json")).unwrap();
    let shift: Shift = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        shift.days,
        [
            date(2026, 10, 19).at(23, 0, 0, 0),
            date(2026, 10, 20).at(8, 0, 0, 0)
        ]
    );
}

#[test]
fn clippy_starts_in_the_bottom_right_corner() {
    let directory = TempDir::new().unwrap();
    let app = assemble(&directory, Settings::default());
    let home = app.home(SCREEN);
    assert_eq!((home.right(), home.bottom()), (1435, 895));
    assert_eq!((home.width, home.height), (124, 93));
}

#[test]
fn toggling_is_quick() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    app.handle(Event::Command(Command::Toggle), now);
    let hidden = settle(&mut app, now, is_absent);
    app.handle(Event::Command(Command::Toggle), hidden);
    let shown = settle(&mut app, hidden, App::is_present);
    assert!(hidden - now < QUICK);
    assert!(shown - hidden < QUICK);
}

#[test]
fn scale_enlarges_every_pixel() {
    let directory = TempDir::new().unwrap();
    let settings = Settings {
        scale: NonZeroU8::new(2).unwrap(),
        ..Settings::default()
    };
    let (mut app, now) = launch(&directory, settings);
    choose(&mut app, "Write...", now);
    assert_eq!(app.clippy_size(), Size::new(248, 186));
    app.place(Rect::new(1000, 600, 248, 186), SCREEN);
    let scene = app.scene();
    assert_eq!(scene.clippy.unwrap().size(), Size::new(248, 186));
    let balloon = scene.balloon.unwrap();
    assert_eq!(balloon.canvas.size().width, balloon.frame.width);
    let field = balloon.field.unwrap().frame;
    assert!(
        [field.x, field.y, field.width, field.height]
            .iter()
            .all(|length| length % 2 == 0)
    );
}

#[test]
fn sounds_can_be_turned_off() {
    let directory = TempDir::new().unwrap();
    let plays = |app: &mut App| {
        app.take_effects()
            .iter()
            .any(|effect| matches!(effect, Effect::Play(_)))
    };
    let (mut loud, _) = launch(&directory, Settings::default());
    assert!(plays(&mut loud));
    let muted = Settings {
        sounds: false,
        ..Settings::default()
    };
    let (mut quiet, _) = launch(&directory, muted);
    assert!(!plays(&mut quiet));
}

#[test]
fn clippy_falls_asleep_the_moment_a_plugin_wants_him_to() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(21, 59);
    fastrand::seed(7);
    let day_long = Duration::from_hours(24);
    let settings = Settings {
        idle_pause: day_long..=day_long,
        plugins: enabling(&["Sleeper"]).plugins,
        ..Settings::default()
    };
    let mut app = App::builder()
        .plugin::<Sleeper>()
        .clock(day.clock())
        .build(settings, Storage::new(directory.path()));
    app.place(CLIPPY, SCREEN);
    app.handle(Event::Launch, day.origin);
    let now = settle(&mut app, day.origin, App::is_present);
    let asleep = |app: &App| app.character.is_asleep();
    assert!(!asleep(&app));
    assert_eq!(app.deadline(), Some(day.at(22, 0)));
    assert_eq!(settle(&mut app, now, asleep), day.at(22, 0));
    app.handle(Event::Tick, day.at(23, 0));
    assert!(asleep(&app));
}

#[test]
fn plugins_build_on_what_other_programs_wrote() {
    let directory = TempDir::new().unwrap();
    let file = directory.path().join("Tally.json");
    let tally = || {
        serde_json::from_str::<Tally>(&fs::read_to_string(&file).unwrap())
            .unwrap()
            .count
    };
    let day = Day::starting_at(23, 0);
    let mut app = App::builder()
        .plugin::<Tally>()
        .clock(day.clock())
        .build(enabling(&["Tally"]), Storage::new(directory.path()));
    app.place(CLIPPY, SCREEN);
    app.handle(Event::Launch, day.origin);
    choose(&mut app, "Count", day.origin);
    assert_eq!(tally(), 1);
    fs::write(&file, r#"{"count": 10}"#).unwrap();
    choose(&mut app, "Count", day.origin);
    assert_eq!(tally(), 11);
    fs::write(&file, r#"{"count": 5}"#).unwrap();
    app.handle(Event::Tick, day.at(23, 30));
    app.handle(Event::Tick, day.origin + Duration::from_hours(2));
    assert_eq!(tally(), 5);
    choose(&mut app, "Count", day.origin + Duration::from_hours(2));
    assert_eq!(tally(), 6);
}

#[test]
fn the_calendar_outlives_reloading_the_plugin_that_reads_it() {
    let directory = TempDir::new().unwrap();
    let build = |plugins| {
        App::builder()
            .plugin::<Diary>()
            .plugin::<Tally>()
            .build(enabling(plugins), Storage::new(directory.path()))
    };
    assert!(!build(&["Tally"]).reads_calendar());
    let mut app = build(&["Diary", "Tally"]);
    assert!(app.reads_calendar());
    app.place(CLIPPY, SCREEN);
    let (mut app, now) = start(app);
    let standup = Appointment {
        id: "standup".to_owned(),
        calendar: "Work".to_owned(),
        title: "Standup".to_owned(),
        start: date(2026, 10, 19).at(10, 0, 0, 0),
        end: date(2026, 10, 19).at(10, 15, 0, 0),
        all_day: false,
    };
    app.handle(Event::Calendar(Agenda::Read(vec![standup])), now);
    for _ in 0..2 {
        choose(&mut app, "Standup", now);
    }
    assert_eq!(submenu(&app, "Diary"), [("Standup".to_owned(), true)]);
    let diary: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(directory.path().join("Diary.json")).unwrap())
            .unwrap();
    assert_eq!(diary["opened"], 2);
}

#[test]
fn quitting_waits_for_goodbye() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    app.handle(Event::Command(Command::Quit), now);
    assert!(!app.is_finished());
    let end = settle(&mut app, now, App::is_finished);
    assert!(end - now > QUICK);
}

#[test]
fn hiding_at_once_does_not_wait_for_a_trick_to_end() {
    let directory = TempDir::new().unwrap();
    let settings = Settings {
        hide_at_once: true,
        ..Settings::default()
    };
    let (mut app, now) = launch(&directory, settings);
    app.handle(Event::ClippyDoubleClick, now);
    let now = settle(&mut app, now, |app| app.character.is_performing());
    app.handle(Event::Command(Command::Hide), now);
    let frames = &ANIMATIONS.clip(Animation::Hide).frames;
    let vanishes = frames
        .iter()
        .position(|frame| !frame.shows_clippy())
        .unwrap();
    let shrinking: Duration = frames[..vanishes].iter().map(|frame| frame.duration).sum();
    assert_eq!(settle(&mut app, now, is_absent) - now, shrinking);
}

#[test]
fn quitting_while_hidden_is_immediate() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    app.handle(Event::Command(Command::Hide), now);
    let now = settle(&mut app, now, is_absent);
    app.handle(Event::Command(Command::Quit), now);
    assert!(app.is_finished());
}

#[test]
fn settings_choose_and_order_the_plugins() {
    let directory = TempDir::new().unwrap();
    let app = assemble(&directory, enabling(&["Greeter", "note", "Nope", "Note"]));
    assert_eq!(
        titles(&app.menu(Instant::now())),
        ["Hide", "Animate!", "-", "Greeter", "Note", "-", "Quit"]
    );
    let everything = assemble(&directory, Settings::default());
    assert_eq!(
        titles(&everything.menu(Instant::now())),
        ["Hide", "Animate!", "-", "Note", "Greeter", "-", "Quit"]
    );
}

#[test]
fn first_launch_lists_every_plugin_in_the_settings_file() {
    let directory = TempDir::new().unwrap();
    App::builder()
        .plugin::<Note>()
        .plugin::<Greeter>()
        .load(Storage::new(directory.path()));
    let written = fs::read_to_string(directory.path().join("clippy.toml")).unwrap();
    assert!(written.contains(r#"plugins = ["Note", "Greeter"]"#));
}

#[test]
fn shortcuts_reach_the_toggle_and_plugin_items() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    assert_eq!(
        app.shortcuts(now),
        [Settings::default().toggle_shortcut, WRITE]
    );
    let hide = app.menu(now).into_iter().find_map(|entry| match entry {
        Entry::Item {
            command: Command::Hide,
            shortcut,
            ..
        } => shortcut,
        _ => None,
    });
    assert_eq!(hide, Some(Settings::default().toggle_shortcut));
    app.handle(Event::Shortcut(WRITE), now);
    assert_eq!(draft(&app), Some(""));
    answer(&mut app, "Ship it", now);
    assert_eq!(
        app.shortcuts(now),
        [Settings::default().toggle_shortcut, WRITE, ERASE]
    );
    app.handle(Event::Shortcut(Settings::default().toggle_shortcut), now);
    settle(&mut app, now, is_absent);
}

#[test]
fn shortcut_questions_bring_clippy_back_for_good() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    app.handle(Event::Command(Command::Hide), now);
    let now = settle(&mut app, now, is_absent);
    app.handle(Event::Shortcut(WRITE), now);
    let now = settle(&mut app, now, App::is_present);
    assert_eq!(draft(&app), Some(""));
    answer(&mut app, "Ship it", now);
    let later = now + Duration::from_secs(60);
    app.handle(Event::Tick, later);
    assert!(app.is_present());
    assert_eq!(shown(&app).as_deref(), Some("Ship it"));
    app.handle(Event::Command(Command::Hide), later);
    settle(&mut app, later, is_absent);
}

#[test]
fn summoned_clippy_can_hide_again_once_the_plugin_has_nothing_to_show() {
    let directory = TempDir::new().unwrap();
    let settings = Settings {
        after_summons: AfterSummons::Hide,
        ..Settings::default()
    };
    let (mut app, now) = launch(&directory, settings);
    app.handle(Event::Command(Command::Hide), now);
    let now = settle(&mut app, now, is_absent);
    app.handle(Event::Shortcut(WRITE), now);
    let now = settle(&mut app, now, App::is_present);
    answer(&mut app, "Ship it", now);
    let later = now + Duration::from_secs(60);
    app.handle(Event::Tick, later);
    assert!(app.is_present());
    choose(&mut app, "Erase", later);
    let now = settle(&mut app, later, is_absent);
    app.handle(Event::Shortcut(WRITE), now);
    let now = settle(&mut app, now, App::is_present);
    app.handle(Event::Cancel, now);
    settle(&mut app, now, is_absent);
}

#[test]
fn a_summons_lasts_through_the_congratulations() {
    let directory = TempDir::new().unwrap();
    let settings = Settings {
        after_summons: AfterSummons::Hide,
        ..Settings::default()
    };
    let (mut app, now) = launch(&directory, settings);
    app.handle(Event::Command(Command::Hide), now);
    let now = settle(&mut app, now, is_absent);
    app.handle(Event::Shortcut(WRITE), now);
    let now = settle(&mut app, now, App::is_present);
    answer(&mut app, "Ship it", now);
    app.handle(Event::Shortcut(ERASE), now);
    assert!(celebrates(&app));
    let now = settle(&mut app, now, |app| {
        app.character.is_performing() && celebrates(app)
    });
    let done = settle(&mut app, now, |app| !celebrates(app));
    let gone = settle(&mut app, done, is_absent);
    assert!(gone - done < QUICK);
}

#[test]
fn hiding_cuts_the_congratulations_short() {
    let directory = TempDir::new().unwrap();
    let settings = Settings {
        after_summons: AfterSummons::Hide,
        hide_at_once: true,
        ..Settings::default()
    };
    let (mut app, now) = launch(&directory, settings);
    app.handle(Event::Command(Command::Hide), now);
    let now = settle(&mut app, now, is_absent);
    app.handle(Event::Shortcut(WRITE), now);
    let now = settle(&mut app, now, App::is_present);
    answer(&mut app, "Ship it", now);
    app.handle(Event::Shortcut(ERASE), now);
    app.handle(Event::Command(Command::Hide), now);
    assert!(!celebrates(&app));
    let gone = settle(&mut app, now, is_absent);
    assert!(gone - now < QUICK);
}

#[test]
fn summons_use_the_quick_show_animation() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    app.handle(Event::Command(Command::Hide), now);
    let now = settle(&mut app, now, is_absent);
    app.handle(Event::Shortcut(WRITE), now);
    let shown = settle(&mut app, now, App::is_present);
    assert!(shown - now < QUICK);
}

#[test]
fn full_screen_sends_clippy_away_until_it_ends() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    choose(&mut app, "Write...", now);
    app.handle(Event::FullScreen(true), now);
    let now = settle(&mut app, now, is_absent);
    assert_eq!(draft(&app), None);
    app.handle(Event::FullScreen(true), now);
    assert!(is_absent(&app));
    app.handle(Event::FullScreen(false), now);
    settle(&mut app, now, App::is_present);
}

#[test]
fn full_screen_never_overrides_the_user() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    app.handle(Event::Command(Command::Hide), now);
    let now = settle(&mut app, now, is_absent);
    app.handle(Event::FullScreen(true), now);
    app.handle(Event::FullScreen(false), now);
    assert!(is_absent(&app));
    app.handle(Event::FullScreen(true), now);
    app.handle(Event::Command(Command::Toggle), now);
    let now = settle(&mut app, now, App::is_present);
    app.handle(Event::FullScreen(false), now);
    assert!(app.is_present());
}

#[test]
fn full_screen_hiding_can_be_turned_off() {
    let directory = TempDir::new().unwrap();
    let settings = Settings {
        hide_in_full_screen: false,
        ..Settings::default()
    };
    let (mut app, now) = launch(&directory, settings);
    app.handle(Event::FullScreen(true), now);
    assert!(app.is_present());
}

#[test]
fn launching_in_full_screen_greets_once_it_ends() {
    let directory = TempDir::new().unwrap();
    let mut app = with_note(&directory, Settings::default());
    let now = Instant::now();
    app.handle(Event::FullScreen(true), now);
    app.handle(Event::Launch, now);
    assert!(is_absent(&app));
    assert_eq!(app.take_effects(), Vec::new());
    app.handle(Event::FullScreen(false), now);
    settle(&mut app, now, App::is_present);
    assert!(
        app.take_effects()
            .iter()
            .any(|effect| matches!(effect, Effect::Play(_)))
    );
}

#[test]
fn double_clicking_clippy_plays_a_trick() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    app.handle(Event::ClippyDoubleClick, now);
    settle(&mut app, now, |app| app.character.is_performing());
}

#[test]
fn questions_asked_while_focused_never_steal_focus_later() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    choose(&mut app, "Write...", now);
    app.handle(Event::Focus, now);
    choose(&mut app, "Write...", now);
    app.handle(Event::Blur, now);
    assert!(field(&app).is_some_and(|field| !field.wants_focus));
}

#[test]
fn asking_the_same_question_again_keeps_the_draft() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    app.handle(Event::Shortcut(WRITE), now);
    app.handle(Event::Focus, now);
    edit(&mut app, Edit::Insert("Ship".into()), now);
    app.handle(Event::Blur, now);
    app.handle(Event::Shortcut(WRITE), now);
    assert_eq!(draft(&app), Some("Ship"));
    assert!(field(&app).is_some_and(|field| field.wants_focus));
}

#[test]
fn reminders_tap_on_the_glass_right_on_time() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(9, 59);
    let (mut app, now) = with_chores(&directory, &day);
    assert_eq!(notice(&app), None);
    let now = settle(&mut app, now, noticed);
    assert_eq!(now, day.at(10, 0));
    assert_eq!(notice(&app), Some((STRETCH, UNTICKED)));
    assert!(beckons(&app));
    let balloon = app.scene().balloon.unwrap();
    assert!(balloon.checkbox.is_some() && balloon.field.is_none());
}

#[test]
fn snoozing_brings_back_the_message_underneath() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(9, 59);
    let (mut app, now) = with_chores(&directory, &day);
    choose(&mut app, "Write...", now);
    answer(&mut app, "Ship it", now);
    let now = settle(&mut app, now, noticed);
    assert_eq!(shown(&app), None);
    app.handle(Event::MessageDoubleClick, now);
    assert_eq!(notice(&app), None);
    assert_eq!(shown(&app).as_deref(), Some("Ship it"));
    assert_eq!(draft(&app), None);
    let now = settle(&mut app, now, noticed);
    assert_eq!(now, day.at(10, 15));
}

#[test]
fn a_notice_shortcut_hides_the_notice_and_does_nothing_else() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(9, 59);
    let (mut app, now) = with_chores(&directory, &day);
    assert!(!app.shortcuts(now).contains(&ERASE));
    let now = settle(&mut app, now, noticed);
    assert!(app.shortcuts(now).contains(&ERASE));
    app.handle(Event::Shortcut(ERASE), now);
    assert_eq!(notice(&app), None);
    assert!(!app.shortcuts(now).contains(&ERASE));
    choose(&mut app, "Write...", now);
    answer(&mut app, "Ship it", now);
    let now = settle(&mut app, now, noticed);
    assert_eq!(now, day.at(10, 15));
    app.handle(Event::Shortcut(ERASE), now);
    assert_eq!(notice(&app), None);
    assert_eq!(shown(&app).as_deref(), Some("Ship it"));
    assert!(!celebrates(&app));
    app.handle(Event::Shortcut(ERASE), now);
    assert_eq!(shown(&app), None);
}

#[test]
fn a_check_shortcut_ticks_the_box_as_a_click_would() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(9, 59);
    let (mut app, now) = with_chores(&directory, &day);
    assert!(!app.shortcuts(now).contains(&TICK));
    let now = settle(&mut app, now, noticed);
    assert!(app.shortcuts(now).contains(&TICK));
    app.handle(Event::Shortcut(TICK), now);
    assert_eq!(notice(&app), Some((STRETCH, TICKED)));
    assert!(celebrates(&app));
    assert!(!app.shortcuts(now).contains(&TICK));
    let now = settle(&mut app, now, quiet);
    let now = settle(&mut app, now, noticed);
    assert_eq!(now, day.at(10, 30));
    assert_eq!(notice(&app), Some((WATER, UNTICKED)));
}

#[test]
fn a_check_shortcut_just_hides_a_notice_without_a_box() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(10, 4);
    let mut app = App::builder()
        .plugin::<Alarm>()
        .clock(day.clock())
        .build(enabling(&["Alarm"]), Storage::new(directory.path()));
    app.place(CLIPPY, SCREEN);
    app.handle(Event::Launch, day.origin);
    let now = settle(&mut app, day.origin, |app| app.notices.is_displayed());
    assert!(app.shortcuts(now).contains(&TICK));
    app.handle(Event::Shortcut(TICK), now);
    assert!(!app.notices.is_displayed());
    assert!(!celebrates(&app));
    assert!(!app.shortcuts(now).contains(&TICK));
}

#[test]
fn ticking_a_box_that_does_not_celebrate_just_closes_the_notice() {
    let directory = TempDir::new().unwrap();
    let mut app = App::builder()
        .plugin::<Meeting>()
        .build(enabling(&["Meeting"]), Storage::new(directory.path()));
    app.place(CLIPPY, SCREEN);
    let (mut app, now) = start(app);
    let now = settle(&mut app, now, |app| app.notices.is_displayed());
    app.handle(Event::CheckboxPress, now);
    app.handle(Event::CheckboxRelease { inside: true }, now);
    assert!(!celebrates(&app));
    assert!(!app.notices.is_displayed());
    assert_eq!(shown(&app).as_deref(), Some("Busy"));
}

#[test]
fn a_message_shortcut_hides_that_message_and_nothing_else() {
    let directory = TempDir::new().unwrap();
    let mut app = App::builder().plugin::<Note>().plugin::<Meeting>().build(
        enabling(&["Note", "Meeting"]),
        Storage::new(directory.path()),
    );
    app.place(CLIPPY, SCREEN);
    let (mut app, now) = start(app);
    let now = settle(&mut app, now, |app| app.notices.is_displayed());
    assert!(!app.shortcuts(now).contains(&ERASE));
    app.handle(Event::MessageDoubleClick, now);
    choose(&mut app, "Write...", now);
    answer(&mut app, "Ship it", now);
    assert_eq!(shown(&app).as_deref(), Some("Ship it\n\nBusy"));
    assert!(app.shortcuts(now).contains(&ERASE));
    app.handle(Event::Shortcut(ERASE), now);
    assert_eq!(shown(&app).as_deref(), Some("Ship it"));
    app.handle(Event::Shortcut(ERASE), now);
    assert_eq!(shown(&app), None);
}

#[test]
fn snoozing_brings_back_the_unfinished_answer() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(9, 59);
    let (mut app, now) = with_chores(&directory, &day);
    choose(&mut app, "Write...", now);
    app.handle(Event::Focus, now);
    edit(&mut app, Edit::Insert("Ship".into()), now);
    let now = settle(&mut app, now, noticed);
    assert!(field(&app).is_none());
    edit(&mut app, Edit::Insert(" it".into()), now);
    app.handle(Event::Submit, now);
    app.handle(Event::Blur, now);
    assert_eq!(notice(&app), Some((STRETCH, UNTICKED)));
    app.handle(Event::MessageDoubleClick, now);
    assert_eq!(draft(&app), Some("Ship"));
    assert!(field(&app).is_some_and(|field| field.wants_focus));
    app.handle(Event::Focus, now);
    edit(&mut app, Edit::Insert(" it".into()), now);
    app.handle(Event::Submit, now);
    assert_eq!(shown(&app).as_deref(), Some("Ship it"));
}

#[test]
fn a_notice_hands_the_keyboard_back_only_if_it_took_it() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(9, 59);
    let (mut app, now) = with_chores(&directory, &day);
    choose(&mut app, "Write...", now);
    app.handle(Event::Focus, now);
    app.handle(Event::Blur, now);
    let now = settle(&mut app, now, noticed);
    app.handle(Event::CheckboxPress, now);
    app.handle(Event::CheckboxRelease { inside: true }, now);
    settle(&mut app, now, |app| !celebrates(app));
    assert!(field(&app).is_some_and(|field| !field.wants_focus));
}

#[test]
fn any_action_counts_as_a_snooze() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(9, 59);
    let (mut app, now) = with_chores(&directory, &day);
    let now = settle(&mut app, now, noticed);
    choose(&mut app, "Write...", now);
    assert_eq!(notice(&app), None);
    assert_eq!(draft(&app), Some(""));
    let now = settle(&mut app, now, noticed);
    assert_eq!(now, day.at(10, 15));
    app.handle(Event::ClippyDoubleClick, now);
    assert_eq!(notice(&app), None);
    settle(&mut app, now, |app| app.character.is_performing());
}

#[test]
fn missed_reminders_greet_a_late_launch() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(10, 29);
    let (app, _) = with_chores(&directory, &day);
    assert_eq!(notice(&app), Some((STRETCH, UNTICKED)));
    assert!(beckons(&app));
}

#[test]
fn waking_up_shows_what_was_missed() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(9, 0);
    let (mut app, _) = with_chores(&directory, &day);
    app.handle(Event::Tick, day.at(13, 0));
    assert_eq!(notice(&app), Some((STRETCH, UNTICKED)));
    assert!(beckons(&app));
}

#[test]
fn a_second_reminder_waits_for_the_first() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(10, 29);
    let (mut app, _) = with_chores(&directory, &day);
    let half_past = day.at(10, 30);
    app.handle(Event::Tick, half_past);
    assert_eq!(notice(&app), Some((STRETCH, UNTICKED)));
    app.handle(Event::MessageDoubleClick, half_past);
    assert_eq!(notice(&app), Some((WATER, UNTICKED)));
    assert!(beckons(&app));
    let later = day.at(10, 50);
    app.handle(Event::Tick, later);
    assert_eq!(notice(&app), Some((WATER, UNTICKED)));
    app.handle(Event::CheckboxPress, later);
    app.handle(Event::CheckboxRelease { inside: true }, later);
    assert_eq!(notice(&app), Some((WATER, TICKED)));
    settle(&mut app, later, |app| {
        notice(app) == Some((STRETCH, UNTICKED))
    });
    assert!(beckons(&app) && !celebrates(&app));
}

#[test]
fn an_urgent_notice_takes_over_and_the_other_comes_back_after() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(10, 0);
    let mut app = App::builder()
        .plugin::<Chores>()
        .plugin::<Alarm>()
        .clock(day.clock())
        .build(
            enabling(&["Chores", "Alarm"]),
            Storage::new(directory.path()),
        );
    app.place(CLIPPY, SCREEN);
    app.handle(Event::Launch, day.origin);
    let now = settle(&mut app, day.origin, noticed);
    assert_eq!(notice(&app), Some((STRETCH, UNTICKED)));
    let now = settle(&mut app, now, |app| notice(app).is_none());
    assert_eq!(now, day.at(10, 5));
    assert!(beckons(&app));
    app.handle(Event::MessageDoubleClick, now);
    assert_eq!(notice(&app), Some((STRETCH, UNTICKED)));
}

#[test]
fn ticking_the_box_congratulates_and_retires_the_reminder() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(9, 59);
    let (mut app, now) = with_chores(&directory, &day);
    let now = settle(&mut app, now, noticed);
    app.handle(Event::CheckboxPress, now);
    assert_eq!(notice(&app), Some((STRETCH, PRESSED)));
    app.handle(Event::CheckboxRelease { inside: true }, now);
    assert_eq!(notice(&app), Some((STRETCH, TICKED)));
    assert!(celebrates(&app));
    let now = settle(&mut app, now, quiet);
    assert!(!celebrates(&app));
    let now = settle(&mut app, now, noticed);
    assert_eq!(now, day.at(10, 30));
    assert_eq!(notice(&app), Some((WATER, UNTICKED)));
}

#[test]
fn hidden_clippy_goes_back_into_hiding_right_after_the_congratulations() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(9, 59);
    let (mut app, now) = with_chores_and(&directory, &day, AfterSummons::Hide);
    app.handle(Event::Command(Command::Hide), now);
    let now = settle(&mut app, now, is_absent);
    let now = settle(&mut app, now, noticed);
    app.handle(Event::CheckboxPress, now);
    app.handle(Event::CheckboxRelease { inside: true }, now);
    let done = settle(&mut app, now, quiet);
    let gone = settle(&mut app, done, is_absent);
    assert!(gone - done < QUICK);
}

#[test]
fn a_visiting_clippy_finishes_his_trick_before_hiding_again() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(9, 59);
    let (mut app, now) = with_chores_and(&directory, &day, AfterSummons::Hide);
    app.handle(Event::Command(Command::Hide), now);
    let now = settle(&mut app, now, is_absent);
    let now = settle(&mut app, now, noticed);
    app.handle(Event::ClippyDoubleClick, now);
    assert!(quiet(&app));
    let now = settle(&mut app, now, |app| app.character.is_performing());
    assert!(app.character.is_showing_off());
    let done = settle(&mut app, now, |app| !app.character.is_showing_off());
    let gone = settle(&mut app, done, is_absent);
    assert!(gone - done < QUICK);
}

#[test]
fn hiding_cuts_a_visiting_clippy_trick_short() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(9, 59);
    let (mut app, now) = with_chores_and(&directory, &day, AfterSummons::Hide);
    app.handle(Event::Command(Command::Hide), now);
    let now = settle(&mut app, now, is_absent);
    let now = settle(&mut app, now, noticed);
    app.handle(Event::ClippyDoubleClick, now);
    app.handle(Event::Command(Command::Hide), now);
    assert_eq!(app.character.presence(), Presence::Leaving);
}

#[test]
fn dragging_off_the_box_takes_the_tick_back() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(9, 59);
    let (mut app, now) = with_chores(&directory, &day);
    let now = settle(&mut app, now, noticed);
    app.handle(Event::CheckboxPress, now);
    app.handle(Event::CheckboxDrag { inside: false }, now);
    assert_eq!(notice(&app), Some((STRETCH, UNTICKED)));
    app.handle(Event::CheckboxDrag { inside: true }, now);
    assert_eq!(notice(&app), Some((STRETCH, PRESSED)));
    app.handle(Event::CheckboxDrag { inside: false }, now);
    app.handle(Event::CheckboxRelease { inside: false }, now);
    assert_eq!(notice(&app), Some((STRETCH, UNTICKED)));
    assert!(!celebrates(&app));
}

#[test]
fn acting_during_the_congratulations_cuts_them_short() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(9, 59);
    let (mut app, now) = with_chores(&directory, &day);
    let now = settle(&mut app, now, noticed);
    app.handle(Event::CheckboxPress, now);
    app.handle(Event::CheckboxRelease { inside: true }, now);
    app.handle(Event::Shortcut(WRITE), now);
    assert_eq!(notice(&app), None);
    assert_eq!(draft(&app), Some(""));
    assert!(field(&app).is_some_and(|field| field.wants_focus));
}

#[test]
fn hiding_snoozes_and_reminders_bring_clippy_back() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(10, 29);
    let (mut app, _) = with_chores(&directory, &day);
    let half_past = day.at(10, 30);
    app.handle(Event::Tick, half_past);
    app.handle(Event::Command(Command::Hide), half_past);
    let now = settle(&mut app, half_past, is_absent);
    let now = settle(&mut app, now, noticed);
    assert!(now >= day.at(10, 45));
    assert_eq!(notice(&app), Some((STRETCH, UNTICKED)));
    app.handle(Event::MessageDoubleClick, now);
    assert_eq!(notice(&app), Some((WATER, UNTICKED)));
    app.handle(Event::MessageDoubleClick, now);
    assert_eq!(notice(&app), None);
    let later = now + Duration::from_secs(60);
    app.handle(Event::Tick, later);
    assert!(app.is_present());
}

#[test]
fn reminders_can_leave_a_hidden_clippy_hidden_again() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(9, 59);
    let (mut app, now) = with_chores_and(&directory, &day, AfterSummons::Hide);
    app.handle(Event::Command(Command::Hide), now);
    let now = settle(&mut app, now, is_absent);
    let now = settle(&mut app, now, noticed);
    app.handle(Event::MessageDoubleClick, now);
    settle(&mut app, now, is_absent);
}

#[test]
fn a_question_asked_over_a_reminder_keeps_clippy_while_it_is_open() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(9, 59);
    let (mut app, now) = with_chores_and(&directory, &day, AfterSummons::Hide);
    app.handle(Event::Command(Command::Hide), now);
    let now = settle(&mut app, now, is_absent);
    let now = settle(&mut app, now, noticed);
    choose(&mut app, "Write...", now);
    assert_eq!(notice(&app), None);
    let later = now + Duration::from_secs(60);
    app.handle(Event::Tick, later);
    assert_eq!(draft(&app), Some(""));
    app.handle(Event::Cancel, later);
    settle(&mut app, later, is_absent);
}

#[test]
fn urgent_reminders_show_over_full_screen_and_then_step_aside() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(10, 29);
    let (mut app, now) = with_chores(&directory, &day);
    assert_eq!(notice(&app), Some((STRETCH, UNTICKED)));
    app.handle(Event::FullScreen(true), now);
    settle(&mut app, now, is_absent);
    let half_past = day.at(10, 30);
    app.handle(Event::Tick, half_past);
    let now = settle(&mut app, half_past, noticed);
    assert_eq!(notice(&app), Some((WATER, UNTICKED)));
    app.handle(Event::MessageDoubleClick, now);
    let now = settle(&mut app, now, is_absent);
    app.handle(Event::FullScreen(false), now);
    settle(&mut app, now, noticed);
    assert_eq!(notice(&app), Some((STRETCH, UNTICKED)));
}

#[test]
fn clippy_looks_up_at_an_open_field_and_takes_notes_once_it_has_the_keyboard() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    choose(&mut app, "Write...", now);
    let now = settle(&mut app, now, |app| {
        matches!(app.character.look(), Some(Look::AtTheBalloon(_)))
    });
    app.handle(Event::Focus, now);
    let now = settle(&mut app, now, |app| app.character.is_waiting_with_the_pad());
    app.handle(Event::Blur, now);
    let now = settle(&mut app, now, |app| {
        matches!(app.character.look(), Some(Look::AtTheBalloon(_)))
    });
    app.handle(Event::Cancel, now);
    settle(&mut app, now, |app| app.character.look().is_none());
}

#[test]
fn moving_clippy_turns_his_eyes_to_where_the_field_went() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    choose(&mut app, "Write...", now);
    let now = settle(&mut app, now, |app| {
        app.character.look() == Some(Look::AtTheBalloon(Side::Above))
    });
    app.place(Rect::new(1300, 16, 124, 93), SCREEN);
    app.handle(Event::Tick, now);
    let now = settle(&mut app, now, |app| {
        app.character.look() == Some(Look::AtTheBalloon(Side::Below))
    });
    app.place(CLIPPY, SCREEN);
    app.handle(Event::Tick, now);
    settle(&mut app, now, |app| {
        app.character.look() == Some(Look::AtTheBalloon(Side::Above))
    });
}

#[test]
fn clippy_writes_along_while_you_type() {
    let directory = TempDir::new().unwrap();
    let (mut app, now) = launch(&directory, Settings::default());
    choose(&mut app, "Write...", now);
    app.handle(Event::Focus, now);
    let now = settle(&mut app, now, |app| app.character.is_waiting_with_the_pad());
    edit(&mut app, Edit::Move(Motion::End), now);
    app.handle(Event::Tick, now + Duration::from_secs(1));
    assert!(!app.character.is_writing());
    let now = now + Duration::from_secs(1);
    edit(&mut app, Edit::Insert("Ship".into()), now);
    let now = settle(&mut app, now, |app| app.character.is_writing());
    app.handle(Event::Submit, now);
    settle(&mut app, now, |app| {
        app.character.look().is_none() && !app.character.is_writing()
    });
    assert_eq!(shown(&app).as_deref(), Some("Ship"));
}

#[test]
fn full_screen_holds_reminders_until_it_ends() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(9, 59);
    let (mut app, now) = with_chores(&directory, &day);
    app.handle(Event::FullScreen(true), now);
    settle(&mut app, now, is_absent);
    let later = day.at(10, 20);
    app.handle(Event::Tick, later);
    assert!(is_absent(&app));
    app.handle(Event::FullScreen(false), later);
    settle(&mut app, later, noticed);
    assert_eq!(notice(&app), Some((STRETCH, UNTICKED)));
    assert!(beckons(&app));
}

#[test]
fn leaving_for_full_screen_does_not_snooze() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(9, 59);
    let (mut app, now) = with_chores(&directory, &day);
    let now = settle(&mut app, now, noticed);
    app.handle(Event::FullScreen(true), now);
    let now = settle(&mut app, now, is_absent);
    app.handle(Event::FullScreen(false), now);
    settle(&mut app, now, noticed);
    assert_eq!(notice(&app), Some((STRETCH, UNTICKED)));
}

#[test]
fn quitting_keeps_the_reminder_for_the_next_launch() {
    let directory = TempDir::new().unwrap();
    let day = Day::starting_at(9, 59);
    let (mut app, now) = with_chores(&directory, &day);
    choose(&mut app, "Write...", now);
    answer(&mut app, "Ship it", now);
    let now = settle(&mut app, now, noticed);
    app.handle(Event::Command(Command::Quit), now);
    settle(&mut app, now, App::is_finished);

    let (mut restarted, now) = with_chores(&directory, &Day::starting_at(10, 10));
    settle(&mut restarted, now, noticed);
    assert_eq!(notice(&restarted), Some((STRETCH, UNTICKED)));
    assert_eq!(shown(&restarted), None);
}

#[test]
fn changes_wake_clippy_when_the_wall_clock_first_reaches_them() {
    let europe = TimeZone::posix("CET-1CEST,M3.5.0,M10.5.0/3").unwrap();
    let at = |day: Date, hour, minute| day.at(hour, minute, 0, 0);
    let utc = |text: &str| text.parse::<Timestamp>().unwrap();
    let zoned = |text: &str| utc(text).to_zoned(europe.clone());

    let spring = date(2026, 3, 29);
    let skipped = first_reached(at(spring, 2, 30), &zoned("2026-03-29T00:50:00Z"));
    assert_eq!(skipped, Some(utc("2026-03-29T01:00:00Z")));

    let autumn = date(2026, 10, 25);
    let first_time = first_reached(at(autumn, 2, 30), &zoned("2026-10-25T00:10:00Z"));
    assert_eq!(first_time, Some(utc("2026-10-25T00:30:00Z")));
    let second_time = first_reached(at(autumn, 2, 30), &zoned("2026-10-25T01:10:00Z"));
    assert_eq!(second_time, Some(utc("2026-10-25T01:30:00Z")));

    let past = first_reached(at(spring, 1, 0), &zoned("2026-03-29T00:50:00Z"));
    assert_eq!(past, None);
}

#[test]
fn reminders_keep_their_hour_when_the_clocks_spring_forward() {
    let directory = TempDir::new().unwrap();
    let europe = TimeZone::posix("CET-1CEST,M3.5.0,M10.5.0/3").unwrap();
    let day = Day::of(date(2026, 3, 29), europe, 1, 0);
    let (mut app, now) = with_chores(&directory, &day);
    let now = settle(&mut app, now, noticed);
    assert_eq!(now - day.origin, Duration::from_hours(8));
    assert_eq!(notice(&app), Some((STRETCH, UNTICKED)));
}
