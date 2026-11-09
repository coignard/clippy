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

use std::fs;
use std::time::Instant;

use clippy::menu::Entry;
use clippy::testing::Harness;
use clippy::{App, Command, Edit, Effect, Event, Storage};
use serde_json::json;
use tempfile::TempDir;

use super::*;

const ADD: &str = "Add to Inbox...";
const EXPORT: &str = "Export to Clipboard";
const QUESTION: &str = "What's on your mind?";

fn launch(directory: &TempDir) -> App {
    let mut app = App::builder()
        .plugin::<Inbox>()
        .load(Storage::new(directory.path()));
    app.handle(Event::Launch, Instant::now());
    app
}

fn item(app: &App, title: &str) -> (Command, bool) {
    app.menu(Instant::now())
        .into_iter()
        .find_map(|entry| match entry {
            Entry::Submenu { entries, .. } => entries.into_iter().find_map(|entry| match entry {
                Entry::Item {
                    title: item,
                    command,
                    enabled,
                    ..
                } if item == title => Some((command, enabled)),
                _ => None,
            }),
            Entry::Item { .. } | Entry::Separator => None,
        })
        .unwrap_or_else(|| panic!("the menu should offer {title:?}"))
}

fn add(app: &mut App, text: &str) {
    let now = Instant::now();
    let (command, _) = item(app, ADD);
    app.handle(Event::Command(command), now);
    app.handle(Event::Edit(Edit::Insert(text.to_owned())), now);
    app.handle(Event::Submit, now);
}

fn export(app: &mut App) -> Vec<String> {
    let (command, enabled) = item(app, EXPORT);
    assert!(enabled, "there should be something to export");
    app.take_effects();
    app.handle(Event::Command(command), Instant::now());
    app.take_effects()
        .into_iter()
        .filter_map(|effect| match effect {
            Effect::Copy(text) => Some(text),
            Effect::Play(_) => None,
        })
        .collect()
}

fn read(directory: &TempDir) -> Value {
    let json = fs::read_to_string(directory.path().join("Inbox.json")).unwrap();
    serde_json::from_str(&json).unwrap()
}

fn texts(inbox: &Value) -> Vec<&str> {
    inbox["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["text"].as_str().unwrap())
        .collect()
}

#[test]
fn asks_and_keeps_the_answer_out_of_sight() {
    let mut inbox = Harness::new(Inbox::default());
    assert_eq!(
        inbox.menu(),
        [(ADD.to_owned(), true), (EXPORT.to_owned(), false)]
    );
    inbox.choose(ADD);
    assert_eq!(inbox.question(), Some(QUESTION));
    assert_eq!(inbox.draft(), Some(""));
    inbox.answer("Call the bank");
    assert_eq!(inbox.question(), None);
    assert_eq!(inbox.message(), None);
}

#[test]
fn the_shortcut_asks_too() {
    let mut inbox = Harness::new(Inbox::default());
    inbox.press("Cmd+Option+Shift+Enter".parse().unwrap());
    assert_eq!(inbox.question(), Some(QUESTION));
}

#[test]
fn files_answers_in_inbox_json() {
    let directory = TempDir::new().unwrap();
    let mut app = launch(&directory);
    add(&mut app, "Call the bank");
    add(&mut app, "  ");
    add(&mut app, "Buy milk");
    let inbox = read(&directory);
    assert_eq!(texts(&inbox), ["Call the bank", "Buy milk"]);
    for item in inbox["items"].as_array().unwrap() {
        let added = item["added"].as_str().unwrap();
        assert!(
            added.parse::<DateTime>().is_ok() && !added.contains('.'),
            "{added}"
        );
    }
}

#[test]
fn keeps_what_another_program_did_with_the_inbox() {
    let directory = TempDir::new().unwrap();
    let mut app = launch(&directory);
    add(&mut app, "Call the bank");
    let outside = json!({
        "items": [{ "text": "Buy milk", "done": true }],
        "read_until": 3,
    });
    fs::write(directory.path().join("Inbox.json"), outside.to_string()).unwrap();
    add(&mut app, "Water the plants");
    let inbox = read(&directory);
    assert_eq!(texts(&inbox), ["Buy milk", "Water the plants"]);
    assert_eq!(inbox["items"][0]["done"], json!(true));
    assert_eq!(inbox["read_until"], json!(3));
}

#[test]
fn writes_and_reads_its_settings() {
    let written = toml::to_string(&Settings::default()).unwrap();
    assert_eq!(written, "add_shortcut = \"Cmd+Option+Shift+Enter\"\n");
    assert_eq!(
        toml::from_str::<Settings>(&written).unwrap(),
        Settings::default()
    );
    let custom: Settings = toml::from_str(r#"add_shortcut = "Ctrl+Option+I""#).unwrap();
    assert_eq!(custom.add_shortcut, "Ctrl+Option+I".parse().unwrap());
    assert!(toml::from_str::<Settings>(r#"shortcut = "Cmd+I""#).is_err());
}

#[test]
fn exporting_copies_everything_and_empties_the_inbox() {
    let directory = TempDir::new().unwrap();
    let mut app = launch(&directory);
    add(&mut app, "Call the bank");
    add(&mut app, "Buy milk");
    assert_eq!(export(&mut app), ["- Call the bank\n- Buy milk"]);
    let inbox = read(&directory);
    assert_eq!(texts(&inbox), Vec::<&str>::new());
    assert!(!item(&app, EXPORT).1);
    add(&mut app, "Water the plants");
    assert_eq!(export(&mut app), ["- Water the plants"]);
}

#[test]
fn exporting_keeps_the_last_export_in_case_the_clipboard_lost_it() {
    let directory = TempDir::new().unwrap();
    let mut app = launch(&directory);
    add(&mut app, "Call the bank");
    export(&mut app);
    let last = |inbox: &Value| -> Vec<String> {
        inbox["last_export"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["text"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(last(&read(&directory)), ["Call the bank"]);
    add(&mut app, "Buy milk");
    assert_eq!(last(&read(&directory)), ["Call the bank"]);
    export(&mut app);
    assert_eq!(last(&read(&directory)), ["Buy milk"]);
}

#[test]
fn exports_what_another_program_filed_too() {
    let directory = TempDir::new().unwrap();
    let mut app = launch(&directory);
    add(&mut app, "Call the bank");
    let outside = json!({
        "items": [{ "text": "Call the bank" }, { "text": "Buy milk", "from": "Shortcuts" }],
    });
    fs::write(directory.path().join("Inbox.json"), outside.to_string()).unwrap();
    assert_eq!(export(&mut app), ["- Call the bank\n- Buy milk"]);
    let inbox = read(&directory);
    assert_eq!(inbox["last_export"][1]["from"], json!("Shortcuts"));
}

#[test]
fn an_unreadable_inbox_is_set_aside_not_overwritten() {
    let directory = TempDir::new().unwrap();
    let mut app = launch(&directory);
    add(&mut app, "Call the bank");
    let broken = r#"{ "items": [{ "text": "Buy milk" }"#;
    fs::write(directory.path().join("Inbox.json"), broken).unwrap();
    add(&mut app, "Water the plants");
    assert_eq!(
        texts(&read(&directory)),
        ["Call the bank", "Water the plants"]
    );
    let aside = fs::read_to_string(directory.path().join("Inbox (unreadable).json")).unwrap();
    assert_eq!(aside, broken);
    drop(app);
    fs::write(directory.path().join("Inbox.json"), broken).unwrap();
    let mut app = launch(&directory);
    add(&mut app, "Buy bread");
    let aside = fs::read_to_string(directory.path().join("Inbox (unreadable 2).json")).unwrap();
    assert_eq!(aside, broken);
}
