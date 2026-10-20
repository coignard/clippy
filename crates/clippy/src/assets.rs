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

use std::sync::LazyLock;

use crate::acs;
use crate::animation::Library;
use crate::graphics::{BLACK, Font, Image, NineSlice, WHITE};

const FONT_SIZE: u16 = 11;
const BALLOON_BORDER: i32 = 6;

pub(crate) static CLIPPIT: LazyLock<acs::Character> = LazyLock::new(|| {
    acs::Character::parse(include_bytes!("../assets/clippit.acs"))
        .expect("the embedded character should be a valid Microsoft Agent character")
});

pub(crate) static ANIMATIONS: LazyLock<Library> = LazyLock::new(|| {
    Library::new(&CLIPPIT).expect("the embedded character should have all of Clippy's animations")
});

pub(crate) static FONT: LazyLock<Font> = LazyLock::new(|| {
    Font::parse(
        include_bytes!("../assets/fonts/ms-sans-serif.otb"),
        FONT_SIZE,
    )
    .expect("embedded font should have an 11 px bitmap strike")
});

pub(crate) static BALLOON_FRAME: LazyLock<NineSlice> = LazyLock::new(|| {
    NineSlice::new(
        balloon(include_bytes!("../assets/balloon/frame.png")),
        BALLOON_BORDER,
    )
});

pub(crate) static BALLOON_TIP: LazyLock<Image> =
    LazyLock::new(|| balloon(include_bytes!("../assets/balloon/tip.png")));

fn balloon(png: &[u8]) -> Image {
    let colors = CLIPPIT.balloon;
    image(png).recolored(|color| match color {
        WHITE => colors.fill,
        BLACK => colors.border,
        color => color,
    })
}

fn image(png: &[u8]) -> Image {
    Image::decode(png).expect("embedded balloon images should be 8-bit PNGs")
}
