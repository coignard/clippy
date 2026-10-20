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

use crate::app::Command;
use crate::shortcut::Shortcut;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    Item {
        title: String,
        command: Command,
        enabled: bool,
        shortcut: Option<Shortcut>,
    },
    Submenu {
        title: String,
        entries: Vec<Entry>,
    },
    Separator,
}

impl Entry {
    pub(crate) fn item(title: &str, command: Command, shortcut: Option<Shortcut>) -> Self {
        Self::Item {
            title: title.to_owned(),
            command,
            enabled: true,
            shortcut,
        }
    }
}
