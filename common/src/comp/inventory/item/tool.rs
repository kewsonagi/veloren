// Note: If you changes here "break" old character saves you can change the
// version in voxygen\src\meta.rs in order to reset save files to being empty

use crate::{
    assets::{self, Asset},
    comp::{skills::Skill, CharacterAbility},
};
use hashbrown::HashMap;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolKind {
    Sword,
    Axe,
    Hammer,
    Bow,
    Dagger,
    Staff,
    Sceptre,
    Shield,
    Unique(UniqueKind),
    Debug,
    Farming,
    /// This is an placeholder item, it is used by non-humanoid npcs to attack
    Empty,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Hands {
    One,
    Two,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Stats {
    equip_time_millis: u32,
    power: f32,
    poise_strength: f32,
    speed: f32,
}

impl From<&Tool> for Stats {
    fn from(tool: &Tool) -> Self {
        let raw_stats = tool.stats;
        let (power, speed) = match tool.hands {
            Hands::One => (0.67, 1.33),
            // TODO: Restore this when one-handed weapons are made accessible
            // Hands::Two => (1.5, 0.75),
            Hands::Two => (1.0, 1.0),
        };
        Self {
            equip_time_millis: raw_stats.equip_time_millis,
            power: raw_stats.power * power,
            poise_strength: raw_stats.poise_strength,
            speed: raw_stats.speed * speed,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tool {
    pub kind: ToolKind,
    pub hands: Hands,
    pub stats: Stats,
    // TODO: item specific abilities
}

impl Tool {
    // DO NOT USE UNLESS YOU KNOW WHAT YOU ARE DOING
    // Added for CSV import of stats
    pub fn new(
        kind: ToolKind,
        hands: Hands,
        equip_time_millis: u32,
        power: f32,
        poise_strength: f32,
        speed: f32,
    ) -> Self {
        Self {
            kind,
            hands,
            stats: Stats {
                equip_time_millis,
                power,
                poise_strength,
                speed,
            },
        }
    }

    pub fn empty() -> Self {
        Self {
            kind: ToolKind::Empty,
            hands: Hands::One,
            stats: Stats {
                equip_time_millis: 0,
                power: 1.00,
                poise_strength: 1.00,
                speed: 1.00,
            },
        }
    }

    // Keep power between 0.5 and 2.00
    pub fn base_power(&self) -> f32 { self.stats.power }

    pub fn base_poise_strength(&self) -> f32 { self.stats.poise_strength }

    pub fn base_speed(&self) -> f32 { self.stats.speed }

    pub fn equip_time(&self) -> Duration {
        Duration::from_millis(self.stats.equip_time_millis as u64)
    }

    pub fn get_abilities(&self, map: &AbilityMap) -> AbilitySet<CharacterAbility> {
        if let Some(set) = map.0.get(&self.kind).cloned() {
            set.modified_by_tool(&self)
        } else {
            error!(
                "ToolKind: {:?} has no AbilitySet in the ability map falling back to default",
                &self.kind
            );
            Default::default()
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AbilitySet<T> {
    pub primary: T,
    pub secondary: T,
    pub abilities: Vec<(Option<Skill>, T)>,
}

impl AbilitySet<CharacterAbility> {
    pub fn modified_by_tool(self, tool: &Tool) -> Self {
        let stats = Stats::from(tool);
        self.map(|a| a.adjusted_by_stats(stats.power, stats.poise_strength, stats.speed))
    }
}

impl<T> AbilitySet<T> {
    pub fn map<U, F: FnMut(T) -> U>(self, mut f: F) -> AbilitySet<U> {
        AbilitySet {
            primary: f(self.primary),
            secondary: f(self.secondary),
            abilities: self.abilities.into_iter().map(|(s, x)| (s, f(x))).collect(),
        }
    }

    pub fn map_ref<U, F: FnMut(&T) -> U>(&self, mut f: F) -> AbilitySet<U> {
        AbilitySet {
            primary: f(&self.primary),
            secondary: f(&self.secondary),
            abilities: self.abilities.iter().map(|(s, x)| (*s, f(x))).collect(),
        }
    }
}

impl Default for AbilitySet<CharacterAbility> {
    fn default() -> Self {
        AbilitySet {
            primary: CharacterAbility::default(),
            secondary: CharacterAbility::default(),
            abilities: vec![],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AbilityMap<T = CharacterAbility>(HashMap<ToolKind, AbilitySet<T>>);

impl Asset for AbilityMap<String> {
    type Loader = assets::RonLoader;

    const EXTENSION: &'static str = "ron";
}

impl assets::Compound for AbilityMap {
    fn load<S: assets_manager::source::Source>(
        cache: &assets_manager::AssetCache<S>,
        specifier: &str,
    ) -> Result<Self, assets::Error> {
        let manifest = cache.load::<AbilityMap<String>>(specifier)?.read();

        Ok(AbilityMap(
            manifest
                .0
                .iter()
                .map(|(kind, set)| {
                    (
                        *kind,
                        // expect cannot fail because CharacterAbility always
                        // provides a default value in case of failure
                        set.map_ref(|s| cache.load_expect(&s).cloned()),
                    )
                })
                .collect(),
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UniqueKind {
    StoneGolemFist,
    BeastClaws,
    QuadMedQuick,
    QuadMedJump,
    QuadMedHoof,
    QuadMedBasic,
    QuadMedCharge,
    QuadLowRanged,
    QuadLowBreathe,
    QuadLowTail,
    QuadLowQuick,
    QuadLowBasic,
    QuadSmallBasic,
    TheropodBasic,
    TheropodBird,
}
