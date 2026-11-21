// SPDX-License-Identifier: GPL-3.0-only

use coldfront_core::{Place, has_resolved_coordinates, is_us_zip};
use cosmic::cosmic_config::{self, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};

use crate::app::APP_ID;

#[derive(Debug, Clone, CosmicConfigEntry, Eq, PartialEq)]
#[version = 1]
pub struct Config {
    pub zip: String,
    /// Resolved city name when the location came from geocoding.
    pub city: String,
    pub state_abbreviation: String,
    pub latitude: f64,
    pub longitude: f64,
    pub fahrenheit: bool,
    /// Optional MapTiler key. `MAPTILER_API_KEY` in the environment wins over this.
    pub maptiler_api_key: String,
    /// When there is no MapTiler key, fetch OpenStreetMap raster tiles.
    /// Off by default: the OSMF tile policy disallows using the public servers
    /// as the backend of a distributed app. Turn this on for a personal install.
    pub allow_osm_basemap: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            zip: String::new(),
            city: String::new(),
            state_abbreviation: String::new(),
            latitude: 0.0,
            longitude: 0.0,
            fahrenheit: true,
            maptiler_api_key: String::new(),
            allow_osm_basemap: false,
        }
    }
}

impl Config {
    pub fn clear_resolved_place(&mut self) {
        self.city.clear();
        self.state_abbreviation.clear();
        self.latitude = 0.0;
        self.longitude = 0.0;
    }

    pub fn apply_place(&mut self, place: &Place) {
        self.zip = if is_us_zip(&place.zip) {
            place.zip.clone()
        } else {
            String::new()
        };
        self.city = place.city.clone();
        self.state_abbreviation = place.state_abbreviation.clone();
        self.latitude = place.latitude;
        self.longitude = place.longitude;
    }

    pub fn has_weather_location(&self) -> bool {
        is_us_zip(&self.zip) || has_resolved_coordinates(self.latitude, self.longitude)
    }

    pub fn saved_place(&self) -> Option<Place> {
        if !has_resolved_coordinates(self.latitude, self.longitude) {
            return None;
        }
        Some(Place {
            zip: self.zip.clone(),
            city: self.city.clone(),
            state: String::new(),
            state_abbreviation: self.state_abbreviation.clone(),
            latitude: self.latitude,
            longitude: self.longitude,
        })
    }
}

pub fn load() -> Config {
    cosmic_config::Config::new(APP_ID, Config::VERSION)
        .map(|context| match Config::get_entry(&context) {
            Ok(config) => config,
            Err((_, config)) => config,
        })
        .unwrap_or_default()
}

pub fn save(config: &Config) {
    let Ok(context) = cosmic_config::Config::new(APP_ID, Config::VERSION) else {
        eprintln!("cosmic-coldfront: could not open config");
        return;
    };
    if let Err(error) = config.write_entry(&context) {
        eprintln!("cosmic-coldfront: could not save config: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_only_config_still_loads() {
        let mut config = Config::default();
        config.zip = "95501".into();
        assert!(config.has_weather_location());
        assert!(config.saved_place().is_none());
    }

    #[test]
    fn saved_coordinates_fallback_without_zip() {
        let mut config = Config::default();
        config.city = "Eureka".into();
        config.state_abbreviation = "CA".into();
        config.latitude = 40.8021;
        config.longitude = -124.1637;
        assert!(config.has_weather_location());
        let place = config.saved_place().expect("place");
        assert_eq!(place.city, "Eureka");
        assert_eq!(place.state_abbreviation, "CA");
        assert!(place.zip.is_empty());
    }
}
