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

use jiff::tz::{self, TimeZone};
use jiff::{Timestamp, Zoned};
use objc2_foundation::NSTimeZone;

pub(crate) fn now() -> Zoned {
    Timestamp::now().to_zoned(time_zone())
}

pub(crate) fn forget_time_zone() {
    NSTimeZone::resetSystemTimeZone();
}

fn time_zone() -> TimeZone {
    let name = NSTimeZone::systemTimeZone().name().to_string();
    tz::db().get(&name).unwrap_or_else(|_| TimeZone::system())
}
