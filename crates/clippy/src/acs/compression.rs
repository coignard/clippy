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

const OFFSETS: [(u32, usize); 4] = [(6, 1), (9, 65), (12, 577), (20, 4673)];
const END: usize = (1 << 20) - 1 + 4673;
const LONGEST_RUN_PREFIX: u32 = 12;

pub(super) fn decompress(compressed: &[u8], length: usize) -> Option<Vec<u8>> {
    let mut bits = Bits::new(compressed.get(1..)?);
    let mut output = Vec::new();
    loop {
        if !bits.bit()? {
            output.push(u8::try_from(bits.number(8)?).ok()?);
            continue;
        }
        let (width, base) = OFFSETS[usize::try_from(bits.ones(3)?).ok()?];
        let distance = usize::try_from(bits.number(width)?).ok()? + base;
        if distance == END {
            break;
        }
        let prefix = bits.ones(LONGEST_RUN_PREFIX)?;
        let mut run = usize::try_from((1 << prefix) + 1 + bits.number(prefix)?).ok()?;
        if width == 20 {
            run += 1;
        }
        let start = output.len().checked_sub(distance)?;
        if output.len() + run > length {
            return None;
        }
        for index in start..start + run {
            output.push(output[index]);
        }
    }
    (output.len() == length).then_some(output)
}

struct Bits<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Bits<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn bit(&mut self) -> Option<bool> {
        let byte = self.bytes.get(self.position / 8)?;
        let bit = byte >> (self.position % 8) & 1 == 1;
        self.position += 1;
        Some(bit)
    }

    fn number(&mut self, width: u32) -> Option<u32> {
        (0..width).try_fold(0, |number, place| {
            Some(number | u32::from(self.bit()?) << place)
        })
    }

    fn ones(&mut self, most: u32) -> Option<u32> {
        let mut count = 0;
        while count < most && self.bit()? {
            count += 1;
        }
        Some(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(bits: &str) -> Vec<u8> {
        let mut bytes = vec![0];
        for (index, bit) in bits.chars().filter(|bit| !bit.is_whitespace()).enumerate() {
            if index % 8 == 0 {
                bytes.push(0);
            }
            if bit == '1' {
                *bytes.last_mut().unwrap() |= 1 << (index % 8);
            }
        }
        bytes
    }

    #[test]
    fn copies_literals_and_overlapping_runs() {
        let stream = pack(
            "0 10100000 \
             0 01010000 \
             1 0 100000 0 \
             1 0 000000 110 00 \
             1 111 11111111111111111111",
        );
        assert_eq!(
            decompress(&stream, 9).unwrap(),
            [5, 10, 5, 10, 10, 10, 10, 10, 10]
        );
    }

    #[test]
    fn rejects_runs_that_reach_before_the_start() {
        let stream = pack("0 10100000 1 0 010000 0 1 111 11111111111111111111");
        assert_eq!(decompress(&stream, 3), None);
    }

    #[test]
    fn rejects_streams_that_end_early_or_overflow() {
        let stream = pack("0 10100000 1 111 11111111111111111111");
        assert_eq!(decompress(&stream, 2), None);
        assert_eq!(decompress(&stream, 0), None);
    }
}
