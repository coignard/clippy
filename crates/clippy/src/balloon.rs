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

mod controls;
mod layout;
mod placement;
mod prompt;
mod render;
mod text_field;

pub(crate) use controls::Checkbox;
pub(crate) use layout::Layout;
pub(crate) use placement::{Placement, SCREEN_MARGIN};
pub(crate) use prompt::Prompt;
pub use text_field::{Edit, Motion, Utf16Range};

#[derive(Clone, Copy)]
pub(crate) enum Content<'a> {
    Message(&'a str),
    Prompt {
        prompt: &'a Prompt,
        focused: bool,
    },
    Notice {
        text: &'a str,
        tasks: &'a [(String, Checkbox)],
        checkbox: Option<(&'a str, Checkbox)>,
    },
    Checklist {
        title: &'a str,
        items: &'a [(String, Checkbox)],
    },
}

impl<'a> Content<'a> {
    fn text(self) -> &'a str {
        match self {
            Content::Message(text) | Content::Notice { text, .. } => text,
            Content::Prompt { prompt, .. } if prompt.has_tasks() => "",
            Content::Prompt { prompt, .. } => prompt.question(),
            Content::Checklist { title, .. } => title,
        }
    }

    fn rows(self) -> Vec<(&'a str, Checkbox)> {
        let listed = |items: &'a [(String, Checkbox)]| {
            items.iter().map(|(label, state)| (label.as_str(), *state))
        };
        match self {
            Content::Notice {
                tasks, checkbox, ..
            } => listed(tasks).chain(checkbox).collect(),
            Content::Prompt { prompt, .. } => prompt.tasks().collect(),
            Content::Checklist { items, .. } => listed(items).collect(),
            Content::Message(_) => Vec::new(),
        }
    }

    fn separated_from(self) -> Option<usize> {
        match self {
            Content::Notice {
                text,
                tasks,
                checkbox: Some(_),
            } if !text.is_empty() || !tasks.is_empty() => Some(tasks.len()),
            Content::Prompt { prompt, .. } if prompt.has_tasks() && prompt.shows_field() => {
                Some(prompt.tasks().count())
            }
            _ => None,
        }
    }
}
