use std::mem;
use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    Insert(String),
    Paste(String),
    Move(Motion),
    Select(Motion),
    Delete(Motion),
    SelectAll,
    Copy,
    Cut,
    Undo,
    Redo,
    Replace { range: Utf16Range, text: String },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Utf16Range {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Backward,
    Forward,
    WordBackward,
    WordForward,
    Start,
    End,
}

#[derive(Debug, Default)]
pub(crate) struct TextField {
    state: State,
    undo: Vec<State>,
    redo: Vec<State>,
    run: Option<Run>,
    revision: usize,
}

#[derive(Clone, Debug, Default)]
struct State {
    text: String,
    anchor: usize,
    caret: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Run {
    Typing,
    Deleting,
}

impl TextField {
    pub(crate) fn new(text: String) -> Self {
        let end = text.len();
        Self {
            state: State {
                text,
                anchor: end,
                caret: end,
            },
            ..Self::default()
        }
    }

    pub(crate) fn text(&self) -> &str {
        &self.state.text
    }

    pub(crate) fn caret(&self) -> usize {
        self.state.caret
    }

    pub(crate) fn revision(&self) -> usize {
        self.revision
    }

    pub(crate) fn selection(&self) -> Range<usize> {
        let State { anchor, caret, .. } = self.state;
        anchor.min(caret)..anchor.max(caret)
    }

    pub(crate) fn selection_in_utf16(&self) -> Utf16Range {
        let selection = self.selection();
        let units = |end: usize| self.state.text[..end].encode_utf16().count();
        Utf16Range {
            start: units(selection.start),
            end: units(selection.end),
        }
    }

    pub(crate) fn into_text(self) -> String {
        self.state.text
    }

    pub(crate) fn apply(&mut self, edit: Edit) -> Option<String> {
        match edit {
            Edit::Insert(text) => self.insert(&text, Some(Run::Typing)),
            Edit::Paste(text) => self.insert(&text, None),
            Edit::Move(motion) => {
                let caret = self.moved(motion);
                self.select(caret, caret);
            }
            Edit::Select(motion) => {
                self.select(self.state.anchor, self.target(motion, self.state.caret));
            }
            Edit::Delete(motion) => self.replace(self.deletion(motion), "", Some(Run::Deleting)),
            Edit::SelectAll => self.select_all(),
            Edit::Copy => return self.selected_text(),
            Edit::Cut => {
                let cut = self.selected_text();
                self.replace(self.selection(), "", None);
                return cut;
            }
            Edit::Undo => self.undo(),
            Edit::Redo => self.redo(),
            Edit::Replace { range, text } => {
                let range = self.bytes(range);
                self.replace(range, &single_line(&text), Some(Run::Typing));
            }
        }
        None
    }

    pub(crate) fn place(&mut self, index: usize, extend: bool) {
        let anchor = if extend { self.state.anchor } else { index };
        self.select(anchor, index);
    }

    pub(crate) fn select_word(&mut self, index: usize) {
        let word = word_around(&self.state.text, index);
        self.select(word.start, word.end);
    }

    pub(crate) fn select_all(&mut self) {
        self.select(0, self.state.text.len());
    }

    fn bytes(&self, range: Utf16Range) -> Range<usize> {
        let text = &self.state.text;
        let offset = |units: usize| {
            let mut counted = 0;
            text.char_indices()
                .find(|&(_, character)| {
                    let reached = counted >= units;
                    counted += character.len_utf16();
                    reached
                })
                .map_or(text.len(), |(index, _)| index)
        };
        let start = offset(range.start);
        start..offset(range.end).max(start)
    }

    fn selected_text(&self) -> Option<String> {
        let selection = self.selection();
        (!selection.is_empty()).then(|| self.state.text[selection].to_owned())
    }

    fn moved(&self, motion: Motion) -> usize {
        let selection = self.selection();
        if selection.is_empty() {
            return self.target(motion, self.state.caret);
        }
        match motion {
            Motion::Backward => selection.start,
            Motion::Forward => selection.end,
            Motion::WordBackward | Motion::Start => self.target(motion, selection.start),
            Motion::WordForward | Motion::End => self.target(motion, selection.end),
        }
    }

    fn deletion(&self, motion: Motion) -> Range<usize> {
        let selection = self.selection();
        if !selection.is_empty() {
            return selection;
        }
        let caret = self.state.caret;
        let target = self.target(motion, caret);
        caret.min(target)..caret.max(target)
    }

    fn target(&self, motion: Motion, from: usize) -> usize {
        let text = &self.state.text;
        match motion {
            Motion::Backward => text[..from]
                .grapheme_indices(true)
                .next_back()
                .map_or(0, |(index, _)| index),
            Motion::Forward => text[from..]
                .graphemes(true)
                .next()
                .map_or(from, |grapheme| from + grapheme.len()),
            Motion::WordBackward => text[..from]
                .unicode_word_indices()
                .next_back()
                .map_or(0, |(index, _)| index),
            Motion::WordForward => text[from..]
                .unicode_word_indices()
                .next()
                .map_or(text.len(), |(index, word)| from + index + word.len()),
            Motion::Start => 0,
            Motion::End => text.len(),
        }
    }

    fn insert(&mut self, text: &str, run: Option<Run>) {
        let line = single_line(text);
        if !line.is_empty() {
            self.replace(self.selection(), &line, run);
        }
    }

    fn select(&mut self, anchor: usize, caret: usize) {
        self.state.anchor = anchor;
        self.state.caret = caret;
        self.run = None;
    }

    fn replace(&mut self, range: Range<usize>, replacement: &str, run: Option<Run>) {
        if range.is_empty() && replacement.is_empty() {
            return;
        }
        if run.is_none() || run != self.run {
            self.undo.push(self.state.clone());
        }
        self.redo.clear();
        self.run = run;
        self.revision += 1;
        self.state.text.replace_range(range.clone(), replacement);
        let caret = range.start + replacement.len();
        self.state.anchor = caret;
        self.state.caret = caret;
    }

    fn undo(&mut self) {
        if let Some(previous) = self.undo.pop() {
            self.redo.push(mem::replace(&mut self.state, previous));
            self.revision += 1;
        }
        self.run = None;
    }

    fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(mem::replace(&mut self.state, next));
            self.revision += 1;
        }
        self.run = None;
    }
}

fn word_around(text: &str, index: usize) -> Range<usize> {
    let mut segments = text
        .split_word_bound_indices()
        .map(|(start, segment)| start..start + segment.len());
    let mut last = index..index;
    for segment in segments.by_ref() {
        if segment.contains(&index) {
            return segment;
        }
        last = segment;
    }
    if last.end == index {
        last
    } else {
        index..index
    }
}

fn single_line(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_whitespace() {
                ' '
            } else {
                character
            }
        })
        .filter(|character| !character.is_control())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(text: &str, edits: impl IntoIterator<Item = Edit>) -> TextField {
        let mut field = TextField::new(text.to_owned());
        for edit in edits {
            field.apply(edit);
        }
        field
    }

    #[test]
    fn replaces_ranges_counted_in_utf16_like_macos_does() {
        let mut field = field("naïve 😀 e", []);
        assert_eq!(
            field.selection_in_utf16(),
            Utf16Range { start: 10, end: 10 }
        );
        field.apply(Edit::Replace {
            range: Utf16Range { start: 9, end: 10 },
            text: "é".into(),
        });
        assert_eq!(field.text(), "naïve 😀 é");
        field.apply(Edit::Replace {
            range: Utf16Range { start: 2, end: 3 },
            text: "i".into(),
        });
        assert_eq!(field.text(), "naive 😀 é");
        assert_eq!(field.selection_in_utf16(), Utf16Range { start: 3, end: 3 });
        field.apply(Edit::Undo);
        assert_eq!(field.text(), "naïve 😀 e");
    }

    fn selected(field: &TextField) -> &str {
        &field.text()[field.selection()]
    }

    #[test]
    fn edits_multibyte_text_by_characters() {
        let field = field(
            "ёжик",
            [
                Edit::Move(Motion::Backward),
                Edit::Delete(Motion::Backward),
                Edit::Insert("щ".into()),
            ],
        );
        assert_eq!(field.text(), "ёжщк");
        assert_eq!(&field.text()[..field.caret()], "ёжщ");
    }

    #[test]
    fn treats_combined_characters_and_contractions_as_one() {
        let mut field = field("cafe\u{301} don't", [Edit::Move(Motion::WordBackward)]);
        assert_eq!(&field.text()[field.caret()..], "don't");
        field.apply(Edit::Move(Motion::Backward));
        field.apply(Edit::Move(Motion::Backward));
        assert_eq!(&field.text()[field.caret()..], "e\u{301} don't");
    }

    #[test]
    fn flattens_pasted_lines() {
        let field = field("", [Edit::Paste("one\ntwo\tthree\u{7}".into())]);
        assert_eq!(field.text(), "one two three");
    }

    #[test]
    fn stays_put_at_the_edges() {
        let mut field = field(
            "ab",
            [
                Edit::Insert("c".into()),
                Edit::Delete(Motion::Forward),
                Edit::Move(Motion::Forward),
            ],
        );
        assert_eq!((field.text(), field.caret()), ("abc", 3));
        field.apply(Edit::Move(Motion::Start));
        field.apply(Edit::Delete(Motion::Backward));
        field.apply(Edit::Move(Motion::Backward));
        assert_eq!((field.text(), field.caret()), ("abc", 0));
        field.apply(Edit::Undo);
        assert_eq!(field.text(), "ab");
    }

    #[test]
    fn moves_and_deletes_by_words() {
        let mut field = field("Ship it, now!", [Edit::Move(Motion::WordBackward)]);
        assert_eq!(&field.text()[field.caret()..], "now!");
        field.apply(Edit::Move(Motion::WordBackward));
        assert_eq!(&field.text()[field.caret()..], "it, now!");
        field.apply(Edit::Move(Motion::WordForward));
        assert_eq!(&field.text()[field.caret()..], ", now!");
        field.apply(Edit::Delete(Motion::WordForward));
        assert_eq!(field.text(), "Ship it!");
        field.apply(Edit::Delete(Motion::WordBackward));
        assert_eq!(field.text(), "Ship !");
        field.apply(Edit::Delete(Motion::Start));
        assert_eq!(field.text(), "!");
    }

    #[test]
    fn extends_the_selection_from_its_anchor() {
        let mut field = field(
            "Ship the plan",
            [
                Edit::Move(Motion::WordBackward),
                Edit::Select(Motion::WordBackward),
            ],
        );
        assert_eq!(selected(&field), "the ");
        field.apply(Edit::Select(Motion::End));
        assert_eq!(selected(&field), "plan");
        field.apply(Edit::Select(Motion::Start));
        assert_eq!(selected(&field), "Ship the ");
    }

    #[test]
    fn collapses_the_selection_before_moving() {
        let mut field = field("Ship the plan", [Edit::SelectAll]);
        field.apply(Edit::Move(Motion::Backward));
        assert_eq!((field.selection(), field.caret()), (0..0, 0));
        field.apply(Edit::SelectAll);
        field.apply(Edit::Move(Motion::Forward));
        assert_eq!(field.caret(), field.text().len());
    }

    #[test]
    fn typing_and_deleting_replace_the_selection() {
        let mut field = field("Ship the plan", [Edit::Select(Motion::WordBackward)]);
        field.apply(Edit::Insert("code".into()));
        assert_eq!(field.text(), "Ship the code");
        field.apply(Edit::SelectAll);
        field.apply(Edit::Delete(Motion::Forward));
        assert_eq!(field.text(), "");
    }

    #[test]
    fn copy_and_cut_hand_over_the_selection() {
        let mut field = field("Ship it", [Edit::Select(Motion::WordBackward)]);
        assert_eq!(field.apply(Edit::Copy).as_deref(), Some("it"));
        assert_eq!(field.text(), "Ship it");
        assert_eq!(field.apply(Edit::Cut).as_deref(), Some("it"));
        assert_eq!(field.text(), "Ship ");
        assert_eq!(field.apply(Edit::Copy), None);
        assert_eq!(field.apply(Edit::Cut), None);
    }

    #[test]
    fn undo_reverts_whole_typing_runs() {
        let mut field = field("", "Ship".chars().map(|c| Edit::Insert(c.into())));
        field.apply(Edit::Paste(" it".into()));
        field.apply(Edit::Undo);
        assert_eq!(field.text(), "Ship");
        field.apply(Edit::Undo);
        assert_eq!(field.text(), "");
        field.apply(Edit::Redo);
        assert_eq!(field.text(), "Ship");
        field.apply(Edit::Insert("!".into()));
        field.apply(Edit::Redo);
        assert_eq!(field.text(), "Ship!");
    }

    #[test]
    fn moving_the_caret_starts_a_new_undo_step() {
        let mut field = field(
            "",
            [
                Edit::Insert("a".into()),
                Edit::Move(Motion::Start),
                Edit::Insert("b".into()),
            ],
        );
        field.apply(Edit::Undo);
        assert_eq!(field.text(), "a");
    }

    #[test]
    fn selects_the_word_or_the_gap_under_the_pointer() {
        let mut field = TextField::new("Ship it, now".to_owned());
        field.select_word(1);
        assert_eq!(selected(&field), "Ship");
        field.select_word(7);
        assert_eq!(selected(&field), ",");
        field.select_word(4);
        assert_eq!(selected(&field), " ");
        field.select_word(12);
        assert_eq!(selected(&field), "now");
    }
}
