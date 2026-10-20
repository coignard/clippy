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

use std::cell::Cell;

use clippy::geometry::Rect;
use clippy::graphics::Canvas;
use clippy::{Edit, Event, Field, Utf16Range};
use objc2::rc::{Retained, Weak};
use objc2::runtime::{AnyObject, Sel};
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSCursor, NSEvent, NSEventModifierFlags, NSMenu, NSMenuItem,
    NSMenuItemValidation, NSResponder, NSTextInputClient, NSTrackingAreaOptions, NSView, NSWindow,
};
use objc2_foundation::{
    NSArray, NSAttributedString, NSAttributedStringKey, NSNotFound, NSObject, NSObjectProtocol,
    NSPoint, NSRange, NSRangePointer, NSRect, NSSize, NSString, NSUInteger,
};

use crate::delegate::Delegate;
use crate::menu;
use crate::pasteboard;
use crate::view::editing::{self, TextAction};
use crate::view::surface::Surface;

pub(crate) struct Ivars {
    delegate: Weak<Delegate>,
    surface: Surface,
    field: Cell<Option<Field>>,
    checkbox: Cell<Option<Rect>>,
    selecting: Cell<bool>,
    ticking: Cell<bool>,
    dragging: Cell<Option<Drag>>,
    marked: Cell<Option<Utf16Range>>,
}

#[derive(Clone, Copy)]
struct Drag {
    pointer: NSPoint,
    origin: NSPoint,
}

define_class!(
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    pub(crate) struct BalloonView;

    unsafe impl NSObjectProtocol for BalloonView {}

    unsafe impl NSMenuItemValidation for BalloonView {
        #[unsafe(method(validateMenuItem:))]
        fn validate_menu_item(&self, item: &NSMenuItem) -> bool {
            let has_selection = self
                .field()
                .is_some_and(|field| field.selection.start != field.selection.end);
            match item.action() {
                Some(action) if action == sel!(cut:) || action == sel!(copy:) => has_selection,
                Some(action) if action == sel!(paste:) => pasteboard::has_text(),
                _ => true,
            }
        }
    }

    unsafe impl NSTextInputClient for BalloonView {
        #[unsafe(method(insertText:replacementRange:))]
        fn insert_text(&self, text: &AnyObject, replacement: NSRange) {
            let text = plain(text);
            let range = utf16(replacement).or_else(|| self.ivars().marked.take());
            self.ivars().marked.set(None);
            match range {
                Some(range) => self.send(Event::Edit(Edit::Replace { range, text })),
                None if !text.is_empty() => self.send(Event::Edit(Edit::Insert(text))),
                None => {}
            }
        }

        #[unsafe(method(doCommandBySelector:))]
        fn do_command_by_selector(&self, selector: Sel) {
            if let Some(event) = editing::command(selector) {
                self.send(event);
            }
        }

        #[unsafe(method(setMarkedText:selectedRange:replacementRange:))]
        fn set_marked_text(&self, text: &AnyObject, _selected: NSRange, replacement: NSRange) {
            let Some(field) = self.field() else {
                return;
            };
            let text = plain(text);
            let range = utf16(replacement)
                .or_else(|| self.ivars().marked.get())
                .unwrap_or(field.selection);
            let marked = Utf16Range {
                start: range.start,
                end: range.start + text.encode_utf16().count(),
            };
            self.ivars()
                .marked
                .set((marked.start != marked.end).then_some(marked));
            self.send(Event::Edit(Edit::Replace { range, text }));
        }

        #[unsafe(method(unmarkText))]
        fn unmark_text(&self) {
            self.ivars().marked.set(None);
        }

        #[unsafe(method(selectedRange))]
        fn selected_range(&self) -> NSRange {
            self.field()
                .map_or_else(nowhere, |field| ns_range(field.selection))
        }

        #[unsafe(method(markedRange))]
        fn marked_range(&self) -> NSRange {
            self.ivars().marked.get().map_or_else(nowhere, ns_range)
        }

        #[unsafe(method(hasMarkedText))]
        fn has_marked_text(&self) -> bool {
            self.ivars().marked.get().is_some()
        }

        #[unsafe(method_id(attributedSubstringForProposedRange:actualRange:))]
        fn attributed_substring(
            &self,
            _range: NSRange,
            _actual: NSRangePointer,
        ) -> Option<Retained<NSAttributedString>> {
            None
        }

        #[unsafe(method_id(validAttributesForMarkedText))]
        fn valid_attributes_for_marked_text(&self) -> Retained<NSArray<NSAttributedStringKey>> {
            NSArray::new()
        }

        #[unsafe(method(firstRectForCharacterRange:actualRange:))]
        fn first_rect(&self, _range: NSRange, _actual: NSRangePointer) -> NSRect {
            let Some((field, window)) = self.field().zip(self.window()) else {
                return NSRect::ZERO;
            };
            let height = self.bounds().size.height;
            let frame = field.frame;
            let local = NSRect::new(
                NSPoint::new(f64::from(frame.x), height - f64::from(frame.bottom())),
                NSSize::new(f64::from(frame.width), f64::from(frame.height)),
            );
            window.convertRectToScreen(self.convertRect_toView(local, None))
        }

        #[unsafe(method(characterIndexForPoint:))]
        fn character_index_for_point(&self, _point: NSPoint) -> NSUInteger {
            NSNotFound.unsigned_abs()
        }
    }

    impl BalloonView {
        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(mouseDownCanMoveWindow))]
        fn mouse_down_can_move_window(&self) -> bool {
            false
        }

        #[unsafe(method(needsPanelToBecomeKey))]
        fn needs_panel_to_become_key(&self) -> bool {
            NSApplication::sharedApplication(self.mtm())
                .currentEvent()
                .is_some_and(|event| self.field_x(&event).is_some())
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let clicks = usize::try_from(event.clickCount()).unwrap_or(1);
            if self.on_checkbox(event) {
                self.ivars().ticking.set(true);
                self.send(Event::CheckboxPress);
            } else if let Some(x) = self.field_x(event) {
                self.discard_marked_text();
                self.ivars().selecting.set(true);
                let extend = event.modifierFlags().contains(NSEventModifierFlags::Shift);
                self.send(Event::FieldPress { x, clicks, extend });
            } else if clicks == 2 {
                self.send(Event::MessageDoubleClick);
            } else if let Some(clippy) = self.clippy_window() {
                self.ivars().dragging.set(Some(Drag {
                    pointer: NSEvent::mouseLocation(),
                    origin: clippy.frame().origin,
                }));
            }
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            if let Some(drag) = self.ivars().dragging.get() {
                let pointer = NSEvent::mouseLocation();
                if let Some(clippy) = self.clippy_window() {
                    clippy.setFrameOrigin(NSPoint::new(
                        (drag.origin.x + pointer.x - drag.pointer.x).round(),
                        (drag.origin.y + pointer.y - drag.pointer.y).round(),
                    ));
                }
            } else if self.ivars().ticking.get() {
                self.send(Event::CheckboxDrag {
                    inside: self.on_checkbox(event),
                });
            } else if self.ivars().selecting.get()
                && let Some(field) = self.field()
            {
                let (x, _) = self.point(event);
                self.send(Event::FieldDrag {
                    x: x - field.frame.x,
                });
            }
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            self.ivars().selecting.set(false);
            self.ivars().dragging.set(None);
            if self.ivars().ticking.replace(false) {
                self.send(Event::CheckboxRelease {
                    inside: self.on_checkbox(event),
                });
            }
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            self.update_cursor(event);
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            NSCursor::arrowCursor().set();
        }

        #[unsafe(method_id(menuForEvent:))]
        fn menu_for_event(&self, event: &NSEvent) -> Option<Retained<NSMenu>> {
            if self.field_x(event).is_some() {
                Some(menu::text(self))
            } else {
                self.ivars().delegate.load().map(|delegate| delegate.menu())
            }
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            if !self.perform_shortcut(event) {
                self.interpretKeyEvents(&NSArray::from_slice(&[event]));
            }
        }

        #[unsafe(method(performKeyEquivalent:))]
        fn perform_key_equivalent(&self, event: &NSEvent) -> bool {
            self.perform_shortcut(event)
        }

        #[unsafe(method(selectAll:))]
        fn select_all(&self, _sender: Option<&AnyObject>) {
            self.act(TextAction::SelectAll);
        }

        #[unsafe(method(copy:))]
        fn copy(&self, _sender: Option<&AnyObject>) {
            self.act(TextAction::Copy);
        }

        #[unsafe(method(cut:))]
        fn cut(&self, _sender: Option<&AnyObject>) {
            self.act(TextAction::Cut);
        }

        #[unsafe(method(paste:))]
        fn paste(&self, _sender: Option<&AnyObject>) {
            self.act(TextAction::Paste);
        }
    }
);

impl BalloonView {
    pub(crate) fn new(delegate: &Delegate) -> Retained<Self> {
        let this = Self::alloc(delegate.mtm()).set_ivars(Ivars {
            delegate: Weak::new(delegate),
            surface: Surface::new(),
            field: Cell::new(None),
            checkbox: Cell::new(None),
            selecting: Cell::new(false),
            ticking: Cell::new(false),
            dragging: Cell::new(None),
            marked: Cell::new(None),
        });
        let view: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: NSRect::ZERO] };
        view.ivars().surface.host_in(&view);
        super::track(
            &view,
            NSTrackingAreaOptions::MouseEnteredAndExited | NSTrackingAreaOptions::MouseMoved,
        );
        view
    }

    pub(crate) fn present(&self, canvas: &Canvas, field: Option<Field>, checkbox: Option<Rect>) {
        if field.is_none() {
            self.ivars().marked.set(None);
        }
        self.ivars().field.set(field);
        self.ivars().checkbox.set(checkbox);
        self.ivars().surface.present(canvas, self.bounds());
    }

    fn field(&self) -> Option<Field> {
        self.ivars().field.get()
    }

    fn field_x(&self, event: &NSEvent) -> Option<i32> {
        let field = self.field()?;
        let (x, y) = self.point(event);
        field.frame.contains(x, y).then_some(x - field.frame.x)
    }

    fn clippy_window(&self) -> Option<Retained<NSWindow>> {
        self.window()?.parentWindow()
    }

    fn on_checkbox(&self, event: &NSEvent) -> bool {
        let (x, y) = self.point(event);
        self.ivars()
            .checkbox
            .get()
            .is_some_and(|checkbox| checkbox.contains(x, y))
    }

    #[expect(clippy::cast_possible_truncation)]
    fn point(&self, event: &NSEvent) -> (i32, i32) {
        let point = self.convertPoint_fromView(event.locationInWindow(), None);
        let height = self.bounds().size.height;
        (point.x.floor() as i32, (height - point.y).floor() as i32)
    }

    fn perform_shortcut(&self, event: &NSEvent) -> bool {
        let action = self.field().and_then(|_| editing::shortcut(event));
        if let Some(action) = action {
            self.act(action);
        }
        action.is_some()
    }

    fn act(&self, action: TextAction) {
        let edit = match action {
            TextAction::SelectAll => Edit::SelectAll,
            TextAction::Copy => Edit::Copy,
            TextAction::Cut => Edit::Cut,
            TextAction::Paste => match pasteboard::text() {
                Some(text) => Edit::Paste(text),
                None => return,
            },
            TextAction::Undo => Edit::Undo,
            TextAction::Redo => Edit::Redo,
        };
        self.send(Event::Edit(edit));
    }

    fn discard_marked_text(&self) {
        if self.ivars().marked.take().is_some()
            && let Some(context) = self.inputContext()
        {
            context.discardMarkedText();
        }
    }

    fn update_cursor(&self, event: &NSEvent) {
        let cursor = if self.field_x(event).is_some() {
            NSCursor::IBeamCursor()
        } else {
            NSCursor::arrowCursor()
        };
        cursor.set();
    }

    fn send(&self, event: Event) {
        super::send(&self.ivars().delegate, event);
    }
}

fn plain(text: &AnyObject) -> String {
    match text.downcast_ref::<NSAttributedString>() {
        Some(attributed) => attributed.string().to_string(),
        None => text
            .downcast_ref::<NSString>()
            .map(NSString::to_string)
            .unwrap_or_default(),
    }
}

fn utf16(range: NSRange) -> Option<Utf16Range> {
    (range.location != NSNotFound.unsigned_abs()).then(|| Utf16Range {
        start: range.location,
        end: range.end(),
    })
}

fn ns_range(range: Utf16Range) -> NSRange {
    NSRange::new(range.start, range.end - range.start)
}

fn nowhere() -> NSRange {
    NSRange::new(NSNotFound.unsigned_abs(), 0)
}
