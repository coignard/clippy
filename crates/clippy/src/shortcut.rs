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

use std::fmt;
use std::str::FromStr;

use bitflags::bitflags;
use serde::{Deserialize, Serialize};

bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct Modifiers: u8 {
        const COMMAND = 1;
        const CONTROL = 1 << 1;
        const OPTION = 1 << 2;
        const SHIFT = 1 << 3;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    Minus,
    Equal,
    LeftBracket,
    RightBracket,
    Backslash,
    Semicolon,
    Quote,
    Comma,
    Period,
    Slash,
    Grave,
    Space,
    Enter,
    Delete,
    ForwardDelete,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Shortcut {
    pub modifiers: Modifiers,
    pub key: Key,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ParseShortcutError {
    #[error("unknown modifier `{0}`")]
    UnknownModifier(String),
    #[error("unknown key `{0}`")]
    UnknownKey(String),
}

const NAMED_KEYS: [(Key, &str); 16] = [
    (Key::Space, "Space"),
    (Key::Enter, "Enter"),
    (Key::Delete, "Delete"),
    (Key::ForwardDelete, "ForwardDelete"),
    (Key::F1, "F1"),
    (Key::F2, "F2"),
    (Key::F3, "F3"),
    (Key::F4, "F4"),
    (Key::F5, "F5"),
    (Key::F6, "F6"),
    (Key::F7, "F7"),
    (Key::F8, "F8"),
    (Key::F9, "F9"),
    (Key::F10, "F10"),
    (Key::F11, "F11"),
    (Key::F12, "F12"),
];

const KEY_ALIASES: [(Key, &str); 6] = [
    (Key::Enter, "Return"),
    (Key::Enter, "↩"),
    (Key::Delete, "Backspace"),
    (Key::Delete, "⌫"),
    (Key::ForwardDelete, "Forward Delete"),
    (Key::ForwardDelete, "⌦"),
];

const TYPED_KEYS: [(Key, char); 47] = [
    (Key::A, 'a'),
    (Key::B, 'b'),
    (Key::C, 'c'),
    (Key::D, 'd'),
    (Key::E, 'e'),
    (Key::F, 'f'),
    (Key::G, 'g'),
    (Key::H, 'h'),
    (Key::I, 'i'),
    (Key::J, 'j'),
    (Key::K, 'k'),
    (Key::L, 'l'),
    (Key::M, 'm'),
    (Key::N, 'n'),
    (Key::O, 'o'),
    (Key::P, 'p'),
    (Key::Q, 'q'),
    (Key::R, 'r'),
    (Key::S, 's'),
    (Key::T, 't'),
    (Key::U, 'u'),
    (Key::V, 'v'),
    (Key::W, 'w'),
    (Key::X, 'x'),
    (Key::Y, 'y'),
    (Key::Z, 'z'),
    (Key::Digit0, '0'),
    (Key::Digit1, '1'),
    (Key::Digit2, '2'),
    (Key::Digit3, '3'),
    (Key::Digit4, '4'),
    (Key::Digit5, '5'),
    (Key::Digit6, '6'),
    (Key::Digit7, '7'),
    (Key::Digit8, '8'),
    (Key::Digit9, '9'),
    (Key::Minus, '-'),
    (Key::Equal, '='),
    (Key::LeftBracket, '['),
    (Key::RightBracket, ']'),
    (Key::Backslash, '\\'),
    (Key::Semicolon, ';'),
    (Key::Quote, '\''),
    (Key::Comma, ','),
    (Key::Period, '.'),
    (Key::Slash, '/'),
    (Key::Grave, '`'),
];

const SHIFTED_KEYS: [(Key, char); 21] = [
    (Key::Digit0, ')'),
    (Key::Digit1, '!'),
    (Key::Digit2, '@'),
    (Key::Digit3, '#'),
    (Key::Digit4, '$'),
    (Key::Digit5, '%'),
    (Key::Digit6, '^'),
    (Key::Digit7, '&'),
    (Key::Digit8, '*'),
    (Key::Digit9, '('),
    (Key::Minus, '_'),
    (Key::Equal, '+'),
    (Key::LeftBracket, '{'),
    (Key::RightBracket, '}'),
    (Key::Backslash, '|'),
    (Key::Semicolon, ':'),
    (Key::Quote, '"'),
    (Key::Comma, '<'),
    (Key::Period, '>'),
    (Key::Slash, '?'),
    (Key::Grave, '~'),
];

impl Key {
    pub fn character(self) -> Option<char> {
        TYPED_KEYS
            .iter()
            .find(|&&(key, _)| key == self)
            .map(|&(_, typed)| typed)
    }

    fn parse(name: &str) -> Option<(Self, Modifiers)> {
        if let Some(&(key, _)) = NAMED_KEYS
            .iter()
            .chain(&KEY_ALIASES)
            .find(|(_, named)| named.eq_ignore_ascii_case(name))
        {
            return Some((key, Modifiers::empty()));
        }
        let mut characters = name.chars().flat_map(char::to_lowercase);
        let (Some(character), None) = (characters.next(), characters.next()) else {
            return None;
        };
        let typed = TYPED_KEYS
            .iter()
            .find(|&&(_, typed)| typed == character)
            .map(|&(key, _)| (key, Modifiers::empty()));
        typed.or_else(|| {
            SHIFTED_KEYS
                .iter()
                .find(|&&(_, symbol)| symbol == character)
                .map(|&(key, _)| (key, Modifiers::SHIFT))
        })
    }

    fn write(self, formatter: &mut fmt::Formatter<'_>, shift: bool) -> fmt::Result {
        if let Some(&(_, name)) = NAMED_KEYS.iter().find(|&&(key, _)| key == self) {
            return formatter.write_str(name);
        }
        let symbol = SHIFTED_KEYS
            .iter()
            .find(|&&(key, _)| shift && key == self)
            .map(|&(_, symbol)| symbol);
        let typed = self.character().map(|typed| typed.to_ascii_uppercase());
        let character = symbol
            .or(typed)
            .expect("every key should be named or typed");
        write!(formatter, "{character}")
    }
}

impl FromStr for Shortcut {
    type Err = ParseShortcutError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut modifiers = Modifiers::empty();
        let mut rest = text.trim();
        while let Some((name, tail)) = rest.split_once('+')
            && !tail.is_empty()
        {
            let name = name.trim();
            modifiers |= match name.to_ascii_lowercase().as_str() {
                "cmd" | "command" => Modifiers::COMMAND,
                "ctrl" | "control" => Modifiers::CONTROL,
                "opt" | "option" | "alt" => Modifiers::OPTION,
                "shift" => Modifiers::SHIFT,
                _ => return Err(ParseShortcutError::UnknownModifier(name.to_owned())),
            };
            rest = tail;
        }
        let name = rest.trim();
        let (key, implied) =
            Key::parse(name).ok_or_else(|| ParseShortcutError::UnknownKey(name.to_owned()))?;
        Ok(Self {
            modifiers: modifiers | implied,
            key,
        })
    }
}

impl fmt::Display for Shortcut {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (modifier, name) in [
            (Modifiers::COMMAND, "Cmd"),
            (Modifiers::CONTROL, "Ctrl"),
            (Modifiers::OPTION, "Option"),
            (Modifiers::SHIFT, "Shift"),
        ] {
            if self.modifiers.contains(modifier) {
                write!(formatter, "{name}+")?;
            }
        }
        self.key
            .write(formatter, self.modifiers.contains(Modifiers::SHIFT))
    }
}

impl TryFrom<String> for Shortcut {
    type Error = ParseShortcutError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<Shortcut> for String {
    fn from(shortcut: Shortcut) -> Self {
        shortcut.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Shortcut {
        text.parse().unwrap()
    }

    #[test]
    fn shifted_symbols_imply_shift_on_their_key() {
        let shortcut = parse("Cmd+Shift+|");
        assert_eq!(shortcut.key, Key::Backslash);
        assert_eq!(shortcut.modifiers, Modifiers::COMMAND | Modifiers::SHIFT);
        assert_eq!(parse("Cmd+|"), shortcut);
    }

    #[test]
    fn displays_the_conventional_spelling() {
        for text in [
            "Cmd+Shift+|",
            "Ctrl+Option+Space",
            "Cmd+F5",
            "Shift+K",
            "Cmd+\\",
            "Cmd+Shift++",
            "Cmd+Shift+Enter",
            "Cmd+Shift+Delete",
            "Ctrl+ForwardDelete",
        ] {
            assert_eq!(parse(text).to_string(), text);
        }
    }

    #[test]
    fn every_key_round_trips() {
        let keys = NAMED_KEYS
            .iter()
            .map(|&(key, _)| key)
            .chain(TYPED_KEYS.iter().map(|&(key, _)| key));
        for key in keys {
            for modifiers in [Modifiers::COMMAND, Modifiers::COMMAND | Modifiers::SHIFT] {
                let shortcut = Shortcut { modifiers, key };
                assert_eq!(parse(&shortcut.to_string()), shortcut);
            }
        }
    }

    #[test]
    fn accepts_aliases_and_any_case() {
        assert_eq!(parse("command+alt+k"), parse("Cmd+Option+K"));
        assert_eq!(parse(" control + shift + f12 "), parse("Ctrl+Shift+F12"));
        assert_eq!(parse("Cmd+space"), parse("Cmd+Space"));
        assert_eq!(parse("Cmd+Shift+Return"), parse("Cmd+Shift+Enter"));
        assert_eq!(parse("Cmd+Shift+Backspace"), parse("Cmd+Shift+Delete"));
        assert_eq!(parse("Cmd+Shift+⌫"), parse("Cmd+Shift+Delete"));
        assert_eq!(parse("Cmd+⌦"), parse("Cmd+ForwardDelete"));
        assert_eq!(parse("Cmd+forward delete"), parse("Cmd+ForwardDelete"));
    }

    #[test]
    fn rejects_unknown_parts() {
        assert_eq!(
            "Hyper+K".parse::<Shortcut>(),
            Err(ParseShortcutError::UnknownModifier("Hyper".into()))
        );
        assert_eq!(
            "Cmd+Tab".parse::<Shortcut>(),
            Err(ParseShortcutError::UnknownKey("Tab".into()))
        );
        assert_eq!(
            "Cmd+F13".parse::<Shortcut>(),
            Err(ParseShortcutError::UnknownKey("F13".into()))
        );
        assert_eq!(
            "Cmd+Ж".parse::<Shortcut>(),
            Err(ParseShortcutError::UnknownKey("Ж".into()))
        );
    }
}
