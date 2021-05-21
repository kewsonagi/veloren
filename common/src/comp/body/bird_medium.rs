use crate::{
    comp::{
        fluid_dynamics::{Drag, WingShape, WingState, Glide},
        Ori,
    },
    make_case_elim, make_proj_elim,
};
use rand::{seq::SliceRandom, thread_rng};
use serde::{Deserialize, Serialize};
use vek::*;

make_proj_elim!(
    body,
    #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct Body {
        pub species: Species,
        pub body_type: BodyType,
    }
);

impl Body {
    pub fn random() -> Self {
        let mut rng = thread_rng();
        let species = *(&ALL_SPECIES).choose(&mut rng).unwrap();
        Self::random_with(&mut rng, &species)
    }

    #[inline]
    pub fn random_with(rng: &mut impl rand::Rng, &species: &Species) -> Self {
        let body_type = *(&ALL_BODY_TYPES).choose(rng).unwrap();
        Self { species, body_type }
    }

    /// Dimensions of the body (wings folded)
    pub const fn dimensions(&self) -> Vec3<f32> { Vec3::new(0.5, 1.0, 1.1) }

    /// Distance from wing tip to wing tip and leading edge to trailing edge
    /// respectively
    // TODO: Check
    pub const fn wing_dimensions(&self) -> Vec2<f32> { Vec2::new(2.0, 0.4) }

    pub fn flying<'a>(&'a self, ori: &'a Ori) -> FlyingBirdMedium<'a> {
        FlyingBirdMedium::from((self, ori))
    }
}

impl From<Body> for super::Body {
    fn from(body: Body) -> Self { super::Body::BirdMedium(body) }
}

make_case_elim!(
    species,
    #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[repr(u32)]
    pub enum Species {
        Duck = 0,
        Chicken = 1,
        Goose = 2,
        Peacock = 3,
        Eagle = 4,
        Owl = 5,
        Parrot = 6,
    }
);

/// Data representing per-species generic data.
///
/// NOTE: Deliberately don't (yet?) implement serialize.
#[derive(Clone, Debug, Deserialize)]
pub struct AllSpecies<SpeciesMeta> {
    pub duck: SpeciesMeta,
    pub chicken: SpeciesMeta,
    pub goose: SpeciesMeta,
    pub peacock: SpeciesMeta,
    pub eagle: SpeciesMeta,
    pub owl: SpeciesMeta,
    pub parrot: SpeciesMeta,
}

impl<'a, SpeciesMeta> core::ops::Index<&'a Species> for AllSpecies<SpeciesMeta> {
    type Output = SpeciesMeta;

    #[inline]
    fn index(&self, &index: &'a Species) -> &Self::Output {
        match index {
            Species::Duck => &self.duck,
            Species::Chicken => &self.chicken,
            Species::Goose => &self.goose,
            Species::Peacock => &self.peacock,
            Species::Eagle => &self.eagle,
            Species::Owl => &self.owl,
            Species::Parrot => &self.parrot,
        }
    }
}

pub const ALL_SPECIES: [Species; 7] = [
    Species::Duck,
    Species::Chicken,
    Species::Goose,
    Species::Peacock,
    Species::Eagle,
    Species::Owl,
    Species::Parrot,
];

impl<'a, SpeciesMeta: 'a> IntoIterator for &'a AllSpecies<SpeciesMeta> {
    type IntoIter = std::iter::Copied<std::slice::Iter<'static, Self::Item>>;
    type Item = Species;

    fn into_iter(self) -> Self::IntoIter { ALL_SPECIES.iter().copied() }
}

make_case_elim!(
    body_type,
    #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[repr(u32)]
    pub enum BodyType {
        Female = 0,
        Male = 1,
    }
);
pub const ALL_BODY_TYPES: [BodyType; 2] = [BodyType::Female, BodyType::Male];

#[derive(Copy, Clone)]
pub struct FlyingBirdMedium<'a> {
    wing_shape: WingShape,
    wing_state: WingState,
    planform_area: f32,
    body: &'a Body,
    ori: &'a Ori,
}

impl<'a> From<(&'a Body, &'a Ori)> for FlyingBirdMedium<'a> {
    fn from((body, ori): (&'a Body, &'a Ori)) -> Self {
        let Vec2 {
            x: span_length,
            y: chord_length,
        } = body.wing_dimensions();
        let planform_area = WingShape::elliptical_planform_area(span_length, chord_length);
        FlyingBirdMedium {
            wing_shape: WingShape::elliptical(span_length, chord_length),
            wing_state: WingState::Flapping,
            planform_area,
            body,
            ori,
        }
    }
}

impl Drag for Body {
    fn parasite_drag_coefficient(&self) -> f32 {
        let radius = self.dimensions().map(|a| a * 0.5);
        // "Field Estimates of body::Body Drag Coefficient on the Basis of
        // Dives in Passerine Birds", Anders Hedenström and Felix Liechti, 2001
        const CD: f32 = 0.2;
        CD * std::f32::consts::PI * radius.x * radius.z
    }
}

impl Drag for FlyingBirdMedium<'_> {
    fn parasite_drag_coefficient(&self) -> f32 {
        self.body.parasite_drag_coefficient() + self.planform_area * 0.004
    }
}

impl Glide for FlyingBirdMedium<'_> {
    fn wing_shape(&self) -> &WingShape { &self.wing_shape }

    fn is_gliding(&self) -> bool { matches!(self.wing_state, WingState::Fixed) }

    fn planform_area(&self) -> f32 { self.planform_area }

    fn ori(&self) -> &Ori { self.ori }
}
