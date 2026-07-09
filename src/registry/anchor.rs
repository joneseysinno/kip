//! Dimension anchor key (built-in or custom base dimension).

use crate::dim::{BaseDim, CustomDimId};

/// Which dimension an anchor unit belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DimAnchor {
    /// Built-in base dimension.
    Base(BaseDim),
    /// User-declared `dimension` name.
    Custom(CustomDimId),
}

impl DimAnchor {
    /// Parse built-in dimension name from `anchor Length = ft`.
    pub fn parse_base_name(name: &str) -> Option<BaseDim> {
        match name {
            "Length" => Some(BaseDim::Length),
            "Force" => Some(BaseDim::Force),
            "Time" => Some(BaseDim::Time),
            "Temperature" => Some(BaseDim::Temperature),
            "Angle" => Some(BaseDim::Angle),
            _ => None,
        }
    }

    /// Display name for dump.
    pub fn display_name(self, custom_names: &[(CustomDimId, String)]) -> String {
        match self {
            Self::Base(BaseDim::Length) => "Length".into(),
            Self::Base(BaseDim::Force) => "Force".into(),
            Self::Base(BaseDim::Time) => "Time".into(),
            Self::Base(BaseDim::Temperature) => "Temperature".into(),
            Self::Base(BaseDim::Angle) => "Angle".into(),
            Self::Base(BaseDim::Custom(id)) => custom_names
                .iter()
                .find(|(cid, _)| *cid == id)
                .map(|(_, n)| n.clone())
                .unwrap_or_else(|| format!("Custom({id:?})")),
            Self::Custom(id) => custom_names
                .iter()
                .find(|(cid, _)| *cid == id)
                .map(|(_, n)| n.clone())
                .unwrap_or_else(|| format!("Custom({id:?})")),
        }
    }
}
