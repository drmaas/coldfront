// SPDX-License-Identifier: GPL-3.0-only

use crate::error::Error;
use crate::place::Place;
use crate::zipcode::is_us_zip;
use serde_json::Value;

/// One US city match from Open-Meteo geocoding.
#[derive(Debug, Clone, PartialEq)]
pub struct GeocodeHit {
    pub name: String,
    pub admin1: String,
    pub state_abbreviation: String,
    pub latitude: f64,
    pub longitude: f64,
    pub postcodes: Vec<String>,
}

/// Parse an Open-Meteo `/v1/search` document.
pub fn parse_geocode_search(bytes: &[u8]) -> Result<Vec<GeocodeHit>, Error> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|err| Error::Parse(err.to_string()))?;
    let Some(results) = value.get("results").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut hits = Vec::new();
    for entry in results {
        let Some(hit) = parse_geocode_result(entry) else {
            continue;
        };
        hits.push(hit);
    }
    Ok(hits)
}

/// Turn a geocode hit into a [`Place`] for forecast loading.
pub fn place_from_geocode_hit(hit: &GeocodeHit) -> Place {
    let zip = hit
        .postcodes
        .iter()
        .find(|postcode| is_us_zip(postcode))
        .cloned()
        .unwrap_or_default();
    Place {
        zip,
        city: hit.name.clone(),
        state: hit.admin1.clone(),
        state_abbreviation: hit.state_abbreviation.clone(),
        latitude: hit.latitude,
        longitude: hit.longitude,
    }
}

pub fn geocode_url(query: &str, count: u8) -> String {
    let trimmed = query.trim();
    format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&countryCode=US&count={count}&language=en&format=json",
        urlencoding(trimmed)
    )
}

fn parse_geocode_result(value: &Value) -> Option<GeocodeHit> {
    let country = value.get("country_code").and_then(Value::as_str)?;
    if country != "US" {
        return None;
    }
    let name = string_or_empty(value, "name");
    if name.is_empty() {
        return None;
    }
    let admin1 = string_or_empty(value, "admin1");
    let latitude = value.get("latitude").and_then(Value::as_f64)?;
    let longitude = value.get("longitude").and_then(Value::as_f64)?;
    let postcodes = value
        .get("postcodes")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default();
    Some(GeocodeHit {
        state_abbreviation: state_abbreviation(&admin1),
        name,
        admin1,
        latitude,
        longitude,
        postcodes,
    })
}

pub fn state_abbreviation(admin1: &str) -> String {
    state_abbreviations()
        .iter()
        .find_map(|(name, abbr)| (*name == admin1).then_some(*abbr))
        .unwrap_or(admin1)
        .to_string()
}

pub fn format_geocode_label(hit: &GeocodeHit) -> String {
    let state = if hit.state_abbreviation.is_empty() {
        hit.admin1.clone()
    } else {
        hit.state_abbreviation.clone()
    };
    let location = format!("{}, {}", hit.name, state);
    if let Some(postcode) = hit.postcodes.iter().find(|postcode| is_us_zip(postcode)) {
        format!("{location} ({postcode})")
    } else {
        format!(
            "{location} ({:.2}, {:.2})",
            hit.latitude, hit.longitude
        )
    }
}

fn string_or_empty(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn urlencoding(text: &str) -> String {
    let mut encoded = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            b' ' => encoded.push('+'),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn state_abbreviations() -> &'static [(&'static str, &'static str)] {
    &[
        ("Alabama", "AL"),
        ("Alaska", "AK"),
        ("Arizona", "AZ"),
        ("Arkansas", "AR"),
        ("California", "CA"),
        ("Colorado", "CO"),
        ("Connecticut", "CT"),
        ("Delaware", "DE"),
        ("District of Columbia", "DC"),
        ("Florida", "FL"),
        ("Georgia", "GA"),
        ("Hawaii", "HI"),
        ("Idaho", "ID"),
        ("Illinois", "IL"),
        ("Indiana", "IN"),
        ("Iowa", "IA"),
        ("Kansas", "KS"),
        ("Kentucky", "KY"),
        ("Louisiana", "LA"),
        ("Maine", "ME"),
        ("Maryland", "MD"),
        ("Massachusetts", "MA"),
        ("Michigan", "MI"),
        ("Minnesota", "MN"),
        ("Mississippi", "MS"),
        ("Missouri", "MO"),
        ("Montana", "MT"),
        ("Nebraska", "NE"),
        ("Nevada", "NV"),
        ("New Hampshire", "NH"),
        ("New Jersey", "NJ"),
        ("New Mexico", "NM"),
        ("New York", "NY"),
        ("North Carolina", "NC"),
        ("North Dakota", "ND"),
        ("Ohio", "OH"),
        ("Oklahoma", "OK"),
        ("Oregon", "OR"),
        ("Pennsylvania", "PA"),
        ("Rhode Island", "RI"),
        ("South Carolina", "SC"),
        ("South Dakota", "SD"),
        ("Tennessee", "TN"),
        ("Texas", "TX"),
        ("Utah", "UT"),
        ("Vermont", "VT"),
        ("Virginia", "VA"),
        ("Washington", "WA"),
        ("West Virginia", "WV"),
        ("Wisconsin", "WI"),
        ("Wyoming", "WY"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn fixture(name: &str) -> Vec<u8> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("fixtures")
            .join(name);
        fs::read(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
    }

    #[test]
    fn eureka_matches_include_postcodes() {
        let hits = parse_geocode_search(&fixture("geocode-eureka.json")).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].name, "Eureka");
        assert_eq!(hits[0].state_abbreviation, "CA");
        assert!(hits[0].postcodes.contains(&"95501".to_owned()));
        let place = place_from_geocode_hit(&hits[0]);
        assert_eq!(place.zip, "95501");
        assert_eq!(place.city, "Eureka");
        assert_eq!(place.state_abbreviation, "CA");
    }
}