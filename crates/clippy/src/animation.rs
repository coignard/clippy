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

mod playback;

use std::collections::HashMap;
use std::time::Duration;

use enum_map::{Enum, EnumMap};
use fastrand::Rng;
use strum::{EnumString, VariantArray};

use crate::acs;
use crate::geometry::Size;
use crate::sound::Sound;

pub(crate) use playback::Playback;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Enum, EnumString, VariantArray)]
#[strum(ascii_case_insensitive)]
pub enum Animation {
    Alert,
    CheckingSomething,
    Congratulate,
    EmptyTrash,
    Explain,
    GestureDown,
    GestureLeft,
    GestureRight,
    GestureUp,
    GetArtsy,
    GetAttention,
    GetTechy,
    GetWizardy,
    GoodBye,
    Greeting,
    #[strum(serialize = "Hearing_1")]
    Hearing,
    Hide,
    Idle1_1,
    IdleAtom,
    IdleEyeBrowRaise,
    IdleFingerTap,
    IdleHeadScratch,
    IdleRopePile,
    IdleSideToSide,
    IdleSnooze,
    LookDown,
    LookDownLeft,
    LookDownRight,
    LookLeft,
    LookRight,
    LookUp,
    LookUpLeft,
    LookUpRight,
    Print,
    Processing,
    RestPose,
    Save,
    Searching,
    SendMail,
    Show,
    Thinking,
    Wave,
    Writing,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Enum)]
pub(crate) enum IdleLevel {
    #[default]
    Short,
    Long,
    Deep,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum LibraryError {
    #[error("the character has no {0:?} animation")]
    Missing(Animation),
    #[error("the character has no animations for its {0} state")]
    EmptyState(&'static str),
    #[error("the {state} state plays {animation}, which is not one of Clippy's animations")]
    Unknown {
        state: &'static str,
        animation: String,
    },
}

pub(crate) struct Library {
    frame_size: Size,
    clips: EnumMap<Animation, &'static Clip>,
    idles: EnumMap<IdleLevel, Vec<Animation>>,
}

pub(crate) struct Clip {
    pub(crate) frames: Vec<Frame>,
}

pub(crate) struct Frame {
    pub(crate) duration: Duration,
    pub(crate) images: Vec<FrameImage>,
    pub(crate) sound: Option<Sound>,
    pub(crate) exit: Option<usize>,
    pub(crate) branches: Vec<Branch>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FrameImage {
    pub(crate) image: usize,
    pub(crate) x: i32,
    pub(crate) y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Branch {
    pub(crate) frame: usize,
    pub(crate) weight: u32,
}

impl Animation {
    pub(crate) const FAREWELLS: [Self; 3] = [Self::GoodBye, Self::SendMail, Self::EmptyTrash];

    pub(crate) fn random_trick(rng: &mut Rng) -> Self {
        let tricks: Vec<Self> = Self::VARIANTS
            .iter()
            .copied()
            .filter(|animation| animation.is_trick())
            .collect();
        rng.choice(tricks).expect("Clippy should know some tricks")
    }

    fn is_trick(self) -> bool {
        !matches!(
            self,
            Self::Greeting
                | Self::Show
                | Self::Hide
                | Self::GoodBye
                | Self::RestPose
                | Self::Idle1_1
                | Self::IdleAtom
                | Self::IdleEyeBrowRaise
                | Self::IdleFingerTap
                | Self::IdleHeadScratch
                | Self::IdleRopePile
                | Self::IdleSideToSide
                | Self::IdleSnooze
        )
    }

    fn named(name: &str) -> Option<Self> {
        name.parse().ok()
    }
}

impl IdleLevel {
    pub(crate) fn deeper(self) -> Self {
        match self {
            Self::Short => Self::Long,
            Self::Long | Self::Deep => Self::Deep,
        }
    }

    fn state(self) -> &'static str {
        match self {
            Self::Short => "IDLINGLEVEL1",
            Self::Long => "IDLINGLEVEL2",
            Self::Deep => "IDLINGLEVEL3",
        }
    }
}

impl Library {
    pub(crate) fn new(character: &'static acs::Character) -> Result<Self, LibraryError> {
        let named: HashMap<Animation, &'static Clip> = character
            .animations
            .iter()
            .filter_map(|(name, clip)| Some((Animation::named(name)?, clip)))
            .collect();
        if let Some(&missing) = Animation::VARIANTS
            .iter()
            .find(|animation| !named.contains_key(animation))
        {
            return Err(LibraryError::Missing(missing));
        }
        let mut idles = EnumMap::default();
        for (level, animations) in &mut idles {
            *animations = idle_state(character, level)?;
        }
        Ok(Self {
            frame_size: character.frame_size,
            clips: EnumMap::from_fn(|animation| named[&animation]),
            idles,
        })
    }

    pub(crate) fn frame_size(&self) -> Size {
        self.frame_size
    }

    pub(crate) fn clip(&self, animation: Animation) -> &'static Clip {
        self.clips[animation]
    }

    pub(crate) fn idles(&self, level: IdleLevel) -> &[Animation] {
        &self.idles[level]
    }
}

fn idle_state(
    character: &acs::Character,
    level: IdleLevel,
) -> Result<Vec<Animation>, LibraryError> {
    let state = level.state();
    let animations = character
        .states
        .iter()
        .find(|candidate| candidate.name.eq_ignore_ascii_case(state))
        .map(|state| state.animations.as_slice())
        .unwrap_or_default();
    if animations.is_empty() {
        return Err(LibraryError::EmptyState(state));
    }
    animations
        .iter()
        .map(|name| {
            Animation::named(name).ok_or_else(|| LibraryError::Unknown {
                state,
                animation: name.clone(),
            })
        })
        .collect()
}

impl Frame {
    pub(crate) fn shows_clippy(&self) -> bool {
        !self.images.is_empty()
    }

    pub(crate) fn branch(&self, rng: &mut Rng) -> Option<usize> {
        let mut roll = rng.u32(..100);
        for branch in &self.branches {
            if roll < branch.weight {
                return Some(branch.frame);
            }
            roll -= branch.weight;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::assets::ANIMATIONS;

    #[test]
    fn idling_deepens_as_in_the_original_character() {
        use Animation::*;
        assert_eq!(
            ANIMATIONS.idles(IdleLevel::Short),
            [IdleSideToSide, IdleEyeBrowRaise, IdleHeadScratch]
        );
        assert_eq!(
            ANIMATIONS.idles(IdleLevel::Long),
            [
                IdleSideToSide,
                IdleFingerTap,
                IdleEyeBrowRaise,
                IdleHeadScratch
            ]
        );
        assert_eq!(
            ANIMATIONS.idles(IdleLevel::Deep),
            [
                IdleFingerTap,
                IdleEyeBrowRaise,
                IdleHeadScratch,
                IdleRopePile,
                IdleSnooze,
                IdleAtom
            ]
        );
    }

    #[test]
    fn idles_are_never_tricks() {
        let levels = [IdleLevel::Short, IdleLevel::Long, IdleLevel::Deep];
        for animation in levels.into_iter().flat_map(|level| ANIMATIONS.idles(level)) {
            assert!(!animation.is_trick(), "{animation:?}");
        }
    }

    #[test]
    fn tricks_start_and_end_with_clippy_in_sight() {
        for &animation in Animation::VARIANTS
            .iter()
            .filter(|animation| animation.is_trick())
        {
            let frames = &ANIMATIONS.clip(animation).frames;
            assert!(
                frames.first().is_some_and(Frame::shows_clippy)
                    && frames.last().is_some_and(Frame::shows_clippy),
                "{animation:?}"
            );
        }
    }

    #[test]
    fn farewells_take_clippy_out_of_sight() {
        for animation in Animation::FAREWELLS {
            let frames = &ANIMATIONS.clip(animation).frames;
            assert!(frames[0].shows_clippy(), "{animation:?}");
            assert!(
                frames.iter().any(|frame| !frame.shows_clippy()),
                "{animation:?}"
            );
        }
    }

    #[test]
    fn branches_are_taken_by_weight() {
        let frame = |branches: Vec<Branch>| Frame {
            duration: Duration::ZERO,
            images: Vec::new(),
            sound: None,
            exit: None,
            branches,
        };
        let mut rng = Rng::with_seed(1);
        let certain = frame(vec![
            Branch {
                frame: 3,
                weight: 0,
            },
            Branch {
                frame: 5,
                weight: 100,
            },
        ]);
        assert!((0..100).all(|_| certain.branch(&mut rng) == Some(5)));
        let coin_toss = frame(vec![Branch {
            frame: 3,
            weight: 50,
        }]);
        let outcomes: HashSet<_> = (0..100).map(|_| coin_toss.branch(&mut rng)).collect();
        assert_eq!(outcomes, HashSet::from([Some(3), None]));
    }
}
