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

use std::time::{Duration, Instant};

use fastrand::Rng;
use jiff::civil::{Date, DateTime, date};

use crate::app::{Command, Moment};
use crate::balloon::{Edit, Motion, Prompt};
use crate::character::Character;
use crate::menu::Entry;
use crate::plugin::{Due, Installed, Plugin, Plugins};
use crate::settings::Settings;
use crate::shortcut::Shortcut;

pub struct Harness {
    plugins: Plugins,
    character: Character,
    moment: Moment,
    today: Option<Date>,
}

impl Harness {
    pub fn new<P: Plugin>(plugin: P) -> Self {
        Self::with_settings(plugin, P::Settings::default())
    }

    pub fn with_settings<P: Plugin>(plugin: P, settings: P::Settings) -> Self {
        let mut plugins = Plugins::default();
        plugins.install(Box::new(Installed::new(plugin, settings)));
        Self {
            plugins,
            character: Character::new(&Settings::default(), Rng::with_seed(0)),
            moment: Moment {
                instant: Instant::now(),
                local: date(2001, 10, 25).at(9, 0, 0, 0),
            },
            today: None,
        }
    }

    pub fn relaunch(&mut self) {
        self.plugins.begin_day(self.moment.local);
    }

    pub fn now(&self) -> DateTime {
        self.moment.local
    }

    pub fn set_time(&mut self, local: DateTime) {
        let elapsed = local.duration_since(self.moment.local);
        self.moment = Moment {
            instant: self.moment.instant + Duration::try_from(elapsed).unwrap_or_default(),
            local,
        };
        if self.today != Some(local.date()) {
            self.today = Some(local.date());
            self.plugins.begin_day(local);
        }
    }

    pub fn notices(&self) -> Vec<String> {
        self.due().into_iter().map(|due| due.notice.text).collect()
    }

    pub fn checkbox(&self, text: &str) -> Option<String> {
        self.find(text).notice.checkbox
    }

    pub fn shows_over_full_screen(&self, text: &str) -> bool {
        self.find(text).notice.over_full_screen
    }

    pub fn next_change(&self) -> Option<DateTime> {
        self.plugins.changes_at(self.moment.local)
    }

    pub fn bedtime(&self) -> bool {
        self.plugins.bedtime(self.moment.local)
    }

    pub fn snooze(&mut self, text: &str) {
        let due = self.find(text);
        self.plugins.snooze(&due, &mut self.character, self.moment);
    }

    pub fn check(&mut self, text: &str) {
        let due = self.find(text);
        assert!(
            due.notice.checkbox.is_some(),
            "the notice {text:?} should have a checkbox"
        );
        self.plugins.check(&due, &mut self.character, self.moment);
    }

    pub fn menu(&self) -> Vec<(String, bool)> {
        self.items()
            .into_iter()
            .filter_map(|entry| match entry {
                Entry::Item { title, enabled, .. } => Some((title, enabled)),
                Entry::Submenu { .. } | Entry::Separator => None,
            })
            .collect()
    }

    pub fn message(&self) -> Option<String> {
        self.plugins.message()
    }

    pub fn question(&self) -> Option<&str> {
        self.plugins.prompt().map(Prompt::question)
    }

    pub fn draft(&self) -> Option<&str> {
        self.plugins.prompt().map(|prompt| prompt.field().text())
    }

    pub fn choose(&mut self, title: &str) {
        let command = self
            .items()
            .into_iter()
            .find_map(|entry| match entry {
                Entry::Item {
                    title: item,
                    command,
                    enabled: true,
                    ..
                } if item == title => Some(command),
                _ => None,
            })
            .unwrap_or_else(|| panic!("the menu should have an enabled item titled {title:?}"));
        let Command::Plugin(item) = command else {
            unreachable!("plugin menus only hold plugin commands")
        };
        self.plugins.choose(item, &mut self.character, self.moment);
    }

    pub fn press(&mut self, shortcut: Shortcut) {
        if let Some(item) = self.plugins.item_for(shortcut, self.moment.local) {
            self.plugins.choose(item, &mut self.character, self.moment);
        }
    }

    pub fn double_click(&mut self) {
        self.plugins.double_click(&mut self.character, self.moment);
    }

    pub fn answer(&mut self, text: &str) {
        let prompt = self
            .plugins
            .prompt_mut()
            .expect("there should be a question to answer");
        prompt.edit(Edit::SelectAll, self.moment.instant);
        prompt.edit(Edit::Delete(Motion::Backward), self.moment.instant);
        prompt.edit(Edit::Insert(text.to_owned()), self.moment.instant);
        self.plugins.submit(&mut self.character, self.moment);
    }

    pub fn cancel(&mut self) {
        self.plugins.dismiss();
    }

    fn due(&self) -> Vec<Due> {
        self.plugins.notices(self.moment.local)
    }

    fn find(&self, text: &str) -> Due {
        self.due()
            .into_iter()
            .find(|due| due.notice.text == text)
            .unwrap_or_else(|| panic!("a notice reading {text:?} should be due"))
    }

    fn items(&self) -> Vec<Entry> {
        self.plugins
            .menu(self.moment.local)
            .flat_map(|submenu| match submenu {
                Entry::Submenu { entries, .. } => entries,
                Entry::Item { .. } | Entry::Separator => Vec::new(),
            })
            .collect()
    }
}
