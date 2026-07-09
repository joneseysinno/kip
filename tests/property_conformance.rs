//! Property-based hardening tests (plan §8, M8).

use kip::{RegistryBuilder, UnitExpr};
use num_rational::Ratio;
use num_traits::One;
use proptest::prelude::*;

proptest! {
    #[test]
    fn dump_defs_round_trip_ratio(n in 1i128..10_000) {
        let src = format!("define bench_u_{n} = {n} lbf");
        let mut b = RegistryBuilder::from_seed();
        b.parse_defs(&src).expect("parse defs");
        let reg = b.freeze();
        let dumped = reg.dump_defs();
        let mut b2 = RegistryBuilder::from_seed();
        b2.parse_defs(&dumped).expect("round-trip defs");
        let reg2 = b2.freeze();
        let name = format!("bench_u_{n}");
        let a = reg.unit(&name).expect("unit");
        let b = reg2.unit(&name).expect("unit rt");
        prop_assert_eq!(a.anchor_ratio, b.anchor_ratio);
        prop_assert_eq!(a.dimension.clone(), b.dimension.clone());
    }

    #[test]
    fn convert_ft_in_round_trip(n in 1i32..10_000) {
        let reg = RegistryBuilder::from_seed().freeze();
        let q = kip::Quantity::from_int(i128::from(n), "ft", kip::Dimension::single(kip::BaseDim::Length, Ratio::one()));
        let inches = q.convert_to(&UnitExpr::named("in"), &reg).expect("to in");
        let back = inches.convert_to(&UnitExpr::named("ft"), &reg).expect("to ft");
        prop_assert_eq!(back.exact_ratio(), q.exact_ratio());
        prop_assert_eq!(back.unit.as_str(), "ft");
    }

    #[test]
    fn rational_conversion_composes(n in 1i32..5_000) {
        let reg = RegistryBuilder::from_seed().freeze();
        let q = kip::Quantity::from_int(i128::from(n), "kip", {
            kip::Dimension::single(kip::BaseDim::Force, Ratio::one())
        });
        let lbf = q.convert_to(&UnitExpr::named("lbf"), &reg).expect("kip->lbf");
        let kip = lbf.convert_to(&UnitExpr::named("kip"), &reg).expect("lbf->kip");
        prop_assert_eq!(kip.exact_ratio(), q.exact_ratio());
    }
}
