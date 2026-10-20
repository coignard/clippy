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

use clippy::Command;
use clippy::menu::Entry;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly, sel};
use objc2_app_kit::{NSMenu, NSMenuItem};
use objc2_foundation::{NSString, ns_string};

use crate::delegate::Delegate;
use crate::keyboard;
use crate::view::BalloonView;

pub(crate) fn build(entries: &[Entry], target: &Delegate) -> (Retained<NSMenu>, Vec<Command>) {
    let mut commands = Vec::new();
    let menu = assemble(entries, target, &mut commands);
    (menu, commands)
}

pub(crate) fn text(target: &BalloonView) -> Retained<NSMenu> {
    let mtm = target.mtm();
    let menu = NSMenu::new(mtm);
    for (title, action) in [
        ("Cut", sel!(cut:)),
        ("Copy", sel!(copy:)),
        ("Paste", sel!(paste:)),
    ] {
        menu.addItem(&item(mtm, title, action, target));
    }
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    menu.addItem(&item(mtm, "Select All", sel!(selectAll:), target));
    menu
}

fn assemble(entries: &[Entry], target: &Delegate, commands: &mut Vec<Command>) -> Retained<NSMenu> {
    let mtm = target.mtm();
    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);
    for entry in entries {
        let item = match entry {
            Entry::Item {
                title,
                command,
                enabled,
                shortcut,
            } => {
                let item = item(mtm, title, sel!(perform:), target);
                item.setTag(isize::try_from(commands.len()).expect("menus should stay small"));
                item.setEnabled(*enabled);
                if let Some(shortcut) = shortcut {
                    item.setKeyEquivalent(&NSString::from_str(&keyboard::key_equivalent(
                        shortcut.key,
                    )));
                    item.setKeyEquivalentModifierMask(keyboard::event_modifiers(
                        shortcut.modifiers,
                    ));
                }
                commands.push(*command);
                item
            }
            Entry::Submenu { title, entries } => {
                let item = NSMenuItem::new(mtm);
                item.setTitle(&NSString::from_str(title));
                item.setSubmenu(Some(&assemble(entries, target, commands)));
                item
            }
            Entry::Separator => NSMenuItem::separatorItem(mtm),
        };
        menu.addItem(&item);
    }
    menu
}

fn item(
    mtm: MainThreadMarker,
    title: &str,
    action: Sel,
    target: &AnyObject,
) -> Retained<NSMenuItem> {
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            Some(action),
            ns_string!(""),
        )
    };
    unsafe { item.setTarget(Some(target)) };
    item
}
