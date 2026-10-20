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

#![forbid(unsafe_code)]

use clippy::shortcut::{Key, Modifiers, Shortcut};
use clippy::{Context, MenuItem, Plugin, Question};
use jiff::Unit;
use jiff::civil::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

const COMMAND_OPTION_SHIFT: Modifiers = Modifiers::COMMAND
    .union(Modifiers::OPTION)
    .union(Modifiers::SHIFT);

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Inbox {
    items: Vec<Item>,
    #[serde(flatten)]
    others: Map<String, Value>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Item {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    added: Option<DateTime>,
    #[serde(flatten)]
    others: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    add_shortcut: Shortcut,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Ask,
    Add(String),
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            add_shortcut: Shortcut {
                modifiers: COMMAND_OPTION_SHIFT,
                key: Key::Enter,
            },
        }
    }
}

impl Plugin for Inbox {
    const NAME: &'static str = "Inbox";

    type Action = Action;

    type Settings = Settings;

    fn menu(&self, settings: &Settings, _now: DateTime) -> Vec<MenuItem<Action>> {
        vec![MenuItem::new("Add to Inbox...", Action::Ask).shortcut(settings.add_shortcut)]
    }

    fn perform(&mut self, action: Action, cx: &mut Context<'_, Self>) {
        match action {
            Action::Ask => cx.ask(Question::new("What's on your mind?", Action::Add)),
            Action::Add(text) if text.trim().is_empty() => {}
            Action::Add(text) => {
                let now = cx.now();
                self.items.push(Item {
                    text,
                    added: Some(now.round(Unit::Second).unwrap_or(now)),
                    others: Map::new(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests;
