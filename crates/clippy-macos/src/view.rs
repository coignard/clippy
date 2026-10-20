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

use clippy::Event;
use objc2::AnyThread;
use objc2::rc::Weak;
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSTrackingArea, NSTrackingAreaOptions, NSView};
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

fn send(delegate: &Weak<Delegate>, event: Event) {
    if let Some(delegate) = delegate.load() {
        delegate.send(event);
    }
}
