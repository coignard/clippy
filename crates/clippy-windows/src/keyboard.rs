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

#![cfg(windows)]

use clippy::shortcut::{Key, Modifiers, Shortcut};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, VIRTUAL_KEY, VK_0, VK_1, VK_2,
    VK_3, VK_4, VK_5, VK_6, VK_7, VK_8, VK_9, VK_A, VK_B, VK_BACK, VK_C, VK_D, VK_DELETE, VK_E,
    VK_F, VK_F1, VK_F2, VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_F10, VK_F11, VK_F12,
    VK_G, VK_H, VK_I, VK_J, VK_K, VK_L, VK_M, VK_N, VK_O, VK_OEM_1, VK_OEM_2, VK_OEM_3, VK_OEM_4,
    VK_OEM_5, VK_OEM_6, VK_OEM_7, VK_OEM_COMMA, VK_OEM_MINUS, VK_OEM_PERIOD, VK_OEM_PLUS, VK_P,
    VK_Q, VK_R, VK_RETURN, VK_S, VK_SPACE, VK_T, VK_U, VK_V, VK_W, VK_X, VK_Y, VK_Z,
};

const MODIFIERS: [(Modifiers, HOT_KEY_MODIFIERS, &str); 4] = [
    (Modifiers::COMMAND, MOD_WIN, "Win"),
    (Modifiers::CONTROL, MOD_CONTROL, "Ctrl"),
    (Modifiers::OPTION, MOD_ALT, "Alt"),
    (Modifiers::SHIFT, MOD_SHIFT, "Shift"),
];

pub(crate) fn hot_key_modifiers(modifiers: Modifiers) -> HOT_KEY_MODIFIERS {
    MODIFIERS
        .into_iter()
        .filter(|&(modifier, _, _)| modifiers.contains(modifier))
        .fold(0, |held, (_, flag, _)| held | flag)
}

pub(crate) fn label(shortcut: Shortcut) -> String {
    let mut label = String::new();
    for (modifier, _, name) in MODIFIERS {
        if shortcut.modifiers.contains(modifier) {
            label.push_str(name);
            label.push('+');
        }
    }
    label.push_str(&key_name(shortcut.key));
    label
}

fn key_name(key: Key) -> String {
    let virtual_key = virtual_key(key);
    match key {
        Key::Space => "Space".to_owned(),
        Key::Enter => "Enter".to_owned(),
        Key::Delete => "Backspace".to_owned(),
        Key::ForwardDelete => "Del".to_owned(),
        _ if (VK_F1..=VK_F12).contains(&virtual_key) => {
            format!("F{}", virtual_key - VK_F1 + 1)
        }
        key => key
            .character()
            .map_or_default(|typed| typed.to_ascii_uppercase().to_string()),
    }
}

pub(crate) fn virtual_key(key: Key) -> VIRTUAL_KEY {
    match key {
        Key::A => VK_A,
        Key::B => VK_B,
        Key::C => VK_C,
        Key::D => VK_D,
        Key::E => VK_E,
        Key::F => VK_F,
        Key::G => VK_G,
        Key::H => VK_H,
        Key::I => VK_I,
        Key::J => VK_J,
        Key::K => VK_K,
        Key::L => VK_L,
        Key::M => VK_M,
        Key::N => VK_N,
        Key::O => VK_O,
        Key::P => VK_P,
        Key::Q => VK_Q,
        Key::R => VK_R,
        Key::S => VK_S,
        Key::T => VK_T,
        Key::U => VK_U,
        Key::V => VK_V,
        Key::W => VK_W,
        Key::X => VK_X,
        Key::Y => VK_Y,
        Key::Z => VK_Z,
        Key::Digit0 => VK_0,
        Key::Digit1 => VK_1,
        Key::Digit2 => VK_2,
        Key::Digit3 => VK_3,
        Key::Digit4 => VK_4,
        Key::Digit5 => VK_5,
        Key::Digit6 => VK_6,
        Key::Digit7 => VK_7,
        Key::Digit8 => VK_8,
        Key::Digit9 => VK_9,
        Key::Minus => VK_OEM_MINUS,
        Key::Equal => VK_OEM_PLUS,
        Key::LeftBracket => VK_OEM_4,
        Key::RightBracket => VK_OEM_6,
        Key::Backslash => VK_OEM_5,
        Key::Semicolon => VK_OEM_1,
        Key::Quote => VK_OEM_7,
        Key::Comma => VK_OEM_COMMA,
        Key::Period => VK_OEM_PERIOD,
        Key::Slash => VK_OEM_2,
        Key::Grave => VK_OEM_3,
        Key::Space => VK_SPACE,
        Key::Enter => VK_RETURN,
        Key::Delete => VK_BACK,
        Key::ForwardDelete => VK_DELETE,
        Key::F1 => VK_F1,
        Key::F2 => VK_F2,
        Key::F3 => VK_F3,
        Key::F4 => VK_F4,
        Key::F5 => VK_F5,
        Key::F6 => VK_F6,
        Key::F7 => VK_F7,
        Key::F8 => VK_F8,
        Key::F9 => VK_F9,
        Key::F10 => VK_F10,
        Key::F11 => VK_F11,
        Key::F12 => VK_F12,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPED: &str = r"abcdefghijklmnopqrstuvwxyz0123456789-=[]\;',./`";
    const NAMED: [&str; 16] = [
        "Space",
        "Enter",
        "Delete",
        "ForwardDelete",
        "F1",
        "F2",
        "F3",
        "F4",
        "F5",
        "F6",
        "F7",
        "F8",
        "F9",
        "F10",
        "F11",
        "F12",
    ];

    fn shortcut(text: &str) -> Shortcut {
        text.parse().unwrap()
    }

    fn every_key() -> Vec<Key> {
        TYPED
            .chars()
            .map(String::from)
            .chain(NAMED.map(String::from))
            .map(|name| shortcut(&format!("Cmd+{name}")).key)
            .collect()
    }

    #[test]
    fn menus_spell_shortcuts_the_windows_way() {
        assert_eq!(label(shortcut("Cmd+Shift+|")), "Win+Shift+\\");
        assert_eq!(
            label(shortcut("Cmd+Option+Shift+Delete")),
            "Win+Alt+Shift+Backspace"
        );
        assert_eq!(label(shortcut("Ctrl+F5")), "Ctrl+F5");
        assert_eq!(label(shortcut("Ctrl+ForwardDelete")), "Ctrl+Del");
        assert_eq!(label(shortcut("Shift+K")), "Shift+K");
    }

    #[test]
    fn the_command_key_is_the_windows_key() {
        let modifiers = shortcut("Cmd+Ctrl+Option+Shift+K").modifiers;
        assert_eq!(
            hot_key_modifiers(modifiers),
            MOD_WIN | MOD_CONTROL | MOD_ALT | MOD_SHIFT
        );
        assert_eq!(hot_key_modifiers(Modifiers::empty()), 0);
    }

    #[test]
    fn every_key_has_a_virtual_key_of_its_own() {
        let keys = every_key();
        assert_eq!(keys.len(), TYPED.len() + NAMED.len());
        let mut virtual_keys: Vec<_> = keys.into_iter().map(virtual_key).collect();
        virtual_keys.sort_unstable();
        virtual_keys.dedup();
        assert_eq!(virtual_keys.len(), TYPED.len() + NAMED.len());
    }

    #[test]
    fn every_key_has_a_name() {
        for key in every_key() {
            assert!(!key_name(key).is_empty(), "{key:?}");
        }
    }
}
