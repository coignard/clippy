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

use crate::assets::FONT;
use crate::calendar::Agenda;
use crate::character::Character;
use crate::plugin::{Context, MenuItem, Notice, Plugin, Urgency};
use crate::shortcut::Shortcut;
use crate::storage::{Storage, StorageError};

pub(crate) struct Asked {
    pub(crate) question: String,
    pub(crate) draft: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Noticed {
    pub(crate) id: String,
    pub(crate) text: String,
    pub(crate) checkbox: Option<String>,
    pub(crate) over_full_screen: bool,
    pub(crate) shortcut: Option<Shortcut>,
    pub(crate) urgency: Urgency,
    pub(crate) celebrates: bool,
}

pub(crate) trait AnyPlugin {
    fn name(&self) -> &'static str;

    fn begin_day(&mut self, now: DateTime);

    fn reads_calendar(&self) -> bool;

    fn read_agenda(&mut self, agenda: &Agenda);

    fn menu(&self, now: DateTime) -> Vec<MenuItem<()>>;

    fn message(&self, now: DateTime) -> Option<String>;

    fn answers_double_click(&self, now: DateTime) -> bool;

    fn message_shortcut(&self, now: DateTime) -> Option<Shortcut>;

    fn notices(&self, now: DateTime) -> Vec<Noticed>;

    fn bedtime(&self, now: DateTime) -> bool;

    fn changes_at(&self, now: DateTime) -> Option<DateTime>;

    fn choose(
        &mut self,
        item: usize,
        listed: DateTime,
        character: &mut Character,
        now: DateTime,
    ) -> Option<Asked>;

    fn double_click(&mut self, character: &mut Character, now: DateTime) -> Option<Asked>;

    fn answer(&mut self, answer: String, character: &mut Character, now: DateTime)
    -> Option<Asked>;

    fn snooze(&mut self, id: &str, character: &mut Character, now: DateTime) -> Option<Asked>;

    fn check(&mut self, id: &str, character: &mut Character, now: DateTime) -> Option<Asked>;

    fn dismiss(&mut self);

    fn refresh(&mut self, storage: &Storage);

    fn save(&mut self, storage: &Storage) -> Result<(), StorageError>;
}

pub(crate) struct Installed<P: Plugin> {
    plugin: P,
    settings: P::Settings,
    agenda: Agenda,
    on_answer: Option<Box<dyn FnOnce(String) -> P::Action>>,
    unsaved: bool,
}

impl<P: Plugin> Installed<P> {
    pub(crate) fn new(plugin: P, settings: P::Settings) -> Self {
        Self {
            plugin,
            settings,
            agenda: Agenda::default(),
            on_answer: None,
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

    fn perform(
        &mut self,
        action: P::Action,
        character: &mut Character,
        now: DateTime,
    ) -> Option<Asked> {
        let mut cx = Context {
            character,
            question: None,
            now,
        };
        self.plugin.perform(action, &mut cx);
        self.unsaved = true;
        let question = cx.question?;
        self.on_answer = Some(question.answer);
        Some(Asked {
            question: question.text,
            draft: question.draft,
        })
    }

    fn notice(&self, id: &str, now: DateTime) -> Option<Notice<P::Action>> {
        self.plugin
            .notices(&self.settings, now)
            .into_iter()
            .find(|notice| notice.id == id)
    }
}

impl<P: Plugin> AnyPlugin for Installed<P> {
    fn name(&self) -> &'static str {
        P::NAME
    }

    fn begin_day(&mut self, now: DateTime) {
        self.plugin.begin_day(&self.settings, now);
        self.unsaved = true;
    }

    fn reads_calendar(&self) -> bool {
        P::READS_CALENDAR
    }

    fn read_agenda(&mut self, agenda: &Agenda) {
        if P::READS_CALENDAR {
            self.agenda = agenda.clone();
            self.plugin.read_agenda(agenda);
        }
    }

    fn menu(&self, now: DateTime) -> Vec<MenuItem<()>> {
        self.plugin
            .menu(&self.settings, now)
            .into_iter()
            .map(|item| MenuItem {
                title: item.title,
                action: item.action.map(drop),
                enabled: item.enabled,
                shortcut: item.shortcut,
            })
            .collect()
    }

    fn message(&self, now: DateTime) -> Option<String> {
        self.plugin
            .message(&self.settings, now)
            .map(|message| FONT.printable(&message.text).into_owned())
            .filter(|text| !text.is_empty())
    }

    fn answers_double_click(&self, now: DateTime) -> bool {
        self.message(now).is_some()
            && self
                .plugin
                .message(&self.settings, now)
                .is_some_and(|message| message.on_double_click.is_some())
    }

    fn message_shortcut(&self, now: DateTime) -> Option<Shortcut> {
        self.message(now)?;
        self.plugin.message(&self.settings, now)?.shortcut
    }

    fn notices(&self, now: DateTime) -> Vec<Noticed> {
        self.plugin
            .notices(&self.settings, now)
            .into_iter()
            .map(|notice| Noticed {
                id: notice.id,
                text: FONT.printable(&notice.text).into_owned(),
                checkbox: notice
                    .checkbox
                    .map(|(label, _)| FONT.printable(&label).into_owned()),
                over_full_screen: notice.over_full_screen,
                shortcut: notice.shortcut,
                urgency: notice.urgency,
                celebrates: notice.celebrates,
            })
            .collect()
    }

    fn bedtime(&self, now: DateTime) -> bool {
        self.plugin.bedtime(&self.settings, now)
    }

    fn changes_at(&self, now: DateTime) -> Option<DateTime> {
        self.plugin.changes_at(&self.settings, now)
    }

    fn choose(
        &mut self,
        item: usize,
        listed: DateTime,
        character: &mut Character,
        now: DateTime,
    ) -> Option<Asked> {
        let item = self
            .plugin
            .menu(&self.settings, listed)
            .into_iter()
            .nth(item)
            .filter(|item| item.enabled)?;
        self.perform(item.action?, character, now)
    }

    fn double_click(&mut self, character: &mut Character, now: DateTime) -> Option<Asked> {
        let action = self.plugin.message(&self.settings, now)?.on_double_click?;
        self.perform(action, character, now)
    }

    fn answer(
        &mut self,
        answer: String,
        character: &mut Character,
        now: DateTime,
    ) -> Option<Asked> {
        let on_answer = self.on_answer.take()?;
        self.perform(on_answer(answer), character, now)
    }

    fn snooze(&mut self, id: &str, character: &mut Character, now: DateTime) -> Option<Asked> {
        let notice = self.notice(id, now)?;
        self.perform(notice.on_snooze, character, now)
    }

    fn check(&mut self, id: &str, character: &mut Character, now: DateTime) -> Option<Asked> {
        let (_, on_check) = self.notice(id, now)?.checkbox?;
        self.perform(on_check, character, now)
    }

    fn dismiss(&mut self) {
        self.on_answer = None;
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
