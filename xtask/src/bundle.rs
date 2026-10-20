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

use std::env;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use cargo_metadata::{Message, MetadataCommand};
use clippy::graphics::Canvas;
use icns::{IconFamily, IconType, Image, PixelFormat};
use serde::Serialize;

use crate::Failure;

const NAME: &str = "Clippy";
const EXECUTABLE: &str = "clippy-macos";
const IDENTIFIER: &str = "com.renecoignard.clippy";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const MINIMUM_SYSTEM: &str = "11.0";
const ARCHITECTURES: [&str; 2] = ["aarch64-apple-darwin", "x86_64-apple-darwin"];
const UNIVERSAL: &str = "universal-apple-darwin";
const ICONS: [IconType; 10] = [
    IconType::RGBA32_16x16,
    IconType::RGBA32_16x16_2x,
    IconType::RGBA32_32x32,
    IconType::RGBA32_32x32_2x,
    IconType::RGBA32_128x128,
    IconType::RGBA32_128x128_2x,
    IconType::RGBA32_256x256,
    IconType::RGBA32_256x256_2x,
    IconType::RGBA32_512x512,
    IconType::RGBA32_512x512_2x,
];

struct Bounds {
    left: u32,
    top: u32,
    width: u32,
    height: u32,
}

#[derive(Serialize)]
struct Information {
    #[serde(rename = "CFBundleDevelopmentRegion")]
    development_region: &'static str,
    #[serde(rename = "CFBundleDisplayName")]
    display_name: &'static str,
    #[serde(rename = "CFBundleExecutable")]
    executable: &'static str,
    #[serde(rename = "CFBundleIconFile")]
    icon_file: &'static str,
    #[serde(rename = "CFBundleIdentifier")]
    identifier: &'static str,
    #[serde(rename = "CFBundleInfoDictionaryVersion")]
    dictionary_version: &'static str,
    #[serde(rename = "CFBundleName")]
    name: &'static str,
    #[serde(rename = "CFBundlePackageType")]
    package_type: &'static str,
    #[serde(rename = "CFBundleShortVersionString")]
    short_version: &'static str,
    #[serde(rename = "CFBundleVersion")]
    version: &'static str,
    #[serde(rename = "LSMinimumSystemVersion")]
    minimum_system: &'static str,
    #[serde(rename = "LSUIElement")]
    agent: bool,
    #[serde(rename = "NSHighResolutionCapable")]
    high_resolution: bool,
}

pub(crate) fn bundle(universal: bool) -> Result<(), Failure> {
    let executable = if universal {
        build_universal()?
    } else {
        build(None)?
    };
    let app = executable.with_file_name(format!("{NAME}.app"));
    assemble(&app, &executable)?;
    run(
        Command::new("codesign")
            .args(["--force", "--sign", "-"])
            .arg(&app),
        "codesign could not sign the application",
    )?;
    println!("{}", app.display());
    Ok(())
}

fn build_universal() -> Result<PathBuf, Failure> {
    let slices = ARCHITECTURES
        .into_iter()
        .map(|target| build(Some(target)))
        .collect::<Result<Vec<_>, _>>()?;
    let directory = MetadataCommand::new()
        .current_dir(workspace()?)
        .no_deps()
        .exec()?
        .target_directory
        .join(UNIVERSAL)
        .join("release");
    fs::create_dir_all(&directory)?;
    let executable = directory.join(EXECUTABLE).into_std_path_buf();
    run(
        Command::new("lipo")
            .arg("-create")
            .args(&slices)
            .arg("-output")
            .arg(&executable),
        "lipo could not make a universal executable",
    )?;
    Ok(executable)
}

fn build(target: Option<&str>) -> Result<PathBuf, Failure> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut build = Command::new(cargo)
        .current_dir(workspace()?)
        .args(["build", "--release", "--locked", "--package", EXECUTABLE])
        .args(target.into_iter().flat_map(|target| ["--target", target]))
        .arg("--message-format=json-render-diagnostics")
        .stdout(Stdio::piped())
        .spawn()?;
    let output = build
        .stdout
        .take()
        .ok_or("cargo should report what it builds")?;
    let mut executable = None;
    for message in Message::parse_stream(BufReader::new(output)) {
        if let Message::CompilerArtifact(artifact) = message?
            && artifact.target.name == EXECUTABLE
        {
            executable = artifact.executable.or(executable);
        }
    }
    if !build.wait()?.success() {
        return Err("the release build failed".into());
    }
    let executable = executable.ok_or("cargo did not report the Clippy executable")?;
    Ok(executable.into_std_path_buf())
}

fn workspace() -> Result<&'static Path, Failure> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask should live inside the workspace".into())
}

fn run(command: &mut Command, failure: &str) -> Result<(), Failure> {
    if command.status()?.success() {
        Ok(())
    } else {
        Err(failure.into())
    }
}

fn assemble(app: &Path, executable: &Path) -> Result<(), Failure> {
    if app.exists() {
        fs::remove_dir_all(app)?;
    }
    let contents = app.join("Contents");
    let binaries = contents.join("MacOS");
    let resources = contents.join("Resources");
    fs::create_dir_all(&binaries)?;
    fs::create_dir_all(&resources)?;
    fs::copy(executable, binaries.join(NAME))?;
    plist::to_file_xml(contents.join("Info.plist"), &information())?;
    icon()?.write(File::create(resources.join(format!("{NAME}.icns")))?)?;
    Ok(())
}

fn information() -> Information {
    Information {
        development_region: "en",
        display_name: NAME,
        executable: NAME,
        icon_file: NAME,
        identifier: IDENTIFIER,
        dictionary_version: "6.0",
        name: NAME,
        package_type: "APPL",
        short_version: VERSION,
        version: VERSION,
        minimum_system: MINIMUM_SYSTEM,
        agent: true,
        high_resolution: true,
    }
}

fn icon() -> Result<IconFamily, Failure> {
    let portrait = clippy::portrait();
    let bounds = opaque_bounds(&portrait)?;
    let mut family = IconFamily::new();
    for kind in ICONS {
        let size = kind.pixel_width();
        let pixels = square(&portrait, &bounds, size)?;
        family.add_icon_with_type(
            &Image::from_data(PixelFormat::RGBA, size, size, pixels)?,
            kind,
        )?;
    }
    Ok(family)
}

fn opaque_bounds(canvas: &Canvas) -> Result<Bounds, Failure> {
    let width = u32::try_from(canvas.size().width)?;
    let (left, top, right, bottom) = (0..)
        .zip(canvas.rgba().as_chunks::<4>().0)
        .filter(|(_, pixel)| pixel[3] != 0)
        .map(|(index, _)| (index % width, index / width))
        .fold(None, |edges: Option<(u32, u32, u32, u32)>, (x, y)| {
            Some(edges.map_or((x, y, x, y), |(left, top, right, bottom)| {
                (left.min(x), top.min(y), right.max(x), bottom.max(y))
            }))
        })
        .ok_or("Clippy should be visible at rest")?;
    Ok(Bounds {
        left,
        top,
        width: right - left + 1,
        height: bottom - top + 1,
    })
}

fn square(canvas: &Canvas, bounds: &Bounds, size: u32) -> Result<Vec<u8>, Failure> {
    let room = size * 7 / 8;
    let longest = bounds.width.max(bounds.height);
    let (width, height) = match room / longest {
        0 => (
            bounds.width * room / longest,
            bounds.height * room / longest,
        ),
        factor => (bounds.width * factor, bounds.height * factor),
    };
    let (left, top) = ((size - width) / 2, (size - height) / 2);
    let stride = u32::try_from(canvas.size().width)?;
    let source = canvas.rgba();
    let mut pixels = vec![0; usize::try_from(size * size * 4)?];
    for y in top..top + height {
        for x in left..left + width {
            let from_x = bounds.left + (x - left) * bounds.width / width;
            let from_y = bounds.top + (y - top) * bounds.height / height;
            let from = usize::try_from((from_y * stride + from_x) * 4)?;
            let to = usize::try_from((y * size + x) * 4)?;
            pixels[to..to + 4].copy_from_slice(&source[from..from + 4]);
        }
    }
    Ok(pixels)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use plist::Value;

    use super::*;

    #[test]
    fn assembles_an_application_over_an_old_one() {
        let directory = tempfile::TempDir::new().unwrap();
        let executable = directory.path().join(EXECUTABLE);
        fs::write(&executable, b"binary").unwrap();
        let app = directory.path().join("Clippy.app");
        fs::create_dir_all(app.join("Contents/Stale")).unwrap();
        assemble(&app, &executable).unwrap();
        let contents = app.join("Contents");
        assert!(!contents.join("Stale").exists());
        assert_eq!(fs::read(contents.join("MacOS/Clippy")).unwrap(), b"binary");
        let information = Value::from_file(contents.join("Info.plist")).unwrap();
        let information = information.as_dictionary().unwrap();
        assert_eq!(information.get("LSUIElement"), Some(&Value::Boolean(true)));
        assert_eq!(
            information
                .get("CFBundleIdentifier")
                .and_then(Value::as_string),
            Some("com.renecoignard.clippy")
        );
        assert_eq!(
            information
                .get("CFBundleShortVersionString")
                .and_then(Value::as_string),
            Some(VERSION)
        );
    }

    #[test]
    fn icon_holds_clippy_at_every_size() {
        let mut icns = Vec::new();
        icon().unwrap().write(&mut icns).unwrap();
        let family = IconFamily::read(Cursor::new(icns)).unwrap();
        for kind in ICONS {
            let image = family.get_icon_with_type(kind).unwrap();
            assert_eq!(
                (image.width(), image.height()),
                (kind.pixel_width(), kind.pixel_height())
            );
            assert!(
                image
                    .data()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|pixel| pixel[3] == 255)
            );
        }
    }
}
