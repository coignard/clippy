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

use crate::app::{Command, Moment};
use crate::balloon::{Content, Press, Prompt};
use crate::calendar::Agenda;
use crate::character::Character;
use crate::menu::Entry;
use crate::plugin::installed::{AnyPlugin, Trigger};
use crate::plugin::{Item, Listed, Noticed};
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
    copies: Vec<String>,
    prompt_shown: bool,
}

struct Asking {
    plugin: usize,
    prompt: Prompt,
}

struct Showing {
    plugin: usize,
    list: Listed,
    press: Press,
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
        self.items(now)
            .filter_map(|(_, name, item)| Some((name, item.shortcut?)))
    }

    pub(crate) fn active_shortcuts(&self, now: DateTime) -> impl Iterator<Item = Shortcut> {
        self.items(now)
            .filter_map(|(.., item)| item.shortcut.filter(|_| item.enabled))
    }

    pub(crate) fn item_for(&self, shortcut: Shortcut, now: DateTime) -> Option<PluginItem> {
        self.items(now)
            .find(|(.., item)| item.enabled && item.shortcut == Some(shortcut))
            .map(|(chosen, ..)| chosen)
    }

    pub(crate) fn menu(&self, now: DateTime) -> Vec<Entry> {
        let mut submenus = Vec::new();
        for (chosen, name, item) in self.items(now) {
            let entry = Entry::Item {
                title: item.title,
                command: Command::Plugin(chosen),
                enabled: item.enabled,
                shortcut: item.shortcut,
            };
            match submenus.last_mut() {
                Some(Entry::Submenu { title, entries }) if title == name => entries.push(entry),
                _ => submenus.push(Entry::Submenu {
                    title: name.to_owned(),
                    entries: vec![entry],
                }),
            }
        }
        submenus
    }

    fn items(&self, now: DateTime) -> impl Iterator<Item = (PluginItem, &'static str, Item)> {
        let at = now;
        (0..)
            .zip(&self.installed)
            .flat_map(move |(plugin, installed)| {
                let name = installed.name();
                (0..)
                    .zip(installed.menu(now))
                    .map(move |(item, view)| (PluginItem { plugin, item, at }, name, view))
            })
    }

    pub(crate) fn message(&self, now: DateTime) -> Option<String> {
        let messages: Vec<String> = self
            .installed
            .iter()
            .filter_map(|installed| Some(installed.message(now)?.text))
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
            .filter_map(move |installed| installed.message(now)?.shortcut)
    }

    pub(crate) fn press_message(
        &mut self,
        shortcut: Shortcut,
        character: &mut Character,
        moment: Moment,
    ) -> bool {
        let owner = self.installed.iter().position(|installed| {
            let message = installed.message(moment.local);
            message.is_some_and(|said| said.shortcut == Some(shortcut))
        });
        if let Some(plugin) = owner {
            self.act(plugin, Trigger::DoubleClick, character, moment);
        }
        owner.is_some()
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

    pub(crate) fn checklist(&self) -> Option<Content<'_>> {
        let showing = self.showing.as_ref()?;
        Some(Content::Checklist {
            title: &showing.list.title,
            items: &showing.list.items,
            press: showing.press,
        })
    }

    pub(crate) fn refresh_checklist(&mut self, now: DateTime) {
        let Some(showing) = &mut self.showing else {
            return;
        };
        match self.installed[showing.plugin].checklist(now) {
            Some(list) => showing.list = list,
            None => self.showing = None,
        }
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
        let chosen = Trigger::Choose {
            item: item.item,
            listed: item.at,
        };
        self.act(item.plugin, chosen, character, moment);
    }

    pub(crate) fn double_click(&mut self, character: &mut Character, moment: Moment) {
        let owner = self.installed.iter().position(|installed| {
            let message = installed.message(moment.local);
            message.is_some_and(|said| said.answers_double_click)
        });
        if let Some(plugin) = owner.filter(|_| self.asking.is_none()) {
            self.act(plugin, Trigger::DoubleClick, character, moment);
        }
    }

    pub(crate) fn submit(&mut self, shift: bool, character: &mut Character, moment: Moment) {
        if let Some(Asking { plugin, prompt }) =
            self.asking.take_if(|asking| asking.prompt.shows_field())
        {
            let text = prompt.into_answer();
            self.act(plugin, Trigger::Answer { text, shift }, character, moment);
        }
    }

    pub(crate) fn press(&mut self, row: usize) {
        if let Some(asking) = &mut self.asking {
            asking.prompt.press_task(row);
        } else if let Some(showing) = &mut self.showing {
            showing.press.begin(row, showing.list.items.len());
        }
    }

    pub(crate) fn drag(&mut self, inside: bool) {
        if let Some(asking) = &mut self.asking {
            asking.prompt.drag_task(inside);
        }
        if let Some(showing) = &mut self.showing {
            showing.press.drag(inside);
        }
    }

    pub(crate) fn release(&mut self, inside: bool, character: &mut Character, moment: Moment) {
        self.release_asked_task(inside, character, moment);
        if let Some(showing) = &mut self.showing
            && let Some(item) = showing.press.release(inside)
        {
            let plugin = showing.plugin;
            self.act(plugin, Trigger::Toggle(item), character, moment);
        }
    }

    fn release_asked_task(&mut self, inside: bool, character: &mut Character, moment: Moment) {
        let Some(asking) = &mut self.asking else {
            return;
        };
        let Some(task) = asking.prompt.release_task(inside) else {
            return;
        };
        let plugin = asking.plugin;
        let wanted_focus = asking.prompt.wants_focus();
        let field_shown = asking.prompt.shows_field();
        self.act(plugin, Trigger::TickAsked(task), character, moment);
        if let Some(asking) = &mut self.asking {
            asking.prompt.show_field(field_shown);
            if !wanted_focus {
                asking.prompt.leave_focus();
            }
        }
        self.close_empty_list();
    }

    pub(crate) fn set_prompt_shown(&mut self, shown: bool) {
        self.prompt_shown = shown;
    }

    pub(crate) fn take_copies(&mut self) -> Vec<String> {
        mem::take(&mut self.copies)
    }

    pub(crate) fn snooze(&mut self, due: &Due, character: &mut Character, moment: Moment) {
        let snoozed = Trigger::Snooze(&due.notice.id);
        self.act(due.plugin, snoozed, character, moment);
    }

    pub(crate) fn check(&mut self, due: &Due, character: &mut Character, moment: Moment) {
        let checked = Trigger::Check(&due.notice.id);
        self.act(due.plugin, checked, character, moment);
    }

    pub(crate) fn toggle_task(
        &mut self,
        due: &Due,
        task: usize,
        character: &mut Character,
        moment: Moment,
    ) {
        let toggled = Trigger::ToggleTask(&due.notice.id, task);
        self.act(due.plugin, toggled, character, moment);
    }

    pub(crate) fn dismiss(&mut self) {
        if let Some(Asking { plugin, .. }) = self.asking.take() {
            self.installed[plugin].dismiss();
        }
    }

    pub(crate) fn put_aside(&mut self) {
        if self
            .asking
            .as_ref()
            .is_some_and(|asking| !asking.prompt.is_standing())
        {
            self.dismiss();
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
        trigger: Trigger<'_>,
        character: &mut Character,
        moment: Moment,
    ) {
        let question_shown = self.asker() == Some(plugin) && self.prompt_shown;
        let Some(installed) = self.installed.get_mut(plugin) else {
            return;
        };
        if let Some(storage) = &self.storage {
            installed.refresh(storage);
        }
        let outcome = installed.act(trigger, question_shown, character, moment.local);
        self.copies.extend(outcome.copied);
        if outcome.toggles_checklist {
            self.toggle_checklist(plugin, moment.local);
        }
        if outcome.toggles_field
            && let Some(asking) = &mut self.asking
            && asking.plugin == plugin
        {
            let shown = !asking.prompt.shows_field();
            asking.prompt.show_field(shown);
            if shown {
                asking.prompt.request_focus();
            }
            self.close_empty_list();
        }
        let Some(asked) = outcome.asked else {
            return;
        };
        self.questions += 1;
        match &mut self.asking {
            Some(asking)
                if asking.plugin == plugin && asking.prompt.question() == asked.question =>
            {
                asking.prompt.ask_again(asked);
            }
            asking => {
                let prompt = Prompt::new(asked, moment.instant);
                if let Some(previous) = asking.replace(Asking { plugin, prompt })
                    && previous.plugin != plugin
                {
                    self.installed[previous.plugin].dismiss();
                }
            }
        }
    }

    fn close_empty_list(&mut self) {
        if self
            .asking
            .as_ref()
            .is_some_and(|asking| !asking.prompt.shows_field() && !asking.prompt.has_tasks())
        {
            self.dismiss();
        }
    }

    fn toggle_checklist(&mut self, plugin: usize, now: DateTime) {
        if self.lister() == Some(plugin) {
            self.showing = None;
            return;
        }
        let press = Press::default();
        let list = self.installed[plugin].checklist(now);
        self.showing = list.map(|list| Showing {
            plugin,
            list,
            press,
        });
        if self.showing.is_some() {
            self.checklists += 1;
            self.dismiss();
        }
    }
}
