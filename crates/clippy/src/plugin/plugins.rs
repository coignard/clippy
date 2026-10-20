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

use jiff::civil::DateTime;

use crate::app::{Command, Moment};
use crate::balloon::Prompt;
use crate::character::Character;
use crate::menu::Entry;
use crate::plugin::MenuItem;
use crate::plugin::installed::{AnyPlugin, Asked, Noticed};
use crate::shortcut::Shortcut;
use crate::storage::Storage;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PluginItem {
    plugin: usize,
    item: usize,
    at: DateTime,
}

#[derive(Default)]
pub(crate) struct Plugins {
    installed: Vec<Box<dyn AnyPlugin>>,
    storage: Option<Storage>,
    asking: Option<Asking>,
    questions: usize,
}

struct Asking {
    plugin: usize,
    prompt: Prompt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Due {
    pub(crate) plugin: usize,
    pub(crate) notice: Noticed,
}

impl Due {
    pub(crate) fn is(&self, other: &Self) -> bool {
        self.plugin == other.plugin && self.notice.id == other.notice.id
    }
}

impl Plugins {
    pub(crate) fn install(&mut self, plugin: Box<dyn AnyPlugin>) {
        assert!(
            self.installed
                .iter()
                .all(|installed| installed.name() != plugin.name()),
            "{} should be installed only once",
            plugin.name()
        );
        self.installed.push(plugin);
    }

    pub(crate) fn contains(&self, name: &str) -> bool {
        self.installed
            .iter()
            .any(|installed| installed.name().eq_ignore_ascii_case(name))
    }

    pub(crate) fn keep_in(&mut self, storage: Storage) {
        self.storage = Some(storage);
    }

    pub(crate) fn begin_day(&mut self, now: DateTime) {
        for installed in &mut self.installed {
            if let Some(storage) = &self.storage {
                installed.refresh(storage);
            }
            installed.begin_day(now);
        }
    }

    pub(crate) fn shortcuts(
        &self,
        now: DateTime,
    ) -> impl Iterator<Item = (&'static str, Shortcut)> {
        self.installed.iter().flat_map(move |installed| {
            let name = installed.name();
            installed
                .menu(now)
                .into_iter()
                .filter_map(move |item| Some((name, item.shortcut?)))
        })
    }

    pub(crate) fn active_shortcuts(&self, now: DateTime) -> impl Iterator<Item = Shortcut> {
        self.installed.iter().flat_map(move |installed| {
            installed
                .menu(now)
                .into_iter()
                .filter(|item| item.enabled)
                .filter_map(|item| item.shortcut)
        })
    }

    pub(crate) fn item_for(&self, shortcut: Shortcut, now: DateTime) -> Option<PluginItem> {
        self.installed
            .iter()
            .enumerate()
            .find_map(|(plugin, installed)| {
                let item = installed
                    .menu(now)
                    .iter()
                    .position(|item| item.enabled && item.shortcut == Some(shortcut))?;
                Some(PluginItem {
                    plugin,
                    item,
                    at: now,
                })
            })
    }

    pub(crate) fn menu(&self, now: DateTime) -> impl Iterator<Item = Entry> {
        self.installed
            .iter()
            .enumerate()
            .filter_map(move |(plugin, installed)| {
                let entries: Vec<Entry> = installed
                    .menu(now)
                    .into_iter()
                    .enumerate()
                    .map(
                        |(
                            item,
                            MenuItem {
                                title,
                                enabled,
                                shortcut,
                                ..
                            },
                        )| Entry::Item {
                            title,
                            command: Command::Plugin(PluginItem {
                                plugin,
                                item,
                                at: now,
                            }),
                            enabled,
                            shortcut,
                        },
                    )
                    .collect();
                (!entries.is_empty()).then(|| Entry::Submenu {
                    title: installed.name().to_owned(),
                    entries,
                })
            })
    }

    pub(crate) fn message(&self) -> Option<String> {
        self.installed
            .iter()
            .find_map(|installed| installed.message())
    }

    pub(crate) fn notices(&self, now: DateTime) -> Vec<Due> {
        (0..)
            .zip(&self.installed)
            .flat_map(|(plugin, installed)| {
                installed
                    .notices(now)
                    .into_iter()
                    .map(move |notice| Due { plugin, notice })
            })
            .collect()
    }

    pub(crate) fn bedtime(&self, now: DateTime) -> bool {
        self.installed
            .iter()
            .any(|installed| installed.bedtime(now))
    }

    pub(crate) fn changes_at(&self, now: DateTime) -> Option<DateTime> {
        self.installed
            .iter()
            .filter_map(|installed| installed.changes_at(now))
            .min()
    }

    pub(crate) fn asker(&self) -> Option<usize> {
        self.asking.as_ref().map(|asking| asking.plugin)
    }

    pub(crate) fn questions_asked(&self) -> usize {
        self.questions
    }

    pub(crate) fn shows_anything(&self, plugin: usize, due: &[Due]) -> bool {
        self.asker() == Some(plugin)
            || self
                .installed
                .get(plugin)
                .is_some_and(|installed| installed.message().is_some())
            || due.iter().any(|due| due.plugin == plugin)
    }

    pub(crate) fn prompt(&self) -> Option<&Prompt> {
        self.asking.as_ref().map(|asking| &asking.prompt)
    }

    pub(crate) fn prompt_mut(&mut self) -> Option<&mut Prompt> {
        self.asking.as_mut().map(|asking| &mut asking.prompt)
    }

    pub(crate) fn choose(&mut self, item: PluginItem, character: &mut Character, moment: Moment) {
        self.act(item.plugin, moment, |installed| {
            installed.choose(item.item, item.at, character, moment.local)
        });
    }

    pub(crate) fn double_click(&mut self, character: &mut Character, moment: Moment) {
        if self.asking.is_some() {
            return;
        }
        let owner = self
            .installed
            .iter()
            .position(|installed| installed.message().is_some());
        if let Some(plugin) = owner {
            self.act(plugin, moment, |installed| {
                installed.double_click(character, moment.local)
            });
        }
    }

    pub(crate) fn submit(&mut self, character: &mut Character, moment: Moment) {
        let Some(Asking { plugin, prompt }) = self.asking.take() else {
            return;
        };
        let answer = prompt.into_answer();
        self.act(plugin, moment, |installed| {
            installed.answer(answer, character, moment.local)
        });
    }

    pub(crate) fn snooze(&mut self, due: &Due, character: &mut Character, moment: Moment) {
        self.act(due.plugin, moment, |installed| {
            installed.snooze(&due.notice.id, character, moment.local)
        });
    }

    pub(crate) fn check(&mut self, due: &Due, character: &mut Character, moment: Moment) {
        self.act(due.plugin, moment, |installed| {
            installed.check(&due.notice.id, character, moment.local)
        });
    }

    pub(crate) fn dismiss(&mut self) {
        if let Some(Asking { plugin, .. }) = self.asking.take() {
            self.installed[plugin].dismiss();
        }
    }

    pub(crate) fn save(&mut self) {
        let Some(storage) = &self.storage else {
            return;
        };
        for installed in &mut self.installed {
            if let Err(error) = installed.save(storage) {
                log::error!("Could not save {}: {error}", installed.name());
            }
        }
    }

    fn act(
        &mut self,
        plugin: usize,
        moment: Moment,
        perform: impl FnOnce(&mut dyn AnyPlugin) -> Option<Asked>,
    ) {
        let Some(installed) = self.installed.get_mut(plugin) else {
            return;
        };
        if let Some(storage) = &self.storage {
            installed.refresh(storage);
        }
        let Some(Asked { question, draft }) = perform(installed.as_mut()) else {
            return;
        };
        if let Some(asking) = &mut self.asking
            && asking.plugin == plugin
            && asking.prompt.question() == question
        {
            asking.prompt.request_focus();
            self.questions += 1;
            return;
        }
        if let Some(previous) = self.asking.take()
            && previous.plugin != plugin
        {
            self.installed[previous.plugin].dismiss();
        }
        self.asking = Some(Asking {
            plugin,
            prompt: Prompt::new(question, draft, moment.instant),
        });
        self.questions += 1;
    }
}
