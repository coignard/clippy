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

use std::mem;

use jiff::civil::DateTime;

use crate::calendar::Agenda;
use crate::character::Character;
use crate::plugin::{Answers, Context, Item, Listed, Noticed, Outcome, Plugin, Question, Said};
use crate::storage::{Storage, StorageError};

pub(crate) enum Trigger<'a> {
    Choose { item: usize, listed: DateTime },
    DoubleClick,
    Answer { text: String, shift: bool },
    TickAsked(usize),
    Snooze(&'a str),
    Check(&'a str),
    ToggleTask(&'a str, usize),
    Toggle(usize),
}

pub(crate) trait AnyPlugin {
    fn name(&self) -> &'static str;

    fn reads_calendar(&self) -> bool;

    fn begin_day(&mut self, now: DateTime);

    fn read_agenda(&mut self, agenda: &Agenda);

    fn menu(&self, now: DateTime) -> Vec<Item>;

    fn message(&self, now: DateTime) -> Option<Said>;

    fn notices(&self, now: DateTime) -> Vec<Noticed>;

    fn checklist(&self, now: DateTime) -> Option<Listed>;

    fn bedtime(&self, now: DateTime) -> bool;

    fn changes_at(&self, now: DateTime) -> Option<DateTime>;

    fn act(
        &mut self,
        trigger: Trigger<'_>,
        question_shown: bool,
        character: &mut Character,
        now: DateTime,
    ) -> Outcome;

    fn dismiss(&mut self);

    fn refresh(&mut self, storage: &Storage);

    fn save(&mut self, storage: &Storage) -> Result<(), StorageError>;
}

pub(crate) struct Installed<P: Plugin> {
    plugin: P,
    settings: P::Settings,
    agenda: Agenda,
    answers: Option<Answers<P::Action>>,
    unsaved: bool,
}

impl<P: Plugin> Installed<P> {
    pub(crate) fn new(plugin: P, settings: P::Settings) -> Self {
        Self {
            plugin,
            settings,
            agenda: Agenda::default(),
            answers: None,
            unsaved: false,
        }
    }

    pub(crate) fn load(storage: &Storage) -> Self {
        let plugin = storage.load(P::NAME).unwrap_or_else(|error| {
            log::warn!("Starting {} afresh: {error}", P::NAME);
            None
        });
        let settings = storage
            .load_plugin_settings(P::NAME)
            .unwrap_or_else(|error| {
                log::warn!("Using default {} settings: {error}", P::NAME);
                P::Settings::default()
            });
        Self::new(plugin.unwrap_or_default(), settings)
    }

    fn action(&mut self, trigger: Trigger<'_>, now: DateTime) -> Option<P::Action> {
        let (plugin, settings) = (&self.plugin, &self.settings);
        let notice = |id: &str| {
            let mut notices = plugin.notices(settings, now).into_iter();
            notices.find(|notice| notice.view.id == id)
        };
        match trigger {
            Trigger::Choose { item, listed } => {
                let item = plugin.menu(settings, listed).into_iter().nth(item)?;
                item.action.filter(|_| item.view.enabled)
            }
            Trigger::DoubleClick => plugin.message(settings, now)?.on_double_click,
            Trigger::Answer { text, shift } => {
                let answers = self.answers.take()?;
                let answer = answers.shift_enter.filter(|_| shift);
                Some(answer.unwrap_or(answers.enter)(text))
            }
            Trigger::TickAsked(task) => {
                let ticks = &mut self.answers.as_mut()?.ticks;
                (task < ticks.len()).then(|| ticks.remove(task))
            }
            Trigger::Snooze(id) => Some(notice(id)?.on_snooze),
            Trigger::Check(id) => notice(id)?.on_check,
            Trigger::ToggleTask(id, task) => notice(id)?.on_toggle.into_iter().nth(task),
            Trigger::Toggle(item) => {
                let checklist = plugin.checklist(settings, now)?;
                checklist.on_toggle.into_iter().nth(item)
            }
        }
    }
}

impl<P: Plugin> AnyPlugin for Installed<P> {
    fn name(&self) -> &'static str {
        P::NAME
    }

    fn reads_calendar(&self) -> bool {
        P::READS_CALENDAR
    }

    fn begin_day(&mut self, now: DateTime) {
        self.plugin.begin_day(&self.settings, now);
        self.unsaved = true;
    }

    fn read_agenda(&mut self, agenda: &Agenda) {
        if P::READS_CALENDAR {
            self.agenda = agenda.clone();
            self.plugin.read_agenda(agenda);
        }
    }

    fn menu(&self, now: DateTime) -> Vec<Item> {
        let menu = self.plugin.menu(&self.settings, now);
        menu.into_iter().map(|item| item.view).collect()
    }

    fn message(&self, now: DateTime) -> Option<Said> {
        let message = self.plugin.message(&self.settings, now)?;
        Some(message.view).filter(|said| !said.text.is_empty())
    }

    fn notices(&self, now: DateTime) -> Vec<Noticed> {
        let notices = self.plugin.notices(&self.settings, now);
        notices.into_iter().map(|notice| notice.view).collect()
    }

    fn checklist(&self, now: DateTime) -> Option<Listed> {
        let checklist = self.plugin.checklist(&self.settings, now)?;
        Some(checklist.view).filter(|listed| !listed.items.is_empty())
    }

    fn bedtime(&self, now: DateTime) -> bool {
        self.plugin.bedtime(&self.settings, now)
    }

    fn changes_at(&self, now: DateTime) -> Option<DateTime> {
        self.plugin.changes_at(&self.settings, now)
    }

    fn act(
        &mut self,
        trigger: Trigger<'_>,
        question_shown: bool,
        character: &mut Character,
        now: DateTime,
    ) -> Outcome {
        let Some(action) = self.action(trigger, now) else {
            return Outcome::default();
        };
        let mut cx = Context {
            character,
            settings: &self.settings,
            now,
            question_shown,
            question: None,
            outcome: Outcome::default(),
        };
        self.plugin.perform(action, &mut cx);
        self.unsaved = true;
        let Context {
            question,
            mut outcome,
            ..
        } = cx;
        if let Some(Question { view, answers }) = question {
            self.answers = Some(answers);
            outcome.asked = Some(view);
        }
        outcome
    }

    fn dismiss(&mut self) {
        self.answers = None;
    }

    fn refresh(&mut self, storage: &Storage) {
        if self.unsaved {
            return;
        }
        match storage.load(P::NAME) {
            Ok(plugin) => {
                self.plugin = plugin.unwrap_or_default();
                if P::READS_CALENDAR {
                    self.plugin.read_agenda(&self.agenda);
                }
            }
            Err(error) => log::warn!("Keeping {} as it was: {error}", P::NAME),
        }
    }

    fn save(&mut self, storage: &Storage) -> Result<(), StorageError> {
        if mem::take(&mut self.unsaved) {
            storage.save(P::NAME, &self.plugin)
        } else {
            Ok(())
        }
    }
}
