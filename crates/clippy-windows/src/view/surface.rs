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

use std::cell::RefCell;
use std::{io, ptr, slice};

use clippy::geometry::Size;
use clippy::graphics::Canvas;
use windows_sys::Win32::Foundation::{HWND, POINT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, HBITMAP, HDC,
    SelectObject,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{ULW_ALPHA, UpdateLayeredWindow};

use crate::win32::size_of_u32;

const BYTES_PER_PIXEL: usize = size_of::<[u8; 4]>();

#[expect(clippy::cast_possible_truncation)]
const BLEND: BLENDFUNCTION = BLENDFUNCTION {
    BlendOp: AC_SRC_OVER as u8,
    BlendFlags: 0,
    SourceConstantAlpha: u8::MAX,
    AlphaFormat: AC_SRC_ALPHA as u8,
};

pub(super) struct Surface {
    window: HWND,
    context: HDC,
    bitmap: RefCell<Option<Bitmap>>,
}

struct Bitmap {
    handle: HBITMAP,
    pixels: *mut u8,
    size: Size,
}

impl Surface {
    pub(super) fn new(window: HWND) -> Self {
        Self {
            window,
            context: unsafe { CreateCompatibleDC(ptr::null_mut()) },
            bitmap: RefCell::new(None),
        }
    }

    pub(super) fn present(&self, canvas: &Canvas, origin: (i32, i32), scale: i32) {
        let size = canvas.size().scaled(scale);
        let mut bitmap = self.bitmap.borrow_mut();
        if bitmap.as_ref().is_none_or(|bitmap| bitmap.size != size) {
            *bitmap = Bitmap::new(self.context, size);
        }
        let Some(bitmap) = bitmap.as_ref() else {
            log::warn!("Could not draw a {:?} canvas", canvas.size());
            return;
        };
        bitmap.paint(canvas, scale);
        let position = POINT {
            x: origin.0,
            y: origin.1,
        };
        let extent = SIZE {
            cx: size.width,
            cy: size.height,
        };
        let corner = POINT { x: 0, y: 0 };
        let updated = unsafe {
            UpdateLayeredWindow(
                self.window,
                ptr::null_mut(),
                &raw const position,
                &raw const extent,
                self.context,
                &raw const corner,
                0,
                &BLEND,
                ULW_ALPHA,
            )
        };
        if updated == 0 {
            log::warn!("Could not show Clippy: {}", io::Error::last_os_error());
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe { DeleteDC(self.context) };
    }
}

impl Bitmap {
    fn new(context: HDC, size: Size) -> Option<Self> {
        if size.width <= 0 || size.height <= 0 {
            return None;
        }
        let information = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of_u32::<BITMAPINFOHEADER>(),
                biWidth: size.width,
                biHeight: -size.height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..BITMAPINFOHEADER::default()
            },
            ..BITMAPINFO::default()
        };
        let mut pixels = ptr::null_mut();
        let handle = unsafe {
            CreateDIBSection(
                context,
                &raw const information,
                DIB_RGB_COLORS,
                &raw mut pixels,
                ptr::null_mut(),
                0,
            )
        };
        if handle.is_null() || pixels.is_null() {
            return None;
        }
        unsafe { SelectObject(context, handle) };
        Some(Self {
            handle,
            pixels: pixels.cast(),
            size,
        })
    }

    fn paint(&self, canvas: &Canvas, scale: i32) {
        let (Ok(width), Ok(height), Ok(scale), Ok(source_width)) = (
            usize::try_from(self.size.width),
            usize::try_from(self.size.height),
            usize::try_from(scale),
            usize::try_from(canvas.size().width),
        ) else {
            return;
        };
        let pixels =
            unsafe { slice::from_raw_parts_mut(self.pixels, width * height * BYTES_PER_PIXEL) };
        let source = canvas.rgba().as_chunks::<BYTES_PER_PIXEL>().0;
        for (y, row) in pixels.chunks_exact_mut(width * BYTES_PER_PIXEL).enumerate() {
            let line = &source[y / scale * source_width..][..source_width];
            for (x, pixel) in row
                .as_chunks_mut::<BYTES_PER_PIXEL>()
                .0
                .iter_mut()
                .enumerate()
            {
                let [red, green, blue, alpha] = line[x / scale];
                pixel.copy_from_slice(&[blue, green, red, alpha]);
            }
        }
    }
}

impl Drop for Bitmap {
    fn drop(&mut self) {
        unsafe { DeleteObject(self.handle) };
    }
}
