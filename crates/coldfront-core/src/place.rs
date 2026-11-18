// SPDX-License-Identifier: GPL-3.0-only

use crate::error::Error;
use serde_json::Value;

/// A ZIP code resolved through Zippopotam.us.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub zip: String,
    pub city: String,
    pub state: String,
    pub state_abbreviation: String,
    pub latitude: f64,
    pub longitude: f64,
}

/// Parse a Zippopotam `/us/{zip}` document.
pub fn parse_zippopotam(bytes: &[u8]) -> Result<Place, Error> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|err| Error::Parse(err.to_string()))?;
    let zip = string_field(&value, "post code")?;
    let places = value
        .get("places")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Parse("missing places".into()))?;
    let place = places.first().ok_or(Error::NotFound)?;
    let latitude = coord(place, "latitude")?;
    let longitude = coord(place, "longitude")?;
    Ok(Place {
        zip,
        city: string_field(place, "place name")?,
        state: string_field(place, "state")?,
        state_abbreviation: string_field(place, "state abbreviation")?,
        latitude,
        longitude,
    })
}

fn string_field(value: &Value, key: &str) -> Result<String, Error> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| Error::Parse(format!("missing {key}")))
}

fn coord(value: &Value, key: &str) -> Result<f64, Error> {
    let text = string_field(value, key)?;
    text.parse::<f64>()
        .map_err(|_| Error::Parse(format!("bad {key}")))
}
