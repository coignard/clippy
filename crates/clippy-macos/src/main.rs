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

mod clock;
mod delegate;
mod full_screen;
mod hotkeys;
mod keyboard;
mod menu;
mod panel;
mod pasteboard;
mod sounds;
mod stage;
mod view;

use std::fs::{self, File};
use std::io::{self, IsTerminal};
use std::path::PathBuf;

use clippy::{App, Storage};
use clippy_focus::Focus;
use clippy_inbox::Inbox;
use clippy_reminders::Reminders;
use clippy_sleepy::Sleepy;
use objc2::MainThreadMarker;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
use objc2_foundation::{NSFileManager, NSSearchPathDirectory, NSSearchPathDomainMask};

use crate::delegate::Delegate;

fn main() {
    let mut logger =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"));
    if !io::stderr().is_terminal()
        && let Some(log) = log_file()
    {
        logger.target(env_logger::Target::Pipe(Box::new(log)));
    }
    logger.init();
    let app = App::builder()
        .plugin::<Focus>()
        .plugin::<Inbox>()
        .plugin::<Reminders>()
        .plugin::<Sleepy>()
        .clock(|_| clock::now())
        .load(Storage::new(data_directory()));
    run(app);
}

fn run(app: App) {
    let mtm = MainThreadMarker::new().expect("Clippy should start on the main thread");
    let application = NSApplication::sharedApplication(mtm);
    application.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let delegate = Delegate::new(mtm, app);
    application.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    application.run();
}

fn data_directory() -> PathBuf {
    user_directory(NSSearchPathDirectory::ApplicationSupportDirectory)
        .expect("macOS should provide an Application Support directory")
        .join("Clippy")
}

fn log_file() -> Option<File> {
    let logs = user_directory(NSSearchPathDirectory::LibraryDirectory)?.join("Logs");
    fs::create_dir_all(&logs).ok()?;
    File::options()
        .create(true)
        .append(true)
        .open(logs.join("Clippy.log"))
        .ok()
}

fn user_directory(directory: NSSearchPathDirectory) -> Option<PathBuf> {
    NSFileManager::defaultManager()
        .URLsForDirectory_inDomains(directory, NSSearchPathDomainMask::UserDomainMask)
        .firstObject()
        .and_then(|url| url.to_file_path())
}
