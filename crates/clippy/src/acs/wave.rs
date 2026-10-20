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

use std::io::Cursor;

use hound::{SampleFormat, WavSpec, WavWriter};

const PCM: u16 = 1;
const MICROSOFT_ADPCM: u16 = 2;
const ADPCM_HEADER: usize = 7;
const ADAPTATION: [i32; 16] = [
    230, 230, 230, 230, 307, 409, 512, 614, 768, 614, 512, 409, 307, 230, 230, 230,
];
const SMALLEST_STEP: i32 = 16;
const LARGEST_STEP: i32 = i32::MAX / 768;

#[derive(Debug, thiserror::Error)]
pub(crate) enum WaveError {
    #[error("not a RIFF wave")]
    Malformed,
    #[error("unsupported {bits}-bit format {format} with {channels} channels")]
    Unsupported {
        format: u16,
        bits: u16,
        channels: u16,
    },
    #[error("could not write the sound: {0}")]
    Writing(#[from] hound::Error),
}

type Chunk<'a> = ([u8; 4], &'a [u8]);

struct Format<'a> {
    tag: u16,
    channels: u16,
    rate: u32,
    block_align: usize,
    bits: u16,
    extension: &'a [u8],
}

pub(super) fn to_pcm(wave: &[u8]) -> Result<Vec<u8>, WaveError> {
    let mut format = None;
    let mut data = None;
    let mut length = None;
    for (id, body) in chunks(wave)? {
        match &id {
            b"fmt " => format = Some(Format::parse(body)?),
            b"data" => data = Some(body),
            b"fact" => length = body.first_chunk().copied().map(u32::from_le_bytes),
            _ => {}
        }
    }
    let (format, data) = format.zip(data).ok_or(WaveError::Malformed)?;
    let samples = match (format.tag, format.bits, format.channels) {
        (PCM, 8, 1) => data
            .iter()
            .map(|&sample| (i16::from(sample) - 128) << 8)
            .collect(),
        (PCM, 16, 1) => data
            .as_chunks()
            .0
            .iter()
            .map(|&sample| i16::from_le_bytes(sample))
            .collect(),
        (MICROSOFT_ADPCM, 4, 1) => {
            let mut samples = adpcm(&format, data)?;
            if let Some(length) = length.and_then(|length| usize::try_from(length).ok()) {
                samples.truncate(length);
            }
            samples
        }
        (format, bits, channels) => {
            return Err(WaveError::Unsupported {
                format,
                bits,
                channels,
            });
        }
    };
    riff(format.rate, &samples)
}

fn chunks(wave: &[u8]) -> Result<Vec<Chunk<'_>>, WaveError> {
    let mut rest = wave
        .strip_prefix(b"RIFF")
        .and_then(|rest| rest.get(4..))
        .and_then(|rest| rest.strip_prefix(b"WAVE"))
        .ok_or(WaveError::Malformed)?;
    let mut chunks = Vec::new();
    while let Some((&[a, b, c, d, e, f, g, h], after)) = rest.split_first_chunk::<8>() {
        let size =
            usize::try_from(u32::from_le_bytes([e, f, g, h])).map_err(|_| WaveError::Malformed)?;
        chunks.push(([a, b, c, d], after.get(..size).ok_or(WaveError::Malformed)?));
        rest = after.get(size + size % 2..).unwrap_or_default();
    }
    Ok(chunks)
}

impl<'a> Format<'a> {
    fn parse(body: &'a [u8]) -> Result<Self, WaveError> {
        let field = |at: usize| {
            body.get(at..at + 2)
                .and_then(|bytes| bytes.try_into().ok())
                .map(u16::from_le_bytes)
                .ok_or(WaveError::Malformed)
        };
        Ok(Self {
            tag: field(0)?,
            channels: field(2)?,
            rate: u32::from(field(4)?) | u32::from(field(6)?) << 16,
            block_align: usize::from(field(12)?),
            bits: field(14)?,
            extension: body.get(18..).unwrap_or_default(),
        })
    }
}

fn adpcm(format: &Format<'_>, data: &[u8]) -> Result<Vec<i16>, WaveError> {
    let words: Vec<i16> = format
        .extension
        .as_chunks()
        .0
        .iter()
        .map(|&word| i16::from_le_bytes(word))
        .collect();
    let [per_block, count, coefficients @ ..] = words.as_slice() else {
        return Err(WaveError::Malformed);
    };
    let per_block = usize::try_from(*per_block).map_err(|_| WaveError::Malformed)?;
    let count = usize::try_from(*count).map_err(|_| WaveError::Malformed)?;
    let coefficients = coefficients
        .as_chunks::<2>()
        .0
        .get(..count)
        .ok_or(WaveError::Malformed)?;
    if format.block_align <= ADPCM_HEADER {
        return Err(WaveError::Malformed);
    }
    let mut samples = Vec::new();
    for block in data.chunks(format.block_align) {
        let Some((header, nibbles)) = block.split_first_chunk::<ADPCM_HEADER>() else {
            break;
        };
        let word = |at: usize| i32::from(i16::from_le_bytes([header[at], header[at + 1]]));
        let [first, second] = *coefficients
            .get(usize::from(header[0]))
            .ok_or(WaveError::Malformed)?;
        let mut step = word(1);
        let mut previous = word(3);
        let mut before = word(5);
        samples.extend([clamp(before), clamp(previous)]);
        let codes = nibbles.iter().flat_map(|&byte| [byte >> 4, byte & 0x0F]);
        for code in codes.take(per_block.saturating_sub(2)) {
            let delta = i32::from(code) - if code & 0x08 == 0 { 0 } else { 16 };
            let predicted = (previous * i32::from(first) + before * i32::from(second)) / 256;
            let sample = clamp(predicted + delta * step);
            step = (ADAPTATION[usize::from(code)] * step / 256).clamp(SMALLEST_STEP, LARGEST_STEP);
            before = previous;
            previous = i32::from(sample);
            samples.push(sample);
        }
    }
    Ok(samples)
}

fn clamp(sample: i32) -> i16 {
    i16::try_from(sample.clamp(i16::MIN.into(), i16::MAX.into())).unwrap_or_default()
}

fn riff(rate: u32, samples: &[i16]) -> Result<Vec<u8>, WaveError> {
    let spec = WavSpec {
        channels: 1,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut wave = Cursor::new(Vec::new());
    let mut writer = WavWriter::new(&mut wave, spec)?;
    for &sample in samples {
        writer.write_sample(sample)?;
    }
    writer.finalize()?;
    Ok(wave.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wave(format: u16, bits: u16, channels: u16, data: &[u8]) -> Vec<u8> {
        let mut wave = Vec::new();
        wave.extend_from_slice(b"RIFF\0\0\0\0WAVEfmt ");
        wave.extend_from_slice(&16_u32.to_le_bytes());
        for field in [format, channels] {
            wave.extend_from_slice(&field.to_le_bytes());
        }
        wave.extend_from_slice(&8000_u32.to_le_bytes());
        wave.extend_from_slice(&8000_u32.to_le_bytes());
        for field in [channels, bits] {
            wave.extend_from_slice(&field.to_le_bytes());
        }
        wave.extend_from_slice(b"data");
        wave.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
        wave.extend_from_slice(data);
        wave
    }

    #[test]
    fn widens_unsigned_8_bit_samples() {
        let pcm = to_pcm(&wave(PCM, 8, 1, &[0, 128, 255])).unwrap();
        let reader = hound::WavReader::new(Cursor::new(pcm)).unwrap();
        assert_eq!(reader.spec().sample_rate, 8000);
        let samples: Vec<i16> = reader.into_samples().map(Result::unwrap).collect();
        assert_eq!(samples, [i16::MIN, 0, 127 << 8]);
    }

    #[test]
    fn rejects_what_it_cannot_play() {
        assert!(matches!(to_pcm(b"RIFX"), Err(WaveError::Malformed)));
        let mut overrun = wave(PCM, 8, 1, &[0, 0]);
        overrun.truncate(overrun.len() - 1);
        assert!(matches!(to_pcm(&overrun), Err(WaveError::Malformed)));
        assert!(matches!(
            to_pcm(&wave(PCM, 8, 2, &[0, 0])),
            Err(WaveError::Unsupported { channels: 2, .. })
        ));
    }
}
