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

use std::{io, ptr};

use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, IDC_ARROW, LoadCursorW, RegisterClassExW, WINDOW_EX_STYLE,
    WNDCLASSEXW, WNDPROC, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_POPUP,
};
use windows_sys::core::{PCWSTR, w};

use crate::shell::{self, Shell};
use crate::win32::size_of_u32;

pub(crate) const SHELL_CLASS: PCWSTR = w!("ClippyShell");
const CLIPPY_CLASS: PCWSTR = w!("Clippy");
const BALLOON_CLASS: PCWSTR = w!("ClippyBalloon");
const FLOATING: WINDOW_EX_STYLE =
    WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE;

pub(crate) fn shell() -> HWND {
    register(SHELL_CLASS, Some(shell_procedure));
    create(WS_EX_TOOLWINDOW, SHELL_CLASS, ptr::null_mut())
}

pub(crate) fn clippy() -> HWND {
    register(CLIPPY_CLASS, Some(clippy_procedure));
    create(FLOATING, CLIPPY_CLASS, ptr::null_mut())
}

pub(crate) fn balloon(clippy: HWND) -> HWND {
    register(BALLOON_CLASS, Some(balloon_procedure));
    create(FLOATING, BALLOON_CLASS, clippy)
}

fn register(class: PCWSTR, procedure: WNDPROC) {
    let class = WNDCLASSEXW {
        cbSize: size_of_u32::<WNDCLASSEXW>(),
        lpfnWndProc: procedure,
        hInstance: instance(),
        hCursor: unsafe { LoadCursorW(ptr::null_mut(), IDC_ARROW) },
        lpszClassName: class,
        ..WNDCLASSEXW::default()
    };
    if unsafe { RegisterClassExW(&raw const class) } == 0 {
        log::error!(
            "Could not register a window class: {}",
            io::Error::last_os_error()
        );
    }
}

fn create(style: WINDOW_EX_STYLE, class: PCWSTR, owner: HWND) -> HWND {
    let window = unsafe {
        CreateWindowExW(
            style,
            class,
            w!("Clippy"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            owner,
            ptr::null_mut(),
            instance(),
            ptr::null(),
        )
    };
    assert!(
        !window.is_null(),
        "Windows should make room for Clippy: {}",
        io::Error::last_os_error()
    );
    window
}

fn instance() -> HINSTANCE {
    unsafe { GetModuleHandleW(ptr::null()) }
}

extern "system" fn shell_procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let handled = shell::with(|shell| shell.handle(message, wparam, lparam)).flatten();
    handled.unwrap_or_else(|| unsafe { DefWindowProcW(window, message, wparam, lparam) })
}

extern "system" fn clippy_procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let handled = shell::with(|shell| {
        Shell::stage(shell)?
            .clippy()
            .handle(message, wparam, lparam)
    })
    .flatten();
    handled.unwrap_or_else(|| unsafe { DefWindowProcW(window, message, wparam, lparam) })
}

extern "system" fn balloon_procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let handled = shell::with(|shell| {
        Shell::stage(shell)?
            .balloon()
            .handle(message, wparam, lparam)
    })
    .flatten();
    handled.unwrap_or_else(|| unsafe { DefWindowProcW(window, message, wparam, lparam) })
}
