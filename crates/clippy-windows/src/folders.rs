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

use std::ffi::OsString;
use std::fs::{self, File};
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::{ptr, slice};

use windows_sys::Win32::Foundation::S_OK;
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::UI::Shell::{
    FOLDERID_LocalAppData, FOLDERID_RoamingAppData, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
};
use windows_sys::core::GUID;

pub(crate) fn data_directory() -> PathBuf {
    known_folder(&FOLDERID_RoamingAppData)
        .expect("Windows should provide a roaming AppData folder")
        .join("Clippy")
}

pub(crate) fn log_file() -> Option<File> {
    let logs = known_folder(&FOLDERID_LocalAppData)?.join("Clippy");
    fs::create_dir_all(&logs).ok()?;
    File::options()
        .create(true)
        .append(true)
        .open(logs.join("Clippy.log"))
        .ok()
}

fn known_folder(folder: &GUID) -> Option<PathBuf> {
    let mut path = ptr::null_mut();
    let found = unsafe {
        SHGetKnownFolderPath(
            folder,
            KF_FLAG_DEFAULT.cast_unsigned(),
            ptr::null_mut(),
            &raw mut path,
        )
    } == S_OK;
    let folder = found.then(|| {
        let mut length = 0;
        while unsafe { *path.add(length) } != 0 {
            length += 1;
        }
        OsString::from_wide(unsafe { slice::from_raw_parts(path, length) })
    });
    unsafe { CoTaskMemFree(path.cast()) };
    folder.map(PathBuf::from)
}
