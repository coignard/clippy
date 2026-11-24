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
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const ICON_SIZES: [i32; 9] = [16, 20, 24, 32, 40, 48, 64, 128, 256];
const ICON: u16 = 3;
const ICON_GROUP: u16 = 14;
const VERSION: u16 = 16;
const MANIFEST: u16 = 24;
const ENGLISH: u16 = 0x0409;
const UNICODE: u16 = 0x04B0;
const HEADER_SIZE: u32 = 32;
const MOVEABLE_PURE: u16 = 0x0030;
const NAME: &str = "Clippy";
const COPYRIGHT_YEAR: u16 = 2026;
const FIXED_FILE_INFO_SIGNATURE: u32 = 0xFEEF_04BD;
const FIXED_FILE_INFO_VERSION: u32 = 0x0001_0000;
const FILE_FLAGS_MASK: u32 = 0x3F;
const WINDOWS_NT: u32 = 0x0004_0004;
const APPLICATION: u32 = 1;
const APPLICATION_MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2, PerMonitor</dpiAwareness>
    </windowsSettings>
  </application>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
"#;

enum Value<'a> {
    Nothing,
    Bytes(&'a [u8]),
    Text(&'a str),
}

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=WINDRES");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo should give the build a place"));
    let compiled = out.join("Clippy.res");
    fs::write(&compiled, resources()).expect("the build should write Clippy's resources");
    let links_resources = env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
        || env::var("CARGO_CFG_TARGET_ABI").as_deref() == Ok("llvm");
    let linked = if links_resources {
        Some(compiled)
    } else {
        object(&compiled, &out)
    };
    match linked {
        Some(path) => println!("cargo::rustc-link-arg-bins={}", path.display()),
        None => println!(
            "cargo::warning=Clippy goes without his icon: set WINDRES to a windres that converts resources for this linker"
        ),
    }
}

fn object(compiled: &Path, out: &Path) -> Option<PathBuf> {
    let object = out.join("Clippy.res.o");
    let architecture = env::var("CARGO_CFG_TARGET_ARCH").ok()?;
    let candidates = [
        env::var("WINDRES").ok(),
        Some(format!("{architecture}-w64-mingw32-windres")),
        Some("windres".to_owned()),
    ];
    candidates
        .into_iter()
        .flatten()
        .any(|windres| {
            Command::new(windres)
                .args(["--input-format=res", "--output-format=coff", "--input"])
                .arg(compiled)
                .arg("--output")
                .arg(&object)
                .status()
                .is_ok_and(|status| status.success())
        })
        .then_some(object)
}

fn resources() -> Vec<u8> {
    let mut resources = Vec::new();
    add(&mut resources, 0, 0, 0, &[]);
    let mut group = Vec::new();
    for half in [
        0,
        1,
        u16::try_from(ICON_SIZES.len()).expect("icons should be few"),
    ] {
        group.extend(half.to_le_bytes());
    }
    for (id, size) in (1..).zip(ICON_SIZES) {
        let image = png(size);
        let side = u8::try_from(size).unwrap_or(0);
        group.extend([side, side, 0, 0]);
        group.extend(1_u16.to_le_bytes());
        group.extend(32_u16.to_le_bytes());
        group.extend(length(&image).to_le_bytes());
        group.extend(u16::to_le_bytes(id));
        add(&mut resources, ICON, id, ENGLISH, &image);
    }
    add(&mut resources, ICON_GROUP, 1, ENGLISH, &group);
    add(&mut resources, VERSION, 1, ENGLISH, &version());
    add(
        &mut resources,
        MANIFEST,
        1,
        ENGLISH,
        APPLICATION_MANIFEST.as_bytes(),
    );
    resources
}

fn add(resources: &mut Vec<u8>, kind: u16, id: u16, language: u16, data: &[u8]) {
    let flags = if kind == 0 { 0 } else { MOVEABLE_PURE };
    resources.extend(length(data).to_le_bytes());
    resources.extend(HEADER_SIZE.to_le_bytes());
    for half in [u16::MAX, kind, u16::MAX, id] {
        resources.extend(half.to_le_bytes());
    }
    resources.extend(0_u32.to_le_bytes());
    resources.extend(flags.to_le_bytes());
    resources.extend(language.to_le_bytes());
    resources.extend([0; 8]);
    resources.extend(data);
    align(resources);
}

fn png(size: i32) -> Vec<u8> {
    let icon = clippy::icon(size);
    let width = u32::try_from(size).expect("icon sizes should be positive");
    let mut image = Vec::new();
    let mut encoder = png::Encoder::new(&mut image, width, width);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(icon.rgba()))
        .expect("Clippy's icon should encode as a PNG");
    image
}

fn version() -> Vec<u8> {
    let part = |name: &str| -> u32 {
        env::var(name)
            .ok()
            .and_then(|part| part.parse().ok())
            .unwrap_or_default()
    };
    let (major, minor, patch) = (
        part("CARGO_PKG_VERSION_MAJOR"),
        part("CARGO_PKG_VERSION_MINOR"),
        part("CARGO_PKG_VERSION_PATCH"),
    );
    let mut fixed = Vec::new();
    for word in [
        FIXED_FILE_INFO_SIGNATURE,
        FIXED_FILE_INFO_VERSION,
        major << 16 | minor,
        patch << 16,
        major << 16 | minor,
        patch << 16,
        FILE_FLAGS_MASK,
        0,
        WINDOWS_NT,
        APPLICATION,
        0,
        0,
        0,
    ] {
        fixed.extend(word.to_le_bytes());
    }
    let version = env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let authors = env::var("CARGO_PKG_AUTHORS").unwrap_or_default();
    let author = authors
        .split(':')
        .next()
        .and_then(|author| author.split(" <").next())
        .unwrap_or_default();
    let copyright = format!("© {COPYRIGHT_YEAR} {author}");
    let executable = format!("{NAME}.exe");
    let strings: Vec<Vec<u8>> = [
        ("FileDescription", NAME),
        ("FileVersion", version.as_str()),
        ("InternalName", NAME),
        ("LegalCopyright", copyright.as_str()),
        ("OriginalFilename", executable.as_str()),
        ("ProductName", NAME),
        ("ProductVersion", version.as_str()),
    ]
    .into_iter()
    .map(|(key, text)| node(key, &Value::Text(text), &[]))
    .collect();
    let table = format!("{ENGLISH:04X}{UNICODE:04X}");
    let translation = [ENGLISH.to_le_bytes(), UNICODE.to_le_bytes()].concat();
    node(
        "VS_VERSION_INFO",
        &Value::Bytes(&fixed),
        &[
            node(
                "StringFileInfo",
                &Value::Nothing,
                &[node(&table, &Value::Nothing, &strings)],
            ),
            node(
                "VarFileInfo",
                &Value::Nothing,
                &[node("Translation", &Value::Bytes(&translation), &[])],
            ),
        ],
    )
}

fn node(key: &str, value: &Value<'_>, children: &[Vec<u8>]) -> Vec<u8> {
    let mut node = vec![0; 6];
    node.extend(wide(key));
    align(&mut node);
    let (value_length, text) = match value {
        Value::Nothing => (0, true),
        Value::Bytes(bytes) => {
            node.extend(*bytes);
            (bytes.len(), false)
        }
        Value::Text(text) => {
            let text = wide(text);
            node.extend(&text);
            (text.len() / size_of::<u16>(), true)
        }
    };
    for child in children {
        align(&mut node);
        node.extend(child);
    }
    let length = u16::try_from(node.len()).expect("version information should be small");
    let value_length = u16::try_from(value_length).expect("version values should be small");
    node[..2].copy_from_slice(&length.to_le_bytes());
    node[2..4].copy_from_slice(&value_length.to_le_bytes());
    node[4..6].copy_from_slice(&u16::from(text).to_le_bytes());
    node
}

fn wide(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect()
}

fn align(bytes: &mut Vec<u8>) {
    bytes.resize(bytes.len().next_multiple_of(4), 0);
}

fn length(data: &[u8]) -> u32 {
    u32::try_from(data.len()).expect("resources should be small")
}
