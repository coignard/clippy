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

use std::cell::{Cell, OnceCell, RefCell};
use std::ptr;
use std::rc::Rc;
use std::time::{Duration, Instant};

use clippy::{App, Command, Effect, Event};
use windows_sys::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, WPARAM,
};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, EVENT_SYSTEM_FOREGROUND, FindWindowW, GetForegroundWindow, GetMessageW,
    KillTimer, MSG, PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMESUSPEND, PostMessageW, PostQuitMessage,
    SPI_SETWORKAREA, SetCoalescableTimer, SetForegroundWindow, SetTimer, TranslateMessage,
    USER_TIMER_MAXIMUM, USER_TIMER_MINIMUM, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_APP,
    WM_CLOSE, WM_DISPLAYCHANGE, WM_HOTKEY, WM_POWERBROADCAST, WM_SETTINGCHANGE, WM_TIMECHANGE,
    WM_TIMER,
};
use windows_sys::core::w;

use crate::clipboard;
use crate::clock;
use crate::full_screen;
use crate::hotkeys::HotKeys;
use crate::menu;
use crate::panel;
use crate::sounds::Sounds;
use crate::stage::{Display, Stage};
use crate::win32;

const TICK: usize = 1;
const FULL_SCREEN_CHECK: usize = 2;
const FULL_SCREEN_CHECK_INTERVAL: u32 = 1000;
const FULL_SCREEN_CHECK_TOLERANCE: u32 = FULL_SCREEN_CHECK_INTERVAL / 2;
const FADING_SOUND_CHECK_INTERVAL: Duration = Duration::from_millis(50);
const SUMMON: u32 = WM_APP;

thread_local! {
    static SHELL: OnceCell<Rc<Shell>> = const { OnceCell::new() };
}

pub(crate) struct Shell {
    app: RefCell<App>,
    window: HWND,
    launched: OnceCell<Launched>,
    display: Cell<Option<Display>>,
    full_screen: Cell<Option<bool>>,
    syncing: Cell<bool>,
    stale: Cell<bool>,
}

struct Launched {
    stage: Stage,
    sounds: Sounds,
    hot_keys: RefCell<HotKeys>,
}

pub(crate) fn is_running() -> bool {
    let mutex = unsafe { CreateMutexW(ptr::null(), 0, w!("Local\\Clippy")) };
    !mutex.is_null() && unsafe { GetLastError() } == ERROR_ALREADY_EXISTS
}

pub(crate) fn summon_running() {
    let window = unsafe { FindWindowW(panel::SHELL_CLASS, ptr::null()) };
    if !window.is_null() {
        unsafe { PostMessageW(window, SUMMON, 0, 0) };
    }
}

pub(crate) fn run(app: App) {
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let shell = Rc::new(Shell::new(app, panel::shell()));
    SHELL.with(|cell| {
        let _ = cell.set(Rc::clone(&shell));
    });
    shell.launch();
    drop(shell);
    let mut message = MSG::default();
    while unsafe { GetMessageW(&raw mut message, ptr::null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&raw const message);
            DispatchMessageW(&raw const message);
        }
    }
}

pub(crate) fn with<T>(act: impl FnOnce(&Shell) -> T) -> Option<T> {
    let shell = SHELL.with(|cell| cell.get().cloned())?;
    Some(act(&shell))
}

impl Shell {
    fn new(app: App, window: HWND) -> Self {
        Self {
            app: RefCell::new(app),
            window,
            launched: OnceCell::new(),
            display: Cell::new(None),
            full_screen: Cell::new(None),
            syncing: Cell::new(false),
            stale: Cell::new(false),
        }
    }

    pub(crate) fn stage(&self) -> Option<&Stage> {
        self.launched.get().map(|launched| &launched.stage)
    }

    pub(crate) fn handle(&self, message: u32, wparam: WPARAM, _lparam: LPARAM) -> Option<LRESULT> {
        match message {
            WM_TIMER if wparam == TICK => self.send(Event::Tick),
            WM_TIMER if wparam == FULL_SCREEN_CHECK => self.check_full_screen(),
            WM_HOTKEY => self.hot_key_pressed(wparam),
            WM_TIMECHANGE => self.clock_changed(),
            WM_POWERBROADCAST => {
                if matches!(
                    u32::try_from(wparam),
                    Ok(PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND)
                ) {
                    self.clock_changed();
                }
                return None;
            }
            WM_DISPLAYCHANGE => self.screens_changed(),
            WM_SETTINGCHANGE if u32::try_from(wparam) == Ok(SPI_SETWORKAREA) => {
                self.screens_changed();
            }
            SUMMON => self.summon(),
            WM_CLOSE => self.send(Event::Command(Command::Quit)),
            _ => return None,
        }
        Some(0)
    }

    pub(crate) fn send(&self, event: Event) {
        self.app.borrow_mut().handle(event, Instant::now());
        self.sync();
    }

    pub(crate) fn show_menu(&self, at: (i32, i32)) {
        let entries = self.app.borrow().menu(Instant::now());
        let (menu, commands) = menu::build(&entries);
        let owner = match self.stage() {
            Some(stage) if stage.balloon().has_focus() => stage.balloon().window(),
            _ => self.window,
        };
        let previous = unsafe { GetForegroundWindow() };
        let chosen = menu.track(owner, at);
        drop(menu);
        let command = chosen.and_then(|id| commands.get(id.checked_sub(1)?).copied());
        if let Some(command) = command {
            self.send(Event::Command(command));
        }
        let kept = unsafe { GetForegroundWindow() } == self.window;
        if kept && win32::exists(previous) && !win32::is_ours(previous) {
            unsafe { SetForegroundWindow(previous) };
        }
    }

    pub(crate) fn drag_clippy(&self, origin: (i32, i32)) {
        let Some(stage) = self.stage() else {
            return;
        };
        stage.move_to(origin);
        if stage.display() != self.display.get() {
            self.check_full_screen();
        }
        self.send(Event::Tick);
    }

    pub(crate) fn dropped(&self, from: (i32, i32)) {
        if let Some(stage) = self.stage()
            && stage.origin() != from
        {
            stage.remember();
        }
    }

    pub(crate) fn screens_changed(&self) {
        if let Some(stage) = self.stage() {
            if stage.is_lost() {
                let home = self.app.borrow().home(Stage::home_screen());
                stage.place(home);
            } else {
                stage.settle();
            }
        }
        self.check_full_screen();
        self.sync();
    }

    fn launch(&self) {
        let stage = Stage::new(self.app.borrow().clippy_size());
        let home = self.app.borrow().home(Stage::home_screen());
        stage.return_to_where_left(home);
        let _ = self.launched.set(Launched {
            stage,
            sounds: Sounds::load(),
            hot_keys: RefCell::new(HotKeys::new(self.window)),
        });
        if self.watches_full_screen() {
            self.watch_full_screen();
            self.check_full_screen();
        }
        self.send(Event::Launch);
    }

    fn watches_full_screen(&self) -> bool {
        self.app.borrow().settings().hide_in_full_screen
    }

    fn watch_full_screen(&self) {
        unsafe {
            SetCoalescableTimer(
                self.window,
                FULL_SCREEN_CHECK,
                FULL_SCREEN_CHECK_INTERVAL,
                None,
                FULL_SCREEN_CHECK_TOLERANCE,
            );
        }
        let hook = unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                ptr::null_mut(),
                Some(foreground_changed),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            )
        };
        if hook.is_null() {
            log::warn!("Clippy cannot tell when another window comes to the front");
        }
    }

    fn check_full_screen(&self) {
        let Some(stage) = self.stage() else {
            return;
        };
        if !self.watches_full_screen() {
            return;
        }
        let display = stage.display();
        self.display.set(display);
        let taken = display.is_some_and(|display| full_screen::is_taken(display.frame));
        if self.full_screen.replace(Some(taken)) != Some(taken) {
            log::debug!("Full screen is now {}", if taken { "on" } else { "off" });
            self.send(Event::FullScreen(taken));
        }
    }

    fn hot_key_pressed(&self, id: WPARAM) {
        let shortcut = self
            .launched
            .get()
            .and_then(|launched| launched.hot_keys.borrow().shortcut(id));
        if let Some(shortcut) = shortcut {
            self.send(Event::Shortcut(shortcut));
        }
    }

    fn clock_changed(&self) {
        clock::forget_time_zone();
        self.send(Event::Tick);
    }

    fn summon(&self) {
        if self.stage().is_some_and(|stage| !stage.shows_clippy()) {
            self.send(Event::Command(Command::Toggle));
        }
    }

    fn sync(&self) {
        if self.syncing.replace(true) {
            self.stale.set(true);
            return;
        }
        loop {
            self.present();
            if !self.stale.replace(false) {
                break;
            }
        }
        self.syncing.set(false);
        if self.app.borrow().is_finished() {
            self.finish();
        }
    }

    fn present(&self) {
        let Some(launched) = self.launched.get() else {
            return;
        };
        let (clippy, screen) = launched.stage.geometry();
        let (scene, effects, deadline, shortcuts) = {
            let mut app = self.app.borrow_mut();
            app.place(clippy, screen);
            let shortcuts = app.shortcuts(Instant::now());
            (app.scene(), app.take_effects(), app.deadline(), shortcuts)
        };
        launched.hot_keys.borrow_mut().bind(shortcuts);
        launched.stage.show(&scene);
        for effect in effects {
            match effect {
                Effect::Play(sound) => launched.sounds.play(sound),
                Effect::Copy(text) => clipboard::copy(self.window, &text),
            }
        }
        self.wake_in(deadline.map(|deadline| deadline.saturating_duration_since(Instant::now())));
    }

    fn wake_in(&self, wait: Option<Duration>) {
        match wait {
            Some(wait) => {
                unsafe { SetTimer(self.window, TICK, milliseconds(wait), None) };
            }
            None => {
                unsafe { KillTimer(self.window, TICK) };
            }
        }
    }

    fn finish(&self) {
        if let Some(launched) = self.launched.get()
            && let Some(left) = launched.sounds.time_left()
        {
            self.wake_in(Some(left.max(FADING_SOUND_CHECK_INTERVAL)));
            return;
        }
        unsafe { PostQuitMessage(0) };
    }
}

extern "system" fn foreground_changed(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _window: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    with(Shell::check_full_screen);
}

fn milliseconds(wait: Duration) -> u32 {
    u32::try_from(wait.as_nanos().div_ceil(1_000_000))
        .unwrap_or(USER_TIMER_MAXIMUM)
        .clamp(USER_TIMER_MINIMUM, USER_TIMER_MAXIMUM)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timers_wait_whole_milliseconds_within_what_windows_allows() {
        assert_eq!(milliseconds(Duration::ZERO), USER_TIMER_MINIMUM);
        assert_eq!(milliseconds(Duration::from_micros(100_001)), 101);
        assert_eq!(milliseconds(Duration::from_secs(60)), 60_000);
        assert_eq!(milliseconds(Duration::MAX), USER_TIMER_MAXIMUM);
    }
}
