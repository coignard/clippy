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

use clippy::shortcut::Modifiers;
use clippy::{Edit, Event, Motion};
use objc2::runtime::Sel;
use objc2_app_kit::NSEvent;

use crate::keyboard;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TextAction {
    SelectAll,
    Copy,
    Cut,
    Paste,
    Undo,
    Redo,
}

const COMMAND_SHIFT: Modifiers = Modifiers::COMMAND.union(Modifiers::SHIFT);

const ANSI_LETTERS: [(u16, char); 5] = [
    (0x00, 'a'),
    (0x08, 'c'),
    (0x09, 'v'),
    (0x07, 'x'),
    (0x06, 'z'),
];

pub(super) fn command(selector: Sel) -> Option<Event> {
    let name = selector.name().to_bytes();
    match name {
        b"insertNewline:" | b"insertLineBreak:" => Some(Event::Submit),
        b"cancelOperation:" => Some(Event::Cancel),
        _ => edit(name).map(Event::Edit),
    }
}

fn edit(name: &[u8]) -> Option<Edit> {
    Some(match name {
        b"selectAll:" => Edit::SelectAll,
        b"moveLeft:" | b"moveBackward:" => Edit::Move(Motion::Backward),
        b"moveRight:" | b"moveForward:" => Edit::Move(Motion::Forward),
        b"moveWordLeft:" | b"moveWordBackward:" => Edit::Move(Motion::WordBackward),
        b"moveWordRight:" | b"moveWordForward:" => Edit::Move(Motion::WordForward),
        b"moveToBeginningOfLine:"
        | b"moveToLeftEndOfLine:"
        | b"moveToBeginningOfParagraph:"
        | b"moveToBeginningOfDocument:"
        | b"moveUp:"
        | b"pageUp:"
        | b"scrollPageUp:"
        | b"scrollToBeginningOfDocument:" => Edit::Move(Motion::Start),
        b"moveToEndOfLine:"
        | b"moveToRightEndOfLine:"
        | b"moveToEndOfParagraph:"
        | b"moveToEndOfDocument:"
        | b"moveDown:"
        | b"pageDown:"
        | b"scrollPageDown:"
        | b"scrollToEndOfDocument:" => Edit::Move(Motion::End),
        b"moveLeftAndModifySelection:" | b"moveBackwardAndModifySelection:" => {
            Edit::Select(Motion::Backward)
        }
        b"moveRightAndModifySelection:" | b"moveForwardAndModifySelection:" => {
            Edit::Select(Motion::Forward)
        }
        b"moveWordLeftAndModifySelection:" | b"moveWordBackwardAndModifySelection:" => {
            Edit::Select(Motion::WordBackward)
        }
        b"moveWordRightAndModifySelection:" | b"moveWordForwardAndModifySelection:" => {
            Edit::Select(Motion::WordForward)
        }
        b"moveToBeginningOfLineAndModifySelection:"
        | b"moveToLeftEndOfLineAndModifySelection:"
        | b"moveToBeginningOfParagraphAndModifySelection:"
        | b"moveToBeginningOfDocumentAndModifySelection:"
        | b"moveParagraphBackwardAndModifySelection:"
        | b"moveUpAndModifySelection:"
        | b"pageUpAndModifySelection:" => Edit::Select(Motion::Start),
        b"moveToEndOfLineAndModifySelection:"
        | b"moveToRightEndOfLineAndModifySelection:"
        | b"moveToEndOfParagraphAndModifySelection:"
        | b"moveToEndOfDocumentAndModifySelection:"
        | b"moveParagraphForwardAndModifySelection:"
        | b"moveDownAndModifySelection:"
        | b"pageDownAndModifySelection:" => Edit::Select(Motion::End),
        b"deleteBackward:" | b"deleteBackwardByDecomposingPreviousCharacter:" => {
            Edit::Delete(Motion::Backward)
        }
        b"deleteForward:" => Edit::Delete(Motion::Forward),
        b"deleteWordBackward:" => Edit::Delete(Motion::WordBackward),
        b"deleteWordForward:" => Edit::Delete(Motion::WordForward),
        b"deleteToBeginningOfLine:" | b"deleteToBeginningOfParagraph:" => {
            Edit::Delete(Motion::Start)
        }
        b"deleteToEndOfLine:" | b"deleteToEndOfParagraph:" => Edit::Delete(Motion::End),
        _ => return None,
    })
}

pub(super) fn shortcut(event: &NSEvent) -> Option<TextAction> {
    match (
        keyboard::held_modifiers(event.modifierFlags()),
        letter(event)?,
    ) {
        (Modifiers::COMMAND, 'a') => Some(TextAction::SelectAll),
        (Modifiers::COMMAND, 'c') => Some(TextAction::Copy),
        (Modifiers::COMMAND, 'x') => Some(TextAction::Cut),
        (Modifiers::COMMAND, 'v') => Some(TextAction::Paste),
        (Modifiers::COMMAND, 'z') => Some(TextAction::Undo),
        (COMMAND_SHIFT, 'z') => Some(TextAction::Redo),
        _ => None,
    }
}

fn letter(event: &NSEvent) -> Option<char> {
    let typed = event
        .charactersIgnoringModifiers()
        .and_then(|characters| characters.to_string().chars().next())
        .map(|character| character.to_ascii_lowercase());
    match typed {
        Some(character) if character.is_ascii_alphabetic() => Some(character),
        _ => ANSI_LETTERS
            .iter()
            .find(|&&(code, _)| code == event.keyCode())
            .map(|&(_, letter)| letter),
    }
}
