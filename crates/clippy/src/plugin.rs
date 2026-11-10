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

mod installed;
mod plugins;

use jiff::SignedDuration;
use jiff::civil::DateTime;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::assets::FONT;
use crate::balloon::Asked;
use crate::calendar::Agenda;
use crate::character::Character;
use crate::hours::{TimeOfDay, midnight_after};
use crate::shortcut::Shortcut;

pub(crate) use installed::{AnyPlugin, Installed};
pub use plugins::PluginItem;
pub(crate) use plugins::{Due, Plugins};

const PAUSE: SignedDuration = SignedDuration::from_hours(1);

pub trait Plugin: Default + Serialize + DeserializeOwned + 'static {
    const NAME: &'static str;

    const READS_CALENDAR: bool = false;

    type Action;

    type Settings: Default + Serialize + DeserializeOwned;

    fn begin_day(&mut self, _settings: &Self::Settings, _now: DateTime) {}

    fn read_agenda(&mut self, _agenda: &Agenda) {}

    fn menu(&self, _settings: &Self::Settings, _now: DateTime) -> Vec<MenuItem<Self::Action>> {
        Vec::new()
    }

    fn message(&self, _settings: &Self::Settings, _now: DateTime) -> Option<Message<Self::Action>> {
        None
    }

    fn notices(&self, _settings: &Self::Settings, _now: DateTime) -> Vec<Notice<Self::Action>> {
        Vec::new()
    }

    fn checklist(
        &self,
        _settings: &Self::Settings,
        _now: DateTime,
    ) -> Option<Checklist<Self::Action>> {
        None
    }

    fn bedtime(&self, _settings: &Self::Settings, _now: DateTime) -> bool {
        false
    }

    fn changes_at(&self, _settings: &Self::Settings, _now: DateTime) -> Option<DateTime> {
        None
    }

    fn perform(&mut self, action: Self::Action, cx: &mut Context<'_, Self>);
}

pub struct Context<'a, P: Plugin> {
    character: &'a mut Character,
    settings: &'a P::Settings,
    now: DateTime,
    question_shown: bool,
    question: Option<Question<P::Action>>,
    outcome: Outcome,
}

#[derive(Default)]
pub(crate) struct Outcome {
    pub(crate) asked: Option<Asked>,
    pub(crate) toggles_field: bool,
    pub(crate) toggles_checklist: bool,
    pub(crate) copied: Option<String>,
}

impl<P: Plugin> Context<'_, P> {
    pub fn now(&self) -> DateTime {
        self.now
    }

    pub fn settings(&self) -> &P::Settings {
        self.settings
    }

    pub fn ask(&mut self, question: Question<P::Action>) {
        self.question = Some(question);
    }

    pub fn toggle_question(&mut self, question: Question<P::Action>) {
        if self.question_shown {
            self.outcome.toggles_field = true;
        } else {
            self.ask(question);
        }
    }

    pub fn copy(&mut self, text: impl Into<String>) {
        self.outcome.copied = Some(text.into());
    }

    pub fn toggle_checklist(&mut self) {
        self.outcome.toggles_checklist = !self.outcome.toggles_checklist;
    }

    pub fn celebrate(&mut self) {
        self.character.celebrate();
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoSettings {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Urgency {
    Asked,
    #[default]
    Normal,
    Pressing,
}

#[must_use]
pub struct MenuItem<A> {
    view: Item,
    action: Option<A>,
}

pub(crate) struct Item {
    pub(crate) title: String,
    pub(crate) enabled: bool,
    pub(crate) shortcut: Option<Shortcut>,
}

impl<A> MenuItem<A> {
    pub fn new(title: impl Into<String>, action: A) -> Self {
        let mut item = Self::label(title);
        item.view.enabled = true;
        item.action = Some(action);
        item
    }

    pub fn label(title: impl Into<String>) -> Self {
        let view = Item {
            title: title.into(),
            enabled: false,
            shortcut: None,
        };
        Self { view, action: None }
    }

    pub fn pausing(
        paused_until: Option<DateTime>,
        now: DateTime,
        pause: impl Fn(DateTime) -> A,
        resume: A,
    ) -> Vec<Self> {
        let Some(until) = paused_until.filter(|&until| now < until) else {
            return vec![
                Self::new("Pause for an Hour", pause(now.saturating_add(PAUSE))),
                Self::new("Pause Until Tomorrow", pause(midnight_after(now))),
            ];
        };
        let when = if until == midnight_after(now) {
            "Tomorrow".to_owned()
        } else {
            TimeOfDay::from(until.time()).to_string()
        };
        vec![
            Self::label(format!("Paused Until {when}")),
            Self::new("Resume Reminders", resume),
        ]
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.view.enabled = enabled && self.action.is_some();
        self
    }

    pub fn shortcut(mut self, shortcut: Shortcut) -> Self {
        self.view.shortcut = Some(shortcut);
        self
    }
}

#[must_use]
pub struct Message<A> {
    view: Said,
    on_double_click: Option<A>,
}

pub(crate) struct Said {
    pub(crate) text: String,
    pub(crate) shortcut: Option<Shortcut>,
    pub(crate) answers_double_click: bool,
}

impl<A> Message<A> {
    pub fn new(text: impl AsRef<str>) -> Self {
        let view = Said {
            text: printable(text),
            shortcut: None,
            answers_double_click: false,
        };
        Self {
            view,
            on_double_click: None,
        }
    }

    pub fn on_double_click(mut self, action: A) -> Self {
        self.view.answers_double_click = true;
        self.on_double_click = Some(action);
        self
    }

    pub fn shortcut(mut self, shortcut: Shortcut) -> Self {
        self.view.shortcut = Some(shortcut);
        self
    }
}

#[must_use]
pub struct Notice<A> {
    view: Noticed,
    on_snooze: A,
    on_toggle: Vec<A>,
    on_check: Option<A>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Noticed {
    pub(crate) id: String,
    pub(crate) text: String,
    pub(crate) tasks: Vec<(String, bool)>,
    pub(crate) checkbox: Option<String>,
    pub(crate) over_full_screen: bool,
    pub(crate) shortcut: Option<Shortcut>,
    pub(crate) check_shortcut: Option<Shortcut>,
    pub(crate) urgency: Urgency,
    pub(crate) celebrates: bool,
}

impl<A> Notice<A> {
    pub fn new(id: impl Into<String>, text: impl AsRef<str>, on_snooze: A) -> Self {
        let view = Noticed {
            id: id.into(),
            text: printable(text),
            tasks: Vec::new(),
            checkbox: None,
            over_full_screen: false,
            shortcut: None,
            check_shortcut: None,
            urgency: Urgency::Normal,
            celebrates: true,
        };
        Self {
            view,
            on_snooze,
            on_toggle: Vec::new(),
            on_check: None,
        }
    }

    pub fn task(mut self, label: impl AsRef<str>, done: bool, on_toggle: A) -> Self {
        self.view.tasks.push((printable(label), done));
        self.on_toggle.push(on_toggle);
        self
    }

    pub fn checkbox(mut self, label: impl AsRef<str>, on_check: A) -> Self {
        self.view.checkbox = Some(printable(label));
        self.on_check = Some(on_check);
        self
    }

    pub fn over_full_screen(mut self, over_full_screen: bool) -> Self {
        self.view.over_full_screen = over_full_screen;
        self
    }

    pub fn shortcut(mut self, shortcut: Shortcut) -> Self {
        self.view.shortcut = Some(shortcut);
        self
    }

    pub fn check_shortcut(mut self, shortcut: Shortcut) -> Self {
        self.view.check_shortcut = Some(shortcut);
        self
    }

    pub fn urgency(mut self, urgency: Urgency) -> Self {
        self.view.urgency = urgency;
        self
    }

    pub fn celebrates(mut self, celebrates: bool) -> Self {
        self.view.celebrates = celebrates;
        self
    }
}

#[must_use]
pub struct Checklist<A> {
    view: Listed,
    on_toggle: Vec<A>,
}

pub(crate) struct Listed {
    pub(crate) title: String,
    pub(crate) items: Vec<(String, bool)>,
}

impl<A> Checklist<A> {
    pub fn new(title: impl AsRef<str>) -> Self {
        let view = Listed {
            title: printable(title),
            items: Vec::new(),
        };
        Self {
            view,
            on_toggle: Vec::new(),
        }
    }

    pub fn item(mut self, label: impl AsRef<str>, checked: bool, on_toggle: A) -> Self {
        self.view.items.push((printable(label), checked));
        self.on_toggle.push(on_toggle);
        self
    }
}

#[must_use]
pub struct Question<A> {
    view: Asked,
    answers: Answers<A>,
}

type Answer<A> = Box<dyn FnOnce(String) -> A>;

struct Answers<A> {
    enter: Answer<A>,
    shift_enter: Option<Answer<A>>,
    ticks: Vec<A>,
}

impl<A> Question<A> {
    pub fn new(text: impl Into<String>, answer: impl FnOnce(String) -> A + 'static) -> Self {
        let view = Asked {
            question: text.into(),
            ..Asked::default()
        };
        let answers = Answers {
            enter: Box::new(answer),
            shift_enter: None,
            ticks: Vec::new(),
        };
        Self { view, answers }
    }

    pub fn shortcut(mut self, shortcut: Shortcut) -> Self {
        self.view.shortcut = Some(shortcut);
        self
    }

    pub fn standing(mut self) -> Self {
        self.view.standing = true;
        self
    }

    pub fn on_shift_enter(mut self, answer: impl FnOnce(String) -> A + 'static) -> Self {
        self.answers.shift_enter = Some(Box::new(answer));
        self
    }

    pub fn task(mut self, label: impl AsRef<str>, on_tick: A) -> Self {
        self.view.tasks.push(printable(label));
        self.answers.ticks.push(on_tick);
        self
    }

    pub fn draft(mut self, draft: impl Into<String>) -> Self {
        self.view.draft = draft.into();
        self
    }
}

fn printable(text: impl AsRef<str>) -> String {
    FONT.printable(text.as_ref()).into_owned()
}
