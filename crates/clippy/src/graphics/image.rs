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

use png::{BitDepth, ColorType, Decoder, DecodingError, Transformations};

use crate::geometry::Size;
use crate::graphics::{Bitmap, Color, TRANSPARENT, index};

const OPAQUE_FROM: u8 = 128;

#[derive(Debug, thiserror::Error)]
pub(crate) enum DecodeError {
    #[error(transparent)]
    Png(#[from] DecodingError),
    #[error("expected 8-bit RGBA pixels, found {}-bit {color_type:?}", *.bit_depth as u8)]
    Unsupported {
        color_type: ColorType,
        bit_depth: BitDepth,
    },
}

pub(crate) struct Image {
    size: Size,
    pixels: Vec<Color>,
}

impl Image {
    pub(crate) fn decode(png: &[u8]) -> Result<Self, DecodeError> {
        let mut decoder = Decoder::new(Cursor::new(png));
        decoder.set_transformations(Transformations::EXPAND | Transformations::ALPHA);
        let mut reader = decoder.read_info()?;
        let length = reader
            .output_buffer_size()
            .ok_or(DecodingError::LimitsExceeded)?;
        let mut buffer = vec![0; length];
        let info = reader.next_frame(&mut buffer)?;
        if (info.color_type, info.bit_depth) != (ColorType::Rgba, BitDepth::Eight) {
            return Err(DecodeError::Unsupported {
                color_type: info.color_type,
                bit_depth: info.bit_depth,
            });
        }
        let dimension =
            |pixels: u32| i32::try_from(pixels).map_err(|_| DecodingError::LimitsExceeded);
        let size = Size::new(dimension(info.width)?, dimension(info.height)?);
        let pixels = buffer[..info.buffer_size()]
            .as_chunks()
            .0
            .iter()
            .map(|&color| opaque_or_clear(color))
            .collect();
        Ok(Self { size, pixels })
    }

    pub(crate) fn size(&self) -> Size {
        self.size
    }

    #[must_use]
    pub(crate) fn recolored(mut self, recolor: impl Fn(Color) -> Color) -> Self {
        for pixel in &mut self.pixels {
            *pixel = recolor(*pixel);
        }
        self
    }
}

impl Bitmap for Image {
    fn pixel(&self, x: i32, y: i32) -> Color {
        index(self.size, x, y).map_or(TRANSPARENT, |index| self.pixels[index])
    }
}

fn opaque_or_clear([red, green, blue, alpha]: Color) -> Color {
    if alpha < OPAQUE_FROM {
        TRANSPARENT
    } else {
        [red, green, blue, u8::MAX]
    }
}

#[cfg(test)]
mod tests {
    use png::Encoder;

    use super::*;
    use crate::graphics::{BLACK, WHITE};

    fn encode(bit_depth: BitDepth, pixels: &[u8]) -> Vec<u8> {
        let mut png = Vec::new();
        let mut encoder = Encoder::new(&mut png, 2, 1);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(bit_depth);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(pixels)
            .unwrap();
        png
    }

    #[test]
    fn snaps_alpha_to_opaque_or_clear() {
        let png = encode(BitDepth::Eight, &[10, 20, 30, 127, 10, 20, 30, 128]);
        let image = Image::decode(&png).unwrap();
        assert_eq!(image.pixel(0, 0), TRANSPARENT);
        assert_eq!(image.pixel(1, 0), [10, 20, 30, 255]);
    }

    #[test]
    fn rejects_what_it_cannot_draw() {
        assert!(matches!(Image::decode(b"GIF89a"), Err(DecodeError::Png(_))));
        let deep = encode(BitDepth::Sixteen, &[0; 16]);
        let error = Image::decode(&deep).err().unwrap();
        assert_eq!(
            error.to_string(),
            "expected 8-bit RGBA pixels, found 16-bit Rgba"
        );
    }

    #[test]
    fn recolors_every_pixel() {
        let png = encode(BitDepth::Eight, &[255, 255, 255, 255, 0, 0, 0, 0]);
        let image = Image::decode(&png)
            .unwrap()
            .recolored(|color| if color == WHITE { BLACK } else { color });
        assert_eq!(image.pixel(0, 0), BLACK);
        assert_eq!(image.pixel(1, 0), TRANSPARENT);
    }
}
