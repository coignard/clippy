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

mod balloon_view;
mod clippy_view;
mod editing;
mod surface;

use std::cell::Cell;
use std::ptr;

use clippy::Event;
use windows_sys::Win32::Foundation::{HWND, LPARAM};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetMessageTime, GetSystemMetrics, HWND_TOPMOST, LoadCursorW, SM_CXDOUBLECLK, SM_CYDOUBLECLK,
    SW_HIDE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SetCursor, SetWindowPos,
    ShowWindow,
};
use windows_sys::core::PCWSTR;

use crate::shell::{self, Shell};
use crate::win32;

pub(crate) use balloon_view::BalloonView;
pub(crate) use clippy_view::ClippyView;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Drag {
    pointer: (i32, i32),
    origin: (i32, i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Placed {
    origin: (i32, i32),
    scale: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Click {
    time: u32,
    at: (i32, i32),
    count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Nearby {
    interval: u32,
    width: i32,
    height: i32,
}

#[derive(Debug, Default)]
struct Clicks {
    last: Cell<Option<Click>>,
}

impl Default for Placed {
    fn default() -> Self {
        Self {
            origin: (0, 0),
            scale: 1,
        }
    }
}

impl Placed {
    fn on_screen(self, (x, y): (i32, i32)) -> (i32, i32) {
        (x - self.origin.0, y - self.origin.1)
    }

    fn inside(self, (x, y): (i32, i32)) -> (i32, i32) {
        (x.div_euclid(self.scale), y.div_euclid(self.scale))
    }
}

impl Clicks {
    fn press(&self, at: (i32, i32)) -> usize {
        let time = unsafe { GetMessageTime() }.cast_unsigned();
        let nearby = Nearby {
            interval: unsafe { GetDoubleClickTime() },
            width: unsafe { GetSystemMetrics(SM_CXDOUBLECLK) },
            height: unsafe { GetSystemMetrics(SM_CYDOUBLECLK) },
        };
        self.count(time, at, nearby)
    }

    fn count(&self, time: u32, at: (i32, i32), nearby: Nearby) -> usize {
        let follows = |last: &Click| {
            time.wrapping_sub(last.time) <= nearby.interval
                && (at.0 - last.at.0).abs() <= nearby.width / 2
                && (at.1 - last.at.1).abs() <= nearby.height / 2
        };
        let count = match self.last.get() {
            Some(last) if follows(&last) => last.count + 1,
            _ => 1,
        };
        self.last.set(Some(Click { time, at, count }));
        count
    }
}

fn send(event: Event) {
    shell::with(|shell| shell.send(event));
}

fn drag(drag: Drag) {
    let (x, y) = win32::cursor();
    let origin = (
        drag.origin.0 + x - drag.pointer.0,
        drag.origin.1 + y - drag.pointer.1,
    );
    shell::with(|shell| shell.drag_clippy(origin));
}

fn dropped(drag: Drag) {
    shell::with(|shell| shell.dropped(drag.origin));
}

fn clippy_origin() -> Option<(i32, i32)> {
    shell::with(|shell| Shell::stage(shell).map(super::stage::Stage::origin)).flatten()
}

fn show_menu(at: (i32, i32)) {
    shell::with(|shell| shell.show_menu(at));
}

fn screens_changed() {
    shell::with(Shell::screens_changed);
}

fn menu_position(lparam: LPARAM) -> Option<(i32, i32)> {
    (lparam != -1).then(|| win32::point(lparam))
}

fn bring_forward(window: HWND) {
    if !win32::is_visible(window) {
        let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW;
        unsafe { SetWindowPos(window, HWND_TOPMOST, 0, 0, 0, 0, flags) };
    }
}

fn hide(window: HWND) {
    if win32::is_visible(window) {
        unsafe { ShowWindow(window, SW_HIDE) };
    }
}

fn set_cursor(cursor: PCWSTR) {
    unsafe { SetCursor(LoadCursorW(ptr::null_mut(), cursor)) };
}

#[cfg(test)]
mod tests {
    use super::*;

    const NEARBY: Nearby = Nearby {
        interval: 500,
        width: 4,
        height: 4,
    };

    #[test]
    fn quick_clicks_in_one_place_add_up() {
        let clicks = Clicks::default();
        assert_eq!(clicks.count(1000, (10, 10), NEARBY), 1);
        assert_eq!(clicks.count(1200, (11, 9), NEARBY), 2);
        assert_eq!(clicks.count(1400, (11, 9), NEARBY), 3);
    }

    #[test]
    fn slow_or_distant_clicks_start_over() {
        let clicks = Clicks::default();
        clicks.count(1000, (10, 10), NEARBY);
        assert_eq!(clicks.count(1600, (10, 10), NEARBY), 1);
        assert_eq!(clicks.count(1700, (20, 10), NEARBY), 1);
        assert_eq!(clicks.count(u32::MAX, (20, 10), NEARBY), 1);
        assert_eq!(clicks.count(100, (20, 10), NEARBY), 2);
    }

    #[test]
    fn pointers_turn_into_points_on_the_canvas() {
        let placed = Placed {
            origin: (100, 50),
            scale: 2,
        };
        assert_eq!(placed.inside(placed.on_screen((105, 49))), (2, -1));
        assert_eq!(Placed::default().inside((7, 3)), (7, 3));
    }
}
