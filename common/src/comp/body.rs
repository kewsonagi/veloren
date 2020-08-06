pub mod biped_large;
pub mod bird_medium;
pub mod bird_small;
pub mod critter;
pub mod dragon;
pub mod fish_medium;
pub mod fish_small;
pub mod golem;
pub mod humanoid;
pub mod object;
pub mod quadruped_low;
pub mod quadruped_medium;
pub mod quadruped_small;

use crate::{
    assets::{self, Asset},
    npc::NpcKind,
};
use serde::{Deserialize, Serialize};
use specs::{Component, FlaggedStorage};
use specs_idvs::IdvStorage;
use std::{fs::File, io::BufReader};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u32)]
pub enum Body {
    Humanoid(humanoid::Body) = 0,
    QuadrupedSmall(quadruped_small::Body) = 1,
    QuadrupedMedium(quadruped_medium::Body) = 2,
    BirdMedium(bird_medium::Body) = 3,
    FishMedium(fish_medium::Body) = 4,
    Dragon(dragon::Body) = 5,
    BirdSmall(bird_small::Body) = 6,
    FishSmall(fish_small::Body) = 7,
    BipedLarge(biped_large::Body) = 8,
    Object(object::Body) = 9,
    Golem(golem::Body) = 10,
    Critter(critter::Body) = 11,
    QuadrupedLow(quadruped_low::Body) = 12,
}

/// Data representing data generic to the body together with per-species data.
///
/// NOTE: Deliberately don't (yet?) implement serialize.
#[derive(Clone, Debug, Deserialize)]
pub struct BodyData<BodyMeta, SpeciesData> {
    /// Shared metadata for this whole body type.
    pub body: BodyMeta,
    /// All the metadata for species with this body type.
    pub species: SpeciesData,
}

/// Metadata intended to be stored per-body, together with data intended to be
/// stored for each species for each body.
///
/// NOTE: Deliberately don't (yet?) implement serialize.
#[derive(Clone, Debug, Deserialize)]
pub struct AllBodies<BodyMeta, SpeciesMeta> {
    pub humanoid: BodyData<BodyMeta, humanoid::AllSpecies<SpeciesMeta>>,
    pub quadruped_small: BodyData<BodyMeta, quadruped_small::AllSpecies<SpeciesMeta>>,
    pub quadruped_medium: BodyData<BodyMeta, quadruped_medium::AllSpecies<SpeciesMeta>>,
    pub bird_medium: BodyData<BodyMeta, bird_medium::AllSpecies<SpeciesMeta>>,
    pub biped_large: BodyData<BodyMeta, biped_large::AllSpecies<SpeciesMeta>>,
    pub golem: BodyData<BodyMeta, golem::AllSpecies<SpeciesMeta>>,
    pub critter: BodyData<BodyMeta, critter::AllSpecies<SpeciesMeta>>,
    pub dragon: BodyData<BodyMeta, dragon::AllSpecies<SpeciesMeta>>,
    pub quadruped_low: BodyData<BodyMeta, quadruped_low::AllSpecies<SpeciesMeta>>,
}

/// Can only retrieve body metadata by direct index.
impl<BodyMeta, SpeciesMeta> core::ops::Index<NpcKind> for AllBodies<BodyMeta, SpeciesMeta> {
    type Output = BodyMeta;

    #[inline]
    fn index(&self, index: NpcKind) -> &Self::Output {
        match index {
            NpcKind::Humanoid => &self.humanoid.body,
            NpcKind::Pig => &self.quadruped_small.body,
            NpcKind::Wolf => &self.quadruped_medium.body,
            NpcKind::Duck => &self.bird_medium.body,
            NpcKind::Ogre => &self.biped_large.body,
            NpcKind::StoneGolem => &self.golem.body,
            NpcKind::Rat => &self.critter.body,
            NpcKind::Reddragon => &self.dragon.body,
            NpcKind::Crocodile => &self.quadruped_low.body,
        }
    }
}

impl<
    BodyMeta: Send + Sync + for<'de> serde::Deserialize<'de>,
    SpeciesMeta: Send + Sync + for<'de> serde::Deserialize<'de>,
> Asset for AllBodies<BodyMeta, SpeciesMeta>
{
    const ENDINGS: &'static [&'static str] = &["json"];

    fn parse(buf_reader: BufReader<File>) -> Result<Self, assets::Error> {
        serde_json::de::from_reader(buf_reader).map_err(assets::Error::parse_error)
    }
}

impl Body {
    pub fn is_humanoid(&self) -> bool {
        match self {
            Body::Humanoid(_) => true,
            _ => false,
        }
    }

    // Note: this might need to be refined to something more complex for realistic
    // behavior with less cylindrical bodies (e.g. wolfs)
    pub fn radius(&self) -> f32 {
        // TODO: Improve these values (some might be reliant on more info in inner type)
        match self {
            Body::Humanoid(_) => 0.2,
            Body::QuadrupedSmall(_) => 0.3,
            Body::QuadrupedMedium(_) => 0.9,
            Body::Critter(_) => 0.2,
            Body::BirdMedium(_) => 0.5,
            Body::FishMedium(_) => 0.5,
            Body::Dragon(_) => 2.5,
            Body::BirdSmall(_) => 0.2,
            Body::FishSmall(_) => 0.2,
            Body::BipedLarge(_) => 2.0,
            Body::Golem(_) => 2.5,
            Body::QuadrupedLow(_) => 1.0,
            Body::Object(_) => 0.3,
        }
    }

    pub fn height(&self) -> f32 {
        match self {
            Body::Humanoid(humanoid) => match humanoid.species {
                humanoid::Species::Danari => 0.8,
                humanoid::Species::Dwarf => 0.9,
                humanoid::Species::Orc => 1.14,
                humanoid::Species::Undead => 0.95,
                _ => 1.0,
            },
            Body::QuadrupedSmall(_) => 0.6,
            Body::QuadrupedMedium(_) => 0.5,
            Body::Critter(_) => 0.4,
            Body::BirdMedium(_) => 1.2,
            Body::FishMedium(_) => 1.0,
            Body::Dragon(_) => 5.0,
            Body::BirdSmall(_) => 0.4,
            Body::FishSmall(_) => 0.4,
            Body::BipedLarge(_) => 4.0,
            Body::Golem(_) => 5.0,
            Body::QuadrupedLow(_) => 0.5,
            Body::Object(_) => 0.6,
        }
    }

    pub fn base_health(&self) -> u32 {
        match self {
            Body::Humanoid(_) => 520,
            Body::QuadrupedSmall(_) => 440,
            Body::QuadrupedMedium(_) => 720,
            Body::BirdMedium(_) => 360,
            Body::FishMedium(_) => 320,
            Body::Dragon(_) => 2560,
            Body::BirdSmall(_) => 240,
            Body::FishSmall(_) => 200,
            Body::BipedLarge(_) => 1440,
            Body::Object(_) => 1000,
            Body::Golem(_) => 1680,
            Body::Critter(_) => 320,
            Body::QuadrupedLow(_) => 640,
        }
    }

    pub fn base_health_increase(&self) -> u32 {
        match self {
            Body::Humanoid(_) => 50,
            Body::QuadrupedSmall(_) => 40,
            Body::QuadrupedMedium(_) => 70,
            Body::BirdMedium(_) => 40,
            Body::FishMedium(_) => 30,
            Body::Dragon(_) => 260,
            Body::BirdSmall(_) => 20,
            Body::FishSmall(_) => 20,
            Body::BipedLarge(_) => 140,
            Body::Object(_) => 0,
            Body::Golem(_) => 170,
            Body::Critter(_) => 30,
            Body::QuadrupedLow(_) => 60,
        }
    }

    pub fn base_exp(&self) -> u32 {
        match self {
            Body::Humanoid(_) => 15,
            Body::QuadrupedSmall(_) => 12,
            Body::QuadrupedMedium(_) => 28,
            Body::BirdMedium(_) => 10,
            Body::FishMedium(_) => 8,
            Body::Dragon(_) => 160,
            Body::BirdSmall(_) => 5,
            Body::FishSmall(_) => 4,
            Body::BipedLarge(_) => 75,
            Body::Object(_) => 0,
            Body::Golem(_) => 75,
            Body::Critter(_) => 8,
            Body::QuadrupedLow(_) => 24,
        }
    }

    pub fn base_exp_increase(&self) -> u32 {
        match self {
            Body::Humanoid(_) => 3,
            Body::QuadrupedSmall(_) => 2,
            Body::QuadrupedMedium(_) => 6,
            Body::BirdMedium(_) => 2,
            Body::FishMedium(_) => 2,
            Body::Dragon(_) => 32,
            Body::BirdSmall(_) => 1,
            Body::FishSmall(_) => 1,
            Body::BipedLarge(_) => 15,
            Body::Object(_) => 0,
            Body::Golem(_) => 15,
            Body::Critter(_) => 2,
            Body::QuadrupedLow(_) => 5,
        }
    }

    pub fn base_dmg(&self) -> u32 {
        match self {
            Body::Humanoid(_) => 60,
            Body::QuadrupedSmall(_) => 80,
            Body::QuadrupedMedium(_) => 120,
            Body::BirdMedium(_) => 70,
            Body::FishMedium(_) => 60,
            Body::Dragon(_) => 900,
            Body::BirdSmall(_) => 50,
            Body::FishSmall(_) => 30,
            Body::BipedLarge(_) => 360,
            Body::Object(_) => 0,
            Body::Golem(_) => 360,
            Body::Critter(_) => 70,
            Body::QuadrupedLow(_) => 110,
        }
    }

    pub fn base_range(&self) -> f32 {
        match self {
            Body::Humanoid(_) => 5.0,
            Body::QuadrupedSmall(_) => 4.5,
            Body::QuadrupedMedium(_) => 5.5,
            Body::BirdMedium(_) => 3.5,
            Body::FishMedium(_) => 3.5,
            Body::Dragon(_) => 12.5,
            Body::BirdSmall(_) => 3.0,
            Body::FishSmall(_) => 3.0,
            Body::BipedLarge(_) => 10.0,
            Body::Object(_) => 3.0,
            Body::Golem(_) => 7.5,
            Body::Critter(_) => 3.0,
            Body::QuadrupedLow(_) => 4.5,
        }
    }
}

impl Component for Body {
    type Storage = FlaggedStorage<Self, IdvStorage<Self>>;
}
