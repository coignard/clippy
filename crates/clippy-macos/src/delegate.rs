use std::cell::{Cell, OnceCell, RefCell};
use std::time::Instant;

use clippy::{App, Command, Effect, Event};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::runtime::ProtocolObject;
use objc2::runtime::Sel;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationDelegate, NSApplicationTerminateReply, NSMenu, NSMenuItem, NSPanel,
    NSWindowDelegate, NSWorkspace, NSWorkspaceActiveSpaceDidChangeNotification,
    NSWorkspaceDidActivateApplicationNotification, NSWorkspaceDidWakeNotification,
};
use objc2_foundation::{
    NSDate, NSNotification, NSNotificationCenter, NSNotificationName, NSObject, NSObjectProtocol,
    NSRunLoop, NSRunLoopCommonModes, NSSystemClockDidChangeNotification,
    NSSystemTimeZoneDidChangeNotification, NSTimer,
};

use crate::calendars::Calendars;
use crate::clock;
use crate::full_screen::{self, Display};
use crate::hotkeys::HotKeys;
use crate::menu;
use crate::pasteboard;
use crate::sounds::Sounds;
use crate::stage::Stage;

const FULL_SCREEN_CHECK_INTERVAL: f64 = 1.0;
const FADING_SOUND_CHECK_INTERVAL: f64 = 0.05;

pub(crate) struct Ivars {
    app: RefCell<App>,
    launched: OnceCell<Launched>,
    commands: RefCell<Vec<Command>>,
    display: Cell<Option<Display>>,
    full_screen: Cell<Option<bool>>,
    syncing: Cell<bool>,
    stale: Cell<bool>,
    terminating: Cell<bool>,
}

struct Launched {
    stage: Stage,
    sounds: Sounds,
    hot_keys: Option<RefCell<HotKeys>>,
    calendars: Option<Calendars>,
    timer: Retained<NSTimer>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    pub(crate) struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl NSApplicationDelegate for Delegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            self.launch();
        }

        #[unsafe(method(applicationShouldTerminate:))]
        fn should_terminate(&self, _application: &NSApplication) -> NSApplicationTerminateReply {
            let finished = {
                let mut app = self.ivars().app.borrow_mut();
                app.handle(Event::Command(Command::Quit), Instant::now());
                app.is_finished()
            };
            let sounding = self
                .ivars()
                .launched
                .get()
                .is_some_and(|launched| launched.sounds.time_left().is_some());
            if finished && !sounding {
                return NSApplicationTerminateReply::TerminateNow;
            }
            self.ivars().terminating.set(true);
            self.sync();
            NSApplicationTerminateReply::TerminateLater
        }

        #[unsafe(method(applicationDidChangeScreenParameters:))]
        fn did_change_screen_parameters(&self, _notification: &NSNotification) {
            if let Some(launched) = self.ivars().launched.get()
                && launched.stage.is_lost()
            {
                let home = self.ivars().app.borrow().home(launched.stage.home_screen());
                launched.stage.place(home);
            }
            self.check_full_screen();
            self.sync();
        }
    }

    unsafe impl NSWindowDelegate for Delegate {
        #[unsafe(method(windowDidMove:))]
        fn window_did_move(&self, notification: &NSNotification) {
            let Some(launched) = self.ivars().launched.get() else {
                return;
            };
            let moved = notification.object();
            let panel = moved.as_deref().and_then(|object| object.downcast_ref::<NSPanel>());
            if !panel.is_some_and(|panel| launched.stage.is_clippy(panel)) {
                return;
            }
            if launched.stage.display() != self.ivars().display.get() {
                self.check_full_screen();
            }
            self.sync();
        }

        #[unsafe(method(windowDidBecomeKey:))]
        fn window_did_become_key(&self, _notification: &NSNotification) {
            self.send(Event::Focus);
        }

        #[unsafe(method(windowDidResignKey:))]
        fn window_did_resign_key(&self, _notification: &NSNotification) {
            self.send(Event::Blur);
        }
    }

    impl Delegate {
        #[unsafe(method(tick:))]
        fn tick(&self, _timer: &NSTimer) {
            self.send(Event::Tick);
        }

        #[unsafe(method(checkFullScreen:))]
        fn check_full_screen_now(&self, _timer: &NSTimer) {
            self.check_full_screen();
        }

        #[unsafe(method(workspaceDidChange:))]
        fn workspace_did_change(&self, _notification: &NSNotification) {
            self.check_full_screen();
        }

        #[unsafe(method(clockDidChange:))]
        fn clock_did_change(&self, _notification: &NSNotification) {
            clock::forget_time_zone();
            self.send(Event::Tick);
            self.read_calendars();
        }

        #[unsafe(method(perform:))]
        fn perform(&self, item: &NSMenuItem) {
            let command = usize::try_from(item.tag())
                .ok()
                .and_then(|index| self.ivars().commands.borrow().get(index).copied());
            if let Some(command) = command {
                self.send(Event::Command(command));
            }
        }
    }
);

impl Delegate {
    pub(crate) fn new(mtm: MainThreadMarker, app: App) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            app: RefCell::new(app),
            launched: OnceCell::new(),
            commands: RefCell::new(Vec::new()),
            display: Cell::new(None),
            full_screen: Cell::new(None),
            syncing: Cell::new(false),
            stale: Cell::new(false),
            terminating: Cell::new(false),
        });
        unsafe { msg_send![super(this), init] }
    }

    pub(crate) fn shared(mtm: MainThreadMarker) -> Option<Retained<Self>> {
        let delegate = NSApplication::sharedApplication(mtm).delegate()?;
        let object: &AnyObject = delegate.as_ref();
        object.downcast_ref::<Self>().map(Message::retain)
    }

    pub(crate) fn read_calendars(&self) {
        let agenda = self
            .ivars()
            .launched
            .get()
            .and_then(|launched| launched.calendars.as_ref()?.agenda(&clock::now()));
        if let Some(agenda) = agenda {
            self.send(Event::Calendar(agenda));
        }
    }

    pub(crate) fn send(&self, event: Event) {
        self.ivars().app.borrow_mut().handle(event, Instant::now());
        self.sync();
    }

    pub(crate) fn hot_key_pressed(&self, id: u32) {
        let shortcut = self
            .ivars()
            .launched
            .get()
            .and_then(|launched| launched.hot_keys.as_ref()?.borrow().shortcut(id));
        if let Some(shortcut) = shortcut {
            self.send(Event::Shortcut(shortcut));
        }
    }

    pub(crate) fn menu(&self) -> Retained<NSMenu> {
        self.read_calendars();
        let entries = self.ivars().app.borrow().menu(Instant::now());
        let (menu, commands) = menu::build(&entries, self);
        self.ivars().commands.replace(commands);
        menu
    }

    fn launch(&self) {
        let stage = Stage::new(self);
        stage.return_to_where_left(self.ivars().app.borrow().home(stage.home_screen()));
        stage.watch(ProtocolObject::from_ref(self));
        let timer = unsafe {
            NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
                NSDate::distantFuture().timeIntervalSinceNow(),
                self,
                sel!(tick:),
                None,
                true,
            )
        };
        add_to_run_loop(&timer);
        let _ = self.ivars().launched.set(Launched {
            stage,
            sounds: Sounds::load(),
            hot_keys: HotKeys::new()
                .inspect_err(|error| log::error!("Global shortcuts are unavailable: {error}"))
                .ok()
                .map(RefCell::new),
            calendars: self
                .ivars()
                .app
                .borrow()
                .reads_calendar()
                .then(Calendars::new),
            timer,
        });
        if let Some(calendars) = self
            .ivars()
            .launched
            .get()
            .and_then(|launched| launched.calendars.as_ref())
        {
            calendars.watch();
        }
        self.watch_the_clock();
        if self.watches_full_screen() {
            self.watch_full_screen();
            self.check_full_screen();
        }
        self.send(Event::Launch);
        self.read_calendars();
    }

    fn watches_full_screen(&self) -> bool {
        self.ivars().app.borrow().settings().hide_in_full_screen
    }

    fn watch_full_screen(&self) {
        let workspace = NSWorkspace::sharedWorkspace().notificationCenter();
        for name in unsafe {
            [
                NSWorkspaceActiveSpaceDidChangeNotification,
                NSWorkspaceDidActivateApplicationNotification,
            ]
        } {
            self.observe(&workspace, name, sel!(workspaceDidChange:));
        }
        let timer = unsafe {
            NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
                FULL_SCREEN_CHECK_INTERVAL,
                self,
                sel!(checkFullScreen:),
                None,
                true,
            )
        };
        timer.setTolerance(FULL_SCREEN_CHECK_INTERVAL / 2.0);
        add_to_run_loop(&timer);
    }

    fn watch_the_clock(&self) {
        let workspace = NSWorkspace::sharedWorkspace().notificationCenter();
        let system = NSNotificationCenter::defaultCenter();
        let names = unsafe {
            [
                (&workspace, NSWorkspaceDidWakeNotification),
                (&system, NSSystemClockDidChangeNotification),
                (&system, NSSystemTimeZoneDidChangeNotification),
            ]
        };
        for (center, name) in names {
            self.observe(center, name, sel!(clockDidChange:));
        }
    }

    fn observe(&self, center: &NSNotificationCenter, name: &NSNotificationName, selector: Sel) {
        unsafe { center.addObserver_selector_name_object(self, selector, Some(name), None) };
    }

    fn check_full_screen(&self) {
        let Some(launched) = self.ivars().launched.get() else {
            return;
        };
        if !self.watches_full_screen() {
            return;
        }
        let display = launched.stage.display();
        self.ivars().display.set(display);
        let taken = display.is_some_and(full_screen::is_taken);
        if self.ivars().full_screen.replace(Some(taken)) != Some(taken) {
            log::debug!("Full screen is now {}", if taken { "on" } else { "off" });
            self.send(Event::FullScreen(taken));
        }
    }

    fn sync(&self) {
        let ivars = self.ivars();
        if ivars.syncing.replace(true) {
            ivars.stale.set(true);
            return;
        }
        loop {
            self.present();
            if !ivars.stale.replace(false) {
                break;
            }
        }
        ivars.syncing.set(false);
        if ivars.app.borrow().is_finished() {
            self.finish();
        }
    }

    fn present(&self) {
        let Some(launched) = self.ivars().launched.get() else {
            return;
        };
        let (clippy, screen) = launched.stage.geometry();
        let (scene, effects, deadline, shortcuts) = {
            let mut app = self.ivars().app.borrow_mut();
            app.place(clippy, screen);
            let shortcuts = app.shortcuts(Instant::now());
            (app.scene(), app.take_effects(), app.deadline(), shortcuts)
        };
        if let Some(hot_keys) = &launched.hot_keys {
            hot_keys.borrow_mut().bind(shortcuts);
        }
        launched.stage.show(&scene);
        for effect in effects {
            match effect {
                Effect::Play(sound) => launched.sounds.play(sound),
                Effect::Copy(text) => pasteboard::copy(&text),
            }
        }
        let fire = deadline.map_or_else(NSDate::distantFuture, |deadline| {
            let wait = deadline.saturating_duration_since(Instant::now());
            NSDate::dateWithTimeIntervalSinceNow(wait.as_secs_f64())
        });
        launched.timer.setFireDate(&fire);
    }

    fn finish(&self) {
        if let Some(launched) = self.ivars().launched.get()
            && let Some(left) = launched.sounds.time_left()
        {
            let wait = left.as_secs_f64().max(FADING_SOUND_CHECK_INTERVAL);
            launched
                .timer
                .setFireDate(&NSDate::dateWithTimeIntervalSinceNow(wait));
            return;
        }
        let application = NSApplication::sharedApplication(self.mtm());
        if self.ivars().terminating.get() {
            application.replyToApplicationShouldTerminate(true);
        } else {
            application.terminate(None);
        }
    }
}

fn add_to_run_loop(timer: &NSTimer) {
    unsafe { NSRunLoop::currentRunLoop().addTimer_forMode(timer, NSRunLoopCommonModes) };
}
