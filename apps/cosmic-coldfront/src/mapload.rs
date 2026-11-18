// SPDX-License-Identifier: GPL-3.0-only

//! Fetch basemap and NEXRAD tiles and composite them into one image.
//!
//! Tiles are cached by source, zoom, and coordinates so pan/zoom can recompose
//! without refetching. Radar frames load only when the Radar tab needs them.

use std::collections::HashMap;
use std::fmt;

use coldfront_core::{
    Alert, Error, Forecast, HttpClient, MapCamera, MapViewport, Place, RadarFrame,
    applet_radar_frames, compose_viewport, iem_tile_url, map_viewport, maptiler_tile_url,
    osm_tile_url,
};

pub const MAP_VIEW_WIDTH: u32 = 800;
pub const MAP_VIEW_HEIGHT: u32 = 480;
const TILE_MARGIN: u32 = 1;

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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum TileLayer {
    MapTiler,
    Osm,
    Radar { offset_minutes: u32 },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct TileKey {
    layer: TileLayer,
    zoom: u32,
    x: u32,
    y: u32,
}

#[derive(Clone, Debug, Default)]
pub struct TileCache {
    tiles: HashMap<TileKey, Option<Vec<u8>>>,
}

impl TileCache {
    pub fn clear(&mut self) {
        self.tiles.clear();
    }

    fn get(&self, key: TileKey) -> Option<&Option<Vec<u8>>> {
        self.tiles.get(&key)
    }

    fn insert_ok(&mut self, key: TileKey, bytes: Vec<u8>) {
        self.tiles.insert(key, Some(bytes));
    }

    /// True when the cache has no successful tile yet (absent or prior miss).
    fn needs_fetch(&self, key: TileKey) -> bool {
        !matches!(self.get(key), Some(Some(_)))
    }
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

#[must_use]
pub fn viewport_for(
    latitude: f64,
    longitude: f64,
    zoom: u32,
    pan_x: f64,
    pan_y: f64,
) -> MapViewport {
    map_viewport(MapCamera {
        latitude,
        longitude,
        zoom,
        pan_x,
        pan_y,
        view_width: MAP_VIEW_WIDTH,
        view_height: MAP_VIEW_HEIGHT,
        margin: TILE_MARGIN,
    })
}

/// Place, camera, and frame needed to fill and compose a map.
pub struct MapRequest {
    pub latitude: f64,
    pub longitude: f64,
    pub zoom: u32,
    pub pan_x: f64,
    pub pan_y: f64,
    pub offset_minutes: u32,
}

/// Fetch any tiles missing from the cache for the current viewport and frame,
/// then compose the visible raster. Failed fetches stay absent so the next
/// settle can retry; holes paint as transparent / pale background.
pub async fn ensure_map_frame(
    client: &HttpClient,
    cache: &mut TileCache,
    kind: &BasemapKind,
    request: &MapRequest,
) -> Result<LoadedFrame, Error> {
    let viewport = viewport_for(
        request.latitude,
        request.longitude,
        request.zoom,
        request.pan_x,
        request.pan_y,
    );
    let frame = applet_radar_frames()
        .into_iter()
        .find(|item| item.offset_minutes == request.offset_minutes)
        .ok_or_else(|| Error::Parse("unknown radar frame".into()))?;
    fill_viewport_tiles(client, cache, kind, &viewport, &frame).await;
    compose_cached(cache, kind, &viewport, request.offset_minutes, false)?
        .ok_or_else(|| Error::Parse("map compose produced no raster".into()))
}

/// Recompose from cache.
///
/// When `require_complete` is true, returns `Ok(None)` if any viewport tile key
/// is still absent so the caller can keep the previous image during pan/zoom.
/// When false, absent keys paint as holes (used after a fetch pass).
pub fn compose_cached(
    cache: &TileCache,
    kind: &BasemapKind,
    viewport: &MapViewport,
    offset_minutes: u32,
    require_complete: bool,
) -> Result<Option<LoadedFrame>, Error> {
    let basemap_layer = match kind {
        BasemapKind::MapTiler(_) => Some(TileLayer::MapTiler),
        BasemapKind::Osm => Some(TileLayer::Osm),
        BasemapKind::None => None,
    };
    let radar_layer = TileLayer::Radar { offset_minutes };
    let coords = viewport.tile_coords();
    let mut basemap = Vec::with_capacity(coords.len());
    let mut radar = Vec::with_capacity(coords.len());
    let mut radar_tiles = 0usize;
    for &(x, y) in &coords {
        if let Some(layer) = basemap_layer {
            let key = TileKey {
                layer,
                zoom: viewport.zoom,
                x,
                y,
            };
            match cache.get(key) {
                None if require_complete => return Ok(None),
                None | Some(None) => basemap.push(None),
                Some(Some(bytes)) => basemap.push(Some(bytes.clone())),
            }
        } else {
            basemap.push(None);
        }
        let key = TileKey {
            layer: radar_layer,
            zoom: viewport.zoom,
            x,
            y,
        };
        match cache.get(key) {
            None if require_complete => return Ok(None),
            None | Some(None) => radar.push(None),
            Some(Some(bytes)) => {
                radar_tiles += 1;
                radar.push(Some(bytes.clone()));
            }
        }
    }
    let raster = compose_viewport(viewport, &basemap, &radar)?;
    Ok(Some(LoadedFrame {
        offset_minutes,
        width: raster.width,
        height: raster.height,
        pixels: raster.pixels,
        radar_tiles,
    }))
}

async fn fill_viewport_tiles(
    client: &HttpClient,
    cache: &mut TileCache,
    kind: &BasemapKind,
    viewport: &MapViewport,
    frame: &RadarFrame,
) {
    let mut missing: Vec<(TileKey, String)> = Vec::new();
    for (x, y) in viewport.tile_coords() {
        match kind {
            BasemapKind::MapTiler(key) => {
                let tile_key = TileKey {
                    layer: TileLayer::MapTiler,
                    zoom: viewport.zoom,
                    x,
                    y,
                };
                if cache.needs_fetch(tile_key) {
                    missing.push((tile_key, maptiler_tile_url(viewport.zoom, x, y, key)));
                }
            }
            BasemapKind::Osm => {
                let tile_key = TileKey {
                    layer: TileLayer::Osm,
                    zoom: viewport.zoom,
                    x,
                    y,
                };
                if cache.needs_fetch(tile_key) {
                    missing.push((tile_key, osm_tile_url(viewport.zoom, x, y)));
                }
            }
            BasemapKind::None => {}
        }
        let radar_key = TileKey {
            layer: TileLayer::Radar {
                offset_minutes: frame.offset_minutes,
            },
            zoom: viewport.zoom,
            x,
            y,
        };
        if cache.needs_fetch(radar_key) {
            missing.push((radar_key, iem_tile_url(&frame.layer, viewport.zoom, x, y)));
        }
    }
    if missing.is_empty() {
        return;
    }
    let downloads = missing.into_iter().map(|(key, url)| {
        let client = client.clone();
        async move { (key, client.get_bytes(&url).await.ok()) }
    });
    for (key, bytes) in futures::future::join_all(downloads).await {
        // Leave failures absent so the next settle can retry.
        if let Some(bytes) = bytes {
            cache.insert_ok(key, bytes);
        }
    }
}
