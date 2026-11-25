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

#![cfg(windows)]

use std::cell::RefCell;
use std::ffi::c_char;
use std::sync::OnceLock;
use std::{mem, ptr};

use jiff::tz::{self, TimeZone};
use jiff::{Timestamp, ZonedDateTime};
use windows_sys::Win32::System::LibraryLoader::{
    GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
};
use windows_sys::Win32::System::Time::{
    DYNAMIC_TIME_ZONE_INFORMATION, GetDynamicTimeZoneInformation, TIME_ZONE_ID_INVALID,
};
use windows_sys::core::{s, w};

const LONGEST_NAME: usize = 64;

type WindowsToIana =
    unsafe extern "C" fn(*const u16, i32, *const c_char, *mut u16, i32, *mut i32) -> i32;

thread_local! {
    static TIME_ZONE: RefCell<Option<TimeZone>> = const { RefCell::new(None) };
}

pub(crate) fn now() -> ZonedDateTime {
    Timestamp::now().to_zoned_datetime(time_zone())
}

pub(crate) fn forget_time_zone() {
    TIME_ZONE.with_borrow_mut(|zone| *zone = None);
}

fn time_zone() -> TimeZone {
    TIME_ZONE.with_borrow_mut(|zone| zone.get_or_insert_with(system_time_zone).clone())
}

fn system_time_zone() -> TimeZone {
    windows_zone_name()
        .and_then(|name| tz::db().get(&name).ok())
        .unwrap_or_else(TimeZone::system)
}

fn windows_zone_name() -> Option<String> {
    let mut information = DYNAMIC_TIME_ZONE_INFORMATION::default();
    if unsafe { GetDynamicTimeZoneInformation(&raw mut information) } == TIME_ZONE_ID_INVALID
        || information.DynamicDaylightTimeDisabled
    {
        return None;
    }
    let key = &information.TimeZoneKeyName;
    let key = &key[..key.iter().position(|&unit| unit == 0)?];
    let convert = windows_to_iana()?;
    let mut name = [0_u16; LONGEST_NAME];
    let mut status = 0;
    let written = unsafe {
        convert(
            key.as_ptr(),
            i32::try_from(key.len()).ok()?,
            ptr::null(),
            name.as_mut_ptr(),
            i32::try_from(name.len()).ok()?,
            &raw mut status,
        )
    };
    let written = usize::try_from(written).ok().filter(|_| status <= 0)?;
    String::from_utf16(name.get(..written)?).ok()
}

fn windows_to_iana() -> Option<WindowsToIana> {
    static CONVERT: OnceLock<Option<WindowsToIana>> = OnceLock::new();
    *CONVERT.get_or_init(|| {
        [w!("icu.dll"), w!("icuin.dll")]
            .into_iter()
            .find_map(|library| {
                let module = unsafe {
                    LoadLibraryExW(library, ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32)
                };
                if module.is_null() {
                    return None;
                }
                let function =
                    unsafe { GetProcAddress(module, s!("ucal_getTimeZoneIDForWindowsID")) }?;
                Some(unsafe {
                    mem::transmute::<unsafe extern "system" fn() -> isize, WindowsToIana>(function)
                })
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_reads_the_system_time_zone_again_once_told_to() {
        let before = now();
        forget_time_zone();
        assert!(TIME_ZONE.with_borrow(Option::is_none));
        let after = now();
        assert_eq!(before.time_zone(), after.time_zone());
        assert!(TIME_ZONE.with_borrow(Option::is_some));
    }
}
