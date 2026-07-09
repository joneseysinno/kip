//! Built-in imperial seed data: anchors, derived units, affine temperatures.

use std::collections::BTreeMap;

use num_rational::Ratio;
use num_traits::{FromPrimitive, One};

use super::{RegistryBuilder, UnitRecord};
use crate::dim::BaseDim;

/// Build generation-0 registry with default anchors: `in`, `lbf`, `s`, `°R`, `rad`.
pub fn seed_builder() -> RegistryBuilder {
    let mut b = RegistryBuilder {
        generation: 0,
        anchors: default_anchors(),
        pending_anchors: BTreeMap::new(),
        units: BTreeMap::new(),
        custom_dims: BTreeMap::new(),
        next_custom_dim: 0,
        defs_src: Vec::new(),
    };

    // Anchors (ratio 1 to themselves).
    seed_unit(&mut b, "in", &[], BaseDim::Length, Ratio::one(), false);
    seed_unit(&mut b, "lbf", &[], BaseDim::Force, Ratio::one(), false);
    seed_unit(&mut b, "s", &[], BaseDim::Time, Ratio::one(), false);
    seed_unit(&mut b, "°R", &["R"], BaseDim::Temperature, Ratio::one(), false);
    seed_unit(&mut b, "rad", &[], BaseDim::Angle, Ratio::one(), false);

    // Length derived (international foot; survey foot deferred — see plan §9).
    seed_unit(&mut b, "ft", &[], BaseDim::Length, Ratio::from_i32(12).unwrap(), false);
    seed_unit(&mut b, "yd", &[], BaseDim::Length, Ratio::from_i32(36).unwrap(), false);
    seed_unit(
        &mut b,
        "mi",
        &[],
        BaseDim::Length,
        Ratio::from_i32(63360).unwrap(),
        false,
    );
    seed_unit(&mut b, "mil", &[], BaseDim::Length, Ratio::new(1.into(), 1000.into()), false);

    // Force derived.
    seed_unit(
        &mut b,
        "kip",
        &["kips"],
        BaseDim::Force,
        Ratio::from_i32(1000).unwrap(),
        false,
    );

    // Pressure: psi = lbf/in² (stored as force/length² in derived form later; M2 reduces).
    // M0: register as named derived with documented ratio vs lbf anchor for tests.
    seed_unit(
        &mut b,
        "psi",
        &[],
        BaseDim::Force,
        Ratio::one(),
        false,
    );

    // Time derived.
    seed_unit(&mut b, "min", &[], BaseDim::Time, Ratio::from_i32(60).unwrap(), false);
    seed_unit(
        &mut b,
        "hr",
        &[],
        BaseDim::Time,
        Ratio::from_i32(3600).unwrap(),
        false,
    );

    // Angle derived.
    // Exact deg↔rad is irrational; M2 stores float metadata. Ratio placeholder for M0.
    seed_unit(
        &mut b,
        "deg",
        &["°"],
        BaseDim::Angle,
        Ratio::new(180.into(), 206_265_000_000_000i128), // ~180/π × 10¹² approximate
        false,
    );

    // Affine temperature views (cannot anchor).
    seed_unit(&mut b, "°F", &[], BaseDim::Temperature, Ratio::one(), true);
    seed_unit(&mut b, "°C", &[], BaseDim::Temperature, Ratio::one(), true);
    seed_unit(&mut b, "K", &[], BaseDim::Temperature, Ratio::one(), true);

    // Dimensionless percent (plan §9 recommendation).
    seed_unit(
        &mut b,
        "%",
        &[],
        BaseDim::Length, // M0 placeholder — M2 marks dimensionless
        Ratio::new(1.into(), 100.into()),
        false,
    );

    b
}

fn default_anchors() -> BTreeMap<BaseDim, String> {
    BTreeMap::from([
        (BaseDim::Length, "in".into()),
        (BaseDim::Force, "lbf".into()),
        (BaseDim::Time, "s".into()),
        (BaseDim::Temperature, "°R".into()),
        (BaseDim::Angle, "rad".into()),
    ])
}

fn seed_unit(
    b: &mut RegistryBuilder,
    name: &str,
    aliases: &[&str],
    dimension: BaseDim,
    anchor_ratio: Ratio<i128>,
    affine: bool,
) {
    let record = UnitRecord {
        id: super::UnitId(0),
        name: name.into(),
        aliases: aliases.iter().map(|s| (*s).into()).collect(),
        dimension,
        anchor_ratio,
        affine,
    };
    b.units.insert(name.into(), record.clone());
    for alias in aliases {
        b.units.insert((*alias).into(), record.clone());
    }
}
