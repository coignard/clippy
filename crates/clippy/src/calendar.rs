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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Agenda {
    #[default]
    Unread,
    Denied,
    Read(Vec<Appointment>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Appointment {
    pub id: String,
    pub calendar: String,
    pub title: String,
    pub start: DateTime,
    pub end: DateTime,
    pub all_day: bool,
}
