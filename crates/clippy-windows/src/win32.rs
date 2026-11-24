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

use windows_sys::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetWindowThreadProcessId, IsWindow, IsWindowVisible,
};

#[expect(clippy::cast_possible_truncation)]
pub(crate) const fn size_of_u32<T>() -> u32 {
    size_of::<T>() as u32
}

pub(crate) fn cursor() -> (i32, i32) {
    let mut point = POINT::default();
    unsafe { GetCursorPos(&raw mut point) };
    (point.x, point.y)
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]
pub(crate) fn point(lparam: LPARAM) -> (i32, i32) {
    let x = lparam as u16 as i16;
    let y = (lparam >> 16) as u16 as i16;
    (i32::from(x), i32::from(y))
}

pub(crate) fn is_visible(window: HWND) -> bool {
    unsafe { IsWindowVisible(window) != 0 }
}

pub(crate) fn is_ours(window: HWND) -> bool {
    let mut process = 0;
    unsafe { GetWindowThreadProcessId(window, &raw mut process) };
    process == unsafe { GetCurrentProcessId() }
}

pub(crate) fn exists(window: HWND) -> bool {
    !window.is_null() && unsafe { IsWindow(window) != 0 }
}

pub(crate) fn rect(rect: RECT) -> clippy::geometry::Rect {
    clippy::geometry::Rect::new(
        rect.left,
        rect.top,
        rect.right - rect.left,
        rect.bottom - rect.top,
    )
}
