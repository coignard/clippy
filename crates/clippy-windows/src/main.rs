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

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clipboard;
mod clock;
mod folders;
mod full_screen;
mod hotkeys;
mod keyboard;
mod menu;
mod panel;
mod shell;
mod sounds;
mod stage;
mod view;
mod win32;

#[cfg(windows)]
fn main() {
    use std::io::{self, IsTerminal};

    use clippy::{App, Storage};
    use clippy_focus::Focus;
    use clippy_habits::Habits;
    use clippy_inbox::Inbox;
    use clippy_reminders::Reminders;
    use clippy_sleepy::Sleepy;
    use clippy_todo::Todo;

    let mut logger =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"));
    if !io::stderr().is_terminal()
        && let Some(log) = folders::log_file()
    {
        logger.target(env_logger::Target::Pipe(Box::new(log)));
    }
    logger.init();
    if shell::is_running() {
        shell::summon_running();
        return;
    }
    let app = App::builder()
        .plugin::<Focus>()
        .plugin::<Habits>()
        .plugin::<Inbox>()
        .plugin::<Todo>()
        .plugin::<Reminders>()
        .plugin::<Sleepy>()
        .clock(|_| clock::now())
        .load(Storage::new(folders::data_directory()));
    shell::run(app);
}

#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    eprintln!("clippy-windows runs on Windows; on a Mac, Clippy lives in clippy-macos");
    std::process::ExitCode::FAILURE
}
