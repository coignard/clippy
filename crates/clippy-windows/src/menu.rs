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

use std::{mem, ptr};

use clippy::Command;
use clippy::menu::Entry;
use clippy::shortcut::Shortcut;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, HMENU, MENU_ITEM_FLAGS, MF_GRAYED, MF_POPUP,
    MF_SEPARATOR, MF_STRING, PostMessageW, SetForegroundWindow, TPM_NONOTIFY, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, TrackPopupMenuEx, WM_NULL,
};

use crate::keyboard;

pub(crate) struct Menu {
    handle: HMENU,
}

impl Menu {
    pub(crate) fn new() -> Self {
        Self {
            handle: unsafe { CreatePopupMenu() },
        }
    }

    pub(crate) fn item(&self, id: usize, label: &str, enabled: bool) {
        let flags = if enabled {
            MF_STRING
        } else {
            MF_STRING | MF_GRAYED
        };
        self.append(flags, id, label);
    }

    pub(crate) fn separator(&self) {
        self.append(MF_SEPARATOR, 0, "");
    }

    fn submenu(&self, label: &str, submenu: Self) {
        let handle = submenu.handle;
        mem::forget(submenu);
        self.append(MF_POPUP, handle.addr(), label);
    }

    fn append(&self, flags: MENU_ITEM_FLAGS, id: usize, label: &str) {
        let label: Vec<u16> = label.encode_utf16().chain([0]).collect();
        unsafe { AppendMenuW(self.handle, flags, id, label.as_ptr()) };
    }

    pub(crate) fn track(&self, owner: HWND, (x, y): (i32, i32)) -> Option<usize> {
        unsafe { SetForegroundWindow(owner) };
        let flags = TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY;
        let chosen = unsafe { TrackPopupMenuEx(self.handle, flags, x, y, owner, ptr::null()) };
        unsafe { PostMessageW(owner, WM_NULL, 0, 0) };
        usize::try_from(chosen).ok().filter(|&id| id != 0)
    }
}

impl Drop for Menu {
    fn drop(&mut self) {
        unsafe { DestroyMenu(self.handle) };
    }
}

pub(crate) fn build(entries: &[Entry]) -> (Menu, Vec<Command>) {
    let mut commands = Vec::new();
    let menu = assemble(entries, &mut commands);
    (menu, commands)
}

fn assemble(entries: &[Entry], commands: &mut Vec<Command>) -> Menu {
    let menu = Menu::new();
    for entry in entries {
        match entry {
            Entry::Item {
                title,
                command,
                enabled,
                shortcut,
            } => {
                commands.push(*command);
                menu.item(commands.len(), &label(title, *shortcut), *enabled);
            }
            Entry::Submenu { title, entries } => {
                let submenu = assemble(entries, commands);
                menu.submenu(&label(title, None), submenu);
            }
            Entry::Separator => menu.separator(),
        }
    }
    menu
}

fn label(title: &str, shortcut: Option<Shortcut>) -> String {
    let title = title.replace('&', "&&");
    match shortcut {
        Some(shortcut) => format!("{title}\t{}", keyboard::label(shortcut)),
        None => title,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_keep_ampersands_and_show_shortcuts() {
        assert_eq!(label("Salt & Pepper", None), "Salt && Pepper");
        let hide = "Cmd+Shift+|".parse().unwrap();
        assert_eq!(label("Hide", Some(hide)), "Hide\tWin+Shift+\\");
    }
}
