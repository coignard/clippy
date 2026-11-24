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

use clippy::{Edit, Event, Motion};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, VIRTUAL_KEY, VK_A, VK_BACK, VK_C, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END,
    VK_ESCAPE, VK_HOME, VK_INSERT, VK_LEFT, VK_MENU, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT,
    VK_SHIFT, VK_UP, VK_V, VK_X, VK_Y, VK_Z,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TextAction {
    SelectAll,
    Copy,
    Cut,
    Paste,
    Undo,
    Redo,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Keystroke {
    Event(Event),
    Action(TextAction),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Held {
    pub(super) control: bool,
    pub(super) shift: bool,
    pub(super) alt: bool,
}

pub(super) fn held() -> Held {
    let down = |key: VIRTUAL_KEY| unsafe { GetKeyState(i32::from(key)) } < 0;
    Held {
        control: down(VK_CONTROL),
        shift: down(VK_SHIFT),
        alt: down(VK_MENU),
    }
}

pub(super) fn keystroke(key: VIRTUAL_KEY, held: Held) -> Option<Keystroke> {
    if held.alt {
        return None;
    }
    let Held { control, shift, .. } = held;
    let event = |event| Some(Keystroke::Event(event));
    let action = |action| Some(Keystroke::Action(action));
    match key {
        VK_RETURN if shift => event(Event::ShiftSubmit),
        VK_RETURN => event(Event::Submit),
        VK_ESCAPE => event(Event::Cancel),
        VK_INSERT if control => action(TextAction::Copy),
        VK_INSERT if shift => action(TextAction::Paste),
        VK_DELETE if shift && !control => action(TextAction::Cut),
        VK_BACK | VK_DELETE => event(Event::Edit(Edit::Delete(deletion(key, control)))),
        _ if control => shortcut(key, shift)
            .map(Keystroke::Action)
            .or_else(|| moving(key, control, shift)),
        _ => moving(key, control, shift),
    }
}

fn deletion(key: VIRTUAL_KEY, control: bool) -> Motion {
    match (key, control) {
        (VK_BACK, true) => Motion::WordBackward,
        (VK_BACK, false) => Motion::Backward,
        (_, true) => Motion::WordForward,
        (_, false) => Motion::Forward,
    }
}

fn shortcut(key: VIRTUAL_KEY, shift: bool) -> Option<TextAction> {
    Some(match key {
        VK_A => TextAction::SelectAll,
        VK_C => TextAction::Copy,
        VK_X => TextAction::Cut,
        VK_V => TextAction::Paste,
        VK_Z if shift => TextAction::Redo,
        VK_Z => TextAction::Undo,
        VK_Y => TextAction::Redo,
        _ => return None,
    })
}

fn moving(key: VIRTUAL_KEY, control: bool, shift: bool) -> Option<Keystroke> {
    let motion = match key {
        VK_LEFT if control => Motion::WordBackward,
        VK_LEFT => Motion::Backward,
        VK_RIGHT if control => Motion::WordForward,
        VK_RIGHT => Motion::Forward,
        VK_HOME | VK_UP | VK_PRIOR => Motion::Start,
        VK_END | VK_DOWN | VK_NEXT => Motion::End,
        _ => return None,
    };
    let edit = if shift {
        Edit::Select(motion)
    } else {
        Edit::Move(motion)
    };
    Some(Keystroke::Event(Event::Edit(edit)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: Held = Held {
        control: false,
        shift: false,
        alt: false,
    };
    const CONTROL: Held = Held {
        control: true,
        ..PLAIN
    };
    const SHIFT: Held = Held {
        shift: true,
        ..PLAIN
    };
    const CONTROL_SHIFT: Held = Held {
        control: true,
        shift: true,
        alt: false,
    };

    fn edit(edit: Edit) -> Keystroke {
        Keystroke::Event(Event::Edit(edit))
    }

    #[test]
    fn enter_answers_and_escape_cancels() {
        let event = |event| Some(Keystroke::Event(event));
        assert_eq!(keystroke(VK_RETURN, PLAIN), event(Event::Submit));
        assert_eq!(keystroke(VK_RETURN, SHIFT), event(Event::ShiftSubmit));
        assert_eq!(keystroke(VK_ESCAPE, PLAIN), event(Event::Cancel));
    }

    #[test]
    fn arrows_move_and_shift_selects() {
        assert_eq!(
            keystroke(VK_LEFT, PLAIN),
            Some(edit(Edit::Move(Motion::Backward)))
        );
        assert_eq!(
            keystroke(VK_RIGHT, CONTROL),
            Some(edit(Edit::Move(Motion::WordForward)))
        );
        assert_eq!(
            keystroke(VK_HOME, SHIFT),
            Some(edit(Edit::Select(Motion::Start)))
        );
        assert_eq!(
            keystroke(VK_LEFT, CONTROL_SHIFT),
            Some(edit(Edit::Select(Motion::WordBackward)))
        );
        assert_eq!(
            keystroke(VK_DOWN, PLAIN),
            Some(edit(Edit::Move(Motion::End)))
        );
    }

    #[test]
    fn backspace_and_delete_take_letters_or_words() {
        assert_eq!(
            keystroke(VK_BACK, PLAIN),
            Some(edit(Edit::Delete(Motion::Backward)))
        );
        assert_eq!(
            keystroke(VK_BACK, CONTROL),
            Some(edit(Edit::Delete(Motion::WordBackward)))
        );
        assert_eq!(
            keystroke(VK_DELETE, PLAIN),
            Some(edit(Edit::Delete(Motion::Forward)))
        );
        assert_eq!(
            keystroke(VK_DELETE, CONTROL),
            Some(edit(Edit::Delete(Motion::WordForward)))
        );
    }

    #[test]
    fn control_letters_edit_like_everywhere_else_on_windows() {
        let action = |action| Some(Keystroke::Action(action));
        assert_eq!(keystroke(VK_A, CONTROL), action(TextAction::SelectAll));
        assert_eq!(keystroke(VK_C, CONTROL), action(TextAction::Copy));
        assert_eq!(keystroke(VK_X, CONTROL), action(TextAction::Cut));
        assert_eq!(keystroke(VK_V, CONTROL), action(TextAction::Paste));
        assert_eq!(keystroke(VK_Z, CONTROL), action(TextAction::Undo));
        assert_eq!(keystroke(VK_Z, CONTROL_SHIFT), action(TextAction::Redo));
        assert_eq!(keystroke(VK_Y, CONTROL), action(TextAction::Redo));
        assert_eq!(keystroke(VK_INSERT, SHIFT), action(TextAction::Paste));
        assert_eq!(keystroke(VK_DELETE, SHIFT), action(TextAction::Cut));
        assert_eq!(keystroke(VK_A, PLAIN), None);
    }

    #[test]
    fn alt_and_alt_gr_are_left_for_typing() {
        let alt_gr = Held {
            alt: true,
            ..CONTROL
        };
        assert_eq!(keystroke(VK_V, alt_gr), None);
        assert_eq!(keystroke(VK_LEFT, Held { alt: true, ..PLAIN }), None);
    }
}
