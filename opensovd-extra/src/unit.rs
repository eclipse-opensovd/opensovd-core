// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! SOVD unit types.

use serde::{Deserialize, Serialize};

/// Physical dimension with SI base unit exponents.
///
/// Represents dimensional analysis using the seven SI base units plus plane angle.
/// Only non-zero exponents are serialized.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhysicalDimension {
    /// Identifier for the dimension (e.g., "voltage", "speed")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Exponent for length (m)
    #[serde(default, skip_serializing_if = "is_zero")]
    pub length: i8,
    /// Exponent for mass (kg)
    #[serde(default, skip_serializing_if = "is_zero")]
    pub mass: i8,
    /// Exponent for time (s)
    #[serde(default, skip_serializing_if = "is_zero")]
    pub time: i8,
    /// Exponent for electric current (A)
    #[serde(default, skip_serializing_if = "is_zero")]
    pub current: i8,
    /// Exponent for thermodynamic temperature (K)
    #[serde(default, skip_serializing_if = "is_zero")]
    pub temperature: i8,
    /// Exponent for molar amount (mol)
    #[serde(default, skip_serializing_if = "is_zero")]
    pub molar_amount: i8,
    /// Exponent for luminous intensity (cd)
    #[serde(default, skip_serializing_if = "is_zero")]
    pub luminous_intensity: i8,
    /// Exponent for plane angle (rad)
    #[serde(default, skip_serializing_if = "is_zero")]
    pub plane_angle: i8,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_zero(v: &i8) -> bool {
    *v == 0
}

/// Unit information.
///
/// Provides display name, conversion factors, and physical dimension for a quantity.
/// Conversion formula: `value_display = factor_si_to_unit * value_si + offset_si_to_unit`
///
/// ## Default Values
/// - `factor_si_to_unit`: 1.0 (M - mandatory with default)
/// - `offset_si_to_unit`: 0.0 (O - optional, omitted when zero)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Unit {
    /// Display string for the unit (e.g., "km/h", "deg C", "V") - Mandatory
    pub display_name: String,
    /// Reference identifier for the unit definition - Optional
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    /// Multiplicative conversion factor from SI to display unit - Mandatory, default: 1.0
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub factor_si_to_unit: f64,
    /// Additive conversion offset from SI to display unit - Optional, default: 0.0
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub offset_si_to_unit: f64,
    /// Physical dimension of the quantity - Optional
    #[serde(skip_serializing_if = "Option::is_none")]
    pub physical_dimension: Option<PhysicalDimension>,
}

const fn one() -> f64 {
    1.0
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_one(v: &f64) -> bool {
    (*v - 1.0).abs() < f64::EPSILON
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero_f64(v: &f64) -> bool {
    v.abs() < f64::EPSILON
}

impl Unit {
    /// Create a new unit with just a display name.
    #[must_use]
    pub fn new(display_name: impl Into<String>) -> Self {
        Self {
            display_name: display_name.into(),
            reference: None,
            factor_si_to_unit: 1.0,
            offset_si_to_unit: 0.0,
            physical_dimension: None,
        }
    }

    /// Set the physical dimension.
    #[must_use]
    pub fn with_dimension(mut self, dimension: PhysicalDimension) -> Self {
        self.physical_dimension = Some(dimension);
        self
    }

    /// Set the conversion factor.
    #[must_use]
    pub const fn with_factor(mut self, factor: f64) -> Self {
        self.factor_si_to_unit = factor;
        self
    }

    /// Set the conversion offset.
    #[must_use]
    pub const fn with_offset(mut self, offset: f64) -> Self {
        self.offset_si_to_unit = offset;
        self
    }
}

/// Common physical dimensions as constants.
impl PhysicalDimension {
    /// Voltage: V = kg*m^2*s^-3*A^-1
    pub const VOLTAGE: Self = Self {
        id: None,
        length: 2,
        mass: 1,
        time: -3,
        current: -1,
        temperature: 0,
        molar_amount: 0,
        luminous_intensity: 0,
        plane_angle: 0,
    };

    /// Temperature: K
    pub const TEMPERATURE: Self = Self {
        id: None,
        length: 0,
        mass: 0,
        time: 0,
        current: 0,
        temperature: 1,
        molar_amount: 0,
        luminous_intensity: 0,
        plane_angle: 0,
    };

    /// Time: s
    pub const TIME: Self = Self {
        id: None,
        length: 0,
        mass: 0,
        time: 1,
        current: 0,
        temperature: 0,
        molar_amount: 0,
        luminous_intensity: 0,
        plane_angle: 0,
    };

    /// Speed: m/s = m*s^-1
    pub const SPEED: Self = Self {
        id: None,
        length: 1,
        mass: 0,
        time: -1,
        current: 0,
        temperature: 0,
        molar_amount: 0,
        luminous_intensity: 0,
        plane_angle: 0,
    };

    /// Length: m
    pub const LENGTH: Self = Self {
        id: None,
        length: 1,
        mass: 0,
        time: 0,
        current: 0,
        temperature: 0,
        molar_amount: 0,
        luminous_intensity: 0,
        plane_angle: 0,
    };

    /// Set the dimension ID.
    #[must_use]
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    #[test]
    fn unit_serializes_correctly() {
        let unit = Unit::new("km/h")
            .with_factor(3.6)
            .with_dimension(PhysicalDimension::SPEED.with_id("speed"));

        let json = serde_json::to_value(&unit).unwrap();
        assert_eq!(json["display_name"], "km/h");
        assert_eq!(json["factor_si_to_unit"], 3.6);
        assert!(json.get("offset_si_to_unit").is_none()); // skipped when 0
    }

    #[test]
    fn unit_skips_factor_when_one() {
        let unit = Unit::new("V");
        let json = serde_json::to_value(&unit).unwrap();
        assert_eq!(json["display_name"], "V");
        assert!(json.get("factor_si_to_unit").is_none()); // skipped when 1
    }

    #[test]
    fn physical_dimension_skips_zero_exponents() {
        let dim = PhysicalDimension::VOLTAGE.with_id("voltage");
        let json = serde_json::to_value(&dim).unwrap();

        assert_eq!(json["length"], 2);
        assert_eq!(json["mass"], 1);
        assert_eq!(json["time"], -3);
        assert_eq!(json["current"], -1);
        assert!(json.get("temperature").is_none()); // skipped
    }

    #[test]
    fn unit_deserializes_correctly() {
        let json = r#"{"display_name":"V"}"#;
        let unit: Unit = serde_json::from_str(json).unwrap();
        assert_eq!(unit.display_name, "V");
        assert!((unit.factor_si_to_unit - 1.0).abs() < f64::EPSILON); // default
        assert!(unit.offset_si_to_unit.abs() < f64::EPSILON); // default
    }

    #[test]
    fn unit_deserializes_with_factor() {
        let json = r#"{"display_name":"km/h","factor_si_to_unit":3.6}"#;
        let unit: Unit = serde_json::from_str(json).unwrap();
        assert_eq!(unit.display_name, "km/h");
        assert!((unit.factor_si_to_unit - 3.6).abs() < f64::EPSILON);
    }

    #[test]
    fn unit_serializes_offset() {
        let unit = Unit::new("deg C").with_offset(-273.15);
        let json = serde_json::to_value(&unit).unwrap();
        assert_eq!(json["offset_si_to_unit"], -273.15);
        assert!(json.get("factor_si_to_unit").is_none()); // still skipped when 1
    }

    #[test]
    fn physical_dimension_deserializes_with_defaults() {
        let json = r#"{"id":"temperature","temperature":1}"#;
        let dim: PhysicalDimension = serde_json::from_str(json).unwrap();
        assert_eq!(dim.id, Some("temperature".to_string()));
        assert_eq!(dim.temperature, 1);
        assert_eq!(dim.length, 0); // default
    }
}
