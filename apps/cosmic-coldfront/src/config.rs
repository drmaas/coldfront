// SPDX-License-Identifier: GPL-3.0-only

use cosmic::cosmic_config::{self, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};

use crate::app::APP_ID;

#[derive(Debug, Clone, CosmicConfigEntry, Eq, PartialEq)]
#[version = 1]
pub struct Config {
    pub zip: String,
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
            fahrenheit: true,
            maptiler_api_key: String::new(),
            allow_osm_basemap: false,
        }
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
