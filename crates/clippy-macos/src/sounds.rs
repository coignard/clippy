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

use std::collections::HashMap;
use std::time::Duration;

use clippy::Sound;
use objc2::AnyThread;
use objc2::rc::Retained;
use objc2_app_kit::NSSound;
use objc2_foundation::NSData;

pub(crate) struct Sounds {
    decoded: HashMap<Sound, Retained<NSSound>>,
}

impl Sounds {
    pub(crate) fn load() -> Self {
        let mut decoded = HashMap::new();
        for sound in Sound::all() {
            match NSSound::initWithData(NSSound::alloc(), &NSData::with_bytes(sound.wav())) {
                Some(player) => {
                    decoded.insert(sound, player);
                }
                None => log::warn!("Could not decode {sound:?}"),
            }
        }
        Self { decoded }
    }

    pub(crate) fn play(&self, sound: Sound) {
        let Some(player) = self.decoded.get(&sound) else {
            return;
        };
        if player.isPlaying() {
            player.stop();
        }
        if !player.play() {
            log::warn!("Could not play {sound:?}");
        }
    }

    pub(crate) fn time_left(&self) -> Option<Duration> {
        self.decoded
            .values()
            .filter(|player| player.isPlaying())
            .map(|player| player.duration() - player.currentTime())
            .max_by(f64::total_cmp)
            .map(|seconds| Duration::try_from_secs_f64(seconds).unwrap_or_default())
    }
}
