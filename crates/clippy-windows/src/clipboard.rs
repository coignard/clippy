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

use std::time::Duration;
use std::{ptr, thread};

use windows_sys::Win32::Foundation::{GlobalFree, HWND};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    SetClipboardData,
};
use windows_sys::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows_sys::Win32::System::Ole::CF_UNICODETEXT;

const ATTEMPTS: usize = 10;
const PAUSE: Duration = Duration::from_millis(10);

struct Opened;

impl Opened {
    fn clipboard(owner: HWND) -> Option<Self> {
        for attempt in 0..ATTEMPTS {
            if unsafe { OpenClipboard(owner) } != 0 {
                return Some(Self);
            }
            if attempt + 1 < ATTEMPTS {
                thread::sleep(PAUSE);
            }
        }
        log::warn!("Another program is holding on to the clipboard");
        None
    }
}

impl Drop for Opened {
    fn drop(&mut self) {
        unsafe { CloseClipboard() };
    }
}

pub(crate) fn copy(owner: HWND, text: &str) {
    let units: Vec<u16> = windows_lines(text).encode_utf16().chain([0]).collect();
    let bytes = units.len() * size_of::<u16>();
    let Some(_opened) = Opened::clipboard(owner) else {
        return;
    };
    unsafe { EmptyClipboard() };
    let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes) };
    if memory.is_null() {
        return;
    }
    let target = unsafe { GlobalLock(memory) };
    if target.is_null() {
        unsafe { GlobalFree(memory) };
        return;
    }
    unsafe {
        ptr::copy_nonoverlapping(units.as_ptr().cast::<u8>(), target.cast::<u8>(), bytes);
        GlobalUnlock(memory);
    }
    if unsafe { SetClipboardData(u32::from(CF_UNICODETEXT), memory) }.is_null() {
        unsafe { GlobalFree(memory) };
        log::warn!("Could not copy to the clipboard");
    }
}

pub(crate) fn has_text() -> bool {
    unsafe { IsClipboardFormatAvailable(u32::from(CF_UNICODETEXT)) != 0 }
}

pub(crate) fn text(owner: HWND) -> Option<String> {
    if !has_text() {
        return None;
    }
    let _opened = Opened::clipboard(owner)?;
    let memory = unsafe { GetClipboardData(u32::from(CF_UNICODETEXT)) };
    if memory.is_null() {
        return None;
    }
    let source = unsafe { GlobalLock(memory) };
    if source.is_null() {
        return None;
    }
    let mut units = vec![0_u16; unsafe { GlobalSize(memory) } / size_of::<u16>()];
    unsafe {
        ptr::copy_nonoverlapping(
            source.cast::<u8>(),
            units.as_mut_ptr().cast::<u8>(),
            units.len() * size_of::<u16>(),
        );
        GlobalUnlock(memory);
    }
    let length = units
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(units.len());
    Some(unix_lines(&String::from_utf16_lossy(&units[..length])))
}

fn windows_lines(text: &str) -> String {
    unix_lines(text).replace('\n', "\r\n")
}

fn unix_lines(text: &str) -> String {
    text.replace("\r\n", "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_lines_the_way_windows_programs_expect() {
        assert_eq!(windows_lines("- Milk\n- Eggs\n"), "- Milk\r\n- Eggs\r\n");
        assert_eq!(windows_lines("- Milk\r\n- Eggs"), "- Milk\r\n- Eggs");
        assert_eq!(unix_lines("Milk\r\nEggs"), "Milk\nEggs");
    }
}
