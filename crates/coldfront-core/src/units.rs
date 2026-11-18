// SPDX-License-Identifier: GPL-3.0-only

/// Half away from zero, matching the TypeScript display helper.
#[must_use]
pub fn round_half_away_from_zero(value: f64) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    let rounded = if value >= 0.0 {
        (value + 0.5).floor()
    } else {
        (value - 0.5).ceil()
    };
    rounded as i32
}

#[must_use]
pub fn celsius_to_fahrenheit(celsius: f64) -> f64 {
    celsius * 9.0 / 5.0 + 32.0
}

#[must_use]
pub fn kmh_to_mph(kmh: f64) -> f64 {
    kmh / 1.609_344
}

#[must_use]
pub fn display_temperature(celsius: f64, fahrenheit: bool) -> i32 {
    let value = if fahrenheit {
        celsius_to_fahrenheit(celsius)
    } else {
        celsius
    };
    round_half_away_from_zero(value)
}

#[must_use]
pub fn display_wind_mph(kmh: f64) -> i32 {
    round_half_away_from_zero(kmh_to_mph(kmh))
}
