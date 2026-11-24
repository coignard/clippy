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

use clippy::geometry::Rect;
use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT, S_OK, TRUE};
use windows_sys::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows_sys::Win32::UI::Shell::{
    QUERY_USER_NOTIFICATION_STATE, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
    SHQueryUserNotificationState,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GWL_EXSTYLE, GWL_STYLE, GetClassNameW, GetShellWindow, GetWindowLongW,
    GetWindowRect, IsIconic, IsZoomed, WS_CAPTION, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
};
use windows_sys::core::BOOL;

use crate::win32::{self, size_of_u32};

const SLACK: i32 = 1;
const DESKTOP_WORKER: &str = "WorkerW";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    Overlooked,
    Fills,
    LeavesRoom,
}

struct Search {
    display: Rect,
    taken: bool,
}

pub(crate) fn is_taken(display: Rect) -> bool {
    if shell_is_busy() {
        log::debug!("Windows reports a full screen game or a presentation");
        return true;
    }
    let mut search = Search {
        display,
        taken: false,
    };
    unsafe { EnumWindows(Some(visit), (&raw mut search) as LPARAM) };
    search.taken
}

fn shell_is_busy() -> bool {
    let mut state: QUERY_USER_NOTIFICATION_STATE = 0;
    let queried = unsafe { SHQueryUserNotificationState(&raw mut state) };
    queried == S_OK && matches!(state, QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_PRESENTATION_MODE)
}

extern "system" fn visit(window: HWND, search: LPARAM) -> BOOL {
    let search = unsafe { &mut *(search as *mut Search) };
    match judge(window, search.display) {
        Verdict::Overlooked => TRUE,
        Verdict::Fills => {
            log::debug!("A window of another program fills {:?}", search.display);
            search.taken = true;
            0
        }
        Verdict::LeavesRoom => 0,
    }
}

fn judge(window: HWND, display: Rect) -> Verdict {
    if !is_shown(window) || win32::is_ours(window) {
        return Verdict::Overlooked;
    }
    let mut bounds = RECT::default();
    if unsafe { GetWindowRect(window, &raw mut bounds) } == 0 {
        return Verdict::Overlooked;
    }
    let bounds = win32::rect(bounds);
    let extended = unsafe { GetWindowLongW(window, GWL_EXSTYLE) }.cast_unsigned();
    let floats = extended & (WS_EX_TOPMOST | WS_EX_TRANSPARENT) != 0;
    if !overlaps(bounds, display) || floats {
        return Verdict::Overlooked;
    }
    if is_desktop(window) {
        return Verdict::LeavesRoom;
    }
    let style = unsafe { GetWindowLongW(window, GWL_STYLE) }.cast_unsigned();
    let maximized = unsafe { IsZoomed(window) } != 0 && style & WS_CAPTION == WS_CAPTION;
    if covers(bounds, display) && !maximized {
        Verdict::Fills
    } else {
        Verdict::LeavesRoom
    }
}

fn is_shown(window: HWND) -> bool {
    let mut cloaked = 0_u32;
    let found = unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_CLOAKED.cast_unsigned(),
            (&raw mut cloaked).cast(),
            size_of_u32::<u32>(),
        )
    } == S_OK;
    win32::is_visible(window) && unsafe { IsIconic(window) } == 0 && !(found && cloaked != 0)
}

fn is_desktop(window: HWND) -> bool {
    if window == unsafe { GetShellWindow() } {
        return true;
    }
    let mut class = [0_u16; 16];
    let length = unsafe { GetClassNameW(window, class.as_mut_ptr(), 16) };
    let class = usize::try_from(length)
        .ok()
        .and_then(|length| class.get(..length))
        .unwrap_or_default();
    String::from_utf16_lossy(class) == DESKTOP_WORKER
}

fn overlaps(window: Rect, display: Rect) -> bool {
    window.x < display.right()
        && display.x < window.right()
        && window.y < display.bottom()
        && display.y < window.bottom()
}

fn covers(window: Rect, display: Rect) -> bool {
    let window = window.inset(-SLACK);
    window.x <= display.x
        && window.y <= display.y
        && window.right() >= display.right()
        && window.bottom() >= display.bottom()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DISPLAY: Rect = Rect::new(0, 0, 1920, 1080);

    #[test]
    fn full_screen_windows_cover_the_whole_display() {
        assert!(covers(DISPLAY, DISPLAY));
        assert!(covers(Rect::new(1, 0, 1919, 1080), DISPLAY));
        assert!(covers(Rect::new(-8, -8, 1936, 1096), DISPLAY));
        assert!(!covers(Rect::new(0, 0, 1920, 1032), DISPLAY));
        assert!(!covers(Rect::new(0, 40, 1920, 1040), DISPLAY));
    }

    #[test]
    fn windows_on_other_displays_do_not_count() {
        assert!(!overlaps(Rect::new(1920, 0, 2560, 1440), DISPLAY));
        assert!(!overlaps(Rect::new(-1280, 0, 1280, 1024), DISPLAY));
        assert!(overlaps(Rect::new(1900, 0, 2560, 1440), DISPLAY));
    }
}
