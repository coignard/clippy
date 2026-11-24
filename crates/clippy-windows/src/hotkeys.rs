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

use std::io;

use clippy::shortcut::Shortcut;
use windows_sys::Win32::Foundation::{HWND, WPARAM};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey,
};

use crate::keyboard;

pub(crate) struct HotKeys {
    window: HWND,
    wanted: Vec<Shortcut>,
    bound: Vec<(i32, Shortcut)>,
}

impl HotKeys {
    pub(crate) fn new(window: HWND) -> Self {
        Self {
            window,
            wanted: Vec::new(),
            bound: Vec::new(),
        }
    }

    pub(crate) fn bind(&mut self, shortcuts: Vec<Shortcut>) {
        if shortcuts == self.wanted {
            return;
        }
        for (id, shortcut) in self.bound.drain(..) {
            if unsafe { UnregisterHotKey(self.window, id) } == 0 {
                let error = io::Error::last_os_error();
                log::warn!("Could not release {}: {error}", keyboard::label(shortcut));
            }
        }
        for &shortcut in &shortcuts {
            let id = id(shortcut);
            let modifiers = keyboard::hot_key_modifiers(shortcut.modifiers) | MOD_NOREPEAT;
            let key = u32::from(keyboard::virtual_key(shortcut.key));
            if unsafe { RegisterHotKey(self.window, id, modifiers, key) } == 0 {
                let error = io::Error::last_os_error();
                log::error!("Could not register {}: {error}", keyboard::label(shortcut));
            } else {
                self.bound.push((id, shortcut));
            }
        }
        self.wanted = shortcuts;
    }

    pub(crate) fn shortcut(&self, id: WPARAM) -> Option<Shortcut> {
        self.bound
            .iter()
            .find(|&&(bound, _)| usize::try_from(bound).is_ok_and(|bound| bound == id))
            .map(|&(_, shortcut)| shortcut)
    }
}

fn id(shortcut: Shortcut) -> i32 {
    let modifiers = keyboard::hot_key_modifiers(shortcut.modifiers);
    let key = u32::from(keyboard::virtual_key(shortcut.key));
    i32::try_from(modifiers << 8 | key).expect("hot key ids should stay small")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_shortcut_keeps_an_id_of_its_own() {
        let shortcuts: Vec<Shortcut> = [
            "Cmd+Shift+|",
            "Cmd+\\",
            "Ctrl+Shift+|",
            "Cmd+Shift+Enter",
            "Cmd+Option+Shift+Delete",
        ]
        .into_iter()
        .map(|text| text.parse().unwrap())
        .collect();
        let mut ids: Vec<i32> = shortcuts.iter().copied().map(id).collect();
        assert!(ids.iter().all(|&id| (1..0xC000).contains(&id)));
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), shortcuts.len());
        assert_eq!(id(shortcuts[0]), id("Win+Shift+\\".parse().unwrap()));
    }
}
