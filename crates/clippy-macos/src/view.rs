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

mod balloon_view;
mod clippy_view;
mod editing;
mod surface;

use std::ptr::NonNull;

use block2::RcBlock;
use clippy::Event;
use objc2::rc::{Retained, Weak};
use objc2::runtime::AnyObject;
use objc2::{AnyThread, Message};
use objc2_app_kit::{
    NSEvent, NSEventMask, NSTrackingArea, NSTrackingAreaOptions, NSView, NSWindow,
};
use objc2_foundation::NSRect;

use crate::delegate::Delegate;

pub(crate) use balloon_view::BalloonView;
pub(crate) use clippy_view::ClippyView;

fn track(view: &NSView, options: NSTrackingAreaOptions) {
    let owner: &AnyObject = view;
    let area = unsafe {
        NSTrackingArea::initWithRect_options_owner_userInfo(
            NSTrackingArea::alloc(),
            NSRect::ZERO,
            options | NSTrackingAreaOptions::ActiveAlways | NSTrackingAreaOptions::InVisibleRect,
            Some(owner),
            None,
        )
    };
    view.addTrackingArea(&area);
}

fn watch_pointer<V: Message + 'static>(view: &Retained<V>, follow: fn(&V)) {
    let follow = {
        let view = Weak::from_retained(view);
        move || {
            if let Some(view) = view.load() {
                follow(&view);
            }
        }
    };
    let elsewhere = RcBlock::new({
        let follow = follow.clone();
        move |_| follow()
    });
    let here = RcBlock::new(move |event: NonNull<NSEvent>| {
        follow();
        event.as_ptr()
    });
    NSEvent::addGlobalMonitorForEventsMatchingMask_handler(NSEventMask::MouseMoved, &elsewhere);
    unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::MouseMoved, &here)
    };
}

#[expect(clippy::cast_possible_truncation)]
fn pointer(view: &NSView, window: &NSWindow) -> (i32, i32) {
    let point = view.convertPoint_fromView(
        window.convertPointFromScreen(NSEvent::mouseLocation()),
        None,
    );
    let x = point.x.floor() as i32;
    let y = (view.bounds().size.height - point.y).floor() as i32;
    (x, y)
}

fn catch_clicks(window: &NSWindow, solid: bool) {
    if window.ignoresMouseEvents() == solid {
        window.setIgnoresMouseEvents(!solid);
    }
}

fn send(delegate: &Weak<Delegate>, event: Event) {
    if let Some(delegate) = delegate.load() {
        delegate.send(event);
    }
}
