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
use clippy::{BalloonScene, Scene};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{MainThreadMarker, MainThreadOnly, available};
use objc2_app_kit::{NSPanel, NSScreen, NSWindowDelegate, NSWindowOrderingMode};
use objc2_core_graphics::CGRectContainsPoint;
use objc2_foundation::{NSPoint, NSRect, NSSize};

use crate::delegate::Delegate;
use crate::full_screen::Display;
use crate::panel;
use crate::view::{BalloonView, ClippyView};

pub(crate) struct Stage {
    clippy: Retained<NSPanel>,
    clippy_view: Retained<ClippyView>,
    balloon: Retained<NSPanel>,
    balloon_view: Retained<BalloonView>,
}

impl Stage {
    pub(crate) fn new(delegate: &Delegate) -> Self {
        let mtm = delegate.mtm();
        let stage = Self {
            clippy: panel::clippy(mtm),
            clippy_view: ClippyView::new(delegate),
            balloon: panel::balloon(mtm),
            balloon_view: BalloonView::new(delegate),
        };
        stage.clippy.setContentView(Some(&stage.clippy_view));
        stage.balloon.setContentView(Some(&stage.balloon_view));
        stage.balloon.makeFirstResponder(Some(&stage.balloon_view));
        stage
    }

    pub(crate) fn watch(&self, delegate: &ProtocolObject<dyn NSWindowDelegate>) {
        self.clippy.setDelegate(Some(delegate));
        self.balloon.setDelegate(Some(delegate));
    }

    pub(crate) fn is_clippy(&self, panel: &NSPanel) -> bool {
        *self.clippy == *panel
    }

    pub(crate) fn home_screen(&self) -> Rect {
        let mtm = self.clippy.mtm();
        let screen = NSScreen::screens(mtm)
            .firstObject()
            .map_or(NSRect::ZERO, |screen| screen.visibleFrame());
        flip(screen, primary_height(mtm))
    }

    pub(crate) fn is_lost(&self) -> bool {
        self.screen().is_none()
    }

    pub(crate) fn place(&self, clippy: Rect) {
        let primary = primary_height(self.clippy.mtm());
        self.clippy.setFrame_display(unflip(clippy, primary), false);
    }

    pub(crate) fn geometry(&self) -> (Rect, Rect) {
        let primary = primary_height(self.clippy.mtm());
        let screen = self.screen().map_or_else(
            || self.home_screen(),
            |screen| flip(screen.visibleFrame(), primary),
        );
        (flip(self.clippy.frame(), primary), screen)
    }

    pub(crate) fn display(&self) -> Option<Display> {
        let screen = self.screen()?;
        let notch = if available!(macos = 12.0) {
            screen.safeAreaInsets().top
        } else {
            0.0
        };
        Some(Display {
            frame: flip(screen.frame(), primary_height(self.clippy.mtm())),
            notch: points(notch),
        })
    }

    fn screen(&self) -> Option<Retained<NSScreen>> {
        let middle = self.clippy.frame().mid();
        NSScreen::screens(self.clippy.mtm())
            .iter()
            .find(|screen| CGRectContainsPoint(screen.frame(), middle))
    }

    pub(crate) fn show(&self, scene: &Scene) {
        match &scene.clippy {
            Some(canvas) => {
                self.clippy_view.present(canvas);
                if !self.clippy.isVisible() {
                    self.clippy.orderFrontRegardless();
                }
            }
            None => self.clippy.orderOut(None),
        }
        match &scene.balloon {
            Some(balloon) => self.show_balloon(balloon),
            None => self.hide_balloon(),
        }
    }

    fn show_balloon(&self, scene: &BalloonScene) {
        if scene.field.is_none() && self.balloon.isKeyWindow() {
            self.hide_balloon();
        }
        let primary = primary_height(self.clippy.mtm());
        self.balloon
            .setFrame_display(unflip(scene.frame, primary), false);
        self.balloon_view
            .present(&scene.canvas, scene.field, scene.checkbox);
        if self.balloon.parentWindow().is_none() {
            unsafe {
                self.clippy
                    .addChildWindow_ordered(&self.balloon, NSWindowOrderingMode::Above);
            }
        }
        if scene.field.is_some_and(|field| field.wants_focus) && !self.balloon.isKeyWindow() {
            self.balloon.makeKeyAndOrderFront(None);
        }
    }

    fn hide_balloon(&self) {
        if self.balloon.parentWindow().is_some() {
            self.clippy.removeChildWindow(&self.balloon);
        }
        self.balloon.orderOut(None);
    }
}

fn primary_height(mtm: MainThreadMarker) -> f64 {
    NSScreen::screens(mtm)
        .firstObject()
        .map_or(0.0, |screen| screen.frame().size.height)
}

#[expect(clippy::cast_possible_truncation)]
fn points(length: f64) -> i32 {
    length.round() as i32
}

fn flip(frame: NSRect, primary_height: f64) -> Rect {
    Rect::new(
        points(frame.origin.x),
        points(primary_height - frame.origin.y - frame.size.height),
        points(frame.size.width),
        points(frame.size.height),
    )
}

fn unflip(rect: Rect, primary_height: f64) -> NSRect {
    NSRect::new(
        NSPoint::new(f64::from(rect.x), primary_height - f64::from(rect.bottom())),
        NSSize::new(f64::from(rect.width), f64::from(rect.height)),
    )
}
