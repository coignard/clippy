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

use std::cell::RefCell;
use std::ptr::NonNull;

use block2::RcBlock;
use clippy::graphics::Canvas;
use clippy::{Event, Hover};
use objc2::rc::{Retained, Weak};
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSEvent, NSEventMask, NSMenu, NSResponder, NSTrackingAreaOptions, NSView};
use objc2_foundation::{NSObject, NSPoint, NSRect};

use crate::delegate::Delegate;
use crate::view::surface::Surface;

pub(crate) struct Ivars {
    delegate: Weak<Delegate>,
    surface: Surface,
    canvas: RefCell<Option<Canvas>>,
    hover: RefCell<Hover>,
}

define_class!(
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    pub(crate) struct ClippyView;

    impl ClippyView {
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(mouseDownCanMoveWindow))]
        fn mouse_down_can_move_window(&self) -> bool {
            false
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            if event.clickCount() == 2 {
                self.send(Event::ClippyDoubleClick);
            } else if let Some(window) = self.window() {
                window.performWindowDragWithEvent(event);
            }
        }

        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, _event: &NSEvent) {
            self.follow_pointer();
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            self.follow_pointer();
        }

        #[unsafe(method_id(menuForEvent:))]
        fn menu_for_event(&self, _event: &NSEvent) -> Option<Retained<NSMenu>> {
            self.ivars().delegate.load().map(|delegate| delegate.menu())
        }
    }
);

impl ClippyView {
    pub(crate) fn new(delegate: &Delegate) -> Retained<Self> {
        let this = Self::alloc(delegate.mtm()).set_ivars(Ivars {
            delegate: Weak::new(delegate),
            surface: Surface::new(),
            canvas: RefCell::new(None),
            hover: RefCell::default(),
        });
        let view: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: NSRect::ZERO] };
        view.ivars().surface.host_in(&view);
        super::track(
            &view,
            NSTrackingAreaOptions::MouseEnteredAndExited | NSTrackingAreaOptions::MouseMoved,
        );
        let follow = {
            let view = Weak::from_retained(&view);
            move || {
                if let Some(view) = view.load() {
                    view.follow_pointer();
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
        view
    }

    pub(crate) fn present(&self, canvas: &Canvas) {
        self.ivars().surface.present(canvas, self.bounds());
        self.ivars().canvas.replace(Some(canvas.clone()));
    }

    pub(crate) fn follow_pointer(&self) {
        let Some(window) = self.window() else {
            return;
        };
        let (x, y) = self.pixel_at(self.convertPoint_fromView(
            window.convertPointFromScreen(NSEvent::mouseLocation()),
            None,
        ));
        let event = {
            let canvas = self.ivars().canvas.borrow();
            let canvas = canvas.as_ref().filter(|_| window.isVisible());
            let ignores = !canvas.is_some_and(|canvas| canvas.is_solid(x, y));
            if window.ignoresMouseEvents() != ignores {
                window.setIgnoresMouseEvents(ignores);
            }
            self.ivars().hover.borrow_mut().follow(canvas, x, y)
        };
        if let Some(event) = event {
            self.send(event);
        }
    }

    #[expect(clippy::cast_possible_truncation)]
    fn pixel_at(&self, point: NSPoint) -> (i32, i32) {
        let x = point.x.floor() as i32;
        let y = (self.bounds().size.height - point.y).floor() as i32;
        (x, y)
    }

    fn send(&self, event: Event) {
        super::send(&self.ivars().delegate, event);
    }
}
