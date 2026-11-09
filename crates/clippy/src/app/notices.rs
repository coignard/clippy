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

use std::mem;

use crate::balloon::{Checkbox, Content};
use crate::plugin::{Due, Urgency};
use crate::shortcut::Shortcut;

#[derive(Default)]
pub(super) struct Board {
    due: Vec<Due>,
    shown: Option<Due>,
    press: Option<(usize, bool)>,
    ticked: Option<Due>,
    displayed: bool,
    tasks: Vec<(String, Checkbox)>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Change {
    pub(super) appeared: bool,
    pub(super) vanished: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Released {
    Checked(Due),
    Toggled(Due, usize),
}

impl Board {
    pub(super) fn interrupt(&mut self) -> Option<Due> {
        self.press = None;
        self.ticked = None;
        let shown = self.shown.take();
        self.refresh();
        shown
    }

    pub(super) fn is_displayed(&self) -> bool {
        self.shown.is_some() || self.ticked.is_some()
    }

    pub(super) fn shortcut(&self) -> Option<Shortcut> {
        self.shown.as_ref()?.notice.shortcut
    }

    pub(super) fn check_shortcut(&self) -> Option<Shortcut> {
        self.shown.as_ref()?.notice.check_shortcut
    }

    pub(super) fn urgency(&self) -> Urgency {
        self.shown
            .as_ref()
            .map_or(Urgency::Normal, |due| due.notice.urgency)
    }

    pub(super) fn celebrated(&self) -> Option<&Due> {
        self.ticked.as_ref()
    }

    pub(super) fn press(&mut self, item: usize) {
        let rows = self.shown.as_ref().map_or(0, |due| {
            due.notice.tasks.len() + usize::from(due.notice.checkbox.is_some())
        });
        if item < rows {
            self.press = Some((item, true));
        }
        self.refresh();
    }

    pub(super) fn drag(&mut self, inside: bool) {
        if let Some((_, pressed)) = &mut self.press {
            *pressed = inside;
        }
        self.refresh();
    }

    pub(super) fn release(&mut self, inside: bool) -> Option<Released> {
        let released = match (self.press.take(), self.shown.clone()) {
            (Some((item, _)), Some(due)) if inside => {
                let tasks = &due.notice.tasks;
                let completes = tasks.get(item).is_some_and(|(_, done)| !done)
                    && (0..tasks.len()).all(|other| other == item || tasks[other].1);
                if item == tasks.len() {
                    self.tick().map(Released::Checked)
                } else if completes && due.notice.checkbox.is_some() {
                    self.tick_with_every_task().map(Released::Checked)
                } else {
                    Some(Released::Toggled(due, item))
                }
            }
            _ => None,
        };
        self.refresh();
        released
    }

    pub(super) fn tick(&mut self) -> Option<Due> {
        self.finish(false)
    }

    fn tick_with_every_task(&mut self) -> Option<Due> {
        self.finish(true)
    }

    fn finish(&mut self, every_task: bool) -> Option<Due> {
        let due = self.shown.take_if(|due| due.notice.checkbox.is_some())?;
        self.press = None;
        let mut ticked = due.clone();
        if every_task {
            for (_, done) in &mut ticked.notice.tasks {
                *done = true;
            }
        }
        self.ticked = Some(ticked);
        self.refresh();
        Some(due)
    }

    pub(super) fn update(&mut self, due: &[Due], present: bool, celebrating: bool) -> Change {
        if !present || !celebrating {
            self.ticked = None;
        }
        let previous = self.shown.take();
        let known = mem::replace(&mut self.due, due.to_vec());
        if present && self.ticked.is_none() {
            let fresh = |urgency: Urgency| {
                due.iter().find(|candidate| {
                    candidate.notice.urgency == urgency
                        && !known.iter().any(|seen| seen.is(candidate))
                })
            };
            let kept = previous
                .as_ref()
                .and_then(|shown| due.iter().find(|candidate| candidate.is(shown)));
            let pressing = due
                .iter()
                .find(|candidate| candidate.notice.urgency == Urgency::Pressing);
            self.shown = fresh(Urgency::Asked)
                .or_else(|| fresh(Urgency::Pressing))
                .or_else(|| {
                    kept.filter(|kept| kept.notice.urgency != Urgency::Normal || pressing.is_none())
                })
                .or(pressing)
                .or(due.first())
                .cloned();
        }
        let same = match (&previous, &self.shown) {
            (Some(previous), Some(shown)) => previous.is(shown),
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.press = None;
        }
        let displayed = self.is_displayed();
        let change = Change {
            appeared: self.shown.is_some() && !same,
            vanished: self.displayed && !displayed,
        };
        self.displayed = displayed;
        self.refresh();
        change
    }

    pub(super) fn content(&self) -> Option<Content<'_>> {
        let (due, checked) = self.on_display()?;
        let pressed = self.pressed() == Some(due.notice.tasks.len());
        Some(Content::Notice {
            text: &due.notice.text,
            tasks: &self.tasks,
            checkbox: due
                .notice
                .checkbox
                .as_deref()
                .map(|label| (label, Checkbox { checked, pressed })),
        })
    }

    fn on_display(&self) -> Option<(&Due, bool)> {
        match (&self.ticked, &self.shown) {
            (Some(ticked), _) => Some((ticked, true)),
            (None, Some(shown)) => Some((shown, false)),
            (None, None) => None,
        }
    }

    fn pressed(&self) -> Option<usize> {
        self.press
            .filter(|_| self.ticked.is_none())
            .and_then(|(item, inside)| inside.then_some(item))
    }

    fn refresh(&mut self) {
        let pressed = self.pressed();
        self.tasks = self
            .on_display()
            .map(|(due, _)| {
                (0..)
                    .zip(&due.notice.tasks)
                    .map(|(item, (label, done))| {
                        let state = Checkbox {
                            checked: *done,
                            pressed: pressed == Some(item),
                        };
                        (label.clone(), state)
                    })
                    .collect()
            })
            .unwrap_or_default();
    }
}
