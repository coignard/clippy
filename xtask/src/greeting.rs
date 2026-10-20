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

use std::convert::Infallible;
use std::fs::{self, File};
use std::io::BufWriter;
use std::num::NonZeroU8;
use std::path::Path;
use std::time::{Duration, Instant};

use clippy::geometry::Rect;
use clippy::graphics::Canvas;
use clippy::{App, Context, Event, Message, NoSettings, Plugin, Scene, Settings, Storage};
use serde::{Deserialize, Serialize};
use tempfile::TempDir;

use crate::Failure;

const SCREEN: Rect = Rect::new(0, 0, 1280, 800);
const SHORTEST_FRAME: Duration = Duration::from_millis(10);
const LONGEST_ARRIVAL: Duration = Duration::from_secs(30);
const MANIFEST: &str = "frames.json";
const BICYCLE_SEED: u64 = 2;

#[derive(Default, Serialize, Deserialize)]
struct Greeting {
    text: String,
}

impl Plugin for Greeting {
    const NAME: &'static str = "Greeting";

    type Action = Infallible;

    type Settings = NoSettings;

    fn message(&self) -> Option<Message<Infallible>> {
        Some(Message::new(&self.text))
    }

    fn perform(&mut self, action: Infallible, _cx: &mut Context<'_, Self>) {
        match action {}
    }
}

struct Shot {
    layers: Vec<(Rect, Canvas)>,
    duration: Duration,
}

#[derive(Serialize)]
struct Frame {
    image: String,
    milliseconds: u64,
}

pub(crate) fn greeting(
    text: String,
    scale: NonZeroU8,
    hold: Duration,
    out: &Path,
) -> Result<(), Failure> {
    let shots = film(text, scale, hold)?;
    let area = shots
        .iter()
        .flat_map(|shot| &shot.layers)
        .map(|&(frame, _)| frame)
        .reduce(union)
        .ok_or("Clippy should appear on the screen")?;
    let mut frames: Vec<(Vec<u8>, Duration)> = Vec::new();
    for shot in shots {
        let pixels = compose(&shot.layers, area)?;
        match frames.last_mut() {
            Some((last, duration)) if *last == pixels => *duration += shot.duration,
            _ => frames.push((pixels, shot.duration)),
        }
    }
    fs::create_dir_all(out)?;
    let mut manifest = Vec::new();
    for (index, (pixels, duration)) in frames.iter().enumerate() {
        let image = format!("{index:04}.png");
        save(&out.join(&image), area, pixels)?;
        manifest.push(Frame {
            image,
            milliseconds: u64::try_from(duration.as_millis())?,
        });
    }
    serde_json::to_writer_pretty(File::create(out.join(MANIFEST))?, &manifest)?;
    Ok(())
}

fn film(text: String, scale: NonZeroU8, hold: Duration) -> Result<Vec<Shot>, Failure> {
    let storage = TempDir::new()?;
    serde_json::to_writer(
        File::create(storage.path().join(format!("{}.json", Greeting::NAME)))?,
        &Greeting { text },
    )?;
    let settings = Settings {
        scale,
        sounds: false,
        plugins: Some(vec![Greeting::NAME.to_owned()]),
        ..Settings::default()
    };
    fastrand::seed(BICYCLE_SEED);
    let mut app = App::builder()
        .plugin::<Greeting>()
        .build(settings, Storage::new(storage.path()));
    let clippy = app.home(SCREEN);
    app.place(clippy, SCREEN);
    let mut now = Instant::now();
    let mut end = now + LONGEST_ARRIVAL;
    let mut greeted = false;
    let mut shots = Vec::new();
    app.handle(Event::Launch, now);
    loop {
        let scene = app.scene();
        if !greeted && scene.balloon.is_some() {
            greeted = true;
            end = now + hold;
        }
        let next = app
            .deadline()
            .map_or(end, |deadline| deadline.min(end))
            .max(now + SHORTEST_FRAME);
        shots.push(Shot {
            layers: layers(scene, clippy),
            duration: next - now,
        });
        if next >= end {
            break;
        }
        now = next;
        app.handle(Event::Tick, now);
    }
    if greeted {
        Ok(shots)
    } else {
        Err(format!(
            "Clippy did not greet within {} seconds",
            LONGEST_ARRIVAL.as_secs()
        )
        .into())
    }
}

fn layers(scene: Scene, clippy: Rect) -> Vec<(Rect, Canvas)> {
    scene
        .clippy
        .map(|canvas| (clippy, canvas))
        .into_iter()
        .chain(scene.balloon.map(|balloon| (balloon.frame, balloon.canvas)))
        .collect()
}

fn union(one: Rect, other: Rect) -> Rect {
    let (x, y) = (one.x.min(other.x), one.y.min(other.y));
    Rect::new(
        x,
        y,
        one.right().max(other.right()) - x,
        one.bottom().max(other.bottom()) - y,
    )
}

fn compose(layers: &[(Rect, Canvas)], area: Rect) -> Result<Vec<u8>, Failure> {
    let stride = usize::try_from(area.width)?;
    let mut pixels = vec![0; stride * usize::try_from(area.height)? * 4];
    for (frame, canvas) in layers {
        let width = usize::try_from(frame.width)?;
        let left = usize::try_from(frame.x - area.x)?;
        let top = usize::try_from(frame.y - area.y)?;
        for (index, pixel) in canvas.rgba().as_chunks::<4>().0.iter().enumerate() {
            if pixel[3] != 0 {
                let at = ((top + index / width) * stride + left + index % width) * 4;
                pixels[at..at + 4].copy_from_slice(pixel);
            }
        }
    }
    Ok(pixels)
}

fn save(path: &Path, area: Rect, pixels: &[u8]) -> Result<(), Failure> {
    let mut encoder = png::Encoder::new(
        BufWriter::new(File::create(path)?),
        u32::try_from(area.width)?,
        u32::try_from(area.height)?,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(pixels)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn films_clippy_riding_in_and_then_greeting() {
        let hold = Duration::from_secs(2);
        let shots = film("Hallo!".to_owned(), NonZeroU8::MIN, hold).unwrap();
        let (arrival, greeting) = shots.split_at(
            shots
                .iter()
                .position(|shot| shot.layers.len() == 2)
                .unwrap(),
        );
        assert!(arrival.len() >= 30);
        assert!(arrival.iter().all(|shot| shot.layers.len() <= 1));
        assert!(greeting.iter().all(|shot| shot.layers.len() == 2));
        assert_eq!(
            greeting.iter().map(|shot| shot.duration).sum::<Duration>(),
            hold
        );
    }

    #[test]
    fn films_the_same_ride_every_time() {
        let film = || film("Hallo!".to_owned(), NonZeroU8::MIN, Duration::from_secs(1));
        let (one, other) = (film().unwrap(), film().unwrap());
        assert_eq!(one.len(), other.len());
        assert!(
            one.iter().zip(&other).all(|(one, other)| {
                one.duration == other.duration && one.layers == other.layers
            })
        );
    }

    #[test]
    fn writes_frames_with_their_durations() {
        let out = TempDir::new().unwrap();
        greeting(
            "Дратути!".to_owned(),
            NonZeroU8::MIN,
            Duration::from_secs(1),
            out.path(),
        )
        .unwrap();
        let manifest: Vec<serde_json::Value> =
            serde_json::from_reader(File::open(out.path().join(MANIFEST)).unwrap()).unwrap();
        assert!(manifest.len() > 10);
        for frame in &manifest {
            assert!(out.path().join(frame["image"].as_str().unwrap()).exists());
            assert!(frame["milliseconds"].as_u64().unwrap() >= 10);
        }
    }
}
