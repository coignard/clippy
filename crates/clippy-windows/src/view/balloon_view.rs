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
use std::ptr;

use clippy::geometry::Rect;
use clippy::graphics::Canvas;
use clippy::{Edit, Event, Field, Utf16Range};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::UI::Input::Ime::{
    CANDIDATEFORM, CFS_EXCLUDE, CFS_POINT, COMPOSITIONFORM, CPS_CANCEL, GCS_COMPSTR, GCS_RESULTSTR,
    HIMC, IME_COMPOSITION_STRING, ISC_SHOWUICOMPOSITIONWINDOW, ImmGetCompositionStringW,
    ImmGetContext, ImmNotifyIME, ImmReleaseContext, ImmSetCandidateWindow, ImmSetCompositionWindow,
    NI_COMPOSITIONSTR,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, GetForegroundWindow, IDC_ARROW, IDC_IBEAM, MA_NOACTIVATE, SW_HIDE,
    SetForegroundWindow, ShowWindow, WM_CAPTURECHANGED, WM_CHAR, WM_CLOSE, WM_CONTEXTMENU,
    WM_DPICHANGED, WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION, WM_IME_SETCONTEXT,
    WM_IME_STARTCOMPOSITION, WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_SETCURSOR, WM_SETFOCUS,
};

use super::editing::{self, Keystroke, TextAction};
use super::surface::Surface;
use super::{Clicks, Drag, Placed};
use crate::clipboard;
use crate::menu::Menu;
use crate::win32;

const CUT: usize = 1;
const COPY: usize = 2;
const PASTE: usize = 3;
const SELECT_ALL: usize = 4;

pub(crate) struct BalloonView {
    window: HWND,
    surface: Surface,
    field: Cell<Option<Field>>,
    controls: RefCell<Vec<Rect>>,
    placed: Cell<Placed>,
    clicks: Clicks,
    selecting: Cell<bool>,
    pressing: Cell<Option<usize>>,
    dragging: Cell<Option<Drag>>,
    marked: Cell<Option<Utf16Range>>,
    surrogate: Cell<Option<u16>>,
    returns_to: Cell<HWND>,
}

struct InputContext {
    window: HWND,
    handle: HIMC,
}

impl BalloonView {
    pub(crate) fn new(window: HWND) -> Self {
        Self {
            window,
            surface: Surface::new(window),
            field: Cell::new(None),
            controls: RefCell::new(Vec::new()),
            placed: Cell::default(),
            clicks: Clicks::default(),
            selecting: Cell::new(false),
            pressing: Cell::new(None),
            dragging: Cell::new(None),
            marked: Cell::new(None),
            surrogate: Cell::new(None),
            returns_to: Cell::new(ptr::null_mut()),
        }
    }

    pub(crate) fn handle(&self, message: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        match message {
            WM_MOUSEACTIVATE => return Some(MA_NOACTIVATE.try_into().unwrap_or_default()),
            WM_SETCURSOR => {
                self.update_cursor();
                return Some(1);
            }
            WM_LBUTTONDOWN => self.mouse_down(lparam),
            WM_MOUSEMOVE => self.mouse_dragged(lparam),
            WM_LBUTTONUP => self.mouse_up(lparam),
            WM_CAPTURECHANGED => self.capture_lost(),
            WM_CONTEXTMENU => self.context_menu(lparam),
            WM_SETFOCUS => super::send(Event::Focus),
            WM_KILLFOCUS => super::send(Event::Blur),
            WM_KEYDOWN => return self.key_down(wparam),
            WM_CHAR => self.typed(wparam),
            WM_IME_SETCONTEXT => {
                let shown = !(ISC_SHOWUICOMPOSITIONWINDOW as usize);
                let lparam = (lparam.cast_unsigned() & shown).cast_signed();
                return Some(unsafe { DefWindowProcW(self.window, message, wparam, lparam) });
            }
            WM_IME_STARTCOMPOSITION => self.place_candidates(),
            WM_IME_COMPOSITION => self.compose(lparam),
            WM_IME_ENDCOMPOSITION => self.end_composition(),
            WM_CLOSE => super::send(Event::Cancel),
            WM_DPICHANGED => super::screens_changed(),
            _ => return None,
        }
        Some(0)
    }

    pub(crate) fn present(
        &self,
        canvas: &Canvas,
        field: Option<Field>,
        controls: &[Rect],
        origin: (i32, i32),
        scale: i32,
    ) {
        if field.is_none() {
            self.marked.set(None);
        }
        self.field.set(field);
        controls.clone_into(&mut self.controls.borrow_mut());
        self.placed.set(Placed { origin, scale });
        self.surface.present(canvas, origin, scale);
        super::bring_forward(self.window);
    }

    pub(crate) fn hide(&self) {
        self.release_focus();
        super::hide(self.window);
    }

    pub(crate) fn window(&self) -> HWND {
        self.window
    }

    pub(crate) fn has_focus(&self) -> bool {
        unsafe { GetForegroundWindow() == self.window }
    }

    pub(crate) fn focus(&self) {
        let foreground = unsafe { GetForegroundWindow() };
        if foreground == self.window {
            return;
        }
        if unsafe { SetForegroundWindow(self.window) } != 0 && !win32::is_ours(foreground) {
            self.returns_to.set(foreground);
        }
    }

    pub(crate) fn release_focus(&self) {
        if !self.has_focus() {
            return;
        }
        let previous = self.returns_to.replace(ptr::null_mut());
        let returned = win32::exists(previous) && unsafe { SetForegroundWindow(previous) } != 0;
        if !returned {
            unsafe { ShowWindow(self.window, SW_HIDE) };
        }
    }

    fn mouse_down(&self, lparam: LPARAM) {
        let (x, y) = self.placed.get().inside(win32::point(lparam));
        let clicks = self.clicks.press(win32::cursor());
        if let Some(item) = self.control_at(x, y) {
            self.pressing.set(Some(item));
            unsafe { SetCapture(self.window) };
            super::send(Event::ControlPress { item });
        } else if let Some(x) = self.field_x(x, y) {
            self.discard_marked_text();
            self.selecting.set(true);
            unsafe { SetCapture(self.window) };
            self.focus();
            let extend = editing::held().shift;
            super::send(Event::FieldPress { x, clicks, extend });
        } else if clicks == 2 {
            super::send(Event::MessageDoubleClick);
        } else if let Some(origin) = super::clippy_origin() {
            self.dragging.set(Some(Drag {
                pointer: win32::cursor(),
                origin,
            }));
            unsafe { SetCapture(self.window) };
        }
    }

    fn mouse_dragged(&self, lparam: LPARAM) {
        let (x, y) = self.placed.get().inside(win32::point(lparam));
        if let Some(drag) = self.dragging.get() {
            super::drag(drag);
        } else if let Some(item) = self.pressing.get() {
            super::send(Event::ControlDrag {
                inside: self.control_at(x, y) == Some(item),
            });
        } else if self.selecting.get()
            && let Some(field) = self.field.get()
        {
            super::send(Event::FieldDrag {
                x: x - field.frame.x,
            });
        }
    }

    fn mouse_up(&self, lparam: LPARAM) {
        let (x, y) = self.placed.get().inside(win32::point(lparam));
        self.selecting.set(false);
        let dragged = self.dragging.take();
        let pressed = self.pressing.take();
        unsafe { ReleaseCapture() };
        if let Some(item) = pressed {
            super::send(Event::ControlRelease {
                inside: self.control_at(x, y) == Some(item),
            });
        }
        if let Some(drag) = dragged {
            super::dropped(drag);
        }
    }

    fn capture_lost(&self) {
        self.selecting.set(false);
        if let Some(drag) = self.dragging.take() {
            super::dropped(drag);
        }
        if self.pressing.take().is_some() {
            super::send(Event::ControlRelease { inside: false });
        }
    }

    fn context_menu(&self, lparam: LPARAM) {
        let placed = self.placed.get();
        let in_field = |at| {
            let (x, y) = placed.inside(placed.on_screen(at));
            self.field_x(x, y).is_some()
        };
        match super::menu_position(lparam) {
            Some(at) if in_field(at) => self.text_menu(at),
            Some(at) => super::show_menu(at),
            None => match self.field.get() {
                Some(field) => {
                    let frame = field.frame.scaled(placed.scale);
                    self.text_menu((placed.origin.0 + frame.x, placed.origin.1 + frame.bottom()));
                }
                None => super::show_menu(placed.origin),
            },
        }
    }

    fn text_menu(&self, at: (i32, i32)) {
        self.focus();
        let selected = self
            .field
            .get()
            .is_some_and(|field| field.selection.start != field.selection.end);
        let menu = Menu::new();
        menu.item(CUT, "Cu&t\tCtrl+X", selected);
        menu.item(COPY, "&Copy\tCtrl+C", selected);
        menu.item(PASTE, "&Paste\tCtrl+V", clipboard::has_text());
        menu.separator();
        menu.item(SELECT_ALL, "Select &All\tCtrl+A", true);
        let action = match menu.track(self.window, at) {
            Some(CUT) => TextAction::Cut,
            Some(COPY) => TextAction::Copy,
            Some(PASTE) => TextAction::Paste,
            Some(SELECT_ALL) => TextAction::SelectAll,
            _ => return,
        };
        self.act(action);
    }

    fn key_down(&self, wparam: WPARAM) -> Option<LRESULT> {
        let key = u16::try_from(wparam).ok()?;
        match editing::keystroke(key, editing::held())? {
            Keystroke::Action(action) if self.field.get().is_some() => self.act(action),
            Keystroke::Action(_) => return None,
            Keystroke::Event(event) => super::send(event),
        }
        Some(0)
    }

    fn typed(&self, wparam: WPARAM) {
        let Ok(unit) = u16::try_from(wparam) else {
            return;
        };
        let units = match (self.surrogate.take(), unit) {
            (_, 0xD800..=0xDBFF) => {
                self.surrogate.set(Some(unit));
                return;
            }
            (Some(high), 0xDC00..=0xDFFF) => vec![high, unit],
            _ => vec![unit],
        };
        let Ok(text) = String::from_utf16(&units) else {
            return;
        };
        if !text.chars().any(char::is_control) {
            super::send(Event::Edit(Edit::Insert(text)));
        }
    }

    fn act(&self, action: TextAction) {
        let edit = match action {
            TextAction::SelectAll => Edit::SelectAll,
            TextAction::Copy => Edit::Copy,
            TextAction::Cut => Edit::Cut,
            TextAction::Paste => match clipboard::text(self.window) {
                Some(text) => Edit::Paste(text),
                None => return,
            },
            TextAction::Undo => Edit::Undo,
            TextAction::Redo => Edit::Redo,
        };
        super::send(Event::Edit(edit));
    }

    fn compose(&self, lparam: LPARAM) {
        let flags = u32::try_from(lparam).unwrap_or_default();
        let Some(context) = InputContext::of(self.window) else {
            return;
        };
        let result = (flags & GCS_RESULTSTR != 0).then(|| context.string(GCS_RESULTSTR));
        let composition = (flags & GCS_COMPSTR != 0).then(|| context.string(GCS_COMPSTR));
        drop(context);
        if let Some(text) = result {
            self.commit(text);
        }
        if let Some(text) = composition {
            self.mark(text);
        }
        self.place_candidates();
    }

    fn commit(&self, text: String) {
        match self.marked.take() {
            Some(range) => super::send(Event::Edit(Edit::Replace { range, text })),
            None if !text.is_empty() => super::send(Event::Edit(Edit::Insert(text))),
            None => {}
        }
    }

    fn mark(&self, text: String) {
        let Some(field) = self.field.get() else {
            return;
        };
        if text.is_empty() && self.marked.get().is_none() {
            return;
        }
        let range = self.marked.get().unwrap_or(field.selection);
        let marked = Utf16Range {
            start: range.start,
            end: range.start + text.encode_utf16().count(),
        };
        self.marked
            .set((marked.start != marked.end).then_some(marked));
        super::send(Event::Edit(Edit::Replace { range, text }));
    }

    fn end_composition(&self) {
        if let Some(range) = self.marked.take() {
            super::send(Event::Edit(Edit::Replace {
                range,
                text: String::new(),
            }));
        }
    }

    fn discard_marked_text(&self) {
        if self.marked.take().is_some()
            && let Some(context) = InputContext::of(self.window)
        {
            unsafe { ImmNotifyIME(context.handle, NI_COMPOSITIONSTR, CPS_CANCEL, 0) };
        }
    }

    fn place_candidates(&self) {
        let (Some(field), Some(context)) = (self.field.get(), InputContext::of(self.window)) else {
            return;
        };
        let frame = field.frame.scaled(self.placed.get().scale);
        let area = RECT {
            left: frame.x,
            top: frame.y,
            right: frame.right(),
            bottom: frame.bottom(),
        };
        let composition = COMPOSITIONFORM {
            dwStyle: CFS_POINT,
            ptCurrentPos: POINT {
                x: frame.x,
                y: frame.y,
            },
            rcArea: area,
        };
        let candidates = CANDIDATEFORM {
            dwIndex: 0,
            dwStyle: CFS_EXCLUDE,
            ptCurrentPos: POINT {
                x: frame.x,
                y: frame.bottom(),
            },
            rcArea: area,
        };
        unsafe {
            ImmSetCompositionWindow(context.handle, &raw const composition);
            ImmSetCandidateWindow(context.handle, &raw const candidates);
        }
    }

    fn update_cursor(&self) {
        let placed = self.placed.get();
        let (x, y) = placed.inside(placed.on_screen(win32::cursor()));
        let cursor = if self.field_x(x, y).is_some() {
            IDC_IBEAM
        } else {
            IDC_ARROW
        };
        super::set_cursor(cursor);
    }

    fn field_x(&self, x: i32, y: i32) -> Option<i32> {
        let field = self.field.get()?;
        field.frame.contains(x, y).then_some(x - field.frame.x)
    }

    fn control_at(&self, x: i32, y: i32) -> Option<usize> {
        self.controls
            .borrow()
            .iter()
            .position(|control| control.contains(x, y))
    }
}

impl InputContext {
    fn of(window: HWND) -> Option<Self> {
        let handle = unsafe { ImmGetContext(window) };
        (!handle.is_null()).then_some(Self { window, handle })
    }

    fn string(&self, kind: IME_COMPOSITION_STRING) -> String {
        let bytes = unsafe { ImmGetCompositionStringW(self.handle, kind, ptr::null_mut(), 0) };
        let (Ok(length), Ok(capacity)) = (usize::try_from(bytes), u32::try_from(bytes)) else {
            return String::new();
        };
        let mut units = vec![0_u16; length / size_of::<u16>()];
        let read = unsafe {
            ImmGetCompositionStringW(self.handle, kind, units.as_mut_ptr().cast(), capacity)
        };
        units.truncate(usize::try_from(read).unwrap_or_default() / size_of::<u16>());
        String::from_utf16_lossy(&units)
    }
}

impl Drop for InputContext {
    fn drop(&mut self) {
        unsafe { ImmReleaseContext(self.window, self.handle) };
    }
}
