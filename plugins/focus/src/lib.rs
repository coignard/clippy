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
use clippy::{Context, MenuItem, Message, Plugin, Question};
use jiff::civil::DateTime;
use serde::{Deserialize, Serialize};

const COMMAND_SHIFT: Modifiers = Modifiers::COMMAND.union(Modifiers::SHIFT);

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Focus {
    #[serde(skip_serializing_if = "Option::is_none")]
    current: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    edit_shortcut: Shortcut,
    clear_shortcut: Shortcut,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Edit,
    Set(String),
    Clear,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            edit_shortcut: Shortcut {
                modifiers: COMMAND_SHIFT,
                key: Key::Enter,
            },
            clear_shortcut: Shortcut {
                modifiers: COMMAND_SHIFT,
                key: Key::Delete,
            },
        }
    }
}

impl Plugin for Focus {
    const NAME: &'static str = "Focus";

    type Action = Action;

    type Settings = Settings;

    fn menu(&self, settings: &Settings, _now: DateTime) -> Vec<MenuItem<Action>> {
        let edit = if self.current.is_some() {
            "Edit Focus..."
        } else {
            "Set Focus..."
        };
        vec![
            MenuItem::new(edit, Action::Edit).shortcut(settings.edit_shortcut),
            MenuItem::new("Clear Focus", Action::Clear)
                .shortcut(settings.clear_shortcut)
                .enabled(self.current.is_some()),
        ]
    }

    fn message(&self) -> Option<Message<Action>> {
        let current = self.current.as_ref()?;
        Some(Message::new(current).on_double_click(Action::Edit))
    }

    fn perform(&mut self, action: Action, cx: &mut Context<'_, Self>) {
        match action {
            Action::Edit => cx.ask(
                Question::new("What are you focusing on?", Action::Set)
                    .draft(self.current.as_deref().unwrap_or_default()),
            ),
            Action::Set(focus) if focus.is_empty() => self.clear(cx),
            Action::Set(focus) => self.current = Some(focus),
            Action::Clear => self.clear(cx),
        }
    }
}

impl Focus {
    fn clear(&mut self, cx: &mut Context<'_, Self>) {
        if self.current.take().is_some() {
            cx.celebrate();
        }
    }
}

#[cfg(test)]
mod tests {
    use clippy::testing::Harness;

    use super::*;

    fn focused_on(text: &str) -> Harness {
        Harness::new(Focus {
            current: Some(text.to_owned()),
        })
    }

    #[test]
    fn offers_to_set_a_focus_at_first() {
        let focus = Harness::new(Focus::default());
        assert_eq!(
            focus.menu(),
            [
                ("Set Focus...".to_owned(), true),
                ("Clear Focus".to_owned(), false)
            ]
        );
        assert_eq!(focus.message(), None);
    }

    #[test]
    fn shows_the_focus_it_was_given() {
        let mut focus = Harness::new(Focus::default());
        focus.choose("Set Focus...");
        assert_eq!(focus.question(), Some("What are you focusing on?"));
        assert_eq!(focus.draft(), Some(""));
        focus.answer("Ship the plan");
        assert_eq!(focus.message().as_deref(), Some("Ship the plan"));
        assert_eq!(
            focus.menu(),
            [
                ("Edit Focus...".to_owned(), true),
                ("Clear Focus".to_owned(), true)
            ]
        );
    }

    #[test]
    fn edits_start_from_the_current_focus() {
        let mut focus = focused_on("Ship it");
        focus.double_click();
        assert_eq!(focus.draft(), Some("Ship it"));
        focus.answer("Ship it today");
        focus.choose("Edit Focus...");
        assert_eq!(focus.draft(), Some("Ship it today"));
    }

    #[test]
    fn cancelling_keeps_the_focus() {
        let mut focus = focused_on("Ship it");
        focus.choose("Edit Focus...");
        focus.cancel();
        assert_eq!(focus.question(), None);
        assert_eq!(focus.message().as_deref(), Some("Ship it"));
    }

    #[test]
    fn a_blank_answer_clears_the_focus() {
        let mut focus = focused_on("Ship it");
        focus.choose("Edit Focus...");
        focus.answer("   ");
        assert_eq!(focus.message(), None);
        assert_eq!(focus.menu()[1], ("Clear Focus".to_owned(), false));
        let mut focus = focused_on("Ship it");
        focus.choose("Edit Focus...");
        focus.answer("");
        assert_eq!(focus.message(), None);
    }

    #[test]
    fn clearing_the_focus_ticks_it_off() {
        let mut focus = focused_on("Ship it");
        focus.press(Settings::default().clear_shortcut);
        assert!(focus.celebrates());
        for blank in ["", "   "] {
            let mut focus = focused_on("Ship it");
            focus.choose("Edit Focus...");
            assert!(!focus.celebrates());
            focus.answer(blank);
            assert!(focus.celebrates(), "{blank:?}");
        }
    }

    #[test]
    fn only_a_focus_that_was_there_gets_ticked_off() {
        let mut focus = Harness::new(Focus::default());
        focus.choose("Set Focus...");
        focus.answer("");
        assert!(!focus.celebrates());
        let mut focus = focused_on("Ship it");
        focus.choose("Edit Focus...");
        focus.answer("Ship it today");
        assert!(!focus.celebrates());
        focus.choose("Edit Focus...");
        focus.cancel();
        assert!(!focus.celebrates());
    }

    #[test]
    fn the_shortcut_opens_the_question() {
        let mut focus = focused_on("Ship it");
        focus.press(Settings::default().edit_shortcut);
        assert_eq!(focus.draft(), Some("Ship it"));
    }

    #[test]
    fn the_clear_shortcut_forgets_the_focus() {
        let mut focus = focused_on("Ship it");
        focus.press(Settings::default().clear_shortcut);
        assert_eq!(focus.message(), None);
        focus.press(Settings::default().clear_shortcut);
        assert_eq!(focus.question(), None);
    }

    #[test]
    fn the_shortcut_follows_the_settings() {
        let settings = Settings {
            edit_shortcut: "Ctrl+Option+F".parse().unwrap(),
            ..Settings::default()
        };
        let mut focus = Harness::with_settings(Focus::default(), settings.clone());
        focus.press(Settings::default().edit_shortcut);
        assert_eq!(focus.question(), None);
        focus.press(settings.edit_shortcut);
        assert_eq!(focus.question(), Some("What are you focusing on?"));
    }

    #[test]
    fn settings_read_like_shortcuts() {
        assert_eq!(
            toml::to_string(&Settings::default()).unwrap(),
            "edit_shortcut = \"Cmd+Shift+Enter\"\nclear_shortcut = \"Cmd+Shift+Delete\"\n"
        );
    }

    #[test]
    fn missing_shortcuts_fall_back_to_their_defaults() {
        let settings: Settings = toml::from_str("edit_shortcut = \"Cmd+F\"").unwrap();
        assert_eq!(settings.clear_shortcut, Settings::default().clear_shortcut);
    }

    #[test]
    fn clearing_forgets_the_focus() {
        let mut focus = focused_on("Ship it");
        focus.choose("Clear Focus");
        assert_eq!(focus.message(), None);
        assert_eq!(focus.menu()[0], ("Set Focus...".to_owned(), true));
    }
}
