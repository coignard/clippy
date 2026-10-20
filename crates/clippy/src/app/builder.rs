use std::time::Instant;

use jiff::Zoned;
use jiff::civil::DateTime;

use crate::app::{App, Clock};
use crate::plugin::{AnyPlugin, Installed, Plugin, Plugins};
use crate::settings::Settings;
use crate::storage::Storage;

#[derive(Default)]
pub struct Builder {
    available: Vec<Available>,
    clock: Option<Clock>,
}

struct Available {
    name: &'static str,
    install: fn(&Storage) -> Box<dyn AnyPlugin>,
}

impl Builder {
    #[must_use]
    pub fn plugin<P: Plugin>(mut self) -> Self {
        self.available.push(Available {
            name: P::NAME,
            install: |storage| Box::new(Installed::<P>::load(storage)),
        });
        self
    }

    #[must_use]
    pub fn clock(mut self, clock: impl Fn(Instant) -> Zoned + 'static) -> Self {
        self.clock = Some(Box::new(clock));
        self
    }

    pub fn load(self, storage: Storage) -> App {
        let defaults = Settings {
            plugins: Some(self.names()),
            ..Settings::default()
        };
        let settings = storage
            .load_settings(defaults.clone())
            .unwrap_or_else(|error| {
                log::warn!("Using default settings: {error}");
                defaults
            });
        self.build(settings, storage)
    }

    pub fn build(mut self, settings: Settings, storage: Storage) -> App {
        let every = self.names();
        let enabled = settings.plugins.as_deref().unwrap_or(&every);
        let mut plugins = Plugins::default();
        for name in enabled {
            match self
                .available
                .iter()
                .find(|available| available.name.eq_ignore_ascii_case(name))
            {
                Some(_) if plugins.contains(name) => {
                    log::warn!("{name} is enabled more than once");
                }
                Some(available) => plugins.install((available.install)(&storage)),
                None => log::warn!("There is no plugin called {name}"),
            }
        }
        let clock = self
            .clock
            .take()
            .unwrap_or_else(|| Box::new(|_| Zoned::now()));
        warn_about_clashes(&settings, &plugins, clock(Instant::now()).datetime());
        plugins.keep_in(storage);
        App::assemble(settings, plugins, clock)
    }

    fn names(&self) -> Vec<String> {
        self.available
            .iter()
            .map(|available| available.name.to_owned())
            .collect()
    }
}

fn warn_about_clashes(settings: &Settings, plugins: &Plugins, now: DateTime) {
    let mut taken = vec![("Clippy", settings.toggle_shortcut)];
    for (name, shortcut) in plugins.shortcuts(now) {
        if let Some((owner, _)) = taken.iter().find(|(_, other)| *other == shortcut) {
            log::warn!("{name} wants {shortcut}, which {owner} already uses");
        } else {
            taken.push((name, shortcut));
        }
    }
}
