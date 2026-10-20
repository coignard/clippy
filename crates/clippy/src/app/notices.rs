use crate::balloon::{Checkbox, Content};
use crate::plugin::Due;

#[derive(Default)]
pub(super) struct Board {
    shown: Option<Due>,
    press: Option<bool>,
    ticked: Option<Due>,
    displayed: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Change {
    pub(super) appeared: bool,
    pub(super) vanished: bool,
}

impl Board {
    pub(super) fn interrupt(&mut self) -> Option<Due> {
        self.press = None;
        self.ticked = None;
        self.shown.take()
    }

    pub(super) fn is_displayed(&self) -> bool {
        self.shown.is_some() || self.ticked.is_some()
    }

    pub(super) fn celebrated(&self) -> Option<&Due> {
        self.ticked.as_ref()
    }

    pub(super) fn press(&mut self) {
        let checkable = self
            .shown
            .as_ref()
            .is_some_and(|due| due.notice.checkbox.is_some());
        if checkable {
            self.press = Some(true);
        }
    }

    pub(super) fn drag(&mut self, inside: bool) {
        if let Some(press) = &mut self.press {
            *press = inside;
        }
    }

    pub(super) fn release(&mut self, inside: bool) -> Option<Due> {
        if self.press.take().is_none() || !inside {
            return None;
        }
        let due = self.shown.take()?;
        self.ticked = Some(due.clone());
        Some(due)
    }

    pub(super) fn update(&mut self, due: Vec<Due>, present: bool, celebrating: bool) -> Change {
        if !present || !celebrating {
            self.ticked = None;
        }
        let previous = self.shown.take();
        if present && self.ticked.is_none() {
            let kept = previous
                .as_ref()
                .and_then(|shown| due.iter().find(|candidate| candidate.is(shown)).cloned());
            self.shown = kept.or_else(|| due.into_iter().next());
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
        change
    }

    pub(super) fn content(&self) -> Option<Content<'_>> {
        let (due, checkbox) = match (&self.ticked, &self.shown) {
            (Some(ticked), _) => (
                ticked,
                Checkbox {
                    checked: true,
                    pressed: false,
                },
            ),
            (None, Some(shown)) => (
                shown,
                Checkbox {
                    checked: false,
                    pressed: self.press == Some(true),
                },
            ),
            (None, None) => return None,
        };
        Some(Content::Notice {
            text: &due.notice.text,
            checkbox: due
                .notice
                .checkbox
                .as_deref()
                .map(|label| (label, checkbox)),
        })
    }
}
