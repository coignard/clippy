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

use jiff::civil::DateTime;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::animation::Animation;
use crate::calendar::Agenda;
use crate::character::Character;
use crate::shortcut::Shortcut;

pub(crate) use installed::{AnyPlugin, Installed};
pub use plugins::PluginItem;
pub(crate) use plugins::{Due, Plugins};

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
    question: Option<Question<P::Action>>,
    toggles_checklist: bool,
    now: DateTime,
}

impl<P: Plugin> Context<'_, P> {
    pub fn now(&self) -> DateTime {
        self.now
    }

    pub fn ask(&mut self, question: Question<P::Action>) {
        self.question = Some(question);
    }

    pub fn toggle_checklist(&mut self) {
        self.toggles_checklist = !self.toggles_checklist;
    }

    pub fn play(&mut self, animation: Animation) {
        self.character.play(animation);
    }

    pub fn celebrate(&mut self) {
        self.character.celebrate();
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoSettings {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem<A> {
    title: String,
    action: Option<A>,
    enabled: bool,
    shortcut: Option<Shortcut>,
}

impl<A> MenuItem<A> {
    pub fn new(title: impl Into<String>, action: A) -> Self {
        Self {
            title: title.into(),
            action: Some(action),
            enabled: true,
            shortcut: None,
        }
    }

    pub fn label(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            action: None,
            enabled: false,
            shortcut: None,
        }
    }

    #[must_use]
    pub fn enabled(self, enabled: bool) -> Self {
        Self {
            enabled: enabled && self.action.is_some(),
            ..self
        }
    }

    #[must_use]
    pub fn shortcut(self, shortcut: Shortcut) -> Self {
        Self {
            shortcut: Some(shortcut),
            ..self
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message<A> {
    text: String,
    on_double_click: Option<A>,
    shortcut: Option<Shortcut>,
}

impl<A> Message<A> {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            on_double_click: None,
            shortcut: None,
        }
    }

    #[must_use]
    pub fn on_double_click(self, action: A) -> Self {
        Self {
            on_double_click: Some(action),
            ..self
        }
    }

    #[must_use]
    pub fn shortcut(self, shortcut: Shortcut) -> Self {
        Self {
            shortcut: Some(shortcut),
            ..self
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice<A> {
    id: String,
    text: String,
    on_snooze: A,
    checkbox: Option<(String, A)>,
    over_full_screen: bool,
    shortcut: Option<Shortcut>,
    check_shortcut: Option<Shortcut>,
    urgency: Urgency,
    celebrates: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Urgency {
    Asked,
    #[default]
    Normal,
    Pressing,
}

impl<A> Notice<A> {
    pub fn new(id: impl Into<String>, text: impl Into<String>, on_snooze: A) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            on_snooze,
            checkbox: None,
            over_full_screen: false,
            shortcut: None,
            check_shortcut: None,
            urgency: Urgency::Normal,
            celebrates: true,
        }
    }

    #[must_use]
    pub fn checkbox(self, label: impl Into<String>, on_check: A) -> Self {
        Self {
            checkbox: Some((label.into(), on_check)),
            ..self
        }
    }

    #[must_use]
    pub fn over_full_screen(self, over_full_screen: bool) -> Self {
        Self {
            over_full_screen,
            ..self
        }
    }

    #[must_use]
    pub fn shortcut(self, shortcut: Shortcut) -> Self {
        Self {
            shortcut: Some(shortcut),
            ..self
        }
    }

    #[must_use]
    pub fn check_shortcut(self, shortcut: Shortcut) -> Self {
        Self {
            check_shortcut: Some(shortcut),
            ..self
        }
    }

    #[must_use]
    pub fn urgency(self, urgency: Urgency) -> Self {
        Self { urgency, ..self }
    }

    #[must_use]
    pub fn celebrates(self, celebrates: bool) -> Self {
        Self { celebrates, ..self }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checklist<A> {
    title: String,
    items: Vec<ChecklistItem<A>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ChecklistItem<A> {
    label: String,
    checked: bool,
    on_toggle: A,
}

impl<A> Checklist<A> {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            items: Vec::new(),
        }
    }

    #[must_use]
    pub fn item(mut self, label: impl Into<String>, checked: bool, on_toggle: A) -> Self {
        self.items.push(ChecklistItem {
            label: label.into(),
            checked,
            on_toggle,
        });
        self
    }
}

pub struct Question<A> {
    text: String,
    draft: String,
    answer: Box<dyn FnOnce(String) -> A>,
}

impl<A> Question<A> {
    pub fn new(text: impl Into<String>, answer: impl FnOnce(String) -> A + 'static) -> Self {
        Self {
            text: text.into(),
            draft: String::new(),
            answer: Box::new(answer),
        }
    }

    #[must_use]
    pub fn draft(self, draft: impl Into<String>) -> Self {
        Self {
            draft: draft.into(),
            ..self
        }
    }
}
