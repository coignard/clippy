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

use clippy::geometry::Rect;
use objc2_core_foundation::{
    CFArray, CFDictionary, CFNumber, CFRetained, CFString, CFType, CGPoint, CGRect, CGSize,
};
use objc2_core_graphics::{
    CGRectContainsRect, CGRectInset, CGRectIntersectsRect, CGRectMakeWithDictionaryRepresentation,
    CGWindowListCopyWindowInfo, CGWindowListOption, kCGNullWindowID, kCGWindowAlpha,
    kCGWindowBounds, kCGWindowLayer, kCGWindowOwnerName, kCGWindowOwnerPID,
};

const NORMAL_LAYER: i64 = 0;
const SLACK: f64 = 1.0;

type WindowInfo = CFDictionary<CFString, CFType>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Display {
    pub(crate) frame: Rect,
    pub(crate) notch: i32,
}

struct Window {
    owner: i64,
    bounds: CGRect,
}

pub(crate) fn is_taken(display: Display) -> bool {
    let Some(windows) = CGWindowListCopyWindowInfo(
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements,
        kCGNullWindowID,
    ) else {
        return false;
    };
    let windows: CFRetained<CFArray<WindowInfo>> = unsafe { CFRetained::cast_unchecked(windows) };
    let ours = i64::from(std::process::id());
    let others: Vec<Window> = windows
        .iter()
        .filter_map(|window| normal(&window))
        .filter(|window| window.owner != ours && overlaps(window.bounds, display.frame))
        .collect();
    let taken = others.iter().find(|window| {
        let below_the_notch = covers(window.bounds, display, display.notch);
        covers(window.bounds, display, 0)
            || (below_the_notch && others.iter().all(|other| other.owner == window.owner))
    });
    if let Some(window) = taken {
        log::debug!("A window of process {} fills {display:?}", window.owner);
    }
    taken.is_some()
}

fn normal(window: &WindowInfo) -> Option<Window> {
    let layer = number(window, unsafe { kCGWindowLayer })?.as_i64()?;
    let alpha = number(window, unsafe { kCGWindowAlpha })?.as_f64()?;
    if layer != NORMAL_LAYER || alpha <= 0.0 {
        return None;
    }
    if log::log_enabled!(log::Level::Trace) {
        let name = window
            .get(unsafe { kCGWindowOwnerName })
            .and_then(|name| name.downcast::<CFString>().ok());
        log::trace!("On screen: {name:?} at {:?}", bounds(window));
    }
    Some(Window {
        owner: number(window, unsafe { kCGWindowOwnerPID })?.as_i64()?,
        bounds: bounds(window)?,
    })
}

fn number(window: &WindowInfo, key: &CFString) -> Option<CFRetained<CFNumber>> {
    window.get(key)?.downcast::<CFNumber>().ok()
}

fn bounds(window: &WindowInfo) -> Option<CGRect> {
    let bounds = window
        .get(unsafe { kCGWindowBounds })?
        .downcast::<CFDictionary>()
        .ok()?;
    let mut rect = CGRect::default();
    unsafe { CGRectMakeWithDictionaryRepresentation(Some(&bounds), &raw mut rect) }.then_some(rect)
}

fn overlaps(window: CGRect, display: Rect) -> bool {
    CGRectIntersectsRect(window, cg(display))
}

fn covers(window: CGRect, display: Display, top_inset: i32) -> bool {
    let frame = display.frame;
    let area = Rect::new(
        frame.x,
        frame.y + top_inset,
        frame.width,
        frame.height - top_inset,
    );
    CGRectContainsRect(CGRectInset(window, -SLACK, -SLACK), cg(area))
}

fn cg(rect: Rect) -> CGRect {
    CGRect::new(
        CGPoint::new(f64::from(rect.x), f64::from(rect.y)),
        CGSize::new(f64::from(rect.width), f64::from(rect.height)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTCHED: Display = Display {
        frame: Rect::new(0, 0, 1512, 982),
        notch: 38,
    };

    fn window(y: f64, height: f64) -> CGRect {
        CGRect::new(CGPoint::new(0.0, y), CGSize::new(1512.0, height))
    }

    #[test]
    fn full_screen_windows_may_start_below_the_notch() {
        assert!(covers(window(0.0, 982.0), NOTCHED, 0));
        assert!(!covers(window(38.0, 944.0), NOTCHED, 0));
        assert!(covers(window(38.0, 944.0), NOTCHED, NOTCHED.notch));
        assert!(!covers(window(38.0, 900.0), NOTCHED, NOTCHED.notch));
        assert!(!covers(window(60.0, 922.0), NOTCHED, NOTCHED.notch));
    }

    #[test]
    fn windows_on_other_displays_do_not_count() {
        let elsewhere = CGRect::new(CGPoint::new(1512.0, 0.0), CGSize::new(1920.0, 1080.0));
        assert!(!overlaps(elsewhere, NOTCHED.frame));
        assert!(overlaps(window(38.0, 944.0), NOTCHED.frame));
    }
}
