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

use std::cell::RefCell;
use std::collections::HashMap;
use std::ptr;
use std::time::{Duration, Instant};

use clippy::Sound;
use windows_sys::Win32::Media::Audio::{
    CALLBACK_NULL, HWAVEOUT, WAVE_MAPPER, WAVEFORMATEX, WAVEHDR, WAVERR_STILLPLAYING, waveOutClose,
    waveOutOpen, waveOutPrepareHeader, waveOutReset, waveOutUnprepareHeader, waveOutWrite,
};

use crate::win32::size_of_u32;

const HEADER: u32 = size_of_u32::<WAVEHDR>();
const NANOS_PER_SECOND: u128 = 1_000_000_000;

pub(crate) struct Sounds {
    clips: HashMap<Sound, Clip>,
    voices: RefCell<HashMap<Sound, Voice>>,
}

struct Clip {
    format: WAVEFORMATEX,
    samples: &'static [u8],
    length: Duration,
}

struct Voice {
    device: HWAVEOUT,
    header: Box<WAVEHDR>,
    started: Instant,
    length: Duration,
}

impl Sounds {
    pub(crate) fn load() -> Self {
        let mut clips = HashMap::new();
        for sound in Sound::all() {
            match Clip::parse(sound.wav()) {
                Some(clip) => {
                    clips.insert(sound, clip);
                }
                None => log::warn!("Could not decode {sound:?}"),
            }
        }
        Self {
            clips,
            voices: RefCell::default(),
        }
    }

    pub(crate) fn play(&self, sound: Sound) {
        let Some(clip) = self.clips.get(&sound) else {
            return;
        };
        let mut voices = self.voices.borrow_mut();
        voices.retain(|_, voice| !voice.has_finished());
        voices.remove(&sound);
        match Voice::start(clip) {
            Ok(voice) => {
                voices.insert(sound, voice);
            }
            Err(error) => log::warn!("Could not play {sound:?}: error {error}"),
        }
    }

    pub(crate) fn time_left(&self) -> Option<Duration> {
        let mut voices = self.voices.borrow_mut();
        voices.retain(|_, voice| !voice.has_finished());
        voices.values().map(Voice::time_left).max()
    }
}

impl Clip {
    fn parse(wave: &'static [u8]) -> Option<Self> {
        let mut rest = wave
            .strip_prefix(b"RIFF")?
            .get(4..)?
            .strip_prefix(b"WAVE")?;
        let (mut format, mut samples) = (None, None);
        while let Some((&[a, b, c, d, e, f, g, h], after)) = rest.split_first_chunk::<8>() {
            let size = usize::try_from(u32::from_le_bytes([e, f, g, h])).ok()?;
            let body = after.get(..size)?;
            match &[a, b, c, d] {
                b"fmt " => format = Some(body),
                b"data" => samples = Some(body),
                _ => {}
            }
            rest = after.get(size + size % 2..).unwrap_or_default();
        }
        let (format, samples) = (format?, samples?);
        let half = |at: usize| Some(u16::from_le_bytes(*format.get(at..)?.first_chunk()?));
        let word = |at: usize| Some(u32::from_le_bytes(*format.get(at..)?.first_chunk()?));
        let format = WAVEFORMATEX {
            wFormatTag: half(0)?,
            nChannels: half(2)?,
            nSamplesPerSec: word(4)?,
            nAvgBytesPerSec: word(8)?,
            nBlockAlign: half(12)?,
            wBitsPerSample: half(14)?,
            cbSize: 0,
        };
        let rate = u128::from(format.nAvgBytesPerSec).max(1);
        let nanos = u128::try_from(samples.len()).ok()? * NANOS_PER_SECOND / rate;
        Some(Self {
            format,
            samples,
            length: Duration::from_nanos(u64::try_from(nanos).ok()?),
        })
    }
}

impl Voice {
    fn start(clip: &Clip) -> Result<Self, u32> {
        let mut device = ptr::null_mut();
        succeed(unsafe {
            waveOutOpen(
                &raw mut device,
                WAVE_MAPPER,
                &raw const clip.format,
                0,
                0,
                CALLBACK_NULL,
            )
        })?;
        let mut voice = Self {
            device,
            header: Box::new(WAVEHDR {
                lpData: clip.samples.as_ptr().cast_mut(),
                dwBufferLength: u32::try_from(clip.samples.len()).unwrap_or(u32::MAX),
                ..WAVEHDR::default()
            }),
            started: Instant::now(),
            length: clip.length,
        };
        succeed(unsafe { waveOutPrepareHeader(voice.device, &raw mut *voice.header, HEADER) })?;
        succeed(unsafe { waveOutWrite(voice.device, &raw mut *voice.header, HEADER) })?;
        voice.started = Instant::now();
        Ok(voice)
    }

    fn has_finished(&mut self) -> bool {
        let unprepared =
            unsafe { waveOutUnprepareHeader(self.device, &raw mut *self.header, HEADER) };
        unprepared != WAVERR_STILLPLAYING
    }

    fn time_left(&self) -> Duration {
        self.length.saturating_sub(self.started.elapsed())
    }
}

impl Drop for Voice {
    fn drop(&mut self) {
        unsafe {
            waveOutReset(self.device);
            waveOutUnprepareHeader(self.device, &raw mut *self.header, HEADER);
            waveOutClose(self.device);
        }
    }
}

fn succeed(result: u32) -> Result<(), u32> {
    if result == 0 { Ok(()) } else { Err(result) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sound_decodes_with_its_length() {
        for sound in Sound::all() {
            let clip = Clip::parse(sound.wav()).unwrap();
            assert_eq!({ clip.format.nChannels }, 1);
            assert_eq!({ clip.format.wBitsPerSample }, 16);
            assert_ne!(clip.samples.len(), 0);
            assert!(clip.length > Duration::ZERO);
        }
    }

    #[test]
    fn rejects_what_is_not_a_wave() {
        assert!(Clip::parse(b"RIFF\0\0\0\0AVI ").is_none());
        assert!(Clip::parse(b"").is_none());
    }
}
