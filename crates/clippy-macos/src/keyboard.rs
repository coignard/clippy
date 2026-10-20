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

use clippy::shortcut::{Key, Modifiers};
use objc2_app_kit::{
    NSBackspaceCharacter, NSCarriageReturnCharacter, NSDeleteFunctionKey, NSEventModifierFlags,
    NSF1FunctionKey, NSF2FunctionKey, NSF3FunctionKey, NSF4FunctionKey, NSF5FunctionKey,
    NSF6FunctionKey, NSF7FunctionKey, NSF8FunctionKey, NSF9FunctionKey, NSF10FunctionKey,
    NSF11FunctionKey, NSF12FunctionKey,
};

const EVENT_MODIFIERS: [(Modifiers, NSEventModifierFlags); 4] = [
    (Modifiers::COMMAND, NSEventModifierFlags::Command),
    (Modifiers::SHIFT, NSEventModifierFlags::Shift),
    (Modifiers::OPTION, NSEventModifierFlags::Option),
    (Modifiers::CONTROL, NSEventModifierFlags::Control),
];

pub(crate) fn key_equivalent(key: Key) -> String {
    let unicode = match key {
        Key::Space => Some(u32::from(' ')),
        Key::Enter => Some(NSCarriageReturnCharacter),
        Key::Delete => Some(NSBackspaceCharacter),
        Key::ForwardDelete => Some(NSDeleteFunctionKey),
        Key::F1 => Some(NSF1FunctionKey),
        Key::F2 => Some(NSF2FunctionKey),
        Key::F3 => Some(NSF3FunctionKey),
        Key::F4 => Some(NSF4FunctionKey),
        Key::F5 => Some(NSF5FunctionKey),
        Key::F6 => Some(NSF6FunctionKey),
        Key::F7 => Some(NSF7FunctionKey),
        Key::F8 => Some(NSF8FunctionKey),
        Key::F9 => Some(NSF9FunctionKey),
        Key::F10 => Some(NSF10FunctionKey),
        Key::F11 => Some(NSF11FunctionKey),
        Key::F12 => Some(NSF12FunctionKey),
        key => key.character().map(u32::from),
    };
    unicode
        .and_then(char::from_u32)
        .map(String::from)
        .unwrap_or_default()
}

pub(crate) fn event_modifiers(modifiers: Modifiers) -> NSEventModifierFlags {
    EVENT_MODIFIERS
        .into_iter()
        .filter(|&(modifier, _)| modifiers.contains(modifier))
        .fold(NSEventModifierFlags::empty(), |flags, (_, flag)| {
            flags | flag
        })
}

pub(crate) fn held_modifiers(flags: NSEventModifierFlags) -> Modifiers {
    EVENT_MODIFIERS
        .into_iter()
        .filter(|&(_, flag)| flags.contains(flag))
        .fold(Modifiers::empty(), |held, (modifier, _)| held | modifier)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menus_show_shortcuts_with_their_keys() {
        assert_eq!(key_equivalent(Key::K), "k");
        assert_eq!(key_equivalent(Key::Backslash), "\\");
        assert_eq!(key_equivalent(Key::Enter), "\r");
        assert_eq!(key_equivalent(Key::F5), "\u{F708}");
        assert_eq!(key_equivalent(Key::Delete), "\u{8}");
        assert_eq!(key_equivalent(Key::ForwardDelete), "\u{F728}");
        let modifiers = Modifiers::COMMAND | Modifiers::SHIFT;
        assert_eq!(held_modifiers(event_modifiers(modifiers)), modifiers);
    }

    #[test]
    fn held_modifiers_ignore_caps_lock_and_fn() {
        let flags = NSEventModifierFlags::Command
            | NSEventModifierFlags::CapsLock
            | NSEventModifierFlags::Function;
        assert_eq!(held_modifiers(flags), Modifiers::COMMAND);
    }
}
