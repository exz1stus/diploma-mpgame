use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Range {
    pub min: f64,
    pub max: f64,
}

impl Range {
    /// The whole normalized height domain, [0, 1].
    pub fn full_height() -> Self {
        Self { min: 0.0, max: 1.0 }
    }

    /// Matches every finite value.
    pub fn unbounded() -> Self {
        Self {
            min: -f64::MAX,
            max: f64::MAX,
        }
    }

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

    /// Valid, and within [0, 1] as normalized heights are.
    pub(crate) fn is_valid_height(&self) -> bool {
        self.is_valid() && self.min >= 0.0 && self.max <= 1.0
    }
}
