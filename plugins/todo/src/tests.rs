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
use clippy::{App, Edit, Event, Storage};
use tempfile::TempDir;

use super::*;

const SHOW: &str = "Show To-Dos...";
const QUESTION: &str = "What's the plan?";

fn todo(tasks: &[&str]) -> Harness {
    let mut todo = Harness::new(Todo {
        tasks: tasks.iter().map(|&task| task.to_owned()).collect(),
    });
    todo.choose(SHOW);
    todo
}

#[test]
fn enter_adds_to_the_bottom_and_shift_enter_to_the_top() {
    let mut todo = todo(&[]);
    todo.answer("Buy milk");
    todo.answer("Call the bank");
    todo.shift_answer("Pay the rent");
    assert_eq!(
        todo.asked_tasks(),
        ["Pay the rent", "Buy milk", "Call the bank"]
    );
    assert_eq!(todo.question(), Some(QUESTION));
    assert_eq!(todo.draft(), Some(""));
}

#[test]
fn enter_on_an_empty_field_puts_the_list_away() {
    for blank in ["", "   "] {
        let mut todo = todo(&["Buy milk"]);
        todo.answer(blank);
        assert_eq!(todo.question(), None, "{blank:?}");
        todo.choose(SHOW);
        assert_eq!(todo.asked_tasks(), ["Buy milk"]);
        todo.shift_answer(blank);
        assert_eq!(todo.question(), None, "{blank:?}");
    }
}

#[test]
fn a_ticked_task_is_gone_for_good() {
    let mut todo = todo(&["Buy milk", "Call the bank", "Pay the rent"]);
    todo.tick_asked_task("Call the bank");
    assert_eq!(todo.asked_tasks(), ["Buy milk", "Pay the rent"]);
    assert_eq!(todo.question(), Some(QUESTION));
    todo.cancel();
    todo.choose(SHOW);
    assert_eq!(todo.asked_tasks(), ["Buy milk", "Pay the rent"]);
}

#[test]
fn ticking_keeps_what_you_were_typing() {
    let mut todo = todo(&["Buy milk", "Call the bank"]);
    todo.type_in("Water the pl");
    todo.tick_asked_task("Buy milk");
    assert_eq!(todo.draft(), Some("Water the pl"));
    todo.answer("Water the plants");
    assert_eq!(todo.asked_tasks(), ["Call the bank", "Water the plants"]);
}

#[test]
fn every_ticked_task_is_worth_a_celebration() {
    let mut first = todo(&["Buy milk", "Call the bank"]);
    assert!(!first.celebrates());
    first.tick_asked_task("Buy milk");
    assert!(first.celebrates());
    let mut last = todo(&["Call the bank"]);
    last.tick_asked_task("Call the bank");
    assert!(last.celebrates());
    assert_eq!(last.question(), Some(QUESTION));
    assert_eq!(last.asked_tasks(), Vec::<String>::new());
}

#[test]
fn adding_is_no_cause_for_celebration() {
    let mut todo = todo(&[]);
    todo.answer("Buy milk");
    todo.shift_answer("Pay the rent");
    todo.answer("");
    assert!(!todo.celebrates());
}

#[test]
fn twins_go_one_at_a_time() {
    let mut todo = todo(&["Buy milk", "Buy milk"]);
    todo.tick_asked_task("Buy milk");
    assert_eq!(todo.asked_tasks(), ["Buy milk"]);
}

#[test]
fn cancelling_keeps_the_list() {
    let mut todo = todo(&["Buy milk"]);
    todo.type_in("Call the bank");
    todo.cancel();
    assert_eq!(todo.question(), None);
    todo.choose(SHOW);
    assert_eq!(todo.asked_tasks(), ["Buy milk"]);
}

#[test]
fn the_shortcut_follows_the_settings() {
    let settings = Settings {
        show_shortcut: "Ctrl+Option+T".parse().unwrap(),
        ..Settings::default()
    };
    let mut todo = Harness::with_settings(Todo::default(), settings.clone());
    todo.press(Settings::default().show_shortcut);
    assert_eq!(todo.question(), None);
    todo.press(settings.show_shortcut);
    assert_eq!(todo.question(), Some(QUESTION));
}

#[test]
fn writes_and_reads_its_settings() {
    let written = toml::to_string(&Settings::default()).unwrap();
    assert_eq!(
        written,
        "show_shortcut = \"Cmd+Option+Shift+T\"\nhide_shortcut = \"Cmd+Shift+Delete\"\n"
    );
    assert_eq!(
        toml::from_str::<Settings>(&written).unwrap(),
        Settings::default()
    );
    assert!(toml::from_str::<Settings>(r#"shortcut = "Cmd+T""#).is_err());
    let custom: Settings = toml::from_str(r#"hide_shortcut = "Cmd+F1""#).unwrap();
    assert_eq!(custom.show_shortcut, Settings::default().show_shortcut);
    assert_eq!(custom.hide_shortcut, "Cmd+F1".parse().unwrap());
}

#[test]
fn the_list_outlives_restarts_but_done_tasks_do_not() {
    let directory = TempDir::new().unwrap();
    let launch = || {
        let mut app = App::builder()
            .plugin::<Todo>()
            .load(Storage::new(directory.path()));
        app.handle(Event::Launch, Instant::now());
        app
    };
    let show = |app: &mut App| {
        let command = app
            .menu(Instant::now())
            .into_iter()
            .find_map(|entry| match entry {
                Entry::Submenu { mut entries, .. } => match entries.remove(0) {
                    Entry::Item { command, .. } => Some(command),
                    Entry::Submenu { .. } | Entry::Separator => None,
                },
                Entry::Item { .. } | Entry::Separator => None,
            })
            .unwrap();
        app.handle(Event::Command(command), Instant::now());
    };
    let mut app = launch();
    show(&mut app);
    for (task, submit) in [
        ("Buy milk", Event::Submit),
        ("Call the bank", Event::Submit),
        ("Pay the rent", Event::ShiftSubmit),
    ] {
        app.handle(Event::Edit(Edit::Insert(task.to_owned())), Instant::now());
        app.handle(submit, Instant::now());
    }
    let saved = || fs::read_to_string(directory.path().join("Todo.json")).unwrap();
    let tasks = |json: String| -> Vec<String> {
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        serde_json::from_value(value["tasks"].clone()).unwrap()
    };
    assert_eq!(
        tasks(saved()),
        ["Pay the rent", "Buy milk", "Call the bank"]
    );
    drop(app);
    let mut app = launch();
    show(&mut app);
    app.handle(Event::CheckboxPress { item: 1 }, Instant::now());
    app.handle(Event::CheckboxRelease { inside: true }, Instant::now());
    assert_eq!(tasks(saved()), ["Pay the rent", "Call the bank"]);
}
