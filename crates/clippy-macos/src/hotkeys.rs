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

use clippy::shortcut::{Key, Modifiers, Shortcut};
use global_hotkey::hotkey::{Code, HotKey, Modifiers as HotKeyModifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use objc2::MainThreadMarker;
use objc2::runtime::AnyObject;
use objc2_app_kit::NSApplication;

use crate::delegate::Delegate;

const MODIFIERS: [(Modifiers, HotKeyModifiers); 4] = [
    (Modifiers::COMMAND, HotKeyModifiers::META),
    (Modifiers::CONTROL, HotKeyModifiers::CONTROL),
    (Modifiers::OPTION, HotKeyModifiers::ALT),
    (Modifiers::SHIFT, HotKeyModifiers::SHIFT),
];

pub(crate) struct HotKeys {
    manager: GlobalHotKeyManager,
    wanted: Vec<Shortcut>,
    bound: Vec<(HotKey, Shortcut)>,
}

impl HotKeys {
    pub(crate) fn new() -> Result<Self, global_hotkey::Error> {
        GlobalHotKeyEvent::set_event_handler(Some(deliver));
        Ok(Self {
            manager: GlobalHotKeyManager::new()?,
            wanted: Vec::new(),
            bound: Vec::new(),
        })
    }

    pub(crate) fn bind(&mut self, shortcuts: Vec<Shortcut>) {
        if shortcuts == self.wanted {
            return;
        }
        for (hot_key, shortcut) in self.bound.drain(..) {
            if let Err(error) = self.manager.unregister(hot_key) {
                log::warn!("Could not release {shortcut}: {error}");
            }
        }
        for &shortcut in &shortcuts {
            let hot_key = hot_key(shortcut);
            match self.manager.register(hot_key) {
                Ok(()) => self.bound.push((hot_key, shortcut)),
                Err(error) => log::error!("Could not register {shortcut}: {error}"),
            }
        }
        self.wanted = shortcuts;
    }

    pub(crate) fn shortcut(&self, id: u32) -> Option<Shortcut> {
        self.bound
            .iter()
            .find(|(hot_key, _)| hot_key.id() == id)
            .map(|&(_, shortcut)| shortcut)
    }
}

fn deliver(event: GlobalHotKeyEvent) {
    if event.state() != HotKeyState::Pressed {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        log::warn!("A hot key arrived off the main thread");
        return;
    };
    let delegate = NSApplication::sharedApplication(mtm).delegate();
    let delegate: Option<&AnyObject> = delegate.as_deref().map(AsRef::as_ref);
    if let Some(delegate) = delegate.and_then(AnyObject::downcast_ref::<Delegate>) {
        delegate.hot_key_pressed(event.id());
    }
}

fn hot_key(shortcut: Shortcut) -> HotKey {
    let modifiers = MODIFIERS
        .into_iter()
        .filter(|&(modifier, _)| shortcut.modifiers.contains(modifier))
        .fold(HotKeyModifiers::empty(), |held, (_, modifier)| {
            held | modifier
        });
    HotKey::new(Some(modifiers), code(shortcut.key))
}

fn code(key: Key) -> Code {
    match key {
        Key::A => Code::KeyA,
        Key::B => Code::KeyB,
        Key::C => Code::KeyC,
        Key::D => Code::KeyD,
        Key::E => Code::KeyE,
        Key::F => Code::KeyF,
        Key::G => Code::KeyG,
        Key::H => Code::KeyH,
        Key::I => Code::KeyI,
        Key::J => Code::KeyJ,
        Key::K => Code::KeyK,
        Key::L => Code::KeyL,
        Key::M => Code::KeyM,
        Key::N => Code::KeyN,
        Key::O => Code::KeyO,
        Key::P => Code::KeyP,
        Key::Q => Code::KeyQ,
        Key::R => Code::KeyR,
        Key::S => Code::KeyS,
        Key::T => Code::KeyT,
        Key::U => Code::KeyU,
        Key::V => Code::KeyV,
        Key::W => Code::KeyW,
        Key::X => Code::KeyX,
        Key::Y => Code::KeyY,
        Key::Z => Code::KeyZ,
        Key::Digit0 => Code::Digit0,
        Key::Digit1 => Code::Digit1,
        Key::Digit2 => Code::Digit2,
        Key::Digit3 => Code::Digit3,
        Key::Digit4 => Code::Digit4,
        Key::Digit5 => Code::Digit5,
        Key::Digit6 => Code::Digit6,
        Key::Digit7 => Code::Digit7,
        Key::Digit8 => Code::Digit8,
        Key::Digit9 => Code::Digit9,
        Key::Minus => Code::Minus,
        Key::Equal => Code::Equal,
        Key::LeftBracket => Code::BracketLeft,
        Key::RightBracket => Code::BracketRight,
        Key::Backslash => Code::Backslash,
        Key::Semicolon => Code::Semicolon,
        Key::Quote => Code::Quote,
        Key::Comma => Code::Comma,
        Key::Period => Code::Period,
        Key::Slash => Code::Slash,
        Key::Grave => Code::Backquote,
        Key::Space => Code::Space,
        Key::Enter => Code::Enter,
        Key::Delete => Code::Backspace,
        Key::ForwardDelete => Code::Delete,
        Key::F1 => Code::F1,
        Key::F2 => Code::F2,
        Key::F3 => Code::F3,
        Key::F4 => Code::F4,
        Key::F5 => Code::F5,
        Key::F6 => Code::F6,
        Key::F7 => Code::F7,
        Key::F8 => Code::F8,
        Key::F9 => Code::F9,
        Key::F10 => Code::F10,
        Key::F11 => Code::F11,
        Key::F12 => Code::F12,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_become_the_same_hot_keys_as_their_spelling() {
        let shortcut: Shortcut = "Cmd+Shift+Delete".parse().unwrap();
        let spelled: HotKey = "cmd+shift+Backspace".parse().unwrap();
        assert_eq!(hot_key(shortcut), spelled);
        let toggle: Shortcut = "Cmd+Shift+|".parse().unwrap();
        assert_eq!(hot_key(toggle), "cmd+shift+Backslash".parse().unwrap());
    }
}
