use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Range {
    pub min: f64,
    pub max: f64,
}

impl Range {
    /// [min, max).
    pub fn in_range(&self, value: f64) -> bool {
        self.min <= value && value < self.max
    }

    /// Include the clamped terrain endpoint when this range ends at height 1.
    pub fn in_height_range(&self, value: f64) -> bool {
        self.in_range(value) || (value == 1.0 && self.max == 1.0)
    }

    pub(crate) fn is_valid(&self) -> bool {
        self.min.is_finite() && self.max.is_finite() && self.min < self.max
    }
}
