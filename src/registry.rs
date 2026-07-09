//! Frozen unit registry and copy-on-write builder.

pub mod defs;
pub mod seed;

use std::collections::BTreeMap;
use std::sync::Arc;

use num_rational::Ratio;
use num_traits::One;

use crate::dim::{BaseDim, CustomDimId, Dimension};
use crate::diag::{Diag, Diagnostic, ErrorCode, Span};
use crate::eval::value::Quantity;

pub use defs::parse_defs;

/// Stable unit identifier within a registry generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitId(pub u32);

/// Record for a registered linear unit.
#[derive(Debug, Clone)]
pub struct UnitRecord {
    /// Unit id (set at freeze time).
    pub id: UnitId,
    /// Canonical primary name.
    pub name: String,
    /// Aliases (e.g. `kips` for `kip`).
    pub aliases: Vec<String>,
    /// Dimension of this unit.
    pub dimension: BaseDim,
    /// Exact ratio to the dimension's anchor unit.
    pub anchor_ratio: Ratio<i128>,
    /// Whether this is an affine temperature view (cannot anchor).
    pub affine: bool,
}

/// Immutable frozen registry (generation N).
#[derive(Debug, Clone)]
pub struct Registry {
    /// Generation number (0 = built-in seed).
    pub generation: u32,
    /// Anchor unit per base dimension.
    pub anchors: BTreeMap<BaseDim, UnitId>,
    /// All units by id.
    units: Arc<BTreeMap<UnitId, UnitRecord>>,
    /// Name → unit id lookup (includes aliases).
    name_index: Arc<BTreeMap<String, UnitId>>,
    /// Custom dimension names.
    custom_dims: Arc<BTreeMap<String, CustomDimId>>,
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

    /// Anchor unit id for a base dimension.
    pub fn anchor(&self, dim: BaseDim) -> Option<UnitId> {
        self.anchors.get(&dim).copied()
    }

    /// Iterate registered units.
    pub fn units(&self) -> impl Iterator<Item = (&UnitId, &UnitRecord)> {
        self.units.iter()
    }

    /// Begin a new generation extending this registry (COW).
    pub fn extend(&self) -> RegistryBuilder {
        RegistryBuilder::from_registry(self)
    }

    /// Emit canonical `define` lines (M2 round-trip expands with anchors).
    pub fn dump_defs(&self) -> String {
        let mut lines = Vec::new();
        for (_, unit) in self.units.iter() {
            if unit.affine {
                continue;
            }
            let names = if unit.aliases.is_empty() {
                unit.name.clone()
            } else {
                format!("{}, {}", unit.name, unit.aliases.join(", "))
            };
            if self.anchors.get(&unit.dimension) == Some(&unit.id) || unit.anchor_ratio == Ratio::one() {
                lines.push(format!("define {names}"));
            } else {
                let anchor_name = self
                    .anchors
                    .get(&unit.dimension)
                    .and_then(|id| self.units.get(id))
                    .map(|u| u.name.as_str())
                    .unwrap_or("?");
                lines.push(format!(
                    "define {names} = {} {anchor_name}",
                    unit.anchor_ratio
                ));
            }
        }
        lines.sort();
        lines.join("\n")
    }
}

/// Mutable registry builder; freezes into immutable [`Registry`].
#[derive(Debug, Clone)]
pub struct RegistryBuilder {
    pub(crate) generation: u32,
    pub(crate) anchors: BTreeMap<BaseDim, String>,
    pending_anchors: BTreeMap<BaseDim, String>,
    pub(crate) units: BTreeMap<String, UnitRecord>,
    custom_dims: BTreeMap<String, CustomDimId>,
    next_custom_dim: u32,
    #[allow(dead_code)]
    defs_src: Vec<String>,
}

impl RegistryBuilder {
    /// Generation-0 builder with built-in imperial seed data.
    pub fn from_seed() -> Self {
        seed::seed_builder()
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
                units.insert(
                    alias.clone(),
                    UnitRecord {
                        id: u.id,
                        name: u.name.clone(),
                        aliases: u.aliases.clone(),
                        dimension: u.dimension,
                        anchor_ratio: u.anchor_ratio,
                        affine: u.affine,
                    },
                );
            }
        }
        Self {
            generation: reg.generation + 1,
            anchors,
            pending_anchors: BTreeMap::new(),
            units,
            custom_dims: reg.custom_dims.as_ref().clone(),
            next_custom_dim: reg.custom_dims.len() as u32,
            defs_src: Vec::new(),
        }
    }

    /// Parse `define` / `dimension` / `anchor` lines from text (M2).
    pub fn parse_defs(&mut self, src: &str) -> Result<(), Diag> {
        parse_defs(self, src)
    }

    /// Programmatic unit definition (same checks as text form).
    pub fn define(
        &mut self,
        primary: &str,
        aliases: &[&str],
        qty: Quantity,
    ) -> Result<(), Diag> {
        if self.units.contains_key(primary) {
            return Err(Diag::new(Diagnostic::error(
                ErrorCode::DupUnit,
                format!("duplicate unit `{primary}`"),
                Span::empty(0),
            )));
        }
        let dimension = infer_base_dim(&qty.dim)?;
        let anchor_ratio = qty.magnitude;
        let record = UnitRecord {
            id: UnitId(0),
            name: primary.into(),
            aliases: aliases.iter().map(|s| (*s).into()).collect(),
            dimension,
            anchor_ratio,
            affine: false,
        };
        self.units.insert(primary.into(), record);
        for alias in aliases {
            if self.units.contains_key(*alias) {
                return Err(Diag::new(Diagnostic::error(
                    ErrorCode::DupUnit,
                    format!("duplicate unit `{alias}`"),
                    Span::empty(0),
                )));
            }
        }
        Ok(())
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

    /// Re-anchor a built-in dimension to a registered linear unit (M2).
    pub fn set_anchor(&mut self, dim: BaseDim, unit_name: &str) -> Result<(), Diag> {
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
        if unit.dimension != dim {
            return Err(Diag::new(Diagnostic::error(
                ErrorCode::AnchorInvalid,
                format!("unit `{unit_name}` is not a linear unit of {dim:?}"),
                Span::empty(0),
            )));
        }
        self.pending_anchors.insert(dim, unit_name.into());
        Ok(())
    }

    /// Freeze into an immutable shared registry.
    pub fn freeze(self) -> Arc<Registry> {
        let mut anchors = self.anchors.clone();
        for (dim, name) in &self.pending_anchors {
            anchors.insert(*dim, name.clone());
        }

        let mut units_by_id = BTreeMap::new();
        let mut name_index = BTreeMap::new();
        let mut next_id = 0u32;
        let mut seen_primary = BTreeMap::new();

        for (key, unit) in &self.units {
            if key != &unit.name {
                continue;
            }
            let id = UnitId(next_id);
            next_id += 1;
            seen_primary.insert(unit.name.clone(), id);
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

        let mut anchor_ids = BTreeMap::new();
        for (dim, name) in &anchors {
            if let Some(&id) = name_index.get(name) {
                anchor_ids.insert(*dim, id);
            }
        }

        Arc::new(Registry {
            generation: self.generation,
            anchors: anchor_ids,
            units: Arc::new(units_by_id),
            name_index: Arc::new(name_index),
            custom_dims: Arc::new(self.custom_dims),
        })
    }
}

fn infer_base_dim(dim: &Dimension) -> Result<BaseDim, Diag> {
    if dim.is_dimensionless() {
        return Err(Diag::new(Diagnostic::error(
            ErrorCode::DefSymbolic,
            "unit definition requires a dimensional quantity",
            Span::empty(0),
        )));
    }
    if dim.exponents().len() != 1 {
        return Err(Diag::new(Diagnostic::error(
            ErrorCode::DefSymbolic,
            "compound dimensions in define must be reduced in M2",
            Span::empty(0),
        )));
    }
    Ok(dim.exponents()[0].0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dim::Dimension;

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
    fn define_custom_unit() {
        let mut b = RegistryBuilder::from_seed();
        b.define(
            "tonf",
            &["tons"],
            crate::eval::value::Quantity::from_int(
                2000,
                "lbf",
                Dimension::single(BaseDim::Force, Ratio::one()),
            ),
        )
        .unwrap();
        let reg = b.freeze();
        assert!(reg.unit("tonf").is_some());
        assert!(reg.unit("tons").is_some());
    }

    #[test]
    fn generation_increment_on_extend() {
        let reg = RegistryBuilder::from_seed().freeze();
        let b = reg.extend();
        assert_eq!(b.generation, 1);
    }
}
