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

use std::num::NonZeroU8;
use std::ops::RangeInclusive;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::shortcut::{Key, Modifiers, Shortcut};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub toggle_shortcut: Shortcut,
    pub scale: NonZeroU8,
    pub sounds: bool,
    #[serde(rename = "idle_pause_seconds", with = "seconds_range")]
    pub idle_pause: RangeInclusive<Duration>,
    #[serde(rename = "animation_timeout_seconds", with = "seconds")]
    pub animation_timeout: Duration,
    #[serde(rename = "attention_span_seconds", with = "seconds")]
    pub attention_span: Duration,
    pub hide_in_full_screen: bool,
    pub after_summons: AfterSummons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<Vec<String>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AfterSummons {
    #[default]
    Stay,
    Hide,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            toggle_shortcut: Shortcut {
                modifiers: Modifiers::COMMAND.union(Modifiers::SHIFT),
                key: Key::Backslash,
            },
            scale: NonZeroU8::MIN,
            sounds: true,
            idle_pause: Duration::from_secs(5)..=Duration::from_secs(15),
            animation_timeout: Duration::from_secs(5),
            attention_span: Duration::from_secs(60),
            hide_in_full_screen: true,
            after_summons: AfterSummons::Stay,
            plugins: None,
        }
    }
}

mod seconds {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub(super) fn serialize<S: Serializer>(
        duration: &Duration,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        duration.as_secs().serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Duration, D::Error> {
        u64::deserialize(deserializer).map(Duration::from_secs)
    }
}

mod seconds_range {
    use std::ops::RangeInclusive;
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub(super) fn serialize<S: Serializer>(
        range: &RangeInclusive<Duration>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        [range.start().as_secs(), range.end().as_secs()].serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<RangeInclusive<Duration>, D::Error> {
        let [first, second] = <[u64; 2]>::deserialize(deserializer)?;
        Ok(Duration::from_secs(first.min(second))..=Duration::from_secs(first.max(second)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_serialize_to_readable_toml() {
        let text = toml::to_string(&Settings::default()).unwrap();
        assert_eq!(
            text,
            "toggle_shortcut = \"Cmd+Shift+|\"\n\
             scale = 1\n\
             sounds = true\n\
             idle_pause_seconds = [5, 15]\n\
             animation_timeout_seconds = 5\n\
             attention_span_seconds = 60\n\
             hide_in_full_screen = true\n\
             after_summons = \"stay\"\n"
        );
        assert_eq!(
            toml::from_str::<Settings>(&text).unwrap(),
            Settings::default()
        );
    }

    #[test]
    fn missing_keys_fall_back_to_defaults() {
        let settings: Settings = toml::from_str("scale = 2\n").unwrap();
        assert_eq!(
            settings,
            Settings {
                scale: NonZeroU8::new(2).unwrap(),
                ..Settings::default()
            }
        );
    }

    #[test]
    fn idle_pause_bounds_may_come_in_any_order() {
        let settings: Settings = toml::from_str("idle_pause_seconds = [30, 10]\n").unwrap();
        assert_eq!(
            settings.idle_pause,
            Duration::from_secs(10)..=Duration::from_secs(30)
        );
    }

    #[test]
    fn lists_enabled_plugins_in_order() {
        let settings: Settings = toml::from_str("plugins = [\"Focus\", \"Note\"]\n").unwrap();
        assert_eq!(
            settings.plugins.as_deref(),
            Some(["Focus".to_owned(), "Note".to_owned()].as_slice())
        );
        assert_eq!(Settings::default().plugins, None);
    }

    #[test]
    fn clippy_can_go_back_into_hiding_after_a_summons() {
        let settings: Settings = toml::from_str("after_summons = \"hide\"\n").unwrap();
        assert_eq!(settings.after_summons, AfterSummons::Hide);
        assert!(toml::from_str::<Settings>("after_summons = \"vanish\"\n").is_err());
    }

    #[test]
    fn rejects_mistakes() {
        let error = toml::from_str::<Settings>("toggle_shortcut = \"Hyper+K\"\n").unwrap_err();
        assert!(error.to_string().contains("unknown modifier `Hyper`"));
        assert!(toml::from_str::<Settings>("scale = 0\n").is_err());
        assert!(toml::from_str::<Settings>("idle = true\n").is_err());
    }
}
