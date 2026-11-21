// SPDX-License-Identifier: GPL-3.0-only

//! Shared Coldfront domain logic.
//!
//! The COSMIC applet calls this crate directly. The web app keeps a parallel
//! TypeScript implementation and both run the JSON fixtures in `fixtures/`.

pub mod alerts;
pub mod chart;
pub mod client;
pub mod error;
pub mod forecast;
pub mod geocode;
pub mod location;
pub mod place;
pub mod radar;
pub mod relief;
pub mod units;
pub mod weather;
pub mod zipcode;

pub use alerts::{Alert, parse_alerts};
pub use chart::{ChartPoint, RangePlacement, chart_points, range_placement};
pub use client::HttpClient;
pub use error::Error;
pub use forecast::{
    Day, Forecast, Hour, day_label, format_as_of, format_clock_time, format_precip_amount_mm,
    hour_label, next_hours, parse_open_meteo,
};
pub use geocode::{
    GEOCODE_QUERY_MAX_LEN, GeocodeHit, format_geocode_label, geocode_query_allowed, geocode_url,
    parse_geocode_search, place_from_geocode_hit, state_abbreviation,
};
pub use location::{
    RouteTarget, coordinates_match, has_resolved_coordinates, is_zip_lookup, location_path,
    parse_route_path, place_from_coordinates,
};
pub use place::{Place, parse_zippopotam};
pub use radar::{
    APPLET_ZOOM_MAX, APPLET_ZOOM_MIN, GestureCamera, IEM_TILE_ORIGIN, MAP_DISPLAY_HEIGHT,
    MAP_DISPLAY_WIDTH, MapCamera, MapViewport, RadarFrame, Raster, SnapshotTransform, TILE_SIZE,
    TileCamera, TilePoint, TileWindow, applet_radar_frames, applet_radar_frames_at,
    compose_radar_map, compose_viewport, compose_viewport_ground, compose_viewport_radar_layer,
    iem_tile_url, lat_lon_to_tile, map_viewport, maptiler_tile_url, osm_tile_url, pin_mercator,
    radar_layer, scale_raster, snapshot_transform, tile_camera, tile_window, web_radar_frames,
    web_radar_frames_at, world_to_screen, zoom_toward,
};
pub use relief::{
    DEFAULT_COLS, DEFAULT_PITCH, DEFAULT_ROWS, DEFAULT_YAW, EchoField, EchoSample, OrthoFit,
    ReliefCamera, band_height, build_echo_field, clamp_relief_distance, clamp_relief_pitch,
    default_relief_camera, denoise_radar, denoise_radar_png, echo_grid, fit_ortho_half_extents,
    hue_degrees, relief_camera_locked, render_relief, sample_echoes,
};
pub use units::{
    celsius_to_fahrenheit, display_temperature, display_wind_mph, kmh_to_mph,
    round_half_away_from_zero,
};
pub use weather::{WeatherInfo, describe, display_icon, symbolic_icon};
pub use zipcode::is_us_zip;
