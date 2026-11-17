// SPDX-License-Identifier: GPL-3.0-only

//! Shared Coldfront domain logic.
//!
//! The COSMIC applet calls this crate directly. The web app keeps a parallel
//! TypeScript implementation and both run the JSON fixtures in `fixtures/`.

pub mod alerts;
pub mod client;
pub mod error;
pub mod forecast;
pub mod place;
pub mod radar;
pub mod units;
pub mod weather;
pub mod zipcode;

pub use alerts::{Alert, parse_alerts};
pub use client::HttpClient;
pub use error::Error;
pub use forecast::{
    Day, Forecast, Hour, day_label, format_as_of, hour_label, next_hours, parse_open_meteo,
};
pub use place::{Place, parse_zippopotam};
pub use radar::{
    IEM_TILE_ORIGIN, MAP_DISPLAY_HEIGHT, MAP_DISPLAY_WIDTH, RadarFrame, Raster, TilePoint,
    TileWindow, applet_radar_frames, compose_radar_map, iem_tile_url, lat_lon_to_tile,
    maptiler_tile_url, osm_tile_url, tile_window, web_radar_frames,
};
pub use units::{
    celsius_to_fahrenheit, display_temperature, display_wind_mph, kmh_to_mph,
    round_half_away_from_zero,
};
pub use weather::{WeatherInfo, describe, symbolic_icon};
pub use zipcode::is_us_zip;
