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
use jiff::civil::DateTime;
use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Todo {
    tasks: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    show_shortcut: Shortcut,
    hide_shortcut: Shortcut,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Show,
    Add(String),
    AddFirst(String),
    Done { index: usize, task: String },
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show_shortcut: Shortcut::new(Modifiers::COMMAND_OPTION_SHIFT, Key::T),
            hide_shortcut: Shortcut::new(Modifiers::COMMAND_SHIFT, Key::Delete),
        }
    }
}

impl Plugin for Todo {
    const NAME: &'static str = "Todo";

    type Action = Action;

    type Settings = Settings;

    fn menu(&self, settings: &Settings, _now: DateTime) -> Vec<MenuItem<Action>> {
        vec![MenuItem::new("Show To-Dos...", Action::Show).shortcut(settings.show_shortcut)]
    }

    fn perform(&mut self, action: Action, cx: &mut Context<'_, Self>) {
        match action {
            Action::Show => return cx.toggle_question(self.question(cx.settings())),
            Action::Add(task) | Action::AddFirst(task) if task.is_empty() => return,
            Action::Add(task) => self.tasks.push(task),
            Action::AddFirst(task) => self.tasks.insert(0, task),
            Action::Done { index, task } => {
                let found = Some(index)
                    .filter(|&index| self.tasks.get(index) == Some(&task))
                    .or_else(|| self.tasks.iter().position(|left| *left == task));
                if let Some(index) = found {
                    self.tasks.remove(index);
                    cx.celebrate();
                }
            }
        }
        cx.ask(self.question(cx.settings()));
    }
}

impl Todo {
    fn question(&self, settings: &Settings) -> Question<Action> {
        (0..).zip(&self.tasks).fold(
            Question::new("What's the plan?", Action::Add)
                .on_shift_enter(Action::AddFirst)
                .shortcut(settings.hide_shortcut)
                .standing(),
            |question, (index, task)| {
                let done = Action::Done {
                    index,
                    task: task.clone(),
                };
                question.task(task, done)
            },
        )
    }
}

#[cfg(test)]
mod tests;
