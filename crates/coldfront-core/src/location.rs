// SPDX-License-Identifier: GPL-3.0-only

use crate::place::Place;
use crate::zipcode::is_us_zip;

const COORD_ROUTE_PREFIX: &str = "/@";

/// Whether the trimmed input should run a ZIP lookup instead of city search.
pub fn is_zip_lookup(value: &str) -> bool {
    is_us_zip(value.trim())
}

/// Whether a saved coordinate pair is usable for weather loading.
pub fn has_resolved_coordinates(latitude: f64, longitude: f64) -> bool {
    latitude.is_finite()
        && longitude.is_finite()
        && (-90.0..=90.0).contains(&latitude)
        && (-180.0..=180.0).contains(&longitude)
        && (latitude != 0.0 || longitude != 0.0)
}

/// Choose the navigation path after a place is resolved.
pub fn location_path(place: &Place) -> String {
    if is_us_zip(&place.zip) {
        format!("/{}", place.zip)
    } else {
        format!(
            "{}{:.4},{:.4}",
            COORD_ROUTE_PREFIX, place.latitude, place.longitude
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RouteTarget {
    Home,
    Zip(String),
    Coordinates { latitude: f64, longitude: f64 },
}

/// Parse a Coldfront web pathname into a forecast target.
pub fn parse_route_path(path: &str) -> RouteTarget {
    let trimmed = path.trim_end_matches('/').trim_start_matches('/');
    if trimmed.is_empty() {
        return RouteTarget::Home;
    }
    if let Some(rest) = trimmed.strip_prefix('@') {
        let Some((lat, lon)) = rest.split_once(',') else {
            return RouteTarget::Home;
        };
        let latitude = lat.parse::<f64>().ok();
        let longitude = lon.parse::<f64>().ok();
        if let (Some(latitude), Some(longitude)) = (latitude, longitude)
            && has_resolved_coordinates(latitude, longitude)
        {
            return RouteTarget::Coordinates {
                latitude,
                longitude,
            };
        }
        return RouteTarget::Home;
    }
    if is_us_zip(trimmed) {
        return RouteTarget::Zip(trimmed.to_owned());
    }
    RouteTarget::Home
}

pub fn place_from_coordinates(latitude: f64, longitude: f64, stored: Option<&Place>) -> Place {
    if let Some(place) = stored
        && coordinates_match(place.latitude, place.longitude, latitude, longitude)
    {
        return place.clone();
    }
    Place {
        zip: String::new(),
        city: String::new(),
        state: String::new(),
        state_abbreviation: String::new(),
        latitude,
        longitude,
    }
}

pub fn coordinates_match(a_lat: f64, a_lon: f64, b_lat: f64, b_lon: f64) -> bool {
    (a_lat - b_lat).abs() < 0.000_05 && (a_lon - b_lon).abs() < 0.000_05
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_inputs_are_detected() {
        assert!(is_zip_lookup("95501"));
        assert!(!is_zip_lookup("Eureka"));
    }

    #[test]
    fn routes_distinguish_zip_and_coordinates() {
        assert_eq!(parse_route_path("/"), RouteTarget::Home);
        assert_eq!(parse_route_path("/95501"), RouteTarget::Zip("95501".into()));
        assert_eq!(
            parse_route_path("/@40.8021,-124.1637"),
            RouteTarget::Coordinates {
                latitude: 40.8021,
                longitude: -124.1637
            }
        );
    }
}
