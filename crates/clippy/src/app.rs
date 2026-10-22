mod builder;
mod notices;
mod visibility;

use std::mem;
use std::time::{Duration, Instant};

use fastrand::Rng;
use jiff::civil::DateTime;
use jiff::tz::AmbiguousOffset;
use jiff::{Timestamp, Zoned};

use crate::animation::Animation;
use crate::balloon::{Content, Edit, Layout, Placement, Prompt, SCREEN_MARGIN};
use crate::calendar::Agenda;
use crate::character::{Character, FieldState, Presence, Side};
use crate::geometry::{Rect, Size};
use crate::menu::Entry;
use crate::plugin::{Due, PluginItem, Plugins, Urgency};
use crate::scene::{BalloonScene, Field, Scene};
use crate::settings::Settings;
use crate::shortcut::Shortcut;
use crate::sound::Sound;

pub use builder::Builder;
use notices::Board;
use visibility::{Lasts, Need, Visibility};

type Clock = Box<dyn Fn(Instant) -> Zoned>;

const STEPPING_ASIDE: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Launch,
    Command(Command),
    Shortcut(Shortcut),
    FullScreen(bool),
    Calendar(Agenda),
    PointerEnter,
    PointerLeave,
    ClippyDoubleClick,
    MessageDoubleClick,
    CheckboxPress,
    CheckboxDrag { inside: bool },
    CheckboxRelease { inside: bool },
    Focus,
    Blur,
    FieldPress { x: i32, clicks: usize, extend: bool },
    FieldDrag { x: i32 },
    Edit(Edit),
    Submit,
    Cancel,
    Tick,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Toggle,
    Hide,
    Animate,
    Quit,
    Plugin(PluginItem),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Play(Sound),
    Copy(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Moment {
    pub(crate) instant: Instant,
    pub(crate) local: DateTime,
}

pub struct App {
    settings: Settings,
    clock: Clock,
    time: Option<(Instant, Zoned)>,
    character: Character,
    plugins: Plugins,
    notices: Board,
    visibility: Visibility,
    focused: bool,
    quitting: bool,
    effects: Vec<Effect>,
    stage: Option<Stage>,
}

#[derive(Clone, Copy, Debug)]
struct Stage {
    clippy: Rect,
    screen: Rect,
}

impl App {
    pub fn builder() -> Builder {
        Builder::default()
    }

    fn assemble(settings: Settings, plugins: Plugins, clock: Clock) -> Self {
        Self {
            character: Character::new(&settings, Rng::new()),
            settings,
            clock,
            time: None,
            plugins,
            notices: Board::default(),
            visibility: Visibility::default(),
            focused: false,
            quitting: false,
            effects: Vec::new(),
            stage: None,
        }
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn reads_calendar(&self) -> bool {
        self.plugins.reads_calendar()
    }

    pub fn clippy_size(&self) -> Size {
        Character::size().scaled(self.scale())
    }

    pub fn home(&self, screen: Rect) -> Rect {
        let size = self.clippy_size();
        let area = screen.inset(SCREEN_MARGIN);
        Rect::new(
            area.right() - size.width,
            area.bottom() - size.height,
            size.width,
            size.height,
        )
    }

    pub fn shortcuts(&self, now: Instant) -> Vec<Shortcut> {
        let mut shortcuts = vec![self.settings.toggle_shortcut];
        let local = self.local(now);
        let notice = self.notices.shortcut();
        let check = self.notices.check_shortcut();
        let messages = self
            .plugins
            .message_shortcuts(local)
            .filter(|_| self.shows_messages());
        for shortcut in notice
            .into_iter()
            .chain(check)
            .chain(messages)
            .chain(self.plugins.active_shortcuts(local))
        {
            if !shortcuts.contains(&shortcut) {
                shortcuts.push(shortcut);
            }
        }
        shortcuts
    }

    pub fn handle(&mut self, event: Event, now: Instant) {
        let zoned = (self.clock)(now);
        let moment = Moment {
            instant: now,
            local: zoned.datetime(),
        };
        let new_day = self
            .time
            .as_ref()
            .is_some_and(|(_, last)| last.date() != zoned.date());
        self.time = Some((now, zoned));
        if new_day {
            self.plugins.begin_day(moment.local);
        }
        if event.comes_from_the_user() {
            self.character.rouse();
        }
        let questions = self.plugins.questions_asked();
        let checking = matches!(event, Event::Shortcut(shortcut) if self.notices.check_shortcut() == Some(shortcut));
        let dismissing = checking
            || matches!(event, Event::Shortcut(shortcut) if self.notices.shortcut() == Some(shortcut));
        let ticked = checking && self.tick_notice(moment);
        let interrupted = !ticked && event.interrupts_notices() && self.interrupt_notices(moment);
        let swallowed = match event {
            Event::MessageDoubleClick => interrupted,
            Event::Shortcut(shortcut) => {
                dismissing || (!interrupted && self.press_message(shortcut, moment))
            }
            _ => event.reaches_the_field() && self.notices.is_displayed(),
        };
        if !swallowed {
            self.respond(event, moment);
        }
        if self.plugins.questions_asked() != questions
            && let Some(asker) = self.plugins.asker()
        {
            self.visibility.summon(asker, self.settings.after_summons);
        }
        self.settle(moment);
    }

    pub fn menu(&self, now: Instant) -> Vec<Entry> {
        let mut entries = vec![
            Entry::item("Hide", Command::Hide, Some(self.settings.toggle_shortcut)),
            Entry::item("Animate!", Command::Animate, None),
            Entry::Separator,
        ];
        let before = entries.len();
        entries.extend(self.plugins.menu(self.local(now)));
        if entries.len() > before {
            entries.push(Entry::Separator);
        }
        entries.push(Entry::item("Quit", Command::Quit, None));
        entries
    }

    pub fn place(&mut self, clippy: Rect, screen: Rect) {
        self.stage = Some(Stage { clippy, screen });
    }

    pub fn scene(&self) -> Scene {
        let scale = self.scale();
        let message = self.message();
        let balloon =
            self.balloon(message.as_deref())
                .and_then(|content| self.arrange(content))
                .map(|(layout, placement, frame)| BalloonScene {
                    frame,
                    canvas: layout.render(placement).scaled(scale),
                    field: self.plugins.prompt().zip(layout.field(placement)).map(
                        |(prompt, area)| Field {
                            frame: area.scaled(scale),
                            selection: prompt.field().selection_in_utf16(),
                            wants_focus: prompt.wants_focus(),
                        },
                    ),
                    checkbox: layout.checkbox(placement).map(|area| area.scaled(scale)),
                });
        Scene {
            clippy: self.character.render().map(|canvas| canvas.scaled(scale)),
            balloon,
        }
    }

    pub fn take_effects(&mut self) -> Vec<Effect> {
        mem::take(&mut self.effects)
    }

    pub fn deadline(&self) -> Option<Instant> {
        let caret = self.caret_deadline();
        let plugins = self.time.as_ref().and_then(|(instant, zoned)| {
            let change = self.plugins.changes_at(zoned.datetime())?;
            let at = first_reached(change, zoned)?;
            let wait = Duration::try_from(zoned.timestamp().duration_until(at)).ok()?;
            Some(*instant + wait)
        });
        [self.character.deadline(), caret, plugins]
            .into_iter()
            .flatten()
            .min()
    }

    pub fn is_finished(&self) -> bool {
        self.quitting && self.character.presence() == Presence::Absent
    }

    fn respond(&mut self, event: Event, moment: Moment) {
        let now = moment.instant;
        let scale = self.scale();
        match event {
            Event::Launch => {
                self.visibility.launch();
                self.plugins.begin_day(moment.local);
            }
            Event::Command(command) => self.run(command, moment),
            Event::Shortcut(shortcut) => self.press(shortcut, moment),
            Event::FullScreen(full_screen) => {
                let hides = full_screen && self.settings.hide_in_full_screen;
                self.visibility.set_full_screen(hides);
            }
            Event::Calendar(agenda) => self.plugins.read_agenda(&agenda),
            Event::PointerEnter => self.character.set_hovered(true, now),
            Event::PointerLeave => self.character.set_hovered(false, now),
            Event::ClippyDoubleClick if self.is_present() => self.show_off(),
            Event::MessageDoubleClick if self.is_present() => {
                self.plugins.double_click(&mut self.character, moment);
            }
            Event::CheckboxPress => self.notices.press(),
            Event::CheckboxDrag { inside } => self.notices.drag(inside),
            Event::CheckboxRelease { inside } => {
                if let Some(due) = self.notices.release(inside) {
                    self.check(&due, moment);
                }
            }
            Event::Focus => self.focused = true,
            Event::Blur => self.focused = false,
            Event::FieldPress { x, clicks, extend } => {
                if let Some(prompt) = self.plugins.prompt_mut() {
                    prompt.press(x.div_euclid(scale), clicks, extend, now);
                }
            }
            Event::FieldDrag { x } => {
                if let Some(prompt) = self.plugins.prompt_mut() {
                    prompt.drag(x.div_euclid(scale), now);
                }
            }
            Event::Edit(edit) => {
                if let Some(prompt) = self.plugins.prompt_mut() {
                    let revision = prompt.field().revision();
                    let copied = prompt.edit(edit, now);
                    if prompt.field().revision() != revision {
                        self.character.scribble(now);
                    }
                    self.effects.extend(copied.map(Effect::Copy));
                }
            }
            Event::Submit => self.plugins.submit(&mut self.character, moment),
            Event::Cancel => self.plugins.dismiss(),
            Event::ClippyDoubleClick | Event::MessageDoubleClick | Event::Tick => {}
        }
    }

    fn settle(&mut self, moment: Moment) {
        let performing = self.is_performing();
        self.catch_up(moment);
        if performing && !self.is_performing() {
            self.catch_up(moment);
        }
    }

    fn catch_up(&mut self, moment: Moment) {
        let now = moment.instant;
        let mut due = self.plugins.notices(moment.local);
        self.end_finished_summons(&due, moment.local);
        if !self.character.is_showing_off() {
            self.visibility.stop_lingering();
        }
        let need = self.need(&due);
        self.reconcile(now, need);
        if self.visibility.is_stepping_aside() {
            due.retain(|due| due.notice.over_full_screen);
        }
        self.character.set_field(self.field_state());
        self.character
            .set_bedtime(self.plugins.bedtime(moment.local));
        self.character.advance(now);
        if self.character.presence() == Presence::Absent {
            self.plugins.dismiss();
        }
        let change = self
            .notices
            .update(&due, self.is_present(), self.is_celebrating());
        if change.appeared {
            self.visibility.keep(self.settings.after_summons);
            let side = self.side_of(self.notices.content());
            self.character
                .beckon(side, self.notices.urgency() == Urgency::Asked);
            self.character.advance(now);
        }
        let covered = self.notices.is_displayed();
        let present = self.is_present();
        if let Some(prompt) = self.plugins.prompt_mut() {
            if change.appeared && self.focused {
                prompt.request_focus();
            }
            if self.focused && !covered && prompt.wants_focus() {
                prompt.focus(now);
            }
            if present {
                prompt.advance(now);
            }
        }
        let sounds = self.character.take_sounds();
        if self.settings.sounds {
            self.effects.extend(sounds.into_iter().map(Effect::Play));
        }
        self.plugins.save();
    }

    fn run(&mut self, command: Command, moment: Moment) {
        match command {
            Command::Toggle => match self.character.presence() {
                Presence::Absent | Presence::Leaving => self.visibility.show(),
                Presence::Arriving | Presence::Present => self.hide(moment),
            },
            Command::Hide => self.hide(moment),
            Command::Animate => self.show_off(),
            Command::Quit => {
                self.quitting = true;
                self.character.say_goodbye();
            }
            Command::Plugin(item) => self.plugins.choose(item, &mut self.character, moment),
        }
    }

    fn press_message(&mut self, shortcut: Shortcut, moment: Moment) -> bool {
        self.shows_messages()
            && self
                .plugins
                .press_message(shortcut, &mut self.character, moment)
    }

    fn shows_messages(&self) -> bool {
        self.is_present() && !self.notices.is_displayed() && self.plugins.prompt().is_none()
    }

    fn press(&mut self, shortcut: Shortcut, moment: Moment) {
        if shortcut == self.settings.toggle_shortcut {
            self.run(Command::Toggle, moment);
        } else if let Some(item) = self.plugins.item_for(shortcut, moment.local) {
            self.run(Command::Plugin(item), moment);
        }
    }

    fn show_off(&mut self) {
        self.character.animate();
        if self.character.is_showing_off() {
            self.visibility.linger();
        }
    }

    fn hide(&mut self, moment: Moment) {
        for due in self.plugins.notices(moment.local) {
            self.plugins.snooze(&due, &mut self.character, moment);
        }
        self.visibility.hide();
    }

    fn tick_notice(&mut self, moment: Moment) -> bool {
        let Some(due) = self.notices.tick() else {
            return false;
        };
        self.check(&due, moment);
        true
    }

    fn check(&mut self, due: &Due, moment: Moment) {
        self.plugins.check(due, &mut self.character, moment);
        if due.notice.celebrates {
            self.character.celebrate();
        }
    }

    fn interrupt_notices(&mut self, moment: Moment) -> bool {
        let displayed = self.notices.is_displayed();
        if let Some(due) = self.notices.interrupt() {
            self.plugins.snooze(&due, &mut self.character, moment);
        }
        displayed
    }

    fn end_finished_summons(&mut self, due: &[Due], now: DateTime) {
        let Some(summons) = self.visibility.summons() else {
            return;
        };
        let lasting = self.is_celebrating()
            || match summons.lasts {
                Lasts::WhileAsking => self.plugins.asker() == Some(summons.owner),
                Lasts::WhileShowing => self.plugins.shows_anything(summons.owner, due, now),
            };
        if !lasting {
            self.visibility.end_summons();
        }
    }

    fn need(&self, due: &[Due]) -> Need {
        let mut notices = due.iter().chain(self.notices.celebrated()).peekable();
        if notices.peek().is_none() {
            Need::None
        } else if notices.any(|due| due.notice.over_full_screen) {
            Need::Urgent
        } else {
            Need::Polite
        }
    }

    fn reconcile(&mut self, now: Instant, need: Need) {
        let wanted = !self.quitting && self.visibility.wants_clippy(need);
        match (wanted, self.character.presence()) {
            (true, Presence::Absent | Presence::Leaving) => {
                let arrival = self.visibility.arrival();
                self.character.arrive(arrival, now);
            }
            (false, Presence::Arriving | Presence::Present) => {
                let by = if self.settings.hide_at_once {
                    Some(now)
                } else {
                    self.visibility
                        .is_stepping_aside()
                        .then_some(now + STEPPING_ASIDE)
                };
                self.character.leave(Animation::Hide, by);
            }
            _ => {}
        }
    }

    fn balloon<'a>(&'a self, message: Option<&'a str>) -> Option<Content<'a>> {
        if !self.is_present() {
            return None;
        }
        if let Some(notice) = self.notices.content() {
            return Some(notice);
        }
        match self.plugins.prompt() {
            Some(prompt) => Some(Content::Prompt {
                prompt,
                focused: self.focused,
            }),
            None => message.map(Content::Message),
        }
    }

    fn arrange<'a>(&self, content: Content<'a>) -> Option<(Layout<'a>, Placement, Rect)> {
        let Stage { clippy, screen } = self.stage?;
        let layout = Layout::new(content);
        let (placement, frame) =
            Placement::choose(clippy, layout.size().scaled(self.scale()), screen);
        Some((layout, placement, frame))
    }

    fn side_of(&self, content: Option<Content<'_>>) -> Side {
        content
            .and_then(|content| self.arrange(content))
            .map_or(Side::Above, |(_, placement, _)| Side::of(placement))
    }

    fn caret_deadline(&self) -> Option<Instant> {
        self.plugins
            .prompt()
            .filter(|_| self.focused && self.is_present() && !self.has_selection())
            .map(Prompt::deadline)
    }

    fn field_state(&self) -> FieldState {
        match self.plugins.prompt() {
            Some(_) if !self.is_present() || self.notices.is_displayed() => FieldState::Closed,
            Some(_) if self.focused => FieldState::Focused,
            Some(prompt) => FieldState::Open(self.side_of(Some(Content::Prompt {
                prompt,
                focused: false,
            }))),
            None => FieldState::Closed,
        }
    }

    fn message(&self) -> Option<String> {
        let (_, now) = self.time.as_ref()?;
        self.plugins.message(now.datetime())
    }

    fn local(&self, now: Instant) -> DateTime {
        (self.clock)(now).datetime()
    }

    fn has_selection(&self) -> bool {
        self.plugins
            .prompt()
            .is_some_and(|prompt| !prompt.field().selection().is_empty())
    }

    fn is_present(&self) -> bool {
        self.character.presence() == Presence::Present
    }

    fn is_celebrating(&self) -> bool {
        self.character.is_busy_with(Animation::Congratulate)
    }

    fn is_performing(&self) -> bool {
        self.is_celebrating() || self.character.is_showing_off()
    }

    fn scale(&self) -> i32 {
        i32::from(self.settings.scale.get())
    }
}

fn first_reached(civil: DateTime, now: &Zoned) -> Option<Timestamp> {
    let zone = now.time_zone();
    let ambiguous = zone.to_ambiguous_timestamp(civil);
    let candidates = match ambiguous.offset() {
        AmbiguousOffset::Gap { .. } => {
            let skipped = ambiguous.earlier().ok()?;
            [
                zone.following(skipped).next().map(|gap| gap.timestamp()),
                None,
            ]
        }
        AmbiguousOffset::Unambiguous { .. } | AmbiguousOffset::Fold { .. } => {
            [ambiguous.earlier().ok(), ambiguous.later().ok()]
        }
    };
    candidates
        .into_iter()
        .flatten()
        .find(|&at| at > now.timestamp())
}

impl Event {
    fn comes_from_the_user(&self) -> bool {
        !matches!(
            self,
            Self::Launch
                | Self::FullScreen(_)
                | Self::Calendar(_)
                | Self::PointerLeave
                | Self::Focus
                | Self::Blur
                | Self::Tick
        )
    }

    fn interrupts_notices(&self) -> bool {
        match self {
            Self::Command(command) => *command != Command::Quit,
            Self::Shortcut(_) | Self::ClippyDoubleClick | Self::MessageDoubleClick => true,
            _ => false,
        }
    }

    fn reaches_the_field(&self) -> bool {
        matches!(
            self,
            Self::FieldPress { .. }
                | Self::FieldDrag { .. }
                | Self::Edit(_)
                | Self::Submit
                | Self::Cancel
        )
    }
}

#[cfg(test)]
mod tests;
