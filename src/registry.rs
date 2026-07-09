//! Frozen unit registry and copy-on-write builder.

pub mod anchor;
pub mod defs;
pub mod eval_expr;
pub mod resolve;
pub mod seed;

use std::collections::BTreeMap;
use std::sync::Arc;

use num_rational::Ratio;
use num_traits::{One, Zero};

use anchor::DimAnchor;
use crate::dim::{BaseDim, CustomDimId, Dimension};
use crate::diag::{Diag, Diagnostic, ErrorCode, Span};
use crate::eval::value::Quantity;
use crate::quantity::UnitExpr;

#[cfg(feature = "packs")]
use crate::packs::equation::{EquationRecord, EquationRegistry};
#[cfg(not(feature = "packs"))]
use crate::packs::equation::EquationRegistry;

pub use defs::parse_defs;

/// Stable unit identifier within a registry generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitId(pub u32);

/// Record for a registered unit.
#[derive(Debug, Clone)]
pub struct UnitRecord {
    /// Unit id (set at freeze time).
    pub id: UnitId,
    /// Canonical primary name.
    pub name: String,
    /// Aliases (e.g. `kips` for `kip`).
    pub aliases: Vec<String>,
    /// Full dimension vector.
    pub dimension: Dimension,
    /// Exact ratio to the dimension's anchor unit.
    pub anchor_ratio: Ratio<i128>,
    /// Whether this is an affine temperature view (cannot anchor).
    pub affine: bool,
}

/// Read-only unit lookup for definition expression evaluation.
#[derive(Debug, Clone)]
pub struct UnitLookup {
    units: BTreeMap<String, UnitRecord>,
}

impl UnitLookup {
    /// Snapshot from a builder.
    pub fn from_builder(builder: &RegistryBuilder) -> Self {
        Self {
            units: builder.primary_units(),
        }
    }

    /// Snapshot from a frozen registry.
    #[allow(dead_code)]
    pub fn from_registry(reg: &Registry) -> Self {
        let mut units = BTreeMap::new();
        for (_, u) in reg.units() {
            units.insert(u.name.clone(), u.clone());
            for a in &u.aliases {
                units.insert(a.clone(), u.clone());
            }
        }
        Self { units }
    }

    /// Lookup by name.
    pub fn get(&self, name: &str) -> Option<&UnitRecord> {
        self.units.get(name)
    }
}

/// Immutable frozen registry (generation N).
#[derive(Debug, Clone)]
pub struct Registry {
    /// Generation number (0 = built-in seed).
    pub generation: u32,
    /// Anchor unit per dimension.
    pub anchors: BTreeMap<DimAnchor, UnitId>,
    units: Arc<BTreeMap<UnitId, UnitRecord>>,
    name_index: Arc<BTreeMap<String, UnitId>>,
    custom_dims: Arc<BTreeMap<String, CustomDimId>>,
    custom_dim_names: Arc<Vec<(CustomDimId, String)>>,
    equations: EquationRegistry,
}

impl Registry {
    /// Generation number.
    pub fn generation(&self) -> u32 {
        self.generation
    }

    /// Lookup unit by name (case-sensitive per grammar).
    pub fn unit(&self, name: &str) -> Option<&UnitRecord> {
        self.name_index
            .get(name)
            .and_then(|id| self.units.get(id))
    }

    /// Anchor unit id for a built-in base dimension.
    pub fn anchor(&self, dim: BaseDim) -> Option<UnitId> {
        self.anchors.get(&DimAnchor::Base(dim)).copied()
    }

    /// Iterate registered primary units.
    pub fn units(&self) -> impl Iterator<Item = (&UnitId, &UnitRecord)> {
        self.units.iter()
    }

    /// Custom dimension id by name.
    pub fn custom_dimension(&self, name: &str) -> Option<CustomDimId> {
        self.custom_dims.get(name).copied()
    }

    /// Loaded code equations for this registry generation.
    pub fn equations(&self) -> &EquationRegistry {
        &self.equations
    }

    /// Begin a new generation extending this registry (COW).
    pub fn extend(&self) -> RegistryBuilder {
        RegistryBuilder::from_registry(self)
    }

    /// Emit canonical `define` / `dimension` / `anchor` lines for round-trip.
    pub fn dump_defs(&self) -> String {
        let mut lines = Vec::new();

        for (name, _) in self.custom_dims.iter() {
            lines.push(format!("dimension {name}"));
        }

        for (dim_anchor, unit_id) in &self.anchors {
            let default = default_anchor_name(dim_anchor);
            let unit = self.units.get(unit_id).map(|u| u.name.as_str()).unwrap_or("");
            if default != unit {
                let dim_name = dim_anchor.display_name(self.custom_dim_names.as_ref());
                lines.push(format!("anchor {dim_name} = {unit}"));
            }
        }

        for (_, unit) in self.units.iter() {
            if unit.affine {
                continue;
            }
            if is_seed_builtin(&unit.name) {
                continue;
            }
            let names = format_names(unit);
            if self.anchors.values().any(|id| *id == unit.id) {
                if let Some(dim_name) = self.custom_dim_name_for_anchor(unit.id) {
                    lines.push(format!("define {names} : {dim_name}"));
                }
                continue;
            }
            if unit.anchor_ratio == Ratio::one() {
                let anchor_name = self.anchor_unit_name_for_dim(&unit.dimension);
                lines.push(format!("define {names} = 1 {anchor_name}"));
            } else {
                let anchor_name = self.anchor_unit_name_for_dim(&unit.dimension);
                lines.push(format!(
                    "define {names} = {} {anchor_name}",
                    unit.anchor_ratio
                ));
            }
        }

        lines.sort();
        lines.join("\n")
    }

    fn custom_dim_name_for_anchor(&self, unit_id: UnitId) -> Option<String> {
        for (dim_anchor, id) in &self.anchors {
            if *id == unit_id {
                if let DimAnchor::Custom(cid) = dim_anchor {
                    return self
                        .custom_dim_names
                        .iter()
                        .find(|(id, _)| *id == *cid)
                        .map(|(_, n)| n.clone());
                }
            }
        }
        None
    }

    fn anchor_unit_name_for_dim(&self, dim: &Dimension) -> String {
        if dim.exponents().len() == 1 {
            let (base, _) = dim.exponents()[0];
            if let Some(id) = self.anchors.get(&DimAnchor::Base(base)) {
                if let Some(u) = self.units.get(id) {
                    return u.name.clone();
                }
            }
        }
        "1".into()
    }
}

fn format_names(unit: &UnitRecord) -> String {
    if unit.aliases.is_empty() {
        unit.name.clone()
    } else {
        format!("{}, {}", unit.name, unit.aliases.join(", "))
    }
}

fn default_anchor_name(dim: &DimAnchor) -> &'static str {
    match dim {
        DimAnchor::Base(BaseDim::Length) => "in",
        DimAnchor::Base(BaseDim::Force) => "lbf",
        DimAnchor::Base(BaseDim::Time) => "s",
        DimAnchor::Base(BaseDim::Temperature) => "°R",
        DimAnchor::Base(BaseDim::Angle) => "rad",
        DimAnchor::Base(BaseDim::Custom(_)) => "",
        DimAnchor::Custom(_) => "",
    }
}

fn is_seed_builtin(name: &str) -> bool {
    matches!(
        name,
        "in" | "lbf"
            | "s"
            | "°R"
            | "R"
            | "rad"
            | "ft"
            | "yd"
            | "mi"
            | "mil"
            | "kip"
            | "kips"
            | "psi"
            | "ksi"
            | "psf"
            | "ksf"
            | "plf"
            | "klf"
            | "pcf"
            | "lbf·ft"
            | "lbf*ft"
            | "kip·ft"
            | "kip*ft"
            | "kip·in"
            | "kip*in"
            | "slug"
            | "lbm"
            | "min"
            | "hr"
            | "deg"
            | "°"
            | "°F"
            | "°C"
            | "K"
            | "%"
    )
}

/// Mutable registry builder; freezes into immutable [`Registry`].
#[derive(Debug, Clone)]
pub struct RegistryBuilder {
    pub(crate) generation: u32,
    pub(crate) anchors: BTreeMap<DimAnchor, String>,
    pub(crate) pending_anchors: BTreeMap<DimAnchor, String>,
    pub(crate) units: BTreeMap<String, UnitRecord>,
    pub(crate) custom_dims: BTreeMap<String, CustomDimId>,
    pub(crate) unit_spans: BTreeMap<String, Span>,
    next_custom_dim: u32,
    #[cfg(feature = "packs")]
    pub(crate) equations: BTreeMap<String, Arc<EquationRecord>>,
}

impl RegistryBuilder {
    /// Generation-0 builder with built-in imperial seed data.
    pub fn from_seed() -> Self {
        seed::seed_builder()
    }

    pub(crate) fn new_empty(generation: u32) -> Self {
        Self {
            generation,
            anchors: BTreeMap::new(),
            pending_anchors: BTreeMap::new(),
            units: BTreeMap::new(),
            custom_dims: BTreeMap::new(),
            unit_spans: BTreeMap::new(),
            next_custom_dim: 0,
            #[cfg(feature = "packs")]
            equations: BTreeMap::new(),
        }
    }

    /// Extend an existing frozen registry (COW).
    pub fn from_registry(reg: &Registry) -> Self {
        let mut anchors = BTreeMap::new();
        for (dim, id) in &reg.anchors {
            if let Some(u) = reg.units.get(id) {
                anchors.insert(*dim, u.name.clone());
            }
        }
        let mut units = BTreeMap::new();
        for (_, u) in reg.units.iter() {
            units.insert(u.name.clone(), u.clone());
            for alias in &u.aliases {
                units.insert(alias.clone(), u.clone());
            }
        }
        let mut equations = BTreeMap::new();
        for eq in reg.equations.equations() {
            equations.insert(eq.path_key.clone(), eq.clone());
        }
        Self {
            generation: reg.generation + 1,
            anchors,
            pending_anchors: BTreeMap::new(),
            units,
            custom_dims: reg.custom_dims.as_ref().clone(),
            unit_spans: BTreeMap::new(),
            next_custom_dim: reg.custom_dims.len() as u32,
            #[cfg(feature = "packs")]
            equations,
        }
    }

    /// Parse `define` / `dimension` / `anchor` lines from text.
    pub fn parse_defs(&mut self, src: &str) -> Result<(), Diag> {
        parse_defs(self, src)
    }

    /// Load equation packs from TOML (requires `packs` feature).
    pub fn load_packs(&mut self, src: &str) -> Result<(), Diag> {
        #[cfg(feature = "packs")]
        {
            crate::packs::loader::load_packs_into(self, src)
        }
        #[cfg(not(feature = "packs"))]
        {
            let _ = src;
            Err(Diag::new(Diagnostic::error(
                ErrorCode::PackParse,
                "equation packs require the `packs` feature",
                Span::empty(0),
            )))
        }
    }

    /// Programmatic unit definition (same checks as text form).
    pub fn define(
        &mut self,
        primary: &str,
        aliases: &[&str],
        qty: Quantity,
    ) -> Result<(), Diag> {
        self.insert_resolved_unit(primary, aliases, qty, Span::empty(0))
    }

    /// Declare a new custom base dimension.
    pub fn new_dimension(&mut self, name: &str) -> Result<(), Diag> {
        if self.custom_dims.contains_key(name) {
            return Err(Diag::new(Diagnostic::error(
                ErrorCode::DupUnit,
                format!("duplicate dimension `{name}`"),
                Span::empty(0),
            )));
        }
        let id = CustomDimId(self.next_custom_dim);
        self.next_custom_dim += 1;
        self.custom_dims.insert(name.into(), id);
        Ok(())
    }

    /// Re-anchor a built-in dimension to a registered linear unit.
    pub fn set_anchor(&mut self, dim: BaseDim, unit_name: &str) -> Result<(), Diag> {
        let key = DimAnchor::Base(dim);
        if self.pending_anchors.contains_key(&key) {
            return Err(Diag::new(Diagnostic::error(
                ErrorCode::DupAnchor,
                format!("duplicate anchor for dimension `{dim:?}`"),
                Span::empty(0),
            )));
        }
        let unit = self.units.get(unit_name).ok_or_else(|| {
            Diag::new(Diagnostic::error(
                ErrorCode::AnchorInvalid,
                format!("unknown anchor unit `{unit_name}`"),
                Span::empty(0),
            ))
        })?;
        if unit.affine {
            return Err(Diag::new(Diagnostic::error(
                ErrorCode::AnchorAffine,
                format!("affine unit `{unit_name}` cannot anchor a dimension"),
                Span::empty(0),
            )));
        }
        if !unit_dimension_matches_base(&unit.dimension, dim) {
            return Err(Diag::new(Diagnostic::error(
                ErrorCode::AnchorInvalid,
                format!("unit `{unit_name}` is not a linear unit of {dim:?}"),
                Span::empty(0),
            )));
        }
        self.pending_anchors.insert(key, unit_name.into());
        Ok(())
    }

    /// Freeze into an immutable shared registry.
    pub fn freeze(mut self) -> Arc<Registry> {
        self.rebase_anchors();

        let mut units_by_id = BTreeMap::new();
        let mut name_index = BTreeMap::new();
        let mut next_id = 0u32;

        for (key, unit) in &self.units {
            if key != &unit.name {
                continue;
            }
            let id = UnitId(next_id);
            next_id += 1;
            let mut unit = unit.clone();
            unit.id = id;
            let primary = unit.name.clone();
            let aliases: Vec<String> = unit.aliases.clone();
            units_by_id.insert(id, unit);
            name_index.insert(primary, id);
            for alias in aliases {
                name_index.insert(alias, id);
            }
        }

        let mut anchors = self.anchors.clone();
        for (dim, name) in &self.pending_anchors {
            anchors.insert(*dim, name.clone());
        }

        let mut anchor_ids = BTreeMap::new();
        for (dim, name) in &anchors {
            if let Some(&id) = name_index.get(name) {
                anchor_ids.insert(*dim, id);
            }
        }

        let custom_dim_names: Vec<(CustomDimId, String)> = self
            .custom_dims
            .iter()
            .map(|(n, id)| (*id, n.clone()))
            .collect();

        Arc::new(Registry {
            generation: self.generation,
            anchors: anchor_ids,
            units: Arc::new(units_by_id),
            name_index: Arc::new(name_index),
            custom_dims: Arc::new(self.custom_dims),
            custom_dim_names: Arc::new(custom_dim_names),
            equations: {
                #[cfg(feature = "packs")]
                {
                    EquationRegistry::from_map(self.equations)
                }
                #[cfg(not(feature = "packs"))]
                {
                    EquationRegistry::empty()
                }
            },
        })
    }

    pub(crate) fn primary_units(&self) -> BTreeMap<String, UnitRecord> {
        let mut map = BTreeMap::new();
        for (key, unit) in &self.units {
            if key == &unit.name {
                map.insert(key.clone(), unit.clone());
            }
        }
        map
    }

    pub(crate) fn insert_resolved_unit(
        &mut self,
        primary: &str,
        aliases: &[&str],
        qty: Quantity,
        span: Span,
    ) -> Result<(), Diag> {
        let anchor_ratio = resolve_anchor_ratio(&qty, self)?;
        self.insert_unit(
            primary,
            aliases,
            qty.dim,
            anchor_ratio,
            false,
            span,
        )
    }

    pub(crate) fn insert_unit(
        &mut self,
        primary: &str,
        aliases: &[&str],
        dimension: Dimension,
        anchor_ratio: Ratio<i128>,
        affine: bool,
        span: Span,
    ) -> Result<(), Diag> {
        if self.units.contains_key(primary) {
            return Err(Diag::new(Diagnostic::error(
                ErrorCode::DupUnit,
                format!("duplicate unit `{primary}`"),
                span,
            )));
        }
        let record = UnitRecord {
            id: UnitId(0),
            name: primary.into(),
            aliases: aliases.iter().map(|s| (*s).into()).collect(),
            dimension,
            anchor_ratio,
            affine,
        };
        self.units.insert(primary.into(), record.clone());
        self.unit_spans.insert(primary.into(), span);
        for alias in aliases {
            if self.units.contains_key(*alias) {
                return Err(Diag::new(Diagnostic::error(
                    ErrorCode::DupUnit,
                    format!("duplicate unit `{alias}`"),
                    span,
                )));
            }
            self.units.insert((*alias).into(), record.clone());
            self.unit_spans.insert((*alias).into(), span);
        }
        Ok(())
    }

    fn rebase_anchors(&mut self) {
        let pending: Vec<_> = self.pending_anchors.clone().into_iter().collect();
        for (dim_anchor, new_anchor_name) in pending {
            let Some(factor) = self.units.get(&new_anchor_name).map(|u| u.anchor_ratio) else {
                continue;
            };
            if factor.is_zero() {
                continue;
            }
            let primaries: Vec<String> = self
                .units
                .iter()
                .filter(|(k, u)| *k == &u.name)
                .map(|(_, u)| u.name.clone())
                .collect();
            for name in primaries {
                let Some(unit) = self.units.get(&name).cloned() else {
                    continue;
                };
                let exp = exponent_for_anchor(&unit.dimension, &dim_anchor, &self.custom_dims);
                if exp.is_zero() || exp.denom() != &1 {
                    continue;
                }
                let e = *exp.numer();
                let divisor = if e >= 0 {
                    factor.pow(e)
                } else {
                    Ratio::one() / factor.pow(-e)
                };
                let mut updated = unit;
                updated.anchor_ratio /= divisor;
                let aliases = updated.aliases.clone();
                let primary = updated.name.clone();
                self.units.insert(primary.clone(), updated.clone());
                for a in aliases {
                    self.units.insert(a, updated.clone());
                }
            }
            self.anchors.insert(dim_anchor, new_anchor_name);
        }
        self.pending_anchors.clear();
    }
}

fn resolve_anchor_ratio(qty: &Quantity, builder: &RegistryBuilder) -> Result<Ratio<i128>, Diag> {
    let mag = qty.mag.exact_ratio().ok_or_else(|| {
        Diag::new(Diagnostic::error(
            ErrorCode::DefSymbolic,
            "anchor resolution requires exact magnitudes",
            Span::empty(0),
        ))
    })?;
    match &qty.unit {
        UnitExpr::Dimensionless => Ok(mag),
        UnitExpr::Named(name) => {
            let record = builder.units.get(name).ok_or_else(|| {
                Diag::new(Diagnostic::error(
                    ErrorCode::DefSymbolic,
                    format!("unknown unit `{name}`"),
                    Span::empty(0),
                ))
            })?;
            Ok(mag * record.anchor_ratio)
        }
        UnitExpr::Product(parts) => {
            let mut ratio = mag;
            for part in parts {
                let part_ratio = resolve_anchor_ratio(
                    &Quantity::from_exact(Ratio::one(), part.clone(), qty.dim.clone()),
                    builder,
                )?;
                ratio *= part_ratio;
            }
            Ok(ratio)
        }
        UnitExpr::Quotient(num, den) => {
            let num_r = resolve_anchor_ratio(
                &Quantity::from_exact(mag, *num.clone(), qty.dim.clone()),
                builder,
            )?;
            let den_r = resolve_anchor_ratio(
                &Quantity::from_exact(Ratio::one(), *den.clone(), qty.dim.clone()),
                builder,
            )?;
            Ok(num_r / den_r)
        }
        UnitExpr::Pow { base, exp } => {
            let base_r = resolve_anchor_ratio(
                &Quantity::from_exact(mag, *base.clone(), qty.dim.clone()),
                builder,
            )?;
            let e = unit_exponent_to_i32(exp)?;
            Ok(if e >= 0 {
                base_r.pow(e)
            } else {
                Ratio::one() / base_r.pow(-e)
            })
        }
    }
}

fn unit_exponent_to_i32(exp: &crate::quantity::UnitExponent) -> Result<i32, Diag> {
    match exp {
        crate::quantity::UnitExponent::Int(n) => Ok(*n),
        crate::quantity::UnitExponent::Ratio { num, den } => {
            if *den == 0 || *num % den != 0 {
                return Err(Diag::new(Diagnostic::error(
                    ErrorCode::DefSymbolic,
                    "non-integer unit exponent in definition",
                    Span::empty(0),
                )));
            }
            Ok(num / den)
        }
        crate::quantity::UnitExponent::Decimal(_) => Err(Diag::new(Diagnostic::error(
            ErrorCode::DefSymbolic,
            "decimal unit exponent in definition",
            Span::empty(0),
        ))),
    }
}

fn unit_dimension_matches_base(dim: &Dimension, base: BaseDim) -> bool {
    dim.exponents().len() == 1 && dim.exponents()[0].0 == base
}

fn exponent_for_anchor(
    dim: &Dimension,
    anchor: &DimAnchor,
    _custom_dims: &BTreeMap<String, CustomDimId>,
) -> Ratio<i32> {
    match anchor {
        DimAnchor::Base(b) => dim
            .exponents()
            .iter()
            .find(|(d, _)| d == b)
            .map(|(_, e)| *e)
            .unwrap_or_else(Ratio::zero),
        DimAnchor::Custom(id) => {
            let base = BaseDim::Custom(*id);
            dim.exponents()
                .iter()
                .find(|(d, _)| *d == base)
                .map(|(_, e)| *e)
                .unwrap_or_else(Ratio::zero)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_registry_has_imperial_anchors() {
        let reg = RegistryBuilder::from_seed().freeze();
        assert!(reg.unit("in").is_some());
        assert!(reg.unit("lbf").is_some());
        assert!(reg.unit("ft").is_some());
        assert!(reg.unit("kip").is_some());
        assert_eq!(reg.generation(), 0);
    }

    #[test]
    fn parse_define_kip_aliases() {
        let mut b = RegistryBuilder::from_seed();
        b.parse_defs("define tonf, tons = 2000 lbf").unwrap();
        let reg = b.freeze();
        assert!(reg.unit("tonf").is_some());
        assert!(reg.unit("tons").is_some());
    }

    #[test]
    fn dump_defs_round_trip() {
        let src = "define tonf, tons = 2000 lbf";
        let mut b = RegistryBuilder::from_seed();
        b.parse_defs(src).unwrap();
        let reg = b.freeze();
        let dumped = reg.dump_defs();
        let mut b2 = RegistryBuilder::from_seed();
        b2.parse_defs(&dumped).unwrap();
        let reg2 = b2.freeze();
        let u1 = reg.unit("tonf").unwrap();
        let u2 = reg2.unit("tonf").unwrap();
        assert_eq!(u1.anchor_ratio, u2.anchor_ratio);
        assert_eq!(u1.dimension, u2.dimension);
    }

    #[test]
    fn def_cycle_error() {
        let mut b = RegistryBuilder::from_seed();
        let err = b
            .parse_defs("define a = 2 b\ndefine b = 3 a")
            .unwrap_err();
        assert_eq!(err.diagnostic().code, ErrorCode::DefCycle.as_str());
    }

    #[test]
    fn def_symbolic_error() {
        let mut b = RegistryBuilder::from_seed();
        let err = b.parse_defs("define x = 2 * L").unwrap_err();
        assert_eq!(err.diagnostic().code, ErrorCode::DefSymbolic.as_str());
    }

    #[test]
    fn generation_increment_on_extend() {
        let reg = RegistryBuilder::from_seed().freeze();
        let b = reg.extend();
        assert_eq!(b.generation, 1);
    }
}
