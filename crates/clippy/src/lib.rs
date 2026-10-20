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

#![forbid(unsafe_code)]

mod acs;
mod animation;
mod app;
mod assets;
mod balloon;
mod character;
pub mod geometry;
pub mod graphics;
pub mod hours;
pub mod menu;
mod plugin;
mod scene;
mod settings;
pub mod shortcut;
mod sound;
mod storage;
#[cfg(feature = "testing")]
pub mod testing;

pub use animation::Animation;
pub use app::{App, Builder, Command, Effect, Event};
pub use balloon::{Edit, Motion, Utf16Range};
pub use character::portrait;
pub use plugin::{Context, MenuItem, Message, NoSettings, Notice, Plugin, PluginItem, Question};
pub use scene::{BalloonScene, Field, Scene};
pub use settings::{AfterSummons, Settings};
pub use sound::Sound;
pub use storage::{Storage, StorageError};
