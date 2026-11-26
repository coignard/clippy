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

use std::cell::Cell;
use std::ptr;

use clippy::geometry::{Rect, Size};
use clippy::{BalloonScene, Scene};
use windows_sys::Win32::Foundation::{ERROR_SUCCESS, POINT};
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTONULL,
    MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint,
};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, KEY_SET_VALUE, REG_DWORD, REG_OPTION_NON_VOLATILE, RRF_RT_REG_DWORD,
    RegCloseKey, RegCreateKeyExW, RegGetValueW, RegSetValueExW,
};
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows_sys::core::{PCWSTR, w};

use crate::panel;
use crate::view::{BalloonView, ClippyView};
use crate::win32::{self, size_of_u32};

const STANDARD_DPI: u32 = 96;
const REMEMBERED: PCWSTR = w!("Software\\Clippy");
const LEFT: PCWSTR = w!("Left");
const TOP: PCWSTR = w!("Top");
const ANYWHERE: Display = Display {
    frame: Rect::new(0, 0, 1024, 768),
    work: Rect::new(0, 0, 1024, 768),
    scale: 1,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Display {
    pub(crate) frame: Rect,
    work: Rect,
    scale: i32,
}

pub(crate) struct Stage {
    clippy: ClippyView,
    balloon: BalloonView,
    size: Size,
    origin: Cell<(i32, i32)>,
    scale: Cell<i32>,
}

impl Stage {
    pub(crate) fn new(size: Size) -> Self {
        let clippy = panel::clippy();
        let balloon = panel::balloon(clippy);
        Self {
            clippy: ClippyView::new(clippy),
            balloon: BalloonView::new(balloon),
            size,
            origin: Cell::new((0, 0)),
            scale: Cell::new(1),
        }
    }

    pub(crate) fn clippy(&self) -> &ClippyView {
        &self.clippy
    }

    pub(crate) fn balloon(&self) -> &BalloonView {
        &self.balloon
    }

    pub(crate) fn home_screen() -> Rect {
        let display = Display::primary();
        display.points(display.work)
    }

    pub(crate) fn is_lost(&self) -> bool {
        self.display().is_none()
    }

    pub(crate) fn place(&self, clippy: Rect) {
        let display = Display::primary();
        let frame = display.pixels(clippy);
        self.scale.set(display.scale);
        self.move_to((frame.x, frame.y));
    }

    pub(crate) fn return_to_where_left(&self, home: Rect) {
        self.place(home);
        if let Some(origin) = remembered() {
            self.move_to(origin);
            if self.is_lost() {
                self.place(home);
            }
        }
    }

    pub(crate) fn remember(&self) {
        remember(self.origin.get());
    }

    pub(crate) fn settle(&self) {
        self.move_to(self.origin.get());
    }

    pub(crate) fn move_to(&self, origin: (i32, i32)) {
        let size = self.size.scaled(self.scale.get());
        let center = (origin.0 + size.width / 2, origin.1 + size.height / 2);
        let display = Display::nearest(center);
        let origin = if display.scale == self.scale.get() {
            origin
        } else {
            let size = self.size.scaled(display.scale);
            (center.0 - size.width / 2, center.1 - size.height / 2)
        };
        self.scale.set(display.scale);
        self.origin.set(display.snap(origin));
    }

    pub(crate) fn origin(&self) -> (i32, i32) {
        self.origin.get()
    }

    pub(crate) fn display(&self) -> Option<Display> {
        let (x, y) = self.origin.get();
        let size = self.size.scaled(self.scale.get());
        Display::containing((x + size.width / 2, y + size.height / 2))
    }

    pub(crate) fn geometry(&self) -> (Rect, Rect) {
        let display = self.display().unwrap_or_else(Display::primary);
        let (x, y) = self.origin.get();
        let origin = display.points(Rect::new(x, y, 0, 0));
        let clippy = Rect::new(origin.x, origin.y, self.size.width, self.size.height);
        (clippy, display.points(display.work))
    }

    pub(crate) fn shows_clippy(&self) -> bool {
        self.clippy.is_shown()
    }

    pub(crate) fn show(&self, scene: &Scene) {
        match &scene.clippy {
            Some(canvas) => self
                .clippy
                .present(canvas, self.origin.get(), self.scale.get()),
            None => self.clippy.hide(),
        }
        self.clippy.follow_pointer();
        match &scene.balloon {
            Some(balloon) => self.show_balloon(balloon),
            None => self.balloon.hide(),
        }
    }

    fn show_balloon(&self, scene: &BalloonScene) {
        if scene.field.is_none() {
            self.balloon.release_focus();
        }
        let display = self.display().unwrap_or_else(Display::primary);
        let frame = display.pixels(scene.frame);
        self.balloon.present(
            &scene.canvas,
            scene.field,
            &scene.controls,
            (frame.x, frame.y),
            display.scale,
        );
        if scene.field.is_some_and(|field| field.wants_focus) {
            self.balloon.focus();
        }
    }
}

impl Display {
    pub(crate) fn primary() -> Self {
        let monitor = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
        Self::of(monitor).unwrap_or(ANYWHERE)
    }

    fn nearest((x, y): (i32, i32)) -> Self {
        let monitor = unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) };
        Self::of(monitor).unwrap_or_else(Self::primary)
    }

    fn containing((x, y): (i32, i32)) -> Option<Self> {
        let monitor = unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONULL) };
        Self::of(monitor)
    }

    fn of(monitor: HMONITOR) -> Option<Self> {
        if monitor.is_null() {
            return None;
        }
        let mut information = MONITORINFO {
            cbSize: size_of_u32::<MONITORINFO>(),
            ..MONITORINFO::default()
        };
        if unsafe { GetMonitorInfoW(monitor, &raw mut information) } == 0 {
            return None;
        }
        let (mut dpi, mut unused) = (STANDARD_DPI, 0);
        unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &raw mut dpi, &raw mut unused) };
        Some(Self {
            frame: win32::rect(information.rcMonitor),
            work: win32::rect(information.rcWork),
            scale: scale_for(dpi),
        })
    }

    fn points(self, rect: Rect) -> Rect {
        let Self { frame, scale, .. } = self;
        Rect::new(
            frame.x + (rect.x - frame.x).div_euclid(scale),
            frame.y + (rect.y - frame.y).div_euclid(scale),
            rect.width / scale,
            rect.height / scale,
        )
    }

    fn pixels(self, rect: Rect) -> Rect {
        let Self { frame, scale, .. } = self;
        Rect::new(
            frame.x + (rect.x - frame.x) * scale,
            frame.y + (rect.y - frame.y) * scale,
            rect.width * scale,
            rect.height * scale,
        )
    }

    fn snap(self, (x, y): (i32, i32)) -> (i32, i32) {
        let snapped = self.pixels(self.points(Rect::new(x, y, 0, 0)));
        (snapped.x, snapped.y)
    }
}

fn scale_for(dpi: u32) -> i32 {
    i32::try_from((dpi + STANDARD_DPI / 2) / STANDARD_DPI)
        .unwrap_or(1)
        .max(1)
}

fn remembered() -> Option<(i32, i32)> {
    Some((remembered_value(LEFT)?, remembered_value(TOP)?))
}

fn remembered_value(name: PCWSTR) -> Option<i32> {
    let mut value = 0_u32;
    let mut size = size_of_u32::<u32>();
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            REMEMBERED,
            name,
            RRF_RT_REG_DWORD,
            ptr::null_mut(),
            (&raw mut value).cast(),
            &raw mut size,
        )
    };
    (status == ERROR_SUCCESS).then_some(value.cast_signed())
}

fn remember((x, y): (i32, i32)) {
    let mut key = ptr::null_mut();
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            REMEMBERED,
            0,
            ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            ptr::null(),
            &raw mut key,
            ptr::null_mut(),
        )
    };
    if status != ERROR_SUCCESS {
        log::warn!("Could not remember where Clippy is: error {status}");
        return;
    }
    for (name, value) in [(LEFT, x), (TOP, y)] {
        let bytes = value.cast_unsigned().to_le_bytes();
        let length = size_of_u32::<u32>();
        unsafe { RegSetValueExW(key, name, 0, REG_DWORD, bytes.as_ptr(), length) };
    }
    unsafe { RegCloseKey(key) };
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHARP: Display = Display {
        frame: Rect::new(1920, -120, 3840, 2160),
        work: Rect::new(1920, -120, 3840, 2088),
        scale: 2,
    };

    #[test]
    fn displays_blow_pixels_up_by_whole_steps() {
        assert_eq!(scale_for(96), 1);
        assert_eq!(scale_for(120), 1);
        assert_eq!(scale_for(144), 2);
        assert_eq!(scale_for(192), 2);
        assert_eq!(scale_for(240), 3);
        assert_eq!(scale_for(0), 1);
    }

    #[test]
    fn points_on_a_display_turn_into_pixels_and_back() {
        let points = SHARP.points(SHARP.work);
        assert_eq!(points, Rect::new(1920, -120, 1920, 1044));
        assert_eq!(SHARP.pixels(points), SHARP.work);
        let clippy = Rect::new(3600, 800, 124, 93);
        assert_eq!(SHARP.points(SHARP.pixels(clippy)), clippy);
    }

    #[test]
    fn clippy_lands_on_whole_points() {
        assert_eq!(SHARP.snap((2001, 1)), (2000, 0));
        assert_eq!(SHARP.snap((2000, -119)), (2000, -120));
        assert_eq!(ANYWHERE.snap((7, -3)), (7, -3));
    }
}
