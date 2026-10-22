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
            Content::Prompt { prompt, .. } => prompt.question(),
            Content::Checklist { title, .. } => title,
        }
    }

    fn rows(self) -> Vec<(&'a str, Checkbox)> {
        match self {
            Content::Notice {
                checkbox: Some(row),
                ..
            } => vec![row],
            Content::Checklist { items, .. } => items
                .iter()
                .map(|(label, state)| (label.as_str(), *state))
                .collect(),
            Content::Message(_) | Content::Prompt { .. } | Content::Notice { .. } => Vec::new(),
        }
    }
}
