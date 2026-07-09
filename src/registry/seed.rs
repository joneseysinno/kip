//! Built-in imperial seed data: anchors, derived units, affine temperatures.

use num_rational::Ratio;
use num_traits::{FromPrimitive, One};

use super::{RegistryBuilder, UnitRecord};
use crate::dim::{BaseDim, Dimension};
use crate::registry::anchor::DimAnchor;

/// Build generation-0 registry with default anchors: `in`, `lbf`, `s`, `°R`, `rad`.
pub fn seed_builder() -> RegistryBuilder {
    let mut b = RegistryBuilder::new_empty(0);

    let length = Dimension::single(BaseDim::Length, Ratio::one());
    let force = Dimension::single(BaseDim::Force, Ratio::one());
    let time = Dimension::single(BaseDim::Time, Ratio::one());
    let temp = Dimension::single(BaseDim::Temperature, Ratio::one());
    let angle = Dimension::single(BaseDim::Angle, Ratio::one());
    let pressure = force.div(&length).div(&length);
    let force_length = force.mul(&length);

    // Anchors
    seed_unit(&mut b, "in", &[], length.clone(), Ratio::one(), false);
    seed_unit(&mut b, "lbf", &[], force.clone(), Ratio::one(), false);
    seed_unit(&mut b, "s", &[], time.clone(), Ratio::one(), false);
    seed_unit(&mut b, "°R", &["R"], temp.clone(), Ratio::one(), false);
    seed_unit(&mut b, "rad", &[], angle.clone(), Ratio::one(), false);

    b.anchors.insert(DimAnchor::Base(BaseDim::Length), "in".into());
    b.anchors.insert(DimAnchor::Base(BaseDim::Force), "lbf".into());
    b.anchors.insert(DimAnchor::Base(BaseDim::Time), "s".into());
    b.anchors.insert(DimAnchor::Base(BaseDim::Temperature), "°R".into());
    b.anchors.insert(DimAnchor::Base(BaseDim::Angle), "rad".into());

    // Length
    seed_unit(&mut b, "ft", &[], length.clone(), Ratio::from_i32(12).unwrap(), false);
    seed_unit(&mut b, "yd", &[], length.clone(), Ratio::from_i32(36).unwrap(), false);
    seed_unit(
        &mut b,
        "mi",
        &[],
        length.clone(),
        Ratio::from_i32(63_360).unwrap(),
        false,
    );
    seed_unit(
        &mut b,
        "mil",
        &[],
        length.clone(),
        Ratio::new(1, 1000),
        false,
    );

    // Force
    seed_unit(
        &mut b,
        "kip",
        &["kips"],
        force.clone(),
        Ratio::from_i32(1000).unwrap(),
        false,
    );

    // Pressure (psi anchors the force/length² family)
    seed_unit(&mut b, "psi", &[], pressure.clone(), Ratio::one(), false);
    seed_unit(
        &mut b,
        "ksi",
        &[],
        pressure.clone(),
        Ratio::from_i32(1000).unwrap(),
        false,
    );
    seed_unit(
        &mut b,
        "psf",
        &[],
        pressure.clone(),
        Ratio::new(1, 144),
        false,
    );
    seed_unit(
        &mut b,
        "ksf",
        &[],
        pressure.clone(),
        Ratio::new(1000, 144),
        false,
    );

    // Linear load / density (representative seed ratios)
    seed_unit(
        &mut b,
        "plf",
        &[],
        force_length.clone().div(&length.clone()),
        Ratio::one(),
        false,
    );
    seed_unit(
        &mut b,
        "klf",
        &[],
        force_length.clone().div(&length),
        Ratio::from_i32(1000).unwrap(),
        false,
    );
    seed_unit(
        &mut b,
        "pcf",
        &[],
        force.clone().div(&Dimension::single(BaseDim::Length, Ratio::from_integer(1))),
        Ratio::new(1, 1728),
        false,
    );

    // Moment
    seed_unit(
        &mut b,
        "lbf·ft",
        &["lbf*ft"],
        force_length.clone(),
        Ratio::from_i32(12).unwrap(),
        false,
    );
    seed_unit(
        &mut b,
        "kip·ft",
        &["kip*ft"],
        force_length.clone(),
        Ratio::from_i32(12_000).unwrap(),
        false,
    );
    seed_unit(
        &mut b,
        "kip·in",
        &["kip*in"],
        force_length,
        Ratio::from_i32(1000).unwrap(),
        false,
    );

    // Mass (slug = lbf·s²/ft under standard gravity)
    let mass = force
        .mul(&time.clone())
        .mul(&time)
        .div(&Dimension::single(BaseDim::Length, Ratio::from_integer(1)));
    seed_unit(
        &mut b,
        "slug",
        &[],
        mass.clone(),
        Ratio::new(1, 12),
        false,
    );
    seed_unit(
        &mut b,
        "lbm",
        &[],
        mass,
        Ratio::new(1, 32_174),
        false,
    );

    // Time
    seed_unit(&mut b, "min", &[], time.clone(), Ratio::from_i32(60).unwrap(), false);
    seed_unit(
        &mut b,
        "hr",
        &[],
        time,
        Ratio::from_i32(3600).unwrap(),
        false,
    );

    // Angle (high-precision rational approximation)
    seed_unit(
        &mut b,
        "deg",
        &["°"],
        angle,
        Ratio::new(180_000_000_000_000i128, 3_141_592_653_589_793i128),
        false,
    );

    // Affine temperature views
    seed_unit(
        &mut b,
        "°F",
        &[],
        Dimension::single(BaseDim::Temperature, Ratio::one()),
        Ratio::one(),
        true,
    );
    seed_unit(
        &mut b,
        "°C",
        &[],
        Dimension::single(BaseDim::Temperature, Ratio::one()),
        Ratio::one(),
        true,
    );
    seed_unit(
        &mut b,
        "K",
        &[],
        Dimension::single(BaseDim::Temperature, Ratio::one()),
        Ratio::one(),
        true,
    );

    // Dimensionless percent (plan §9)
    seed_unit(
        &mut b,
        "%",
        &[],
        Dimension::dimensionless(),
        Ratio::new(1, 100),
        false,
    );

    b
}

fn seed_unit(
    b: &mut RegistryBuilder,
    name: &str,
    aliases: &[&str],
    dimension: Dimension,
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
