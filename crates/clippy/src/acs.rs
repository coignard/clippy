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

mod compression;
mod reader;
mod wave;

use std::sync::Arc;
use std::time::Duration;

use crate::acs::reader::Reader;
use crate::animation::{Branch, Clip, Frame, FrameImage};
use crate::geometry::Size;
use crate::graphics::{Color, PalettedImage, TRANSPARENT};
use crate::sound::Sound;

pub(crate) use wave::WaveError;

const SIGNATURE: u32 = 0xABCD_ABC3;
const HAS_VOICE: u32 = 1 << 5;
const HAS_BALLOON: u32 = 1 << 9;
const SILENT: u16 = u16::MAX;
const TICK: Duration = Duration::from_millis(10);
const VERSION: usize = 2 + 2;
const LOCATOR: usize = 4 + 4;
const GUID: usize = 16;
const CHECKSUM: usize = 4;
const SPEED_AND_PITCH: usize = 4 + 2;
const LANGUAGE: usize = 2;
const GENDER_AND_AGE: usize = 2 + 2;
const BALLOON_SHAPE: usize = 1 + 1;
const FONT_STYLE: usize = 4 + 4 + 1 + 1;
const IMAGE_FLAGS: usize = 1;
const TRANSITION: usize = 1;
const OVERLAY_KIND: usize = 1 + 1 + 2 + 1;
const OVERLAY_BOUNDS: usize = 2 + 2 + 2 + 2;
const TRAY_ICONS: usize = 2;

#[derive(Debug, thiserror::Error)]
pub(crate) enum AcsError {
    #[error("not a Microsoft Agent 2 character")]
    Signature,
    #[error("the character file is cut short")]
    Truncated,
    #[error("the character has no word balloon")]
    NoBalloon,
    #[error("image {0} is corrupt")]
    Image(usize),
    #[error("sound {index}: {source}")]
    Sound { index: usize, source: WaveError },
    #[error("{0} has no frames")]
    Empty(String),
    #[error("{animation} frame {frame} refers to something that does not exist")]
    Dangling { animation: String, frame: usize },
}

pub(crate) struct Character {
    pub(crate) frame_size: Size,
    pub(crate) balloon: Balloon,
    pub(crate) states: Vec<State>,
    pub(crate) animations: Vec<(String, Clip)>,
    pub(crate) images: Vec<PalettedImage>,
    pub(crate) sounds: Vec<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Balloon {
    pub(crate) text: Color,
    pub(crate) fill: Color,
    pub(crate) border: Color,
}

pub(crate) struct State {
    pub(crate) name: String,
    pub(crate) animations: Vec<String>,
}

struct Limits {
    frames: usize,
    images: usize,
    sounds: usize,
}

impl Character {
    pub(crate) fn parse(file: &[u8]) -> Result<Self, AcsError> {
        let mut header = Reader::new(file);
        if header.u32()? != SIGNATURE {
            return Err(AcsError::Signature);
        }
        let mut info = header.located()?;
        let mut animations = header.located()?;
        let mut images = header.located()?;
        let mut sounds = header.located()?;

        info.skip(VERSION + LOCATOR + GUID)?;
        let frame_size = Size::new(i32::from(info.u16()?), i32::from(info.u16()?));
        let transparent = info.u8()?;
        let flags = info.u32()?;
        info.skip(VERSION)?;
        if flags & HAS_VOICE != 0 {
            skip_voice(&mut info)?;
        }
        if flags & HAS_BALLOON == 0 {
            return Err(AcsError::NoBalloon);
        }
        let balloon = read_balloon(&mut info)?;
        let palette = read_palette(&mut info, transparent)?;
        if info.flag()? {
            for _ in 0..TRAY_ICONS {
                let size = info.length()?;
                info.skip(size)?;
            }
        }
        let count = usize::from(info.u16()?);
        let states = info.list(count, read_state)?;

        let count = images.length()?;
        let images = (0..count)
            .map(|index| read_image(&mut images, &palette)?.ok_or(AcsError::Image(index)))
            .collect::<Result<Vec<_>, _>>()?;
        let count = sounds.length()?;
        let sounds = (0..count)
            .map(|index| {
                let wave = sounds.located()?.rest();
                sounds.skip(CHECKSUM)?;
                wave::to_pcm(wave).map_err(|source| AcsError::Sound { index, source })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let count = animations.length()?;
        let animations = animations.list(count, |entry| {
            entry.string()?;
            read_animation(&mut entry.located()?, images.len(), sounds.len())
        })?;
        Ok(Self {
            frame_size,
            balloon,
            states,
            animations,
            images,
            sounds,
        })
    }
}

fn skip_voice(info: &mut Reader<'_>) -> Result<(), AcsError> {
    info.skip(GUID + GUID + SPEED_AND_PITCH)?;
    if info.flag()? {
        info.skip(LANGUAGE)?;
        info.string()?;
        info.skip(GENDER_AND_AGE)?;
        info.string()?;
    }
    Ok(())
}

fn read_balloon(info: &mut Reader<'_>) -> Result<Balloon, AcsError> {
    info.skip(BALLOON_SHAPE)?;
    let balloon = Balloon {
        text: read_color(info)?,
        fill: read_color(info)?,
        border: read_color(info)?,
    };
    info.string()?;
    info.skip(FONT_STYLE)?;
    Ok(balloon)
}

fn read_color(info: &mut Reader<'_>) -> Result<Color, AcsError> {
    let [red, green, blue, _] = info.array()?;
    Ok([red, green, blue, u8::MAX])
}

fn read_palette(info: &mut Reader<'_>, transparent: u8) -> Result<Arc<[Color]>, AcsError> {
    let count = info.length()?;
    let palette = info.list(count, |entry| {
        let [blue, green, red, _] = entry.array()?;
        Ok([red, green, blue, u8::MAX])
    })?;
    Ok(palette
        .into_iter()
        .enumerate()
        .map(|(index, color)| {
            if index == usize::from(transparent) {
                TRANSPARENT
            } else {
                color
            }
        })
        .collect())
}

fn read_state(info: &mut Reader<'_>) -> Result<State, AcsError> {
    let name = info.string()?;
    let count = usize::from(info.u16()?);
    let animations = info.list(count, Reader::string)?;
    Ok(State { name, animations })
}

fn read_image(
    list: &mut Reader<'_>,
    palette: &Arc<[Color]>,
) -> Result<Option<PalettedImage>, AcsError> {
    let mut image = list.located()?;
    list.skip(CHECKSUM)?;
    image.skip(IMAGE_FLAGS)?;
    let size = Size::new(i32::from(image.u16()?), i32::from(image.u16()?));
    let compressed = image.flag()?;
    let length = image.length()?;
    let data = image.bytes(length)?;
    let width = usize::try_from(size.width).unwrap_or_default();
    let height = usize::try_from(size.height).unwrap_or_default();
    if width == 0 || height == 0 {
        return Ok(None);
    }
    let stride = width.next_multiple_of(4);
    let rows = if compressed {
        compression::decompress(data, stride * height)
    } else {
        data.get(..stride * height).map(<[u8]>::to_vec)
    };
    Ok(rows.map(|rows| {
        let indices = rows
            .chunks(stride)
            .rev()
            .flat_map(|row| &row[..width])
            .copied()
            .collect();
        PalettedImage::new(size, indices, Arc::clone(palette))
    }))
}

fn read_animation(
    animation: &mut Reader<'_>,
    images: usize,
    sounds: usize,
) -> Result<(String, Clip), AcsError> {
    let name = animation.string()?;
    animation.skip(TRANSITION)?;
    animation.string()?;
    let limits = Limits {
        frames: usize::from(animation.u16()?),
        images,
        sounds,
    };
    let frames = animation.list(limits.frames, read_frame)?;
    if frames.is_empty() {
        return Err(AcsError::Empty(name));
    }
    if let Some(frame) = frames.iter().position(|frame| !limits.allow(frame)) {
        return Err(AcsError::Dangling {
            animation: name,
            frame,
        });
    }
    Ok((name, Clip { frames }))
}

fn read_frame(frame: &mut Reader<'_>) -> Result<Frame, AcsError> {
    let count = usize::from(frame.u16()?);
    let images = frame.list(count, |image| {
        Ok(FrameImage {
            image: image.length()?,
            x: i32::from(image.i16()?),
            y: i32::from(image.i16()?),
        })
    })?;
    let sound = frame.u16()?;
    let duration = TICK * u32::from(frame.u16()?);
    let exit = usize::try_from(frame.i16()?).ok();
    let count = usize::from(frame.u8()?);
    let branches = frame.list(count, |branch| {
        Ok(Branch {
            frame: usize::from(branch.u16()?),
            weight: u32::from(branch.u16()?),
        })
    })?;
    for _ in 0..frame.u8()? {
        frame.skip(OVERLAY_KIND)?;
        let region = frame.flag()?;
        frame.skip(OVERLAY_BOUNDS)?;
        if region {
            let size = frame.length()?;
            frame.skip(size)?;
        }
    }
    Ok(Frame {
        duration,
        images,
        sound: (sound != SILENT).then(|| Sound::new(usize::from(sound))),
        exit,
        branches,
    })
}

impl Limits {
    fn allow(&self, frame: &Frame) -> bool {
        frame.images.iter().all(|image| image.image < self.images)
            && frame.sound.is_none_or(|sound| sound.index() < self.sounds)
            && frame.exit.is_none_or(|exit| exit < self.frames)
            && frame
                .branches
                .iter()
                .all(|branch| branch.frame < self.frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::CLIPPIT;
    use crate::graphics::{BLACK, Bitmap};

    const FILE: &[u8] = include_bytes!("../assets/clippit.acs");

    fn animation(name: &str) -> &'static Clip {
        let (_, clip) = CLIPPIT
            .animations
            .iter()
            .find(|(candidate, _)| candidate == name)
            .unwrap();
        clip
    }

    fn samples(wave: &[u8]) -> Vec<i16> {
        wave[44..]
            .as_chunks()
            .0
            .iter()
            .map(|&sample| i16::from_le_bytes(sample))
            .collect()
    }

    #[test]
    fn reads_the_original_character() {
        assert_eq!(CLIPPIT.frame_size, Size::new(124, 93));
        assert_eq!(
            (
                CLIPPIT.animations.len(),
                CLIPPIT.images.len(),
                CLIPPIT.sounds.len()
            ),
            (43, 902, 15)
        );
        assert_eq!(
            CLIPPIT.balloon,
            Balloon {
                text: BLACK,
                fill: [0xFF, 0xFF, 0xE1, 0xFF],
                border: BLACK,
            }
        );
        let durations: Vec<_> = animation("SHOW")
            .frames
            .iter()
            .map(|frame| frame.duration.as_millis())
            .collect();
        assert_eq!(durations, [10; 5]);
    }

    #[test]
    fn images_are_upright_on_a_clear_background() {
        let rest = &CLIPPIT.images[animation("RESTPOSE").frames[0].images[0].image];
        assert_eq!(rest.pixel(0, 0), TRANSPARENT);
        assert_eq!(rest.pixel(60, 50), [0xFF, 0xFF, 0xCC, 0xFF]);
        assert_ne!(rest.pixel(100, 26), TRANSPARENT);
        assert_eq!(rest.pixel(100, 66), TRANSPARENT);
    }

    #[test]
    fn sounds_become_plain_16_bit_waves() {
        for wave in &CLIPPIT.sounds {
            assert_eq!(&wave[..4], b"RIFF");
            assert_eq!(&wave[20..24], [1, 0, 1, 0]);
            assert_eq!(&wave[34..36], [16, 0]);
        }
        let adpcm = samples(&CLIPPIT.sounds[2]);
        assert_eq!(adpcm.len(), 11_616);
        assert_eq!(adpcm[5242..5247], [-3803, -3958, -3980, -3935, -3852]);
    }

    #[test]
    fn rejects_files_that_are_not_whole_characters() {
        assert!(matches!(
            Character::parse(b"MZ\x90\x00 not an agent"),
            Err(AcsError::Signature)
        ));
        assert!(matches!(
            Character::parse(&FILE[..1024]),
            Err(AcsError::Truncated)
        ));
    }
}
