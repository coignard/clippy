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

use std::cell::{Cell, RefCell};

use clippy::graphics::Canvas;
use clippy::{Command, Event, Hover};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::Controls::WM_MOUSELEAVE;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    IDC_ARROW, MA_NOACTIVATE, WM_CAPTURECHANGED, WM_CLOSE, WM_CONTEXTMENU, WM_DPICHANGED,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_SETCURSOR,
};

use super::surface::Surface;
use super::{Clicks, Drag, Placed};
use crate::win32::{self, size_of_u32};

pub(crate) struct ClippyView {
    window: HWND,
    surface: Surface,
    canvas: RefCell<Option<Canvas>>,
    hover: RefCell<Hover>,
    placed: Cell<Placed>,
    clicks: Clicks,
    dragging: Cell<Option<Drag>>,
    tracking: Cell<bool>,
}

impl ClippyView {
    pub(crate) fn new(window: HWND) -> Self {
        Self {
            window,
            surface: Surface::new(window),
            canvas: RefCell::new(None),
            hover: RefCell::default(),
            placed: Cell::default(),
            clicks: Clicks::default(),
            dragging: Cell::new(None),
            tracking: Cell::new(false),
        }
    }

    pub(crate) fn handle(&self, message: u32, _wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        match message {
            WM_MOUSEACTIVATE => return Some(MA_NOACTIVATE.try_into().unwrap_or_default()),
            WM_SETCURSOR => {
                super::set_cursor(IDC_ARROW);
                return Some(1);
            }
            WM_LBUTTONDOWN => self.mouse_down(),
            WM_MOUSEMOVE => self.mouse_moved(),
            WM_LBUTTONUP => self.mouse_up(),
            WM_CAPTURECHANGED => self.capture_lost(),
            WM_MOUSELEAVE => {
                self.tracking.set(false);
                self.follow_pointer();
            }
            WM_CONTEXTMENU => {
                let at = super::menu_position(lparam).unwrap_or(self.placed.get().origin);
                super::show_menu(at);
            }
            WM_CLOSE => super::send(Event::Command(Command::Quit)),
            WM_DPICHANGED => super::screens_changed(),
            _ => return None,
        }
        Some(0)
    }

    pub(crate) fn present(&self, canvas: &Canvas, origin: (i32, i32), scale: i32) {
        self.surface.present(canvas, origin, scale);
        self.placed.set(Placed { origin, scale });
        self.canvas.replace(Some(canvas.clone()));
        super::bring_forward(self.window);
    }

    pub(crate) fn hide(&self) {
        super::hide(self.window);
    }

    pub(crate) fn is_shown(&self) -> bool {
        win32::is_visible(self.window)
    }

    pub(crate) fn follow_pointer(&self) {
        let placed = self.placed.get();
        let (x, y) = placed.inside(placed.on_screen(win32::cursor()));
        let event = {
            let canvas = self.canvas.borrow();
            let canvas = canvas.as_ref().filter(|_| self.is_shown());
            self.hover.borrow_mut().follow(canvas, x, y)
        };
        if let Some(event) = event {
            super::send(event);
        }
    }

    fn mouse_down(&self) {
        let pointer = win32::cursor();
        if self.clicks.press(pointer) == 2 {
            super::send(Event::ClippyDoubleClick);
        } else {
            self.dragging.set(Some(Drag {
                pointer,
                origin: self.placed.get().origin,
            }));
            unsafe { SetCapture(self.window) };
        }
    }

    fn mouse_moved(&self) {
        if let Some(drag) = self.dragging.get() {
            super::drag(drag);
            return;
        }
        if !self.tracking.replace(true) {
            let mut leaving = TRACKMOUSEEVENT {
                cbSize: size_of_u32::<TRACKMOUSEEVENT>(),
                dwFlags: TME_LEAVE,
                hwndTrack: self.window,
                dwHoverTime: 0,
            };
            unsafe { TrackMouseEvent(&raw mut leaving) };
        }
        self.follow_pointer();
    }

    fn mouse_up(&self) {
        if let Some(drag) = self.dragging.take() {
            unsafe { ReleaseCapture() };
            super::dropped(drag);
        }
    }

    fn capture_lost(&self) {
        if let Some(drag) = self.dragging.take() {
            super::dropped(drag);
        }
    }
}
