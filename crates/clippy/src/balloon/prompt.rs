use std::time::{Duration, Instant};

use unicode_segmentation::UnicodeSegmentation;

use crate::assets::FONT;
use crate::balloon::layout::{FIELD_INSET, FIELD_TEXT_WIDTH};
use crate::balloon::text_field::{Edit, TextField};

const CARET_BLINK: Duration = Duration::from_millis(530);

pub(crate) struct Prompt {
    question: String,
    field: TextField,
    scroll: i32,
    caret: Caret,
    wants_focus: bool,
}

struct Caret {
    shown: bool,
    toggles_at: Instant,
}

impl Prompt {
    pub(crate) fn new(question: String, draft: String, now: Instant) -> Self {
        let mut prompt = Self {
            question,
            field: TextField::new(draft),
            scroll: 0,
            caret: Caret::shown(now),
            wants_focus: true,
        };
        prompt.field.select_all();
        prompt.reveal_caret();
        prompt
    }

    pub(crate) fn question(&self) -> &str {
        &self.question
    }

    pub(crate) fn field(&self) -> &TextField {
        &self.field
    }

    pub(crate) fn scroll(&self) -> i32 {
        self.scroll
    }

    pub(crate) fn is_caret_shown(&self) -> bool {
        self.caret.shown
    }

    pub(crate) fn wants_focus(&self) -> bool {
        self.wants_focus
    }

    pub(crate) fn request_focus(&mut self) {
        self.wants_focus = true;
    }

    pub(crate) fn focus(&mut self, now: Instant) {
        self.wants_focus = false;
        self.wake(now);
    }

    pub(crate) fn edit(&mut self, edit: Edit, now: Instant) -> Option<String> {
        let copied = self.field.apply(edit);
        self.wake(now);
        copied
    }

    pub(crate) fn press(&mut self, x: i32, clicks: usize, extend: bool, now: Instant) {
        let index = self.index_at(x);
        match clicks {
            0 | 1 => self.field.place(index, extend),
            2 => self.field.select_word(index),
            _ => self.field.select_all(),
        }
        self.wake(now);
    }

    pub(crate) fn drag(&mut self, x: i32, now: Instant) {
        self.field.place(self.index_at(x), true);
        self.wake(now);
    }

    pub(crate) fn advance(&mut self, now: Instant) {
        if now >= self.caret.toggles_at {
            self.caret = Caret {
                shown: !self.caret.shown,
                toggles_at: now + CARET_BLINK,
            };
        }
    }

    pub(crate) fn deadline(&self) -> Instant {
        self.caret.toggles_at
    }

    pub(crate) fn into_answer(self) -> String {
        self.field.into_text().trim().to_owned()
    }

    fn wake(&mut self, now: Instant) {
        self.caret = Caret::shown(now);
        self.reveal_caret();
    }

    fn reveal_caret(&mut self) {
        let text = self.field.text();
        let caret = FONT.width(&text[..self.field.caret()]);
        let overflow = (FONT.width(text) + 1 - FIELD_TEXT_WIDTH).max(0);
        self.scroll = self
            .scroll
            .clamp(caret + 1 - FIELD_TEXT_WIDTH, caret)
            .clamp(0, overflow);
    }

    fn index_at(&self, x: i32) -> usize {
        let target = x - FIELD_INSET + self.scroll;
        let text = self.field.text();
        let mut left = 0;
        for (index, grapheme) in text.grapheme_indices(true) {
            let advance = FONT.width(grapheme);
            if target < left + advance / 2 {
                return index;
            }
            left += advance;
        }
        text.len()
    }
}

impl Caret {
    fn shown(now: Instant) -> Self {
        Self {
            shown: true,
            toggles_at: now + CARET_BLINK,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::balloon::Motion;

    const LONG: &str = "Ship the plan, write the tests, then fix every last bug";

    fn prompt(draft: &str) -> Prompt {
        Prompt::new(
            "What are you focusing on?".to_owned(),
            draft.to_owned(),
            Instant::now(),
        )
    }

    fn selected(prompt: &Prompt) -> &str {
        &prompt.field().text()[prompt.field().selection()]
    }

    fn x_of(prompt: &Prompt, index: usize) -> i32 {
        FIELD_INSET + FONT.width(&prompt.field().text()[..index]) - prompt.scroll()
    }

    #[test]
    fn scrolls_only_as_far_as_the_caret_needs() {
        let now = Instant::now();
        let mut prompt = prompt(LONG);
        let end = FONT.width(LONG) + 1 - FIELD_TEXT_WIDTH;
        assert!(end > 0);
        assert_eq!(prompt.scroll(), end);
        prompt.edit(Edit::Move(Motion::End), now);
        prompt.edit(Edit::Move(Motion::WordBackward), now);
        assert_eq!(prompt.scroll(), end);
        prompt.edit(Edit::Move(Motion::Start), now);
        assert_eq!(prompt.scroll(), 0);
        prompt.edit(Edit::Move(Motion::WordForward), now);
        assert_eq!(prompt.scroll(), 0);
        prompt.edit(Edit::Move(Motion::End), now);
        prompt.edit(Edit::Delete(Motion::WordBackward), now);
        let text = prompt.field().text();
        assert_eq!(prompt.scroll(), FONT.width(text) + 1 - FIELD_TEXT_WIDTH);
        prompt.edit(Edit::SelectAll, now);
        prompt.edit(Edit::Delete(Motion::Backward), now);
        assert_eq!(prompt.scroll(), 0);
    }

    #[test]
    fn presses_land_on_the_nearest_character_boundary() {
        let now = Instant::now();
        let mut prompt = prompt(LONG);
        prompt.edit(Edit::Move(Motion::End), now);
        let index = LONG.find("every").unwrap();
        let middle = x_of(&prompt, index) + FONT.advance('e') / 2;
        prompt.press(middle - 1, 1, false, now);
        assert_eq!(prompt.field().caret(), index);
        prompt.press(middle, 1, false, now);
        assert_eq!(prompt.field().caret(), index + 1);
    }

    #[test]
    fn presses_past_the_text_land_on_its_ends() {
        let now = Instant::now();
        let mut prompt = prompt("Ship");
        prompt.press(-10, 1, false, now);
        assert_eq!(prompt.field().caret(), 0);
        prompt.press(FIELD_INSET + FIELD_TEXT_WIDTH, 1, false, now);
        assert_eq!(prompt.field().caret(), 4);
    }

    #[test]
    fn double_and_triple_presses_select_a_word_and_everything() {
        let now = Instant::now();
        let mut prompt = prompt("Ship the plan");
        let inside_the = x_of(&prompt, 6);
        prompt.press(inside_the, 2, false, now);
        assert_eq!(selected(&prompt), "the");
        prompt.press(inside_the, 3, false, now);
        assert_eq!(selected(&prompt), "Ship the plan");
    }

    #[test]
    fn drags_and_shift_presses_extend_from_where_the_press_began() {
        let now = Instant::now();
        let mut prompt = prompt("Ship the plan");
        prompt.press(x_of(&prompt, 5), 1, false, now);
        prompt.drag(x_of(&prompt, 13), now);
        assert_eq!(selected(&prompt), "the plan");
        prompt.drag(x_of(&prompt, 0), now);
        assert_eq!(selected(&prompt), "Ship ");
        prompt.press(x_of(&prompt, 8), 1, true, now);
        assert_eq!(selected(&prompt), "the");
    }

    #[test]
    fn caret_blinks_and_reappears_on_every_edit() {
        let opened = Instant::now();
        let mut prompt = Prompt::new(String::new(), "Ship".to_owned(), opened);
        prompt.advance(opened + CARET_BLINK / 2);
        assert!(prompt.is_caret_shown());
        prompt.advance(opened + CARET_BLINK);
        assert!(!prompt.is_caret_shown());
        let edited = opened + CARET_BLINK * 3 / 2;
        prompt.edit(Edit::Move(Motion::Start), edited);
        assert!(prompt.is_caret_shown());
        assert_eq!(prompt.deadline(), edited + CARET_BLINK);
    }
}
