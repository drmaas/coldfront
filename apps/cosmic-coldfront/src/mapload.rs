// SPDX-License-Identifier: GPL-3.0-only

//! Fetch basemap and NEXRAD tiles and composite them into one image.
//!
//! The popup shows this image directly. There is no map tab and no extra click.

use std::fmt;

use coldfront_core::{
    Alert, Error, Forecast, HttpClient, Place, RadarFrame, TileWindow, compose_radar_map,
    iem_tile_url, maptiler_tile_url, osm_tile_url,
};

#[derive(Clone)]
pub enum BasemapKind {
    MapTiler(String),
    /// Opt-in only. The OSMF public tile servers are not a default app backend.
    Osm,
    None,
}

impl fmt::Debug for BasemapKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MapTiler(_) => write!(formatter, "MapTiler"),
            Self::Osm => write!(formatter, "Osm"),
            Self::None => write!(formatter, "None"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct WeatherBundle {
    pub place: Place,
    pub forecast: Forecast,
    pub alerts: Vec<Alert>,
}

#[derive(Clone, Debug)]
pub struct LoadedFrame {
    pub offset_minutes: u32,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub radar_tiles: usize,
}

pub async fn load_weather(client: &HttpClient, zip: &str) -> Result<WeatherBundle, Error> {
    let place = client.lookup_zip(zip).await?;
    let forecast = client.forecast(place.latitude, place.longitude).await?;
    let alerts = client
        .alerts(place.latitude, place.longitude)
        .await
        .unwrap_or_default();
    Ok(WeatherBundle {
        place,
        forecast,
        alerts,
    })
}

pub async fn load_basemap(
    client: &HttpClient,
    grid: &TileWindow,
    kind: &BasemapKind,
) -> Vec<Option<Vec<u8>>> {
    match kind {
        BasemapKind::None => vec![None; (grid.span_x * grid.span_y) as usize],
        BasemapKind::MapTiler(key) => {
            let urls = tile_coords(grid)
                .into_iter()
                .map(|(x, y)| maptiler_tile_url(grid.zoom, x, y, key))
                .collect();
            fetch_tiles(client, urls).await
        }
        BasemapKind::Osm => {
            let urls = tile_coords(grid)
                .into_iter()
                .map(|(x, y)| osm_tile_url(grid.zoom, x, y))
                .collect();
            fetch_tiles(client, urls).await
        }
    }
}

pub async fn load_radar_frame(
    client: &HttpClient,
    grid: &TileWindow,
    basemap: &[Option<Vec<u8>>],
    frame: &RadarFrame,
) -> Result<LoadedFrame, Error> {
    let urls = tile_coords(grid)
        .into_iter()
        .map(|(x, y)| iem_tile_url(&frame.layer, grid.zoom, x, y))
        .collect();
    let radar = fetch_tiles(client, urls).await;
    let radar_tiles = radar.iter().filter(|tile| tile.is_some()).count();
    let raster = compose_radar_map(grid, basemap, &radar)?;
    Ok(LoadedFrame {
        offset_minutes: frame.offset_minutes,
        width: raster.width,
        height: raster.height,
        pixels: raster.pixels,
        radar_tiles,
    })
}

fn tile_coords(grid: &TileWindow) -> Vec<(u32, u32)> {
    let mut coords = Vec::new();
    for row in 0..grid.span_y {
        for col in 0..grid.span_x {
            coords.push((grid.origin_x + col, grid.origin_y + row));
        }
    }
    coords
}

async fn fetch_tiles(client: &HttpClient, urls: Vec<String>) -> Vec<Option<Vec<u8>>> {
    let downloads = urls.into_iter().map(|url| {
        let client = client.clone();
        async move { client.get_bytes(&url).await.ok() }
    });
    futures::future::join_all(downloads).await
}
