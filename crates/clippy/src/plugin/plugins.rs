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
use crate::balloon::{Checkbox, Prompt};
use crate::calendar::Agenda;
use crate::character::Character;
use crate::menu::Entry;
use crate::plugin::MenuItem;
use crate::plugin::installed::{AnyPlugin, Asked, Listed, Noticed};
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
    showing: Option<Showing>,
    checklists: usize,
}

struct Asking {
    plugin: usize,
    prompt: Prompt,
}

struct Showing {
    plugin: usize,
    title: String,
    items: Vec<(String, Checkbox)>,
    press: Option<(usize, bool)>,
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

    pub(crate) fn reads_calendar(&self) -> bool {
        self.installed
            .iter()
            .any(|installed| installed.reads_calendar())
    }

    pub(crate) fn read_agenda(&mut self, agenda: &Agenda) {
        for installed in &mut self.installed {
            installed.read_agenda(agenda);
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

    pub(crate) fn message(&self, now: DateTime) -> Option<String> {
        let messages: Vec<String> = self
            .installed
            .iter()
            .filter_map(|installed| installed.message(now))
            .collect();
        (!messages.is_empty()).then(|| messages.join("\n\n"))
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

    pub(crate) fn message_shortcuts(&self, now: DateTime) -> impl Iterator<Item = Shortcut> {
        self.installed
            .iter()
            .filter_map(move |installed| installed.message_shortcut(now))
    }

    pub(crate) fn press_message(
        &mut self,
        shortcut: Shortcut,
        character: &mut Character,
        moment: Moment,
    ) -> bool {
        let Some(plugin) = self
            .installed
            .iter()
            .position(|installed| installed.message_shortcut(moment.local) == Some(shortcut))
        else {
            return false;
        };
        self.act(plugin, moment, |installed| {
            installed.double_click(character, moment.local)
        });
        true
    }

    pub(crate) fn shows_anything(&self, plugin: usize, due: &[Due], now: DateTime) -> bool {
        self.asker() == Some(plugin)
            || self.lister() == Some(plugin)
            || self
                .installed
                .get(plugin)
                .is_some_and(|installed| installed.message(now).is_some())
            || due.iter().any(|due| due.plugin == plugin)
    }

    pub(crate) fn lister(&self) -> Option<usize> {
        self.showing.as_ref().map(|showing| showing.plugin)
    }

    pub(crate) fn checklists_shown(&self) -> usize {
        self.checklists
    }

    pub(crate) fn checklist(&self) -> Option<(&str, &[(String, Checkbox)])> {
        let showing = self.showing.as_ref()?;
        Some((&showing.title, &showing.items))
    }

    pub(crate) fn refresh_checklist(&mut self, now: DateTime) {
        let Some(showing) = &mut self.showing else {
            return;
        };
        let Some(Listed { title, items }) = self.installed[showing.plugin].checklist(now) else {
            self.showing = None;
            return;
        };
        if showing.press.is_some_and(|(item, _)| item >= items.len()) {
            showing.press = None;
        }
        showing.title = title;
        showing.items = (0..)
            .zip(items)
            .map(|(item, (label, checked))| {
                let pressed = showing.press == Some((item, true));
                (label, Checkbox { checked, pressed })
            })
            .collect();
    }

    pub(crate) fn press_checklist(&mut self, item: usize) {
        if let Some(showing) = &mut self.showing
            && item < showing.items.len()
        {
            showing.press = Some((item, true));
        }
    }

    pub(crate) fn drag_checklist(&mut self, inside: bool) {
        if let Some(Showing {
            press: Some((_, pressing)),
            ..
        }) = &mut self.showing
        {
            *pressing = inside;
        }
    }

    pub(crate) fn release_checklist(
        &mut self,
        inside: bool,
        character: &mut Character,
        moment: Moment,
    ) {
        let Some(showing) = &mut self.showing else {
            return;
        };
        let Some((item, _)) = showing.press.take() else {
            return;
        };
        let plugin = showing.plugin;
        if inside {
            self.act(plugin, moment, |installed| {
                installed.toggle(item, character, moment.local)
            });
        }
        self.refresh_checklist(moment.local);
    }

    pub(crate) fn close_checklist(&mut self) {
        self.showing = None;
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
            .position(|installed| installed.answers_double_click(moment.local));
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
        let asked = perform(installed.as_mut());
        if installed.take_checklist_toggle() {
            self.toggle_checklist(plugin, moment.local);
        }
        let Some(Asked { question, draft }) = asked else {
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

    fn toggle_checklist(&mut self, plugin: usize, now: DateTime) {
        if self.lister() == Some(plugin) {
            self.showing = None;
            return;
        }
        self.showing = Some(Showing {
            plugin,
            title: String::new(),
            items: Vec::new(),
            press: None,
        });
        self.refresh_checklist(now);
        if self.showing.is_some() {
            self.checklists += 1;
        }
    }
}
